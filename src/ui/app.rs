use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;
use std::{env, process::Command};

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_component::resizable::{h_resizable, resizable_panel};
use gpui_component::scroll::ScrollableElement;
use gpui_component::tag::Tag;
use gpui_component::{Disableable, Icon, IconName, Sizable, h_flex, v_flex};
use rfd::FileDialog;

use crate::ai::AiClient;
use crate::git::{GitClient, safe_repository_directory_name};
use crate::models::{
    AiProvider, AppSettings, BranchInfo, ChangeEntry, CommitInfo, CommitSuggestion, DiffEntry,
    GitIdentity, INVALID_GIT_AUTHOR_NAME_MESSAGE, RemoteModelOption, RepoSnapshot,
    git_author_name_is_valid,
};
use crate::storage::{push_recent_repo, save_settings};
use crate::ui::automation;
use crate::ui::branch_context_menu::BranchContextAction;
use crate::ui::changes_context_menu::{self, ChangesContextAction};
use crate::ui::domain_state::{
    CommitState, NetworkAction, NetworkState, RepoState, SelectionState,
};
use crate::ui::history_context_menu::HistoryContextMenuAction;
use crate::ui::ids::stable_id_slug;
use crate::ui::settings_modal::{self, SettingsField, SettingsModalState};
use crate::ui::stash_file_list::render_stash_file_list;
use crate::ui::theme;
use crate::ui::ui_state::{
    ActiveDialog, BranchSelectorMode, FilterState, MessageState, NavState, OpenRouterModelsState,
    SettingsScope, SidebarTab,
};

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RepoRefreshReason {
    Manual,
    #[allow(dead_code)]
    Focus,
    Watch,
}

pub(crate) enum AppEvent {
    RepoLoaded(Result<RepoSnapshot, String>),
    RepoRefreshed(PathBuf, Result<RepoSnapshot, String>, RepoRefreshReason),
    BranchSwitched(Result<RepoSnapshot, String>, String),
    BranchMerged(Result<RepoSnapshot, String>, String),
    CommitCreated(Result<RepoSnapshot, String>, String),
    CommitUndone(Result<RepoSnapshot, String>),
    NetworkActionCompleted(Result<RepoSnapshot, String>, String),
    AiCommitGenerated(Result<CommitSuggestion, String>),
    OpenRouterModelsLoaded(Result<Vec<RemoteModelOption>, String>),
    CommitDiffLoaded(String, Result<Vec<DiffEntry>, String>),
    /// A single file's diff was refreshed (path, updated entry).
    FileDiffRefreshed(String, Result<DiffEntry, String>),
    RepoOperationCompleted(Result<RepoSnapshot, String>, String, String),
    CommitDiffCopied(String, Result<String, String>),
    Automation(automation::AutomationRequest),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RepositoryField {
    CreateName,
    CreateDescription,
    CreatePath,
    CloneUrl,
    ClonePath,
}

/// Direction for diff context expansion.
#[derive(Clone, Copy, Debug)]
pub enum DiffExpandDirection {
    Up,
    Down,
    All,
    /// Fill the gap between this hunk and the previous one (Short expansion).
    MergeWithPrevious,
}

// ---------------------------------------------------------------------------
// Actions (dispatched by child views via gpui action system)
// ---------------------------------------------------------------------------

// Toolbar
#[derive(Clone)]
pub enum ToolbarAction {
    ToggleRepoSelector,
    #[allow(dead_code)]
    SwitchBranch(String),
    RunNetworkAction(NetworkAction),
    #[allow(dead_code)]
    FetchOrigin,
    #[allow(dead_code)]
    PullOrigin,
    #[allow(dead_code)]
    PushOrigin,
}

// Sidebar
#[derive(Clone)]
pub enum SidebarAction {
    #[allow(dead_code)]
    OpenRepoDialog,
    OpenRepo(PathBuf),
    #[allow(dead_code)]
    HideRepoSelector,
    SelectChange(String),
    DiscardChange(String),
    IgnorePath(String),
    IgnoreExtension(String),
    CopyFullPath(String),
    CopyRelativePath(String),
    RevealInFinder(String),
    OpenInEditor(String),
    OpenWithDefault(String),
    SelectCommit(String),
    GenerateAiCommit,
    #[allow(dead_code)]
    ShowSettings,
    CommitAll,
}

// Settings
#[derive(Clone)]
pub enum SettingsAction {
    SaveRemote,
    SaveIgnoredFiles,
    SaveGitConfig,
    SaveAiSettings,
    SetGitConfigScope(bool),
    ChangeProvider(AiProvider),
    SelectOpenRouterModel(String),
    RetryOpenRouterModels,
    Close,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PublishField {
    Name,
    Description,
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

/// Sender wrapper that sets an atomic flag before sending,
/// so the poll timer can skip acquiring the app lock when idle.
#[derive(Clone)]
pub(crate) struct NotifySender {
    tx: Sender<AppEvent>,
    pending: Arc<AtomicBool>,
}

impl NotifySender {
    pub(crate) fn send(&self, event: AppEvent) {
        self.pending.store(true, Ordering::Release);
        let _ = self.tx.send(event);
    }
}

// ---------------------------------------------------------------------------
// Zoom actions
// ---------------------------------------------------------------------------

actions!(gitspark, [ZoomIn, ZoomOut, ZoomReset]);

const DEFAULT_REM_SIZE: f32 = 14.0;
const ZOOM_STEP: f32 = 1.0;
const ZOOM_MIN: f32 = 10.0;
const ZOOM_MAX: f32 = 24.0;
const MAX_TAG_NAME_LENGTH: usize = 245;

pub struct GitSparkApp {
    git: GitClient,
    pub settings: AppSettings,
    pub repo: RepoState,
    pub commit: CommitState,
    pub network: NetworkState,
    pub selection: SelectionState,
    pub nav: NavState,
    pub filters: FilterState,
    pub messages: MessageState,
    repo_watch_generation: Arc<AtomicU64>,
    watched_repo_path: Option<PathBuf>,
    pub(crate) event_tx: NotifySender,
    event_rx: Receiver<AppEvent>,
    // Text input state
    summary_focus: FocusHandle,
    description_focus: FocusHandle,
    summary_cursor: usize,
    /// Selection anchor for summary field. When Some, text between anchor and cursor is selected.
    summary_selection: Option<usize>,
    description_cursor: usize,
    /// Selection anchor for description field.
    description_selection: Option<usize>,
    // Filter input state
    branch_filter_focus: FocusHandle,
    branch_filter_cursor: usize,
    repo_filter_focus: FocusHandle,
    repo_filter_cursor: usize,
    new_branch_focus: FocusHandle,
    pub(crate) new_branch_cursor: usize,
    pub(crate) new_branch_selection: Option<usize>,
    pub(crate) publish_focus: FocusHandle,
    pub(crate) publish_active_field: Option<PublishField>,
    pub(crate) publish_name_cursor: usize,
    pub(crate) publish_name_selection: Option<usize>,
    pub(crate) publish_description_cursor: usize,
    pub(crate) publish_description_selection: Option<usize>,
    pub(crate) repository_focus: FocusHandle,
    pub(crate) repository_active_field: Option<RepositoryField>,
    pub(crate) repository_create_name_cursor: usize,
    pub(crate) repository_create_name_selection: Option<usize>,
    pub(crate) repository_create_description_cursor: usize,
    pub(crate) repository_create_description_selection: Option<usize>,
    pub(crate) repository_create_path_cursor: usize,
    pub(crate) repository_create_path_selection: Option<usize>,
    pub(crate) repository_clone_url_cursor: usize,
    pub(crate) repository_clone_url_selection: Option<usize>,
    pub(crate) repository_clone_path_cursor: usize,
    pub(crate) repository_clone_path_selection: Option<usize>,
    pub(crate) settings_modal: SettingsModalState,
    // Zoom
    rem_size: f32,
    render_count: u32,
    was_window_active: bool,
    pending_summary_focus: bool,
    _automation: Option<automation::AutomationHandle>,
}

impl GitSparkApp {
    pub fn new(settings: AppSettings, cx: &mut Context<Self>) -> Self {
        let (tx, event_rx) = mpsc::channel();
        let event_pending = Arc::new(AtomicBool::new(false));
        let event_tx = NotifySender {
            tx,
            pending: Arc::clone(&event_pending),
        };
        let automation = automation::maybe_start(event_tx.clone());

        let error_message = String::new();

        let mut app = Self {
            git: GitClient::new(),
            settings: settings.clone(),
            repo: RepoState::default(),
            commit: CommitState::default(),
            network: NetworkState::default(),
            selection: SelectionState::default(),
            nav: NavState::default(),
            filters: FilterState::default(),
            messages: MessageState::new("Open a repository to get started.", error_message),
            repo_watch_generation: Arc::new(AtomicU64::new(0)),
            watched_repo_path: None,
            event_tx,
            event_rx,
            summary_focus: cx.focus_handle(),
            description_focus: cx.focus_handle(),
            summary_cursor: 0,
            summary_selection: None,
            description_cursor: 0,
            description_selection: None,
            branch_filter_focus: cx.focus_handle(),
            branch_filter_cursor: 0,
            repo_filter_focus: cx.focus_handle(),
            repo_filter_cursor: 0,
            new_branch_focus: cx.focus_handle(),
            new_branch_cursor: 0,
            new_branch_selection: None,
            publish_focus: cx.focus_handle(),
            publish_active_field: Some(PublishField::Name),
            publish_name_cursor: 0,
            publish_name_selection: None,
            publish_description_cursor: 0,
            publish_description_selection: None,
            repository_focus: cx.focus_handle(),
            repository_active_field: None,
            repository_create_name_cursor: 0,
            repository_create_name_selection: None,
            repository_create_description_cursor: 0,
            repository_create_description_selection: None,
            repository_create_path_cursor: 0,
            repository_create_path_selection: None,
            repository_clone_url_cursor: 0,
            repository_clone_url_selection: None,
            repository_clone_path_cursor: 0,
            repository_clone_path_selection: None,
            settings_modal: SettingsModalState::new(cx),
            rem_size: DEFAULT_REM_SIZE,
            render_count: 0,
            was_window_active: false,
            pending_summary_focus: false,
            _automation: automation,
        };

        // Register zoom actions at the window level so they work regardless of focus
        cx.observe_keystrokes(|app, keystroke, _window, cx| {
            let ks = &keystroke.keystroke;
            let cmd = ks.modifiers.secondary();
            let shift = ks.modifiers.shift;

            if cmd && !shift {
                match ks.key.as_str() {
                    // Zoom
                    "=" | "+" => {
                        app.rem_size = (app.rem_size + ZOOM_STEP).min(ZOOM_MAX);
                        let pct = ((app.rem_size / DEFAULT_REM_SIZE) * 100.0).round() as i32;
                        app.messages.status_message = format!("Zoom: {pct}%");
                        cx.notify();
                    }
                    "-" => {
                        app.rem_size = (app.rem_size - ZOOM_STEP).max(ZOOM_MIN);
                        let pct = ((app.rem_size / DEFAULT_REM_SIZE) * 100.0).round() as i32;
                        app.messages.status_message = format!("Zoom: {pct}%");
                        cx.notify();
                    }
                    "0" => {
                        app.rem_size = DEFAULT_REM_SIZE;
                        app.messages.status_message = "Zoom: 100%".to_string();
                        cx.notify();
                    }
                    // Cmd+1 = Changes tab, Cmd+2 = History tab
                    "1" => {
                        app.nav.sidebar_tab = SidebarTab::Changes;
                        cx.notify();
                    }
                    "2" => {
                        app.nav.sidebar_tab = SidebarTab::History;
                        cx.notify();
                    }
                    // Cmd+, = Settings
                    "," => {
                        if app.nav.show_settings {
                            app.close_settings_modal();
                        } else {
                            app.open_global_settings_modal(None, cx);
                        }
                        cx.notify();
                    }
                    // Cmd+Enter = Commit
                    "enter" => {
                        if app.nav.active_dialog == ActiveDialog::None
                            && !app.nav.show_settings
                            && !app.commit.summary.trim().is_empty()
                        {
                            app.commit_all(cx);
                        }
                    }
                    _ => {}
                }
            } else if cmd && shift {
                match ks.key.as_str() {
                    // Cmd+Shift+N = New Branch
                    "n" => {
                        app.menu_new_branch(cx);
                    }
                    _ => {}
                }
            } else if !cmd && !shift {
                // Arrow keys for tab switching (when not in a text field)
                match ks.key.as_str() {
                    "left" | "right" => {
                        // Only switch tabs if no text field is focused
                        if !app.summary_focus.is_focused(_window)
                            && !app.description_focus.is_focused(_window)
                            && !app.branch_filter_focus.is_focused(_window)
                            && !app.repo_filter_focus.is_focused(_window)
                        {
                            app.nav.sidebar_tab = match app.nav.sidebar_tab {
                                SidebarTab::Changes => SidebarTab::History,
                                SidebarTab::History => SidebarTab::Changes,
                            };
                            cx.notify();
                        }
                    }
                    _ => {}
                }
            }
        })
        .detach();

        if let Some(last_repo) = settings.recent_repos.first() {
            app.open_repo(last_repo.clone());
        }

        // Poll loop: only acquires the app lock when the atomic flag
        // indicates events are pending. Idle polls are lock-free.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(32))
                    .await;
                if !event_pending.load(Ordering::Acquire) {
                    continue;
                }
                let _ = cx.update(|cx| {
                    let _ = this.update(cx, |app, cx| {
                        app.process_events(cx);
                    });
                });
            }
        })
        .detach();

        app
    }

    // ------------------------------------------------------------------
    // Event processing — drain the mpsc channel
    // ------------------------------------------------------------------

    pub(crate) fn process_events(&mut self, cx: &mut Context<Self>) {
        self.event_tx.pending.store(false, Ordering::Release);
        let mut had_events = false;
        while let Ok(event) = self.event_rx.try_recv() {
            had_events = true;
            match event {
                AppEvent::RepoLoaded(Ok(snapshot)) => {
                    self.adopt_snapshot(snapshot);
                    self.messages.status_message = "Repository loaded.".to_string();
                    self.messages.error_message.clear();
                }
                AppEvent::RepoLoaded(Err(err)) => {
                    self.messages.error_message = format!("Failed to open repository: {err}");
                }
                AppEvent::RepoRefreshed(path, Ok(snapshot), reason) => {
                    let should_apply = self
                        .repo_path()
                        .map(PathBuf::from)
                        .map(|current_path| current_path == path)
                        .unwrap_or(false);
                    if !should_apply {
                        continue;
                    }
                    self.adopt_snapshot(snapshot);
                    if reason == RepoRefreshReason::Manual {
                        self.messages.status_message = "Repository refreshed.".to_string();
                    }
                    self.messages.error_message.clear();
                }
                AppEvent::RepoRefreshed(path, Err(err), reason) => {
                    let should_apply = self
                        .repo_path()
                        .map(PathBuf::from)
                        .map(|current_path| current_path == path)
                        .unwrap_or(false);
                    if !should_apply {
                        continue;
                    }
                    if reason == RepoRefreshReason::Manual {
                        self.messages.error_message = format!("Refresh failed: {err}");
                    } else {
                        self.messages.error_message = err;
                    }
                }
                AppEvent::BranchSwitched(Ok(snapshot), branch) => {
                    self.adopt_snapshot(snapshot);
                    self.messages.status_message = format!("Switched to branch '{branch}'.");
                    self.messages.error_message.clear();
                }
                AppEvent::BranchSwitched(Err(err), branch) => {
                    if branch_switch_needs_stash(&err) {
                        self.repo.switch_branch_bring_changes = false;
                        self.nav.active_dialog = ActiveDialog::StashAndSwitch {
                            target_branch: branch.clone(),
                        };
                        self.messages.error_message =
                            "Branch switch needs a clean working tree.".to_string();
                    } else {
                        self.messages.error_message = format!("Branch switch failed: {err}");
                    }
                }
                AppEvent::BranchMerged(Ok(snapshot), branch) => {
                    self.adopt_snapshot(snapshot);
                    self.messages.status_message = format!("Merged '{branch}'.");
                    self.messages.error_message.clear();
                }
                AppEvent::BranchMerged(Err(err), _) => {
                    self.messages.error_message = format!("Merge failed: {err}");
                }
                AppEvent::CommitCreated(Ok(snapshot), summary) => {
                    self.adopt_snapshot(snapshot);
                    self.commit.summary.clear();
                    self.commit.body.clear();
                    self.summary_cursor = 0;
                    self.description_cursor = 0;
                    self.commit.ai_preview = None;
                    self.messages.status_message = "Commit created.".to_string();
                    self.messages.error_message.clear();
                    // Undo commit banner
                    self.nav.undo_commit = Some((summary, std::time::Instant::now()));
                }
                AppEvent::CommitCreated(Err(err), _) => {
                    self.messages.error_message = format!("Commit failed: {err}");
                }
                AppEvent::CommitUndone(Ok(snapshot)) => {
                    self.adopt_snapshot(snapshot);
                    self.nav.undo_commit = None;
                    self.messages.status_message = "Undid last commit.".to_string();
                    self.messages.error_message.clear();
                }
                AppEvent::CommitUndone(Err(err)) => {
                    self.messages.error_message = format!("Undo commit failed: {err}");
                }
                AppEvent::NetworkActionCompleted(Ok(snapshot), action_label) => {
                    self.network.active_action = None;
                    self.adopt_snapshot(snapshot);
                    self.messages.status_message = format!("{action_label} complete.");
                    self.messages.error_message.clear();
                }
                AppEvent::NetworkActionCompleted(Err(err), action_label) => {
                    self.network.active_action = None;
                    self.messages.error_message = format!("{action_label} failed: {err}");
                }
                AppEvent::AiCommitGenerated(Ok(suggestion)) => {
                    self.commit.ai_in_flight = false;
                    self.commit.summary = suggestion.subject.clone();
                    self.commit.body = suggestion.body.clone();
                    self.summary_cursor = self.commit.summary.len();
                    self.description_cursor = self.commit.body.len();
                    self.commit.ai_preview = Some(suggestion);
                    self.messages.status_message = "Generated commit suggestion.".to_string();
                    self.messages.error_message.clear();
                }
                AppEvent::AiCommitGenerated(Err(err)) => {
                    self.commit.ai_in_flight = false;
                    self.messages.error_message = format!("AI generation failed: {err}");
                }
                AppEvent::OpenRouterModelsLoaded(Ok(models)) => {
                    if self.settings.ai.provider == AiProvider::OpenRouter
                        && self.settings.ai.model.trim().is_empty()
                    {
                        if let Some(first) = models.first() {
                            self.settings.ai.model = first.id.clone();
                        }
                    }
                    self.filters.openrouter_models = OpenRouterModelsState::Ready(models);
                }
                AppEvent::OpenRouterModelsLoaded(Err(err)) => {
                    self.filters.openrouter_models = OpenRouterModelsState::Error(err);
                }
                AppEvent::CommitDiffLoaded(oid, Ok(diffs)) => {
                    if self.selection.selected_commit.as_deref() == Some(oid.as_str()) {
                        if let Some(first) = diffs.first() {
                            self.selection.selected_commit_file = Some(first.path.clone());
                        }
                        self.selection.commit_diffs = Some(diffs);
                    }
                }
                AppEvent::CommitDiffLoaded(_, Err(err)) => {
                    self.messages.error_message = format!("Failed to load commit details: {err}");
                }
                AppEvent::FileDiffRefreshed(path, Ok(entry)) => {
                    // Update the diff for this file in the current snapshot
                    if let Some(snapshot) = &mut self.repo.snapshot {
                        if let Some(existing) = snapshot.diffs.iter_mut().find(|d| d.path == path) {
                            *existing = entry;
                        }
                    }
                }
                AppEvent::FileDiffRefreshed(_, Err(_)) => {}
                AppEvent::RepoOperationCompleted(Ok(snapshot), _action_label, success_message) => {
                    self.add_recent_repo(snapshot.repo.path.clone());
                    self.adopt_snapshot(snapshot);
                    self.messages.status_message = success_message;
                    self.messages.error_message.clear();
                }
                AppEvent::RepoOperationCompleted(Err(err), action_label, _success_message) => {
                    self.messages.error_message = format!("{action_label} failed: {err}");
                }
                AppEvent::CommitDiffCopied(oid, Ok(diff_text)) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(diff_text));
                    self.messages.status_message =
                        format!("Copied diff for {}.", short_commit_label(&oid));
                    self.messages.error_message.clear();
                }
                AppEvent::CommitDiffCopied(oid, Err(err)) => {
                    self.messages.error_message = format!(
                        "Failed to copy diff for {}: {err}",
                        short_commit_label(&oid)
                    );
                }
                AppEvent::Automation(request) => {
                    let response = self.handle_automation_command(request.command, cx);
                    let _ = request.respond_to.send(response);
                }
            }
        }
        // Only trigger a re-render if we actually processed events.
        if had_events {
            cx.notify();
        }
    }

    // ------------------------------------------------------------------
    // Toolbar action handler
    // ------------------------------------------------------------------

    pub fn handle_toolbar_action(&mut self, action: ToolbarAction, cx: &mut Context<Self>) {
        self.close_history_context_menu();
        match action {
            ToolbarAction::ToggleRepoSelector => {
                self.nav.show_repo_selector = !self.nav.show_repo_selector;
                self.nav.show_branch_selector = false;
                self.nav.branch_selector_mode = BranchSelectorMode::Switch;
                self.repo.pending_cherry_pick_oid = None;
                self.nav.show_network_dropdown = false;
            }
            ToolbarAction::SwitchBranch(name) => {
                self.repo.branch_target = name;
                self.switch_branch(cx);
            }
            ToolbarAction::RunNetworkAction(net_action) => {
                self.run_network_action(net_action, cx);
            }
            ToolbarAction::FetchOrigin => self.fetch_origin(cx),
            ToolbarAction::PullOrigin => self.pull_origin(cx),
            ToolbarAction::PushOrigin => self.push_origin(cx),
        }
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Sidebar action handler
    // ------------------------------------------------------------------

    pub fn handle_sidebar_action(&mut self, action: SidebarAction, cx: &mut Context<Self>) {
        self.close_history_context_menu();
        match action {
            SidebarAction::OpenRepoDialog => self.open_repo_dialog(cx),
            SidebarAction::OpenRepo(path) => self.open_repo_with_notify(path, cx),
            SidebarAction::HideRepoSelector => self.nav.show_repo_selector = false,
            SidebarAction::SelectChange(path) => {
                self.selection.selected_change = Some(path);
            }
            SidebarAction::DiscardChange(path) => self.discard_change(&path),
            SidebarAction::IgnorePath(path) => self.ignore_path(&path),
            SidebarAction::IgnoreExtension(ext) => self.ignore_extension(&ext),
            SidebarAction::CopyFullPath(path) => {
                if let Some(repo_path) = self.repo_path() {
                    let full_path = repo_path.join(&path);
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        full_path.to_string_lossy().to_string(),
                    ));
                    self.messages.status_message = format!("Copied absolute path for '{path}'.");
                    self.messages.error_message.clear();
                }
            }
            SidebarAction::CopyRelativePath(path) => {
                cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
                self.messages.status_message = format!("Copied relative path for '{path}'.");
                self.messages.error_message.clear();
            }
            SidebarAction::RevealInFinder(path) => self.reveal_in_finder(&path),
            SidebarAction::OpenInEditor(path) => self.open_in_external_editor(&path),
            SidebarAction::OpenWithDefault(path) => self.open_with_default_program(&path),
            SidebarAction::SelectCommit(oid) => self.select_commit(oid, cx),
            SidebarAction::GenerateAiCommit => self.generate_ai_commit(cx),
            SidebarAction::ShowSettings => self.open_settings_modal(None, cx),
            SidebarAction::CommitAll => self.commit_all(cx),
        }
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Settings action handler
    // ------------------------------------------------------------------

    pub fn handle_settings_action(&mut self, action: SettingsAction, cx: &mut Context<Self>) {
        match action {
            SettingsAction::SaveRemote => self.save_remote_settings(cx),
            SettingsAction::SaveIgnoredFiles => self.save_ignored_files_settings(cx),
            SettingsAction::SaveGitConfig => self.save_git_config(),
            SettingsAction::SaveAiSettings => {
                if self.settings.ai.provider == AiProvider::OpenRouter {
                    self.settings.ai.endpoint =
                        self.settings.ai.provider.default_endpoint().to_string();
                } else if self.settings.ai.endpoint.trim().is_empty() {
                    self.settings.ai.endpoint =
                        self.settings.ai.provider.default_endpoint().to_string();
                    self.settings_modal.ai_endpoint_cursor = self.settings.ai.endpoint.len();
                } else {
                    self.settings.ai.endpoint = self.settings.ai.endpoint.trim().to_string();
                    self.settings_modal.ai_endpoint_cursor = self.settings.ai.endpoint.len();
                }
                self.persist_settings();
                if self.messages.error_message.is_empty() {
                    self.messages.status_message = "AI settings saved.".to_string();
                }
            }
            SettingsAction::SetGitConfigScope(use_local) => {
                self.repo.use_local_identity = use_local;
                if use_local
                    && self.repo.local_identity.user_name.trim().is_empty()
                    && self.repo.local_identity.user_email.trim().is_empty()
                {
                    self.repo.local_identity.user_name = self.repo.identity.user_name.clone();
                    self.repo.local_identity.user_email = self.repo.identity.user_email.clone();
                }
                let (name_len, email_len) = {
                    let identity = self.active_git_settings_identity();
                    (identity.user_name.len(), identity.user_email.len())
                };
                self.settings_modal.git_user_name_cursor = name_len;
                self.settings_modal.git_user_email_cursor = email_len;
                self.settings_modal.git_user_name_selection = None;
                self.settings_modal.git_user_email_selection = None;
            }
            SettingsAction::ChangeProvider(provider) => {
                self.settings.ai.provider = provider;
                if self.settings.ai.provider == AiProvider::OpenRouter
                    || self.settings.ai.endpoint.trim().is_empty()
                {
                    self.settings.ai.endpoint =
                        self.settings.ai.provider.default_endpoint().to_string();
                    self.settings_modal.ai_endpoint_cursor = self.settings.ai.endpoint.len();
                    self.settings_modal.ai_endpoint_selection = None;
                }
                self.filters.openrouter_model_filter.clear();
                self.settings_modal.openrouter_model_filter_cursor = 0;
                if self.settings.ai.provider == AiProvider::OpenRouter {
                    self.settings_modal.active_field = Some(SettingsField::OpenRouterModelFilter);
                    self.ensure_openrouter_models(cx);
                } else {
                    self.settings_modal.active_field = Some(SettingsField::AiModel);
                    self.settings_modal.ai_model_cursor = self.settings.ai.model.len();
                }
            }
            SettingsAction::SelectOpenRouterModel(model_id) => {
                self.settings.ai.model = model_id;
                self.settings_modal.ai_model_cursor = self.settings.ai.model.len();
            }
            SettingsAction::RetryOpenRouterModels => {
                self.filters.openrouter_models = OpenRouterModelsState::Idle;
                self.ensure_openrouter_models(cx);
            }
            SettingsAction::Close => {
                self.close_settings_modal();
            }
        }
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Repository operations
    // ------------------------------------------------------------------

    fn open_repo_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = FileDialog::new().pick_folder() {
            self.open_repo_with_notify(path, cx);
        }
    }

    fn open_repo_with_notify(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.open_repo(path);
        cx.notify();
    }

    fn open_repo(&mut self, path: PathBuf) {
        self.messages.status_message = "Loading repository...".to_string();
        self.messages.error_message.clear();
        self.nav.show_repo_selector = false;
        self.stop_repo_watch();
        self.add_recent_repo(path.clone());
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.open_repo(path).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::RepoLoaded(res));
        });
    }

    fn default_repository_parent_path(&self) -> String {
        self.repo_path()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .or_else(dirs::document_dir)
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."))
            .to_string_lossy()
            .to_string()
    }

    fn open_create_repository_dialog(&mut self, cx: &mut Context<Self>) {
        if self.repo.create_repo_path.trim().is_empty() {
            self.repo.create_repo_path = self.default_repository_parent_path();
        }
        self.repository_active_field = Some(RepositoryField::CreateName);
        self.repository_create_name_cursor = self.repo.create_repo_name.len();
        self.repository_create_name_selection = None;
        self.nav.active_dialog = ActiveDialog::CreateRepository;
        self.nav.show_repo_selector = false;
        self.nav.show_branch_selector = false;
        self.nav.show_network_dropdown = false;
        self.messages.error_message.clear();
        cx.notify();
    }

    fn open_clone_repository_dialog(&mut self, cx: &mut Context<Self>) {
        self.repository_active_field = Some(RepositoryField::CloneUrl);
        self.repository_clone_url_cursor = self.repo.clone_repo_url.len();
        self.repository_clone_url_selection = None;
        self.nav.active_dialog = ActiveDialog::CloneRepository;
        self.nav.show_repo_selector = false;
        self.nav.show_branch_selector = false;
        self.nav.show_network_dropdown = false;
        self.messages.error_message.clear();
        cx.notify();
    }

    pub(crate) fn create_repository_validation_message(&self) -> Option<String> {
        if self.repo.create_repo_name.trim().is_empty() {
            return Some("Type a repository name.".to_string());
        }
        if safe_repository_directory_name(&self.repo.create_repo_name).is_empty() {
            return Some(format!(
                "{} is not a valid repository name.",
                self.repo.create_repo_name.trim()
            ));
        }
        if self.repo.create_repo_path.trim().is_empty() {
            return Some("Choose a local path.".to_string());
        }
        let destination = PathBuf::from(self.repo.create_repo_path.trim())
            .join(safe_repository_directory_name(&self.repo.create_repo_name));
        if destination.exists() {
            if !destination.is_dir() {
                return Some(format!("{} is not a directory.", destination.display()));
            }
            if std::fs::read_dir(&destination)
                .map(|mut entries| entries.next().is_some())
                .unwrap_or(true)
            {
                return Some(format!(
                    "{} already exists and is not empty.",
                    destination.display()
                ));
            }
        }
        None
    }

    pub(crate) fn clone_repository_validation_message(&self) -> Option<String> {
        if self.repo.clone_repo_url.trim().is_empty() {
            return Some("Type a repository URL.".to_string());
        }
        if self.repo.clone_repo_path.trim().is_empty() {
            return Some("Choose a local path.".to_string());
        }
        let destination = PathBuf::from(self.repo.clone_repo_path.trim());
        if destination.exists() {
            if !destination.is_dir() {
                return Some(format!("{} is not a directory.", destination.display()));
            }
            if std::fs::read_dir(&destination)
                .map(|mut entries| entries.next().is_some())
                .unwrap_or(true)
            {
                return Some(format!(
                    "{} already exists and is not empty.",
                    destination.display()
                ));
            }
        }
        None
    }

    pub(crate) fn create_repository(&mut self, cx: &mut Context<Self>) {
        if let Some(message) = self.create_repository_validation_message() {
            self.messages.error_message = message;
            cx.notify();
            return;
        }
        let parent_path = PathBuf::from(self.repo.create_repo_path.trim());
        let name = self.repo.create_repo_name.trim().to_string();
        let description = self.repo.create_repo_description.trim().to_string();

        self.nav.active_dialog = ActiveDialog::None;
        self.messages.status_message = format!("Creating repository '{name}'...");
        self.messages.error_message.clear();
        self.stop_repo_watch();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git
                .create_repository(&parent_path, &name, &description)
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::RepoOperationCompleted(
                res,
                "Create repository".to_string(),
                format!("Created repository '{name}'."),
            ));
        });
        cx.notify();
    }

    pub(crate) fn clone_repository(&mut self, cx: &mut Context<Self>) {
        if let Some(message) = self.clone_repository_validation_message() {
            self.messages.error_message = message;
            cx.notify();
            return;
        }
        let url = self.repo.clone_repo_url.trim().to_string();
        let destination = PathBuf::from(self.repo.clone_repo_path.trim());

        self.nav.active_dialog = ActiveDialog::None;
        self.messages.status_message = format!("Cloning '{url}'...");
        self.messages.error_message.clear();
        self.stop_repo_watch();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git
                .clone_repository(&url, &destination)
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::RepoOperationCompleted(
                res,
                "Clone repository".to_string(),
                format!("Cloned repository from '{url}'."),
            ));
        });
        cx.notify();
    }

    pub fn refresh_repo(&mut self, cx: &mut Context<Self>) {
        self.request_repo_refresh(RepoRefreshReason::Manual, cx);
    }

    fn request_repo_refresh(&mut self, reason: RepoRefreshReason, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        if reason == RepoRefreshReason::Manual {
            self.messages.status_message = "Refreshing repository...".to_string();
        }
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        let event_path = path.clone();
        thread::spawn(move || {
            let res = git.refresh_repo(&path).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::RepoRefreshed(event_path, res, reason));
        });
        cx.notify();
    }

    fn stop_repo_watch(&mut self) {
        self.repo_watch_generation.fetch_add(1, Ordering::SeqCst);
        self.watched_repo_path = None;
    }

    fn ensure_repo_watch(&mut self, repo_path: &Path) {
        if self.watched_repo_path.as_deref() == Some(repo_path) {
            return;
        }

        let path = repo_path.to_path_buf();
        let token = self.repo_watch_generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.watched_repo_path = Some(path.clone());

        let generation = Arc::clone(&self.repo_watch_generation);
        let tx = self.event_tx.clone();

        thread::spawn(move || {
            let git = GitClient::new();
            let mut last_fingerprint = git.read_watch_fingerprint(&path).ok();

            while generation.load(Ordering::SeqCst) == token {
                thread::sleep(Duration::from_millis(3000));

                if generation.load(Ordering::SeqCst) != token {
                    break;
                }

                let Ok(current_fingerprint) = git.read_watch_fingerprint(&path) else {
                    continue;
                };

                let changed = match &last_fingerprint {
                    Some(previous) => previous != &current_fingerprint,
                    None => true,
                };

                if !changed {
                    continue;
                }

                last_fingerprint = Some(current_fingerprint);
                let res = git.refresh_repo(&path).map_err(|e| e.to_string());
                let _ = tx.send(AppEvent::RepoRefreshed(
                    path.clone(),
                    res,
                    RepoRefreshReason::Watch,
                ));
            }
        });
    }

    // ------------------------------------------------------------------
    // Network operations
    // ------------------------------------------------------------------

    fn fetch_origin(&mut self, cx: &mut Context<Self>) {
        self.run_network_action(NetworkAction::Fetch, cx);
    }

    fn pull_origin(&mut self, cx: &mut Context<Self>) {
        self.run_network_action(NetworkAction::Pull, cx);
    }

    fn push_origin(&mut self, cx: &mut Context<Self>) {
        if self
            .repo
            .snapshot
            .as_ref()
            .map(NetworkAction::from_snapshot)
            == Some(NetworkAction::PublishRepository)
        {
            self.run_network_action(NetworkAction::PublishRepository, cx);
            return;
        }

        self.run_network_action(NetworkAction::Push, cx);
    }

    fn run_network_action(&mut self, action: NetworkAction, cx: &mut Context<Self>) {
        if self.network.active_action.is_some() {
            return;
        }

        if action == NetworkAction::PublishRepository {
            self.prepare_publish_dialog();
            self.nav.active_dialog = ActiveDialog::PublishRepository;
            cx.notify();
            return;
        }

        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let remote_name = self
            .repo
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.repo.remote_name.clone())
            .unwrap_or_else(|| "origin".to_string());
        let action_label = action.title(&remote_name);

        self.messages.status_message = format!("{action_label}...");
        self.messages.error_message.clear();
        self.network.active_action = Some(action);

        let tx = self.event_tx.clone();
        let git = GitClient::new();
        let action_label_for_event = action_label.clone();

        thread::spawn(move || {
            let res = match action {
                NetworkAction::Fetch => git.fetch_origin(&path),
                NetworkAction::Pull => git.pull_origin(&path),
                NetworkAction::Push | NetworkAction::PublishBranch => git.push_origin(&path),
                NetworkAction::PublishRepository => unreachable!("handled before background run"),
            }
            .map_err(|e| e.to_string());

            let _ = tx.send(AppEvent::NetworkActionCompleted(
                res,
                action_label_for_event,
            ));
        });
        cx.notify();
    }

    fn open_publish_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.prepare_publish_dialog();
        self.nav.active_dialog = ActiveDialog::PublishRepository;
        self.nav.show_network_dropdown = false;
        self.publish_active_field = Some(PublishField::Name);
        self.publish_name_cursor = self.network.publish_name.len();
        self.publish_name_selection = None;
        window.focus(&self.publish_focus);
        cx.notify();
    }

    fn prepare_publish_dialog(&mut self) {
        if self.network.publish_name.trim().is_empty() {
            if let Some(snapshot) = &self.repo.snapshot {
                self.network.publish_name = snapshot.repo.name.clone();
            }
        }
        self.publish_name_cursor = self
            .publish_name_cursor
            .min(self.network.publish_name.len());
        self.publish_description_cursor = self
            .publish_description_cursor
            .min(self.network.publish_description.len());
    }

    pub(crate) fn publish_repository(&mut self, cx: &mut Context<Self>) {
        if self.network.active_action.is_some() {
            return;
        }

        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let name = self.network.publish_name.trim().to_string();
        if name.is_empty() {
            self.messages.error_message = "Repository name is required.".to_string();
            return;
        }

        let description = self.network.publish_description.trim().to_string();
        let private = self.network.publish_private;

        self.nav.active_dialog = ActiveDialog::None;
        self.messages.status_message = "Publishing repository...".to_string();
        self.messages.error_message.clear();
        self.network.active_action = Some(NetworkAction::PublishRepository);

        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git
                .publish_repository(&path, &name, &description, private)
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::NetworkActionCompleted(
                res,
                "Publish repository".to_string(),
            ));
        });
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Branch operations
    // ------------------------------------------------------------------

    fn switch_branch(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let target = self.repo.branch_target.trim().to_string();
        if target.is_empty() {
            self.messages.error_message = "Choose a branch first.".to_string();
            return;
        }

        if self.repo.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.repo.current_branch != target && !snapshot.changes.is_empty()
        }) {
            self.repo.switch_branch_bring_changes = false;
            self.nav.active_dialog = ActiveDialog::StashAndSwitch {
                target_branch: target,
            };
            self.messages.error_message = "Branch switch needs a clean working tree.".to_string();
            cx.notify();
            return;
        }

        self.messages.status_message = format!("Switching to '{}'...", target);
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.switch_branch(&path, &target).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::BranchSwitched(res, target));
        });
        cx.notify();
    }

    pub(crate) fn stash_and_switch_branch(&mut self, target: String, cx: &mut Context<Self>) {
        self.nav.active_dialog = ActiveDialog::None;
        self.repo.switch_branch_bring_changes = false;

        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        let target = target.trim().to_string();
        if target.is_empty() {
            self.messages.error_message = "Choose a branch first.".to_string();
            cx.notify();
            return;
        }

        self.messages.status_message = format!("Stashing changes and switching to '{target}'...");
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git
                .stash_all(&path)
                .and_then(|_| git.switch_branch(&path, &target))
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::BranchSwitched(res, target));
        });
        cx.notify();
    }

    pub(crate) fn switch_branch_with_changes(&mut self, target: String, cx: &mut Context<Self>) {
        self.nav.active_dialog = ActiveDialog::None;
        self.repo.switch_branch_bring_changes = false;

        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        let target = target.trim().to_string();
        if target.is_empty() {
            self.messages.error_message = "Choose a branch first.".to_string();
            cx.notify();
            return;
        }

        self.messages.status_message = format!("Switching to '{target}' with changes...");
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.switch_branch(&path, &target).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::BranchSwitched(res, target));
        });
        cx.notify();
    }

    pub(crate) fn show_stash_changes_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = self.repo.snapshot.as_ref() else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        if snapshot.changes.is_empty() {
            self.messages.error_message = "There are no local changes to stash.".to_string();
            cx.notify();
            return;
        }

        self.nav.active_dialog = ActiveDialog::StashChanges;
        self.messages.error_message.clear();
        cx.notify();
    }

    pub(crate) fn stash_changes(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };
        if self
            .repo
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.changes.is_empty())
            .unwrap_or(true)
        {
            self.messages.error_message = "There are no local changes to stash.".to_string();
            cx.notify();
            return;
        }

        self.nav.active_dialog = ActiveDialog::None;
        self.messages.status_message = "Stashing changes...".to_string();
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.stash_all(&path).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::NetworkActionCompleted(
                res,
                "Stashed changes".to_string(),
            ));
        });
        cx.notify();
    }

    pub(crate) fn restore_stash(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };
        if self.repo.stash_files.is_empty() {
            self.messages.error_message =
                "Load the stashed file list before restoring the stash.".to_string();
            cx.notify();
            return;
        }

        self.messages.status_message = "Restoring stash...".to_string();
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.stash_pop(&path).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::NetworkActionCompleted(
                res,
                "Restored stash".to_string(),
            ));
        });
        cx.notify();
    }

    pub(crate) fn show_discard_stash_dialog(&mut self, cx: &mut Context<Self>) {
        if self.repo.stash_files.is_empty() {
            if let Some(path) = self.repo_path().map(PathBuf::from) {
                match self.git.latest_stash_files(&path) {
                    Ok(files) => {
                        self.repo.stash_files = files;
                        self.messages.error_message.clear();
                    }
                    Err(err) => {
                        self.repo.stash_files.clear();
                        self.messages.error_message = format!("Could not read stash files: {err}");
                    }
                }
            }
        }
        self.nav.active_dialog = ActiveDialog::DiscardStash;
        cx.notify();
    }

    pub(crate) fn discard_stash(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };
        if self.repo.stash_files.is_empty() {
            self.messages.error_message =
                "Load the stashed file list before discarding the stash.".to_string();
            cx.notify();
            return;
        }

        self.nav.active_dialog = ActiveDialog::None;
        self.messages.status_message = "Discarding stash...".to_string();
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.stash_drop(&path).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::NetworkActionCompleted(
                res,
                "Discarded stash".to_string(),
            ));
        });
        cx.notify();
    }

    pub(crate) fn show_restore_stash_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self.repo_path().map(PathBuf::from) {
            match self.git.latest_stash_files(&path) {
                Ok(files) => {
                    self.repo.stash_files = files;
                    self.messages.error_message.clear();
                }
                Err(err) => {
                    self.repo.stash_files.clear();
                    self.messages.error_message = format!("Could not read stash files: {err}");
                }
            }
        } else {
            self.repo.stash_files.clear();
        }
        self.nav.active_dialog = ActiveDialog::RestoreStash;
        cx.notify();
    }

    pub(crate) fn create_branch(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        // Use filter text as branch name if new_branch_name is empty
        let proposed_name = if self.repo.new_branch_name.trim().is_empty() {
            self.filters.branch_filter_text.trim()
        } else {
            self.repo.new_branch_name.trim()
        };
        let name = sanitized_ref_name(proposed_name);
        if name.is_empty() {
            self.messages.error_message = if proposed_name.is_empty() {
                "Type a branch name in the filter field, then click New Branch.".to_string()
            } else {
                format!("{proposed_name} is not a valid name.")
            };
            cx.notify();
            return;
        }
        if self.branch_name_exists(&name) {
            self.messages.error_message = format!("A branch named {name} already exists.");
            cx.notify();
            return;
        }

        let start_point = self.repo.new_branch_start_point.clone();
        self.messages.status_message = format!("Creating branch '{name}'...");
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = match start_point {
                Some(oid) => git.create_branch_from_commit(&path, &name, &oid),
                None => git.create_branch(&path, &name),
            }
            .map_err(|e| e.to_string());
            tx.send(AppEvent::BranchSwitched(res, name));
        });
        self.repo.new_branch_name.clear();
        self.repo.new_branch_start_point = None;
        self.new_branch_cursor = 0;
        self.new_branch_selection = None;
        self.filters.branch_filter_text.clear();
        self.branch_filter_cursor = 0;
        self.nav.show_branch_selector = false;
        self.nav.branch_selector_mode = BranchSelectorMode::Switch;
        self.nav.active_dialog = ActiveDialog::None;
        cx.notify();
    }

    pub(crate) fn rename_branch(&mut self, old_name: String, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let proposed_name = self.repo.new_branch_name.trim();
        let new_name = sanitized_ref_name(proposed_name);
        if new_name.is_empty() {
            self.messages.error_message = if proposed_name.is_empty() {
                "Type a new branch name.".to_string()
            } else {
                format!("{proposed_name} is not a valid name.")
            };
            cx.notify();
            return;
        }
        if new_name == old_name {
            self.nav.active_dialog = ActiveDialog::None;
            cx.notify();
            return;
        }
        if self.branch_name_exists(&new_name) {
            self.messages.error_message = format!("A branch named {new_name} already exists.");
            cx.notify();
            return;
        }

        self.messages.status_message = format!("Renaming branch '{old_name}' to '{new_name}'...");
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        let old = old_name.clone();
        let new_name_for_event = new_name.clone();
        thread::spawn(move || {
            let res = git
                .rename_branch(&path, &old, &new_name)
                .map_err(|e| e.to_string());
            tx.send(AppEvent::NetworkActionCompleted(
                res,
                format!("Renamed branch to '{new_name_for_event}'"),
            ));
        });
        self.repo.new_branch_name.clear();
        self.new_branch_cursor = 0;
        self.new_branch_selection = None;
        self.nav.active_dialog = ActiveDialog::None;
        cx.notify();
    }

    pub(crate) fn create_tag(&mut self, target_oid: String, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let tag_name = self.repo.new_branch_name.trim().to_string();
        if let Some(message) = self.create_tag_validation_message() {
            self.messages.error_message = message;
            cx.notify();
            return;
        }

        self.messages.status_message = format!(
            "Creating tag '{tag_name}' on {}...",
            short_commit_label(&target_oid)
        );
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        let tag_name_for_event = tag_name.clone();
        thread::spawn(move || {
            let res = git
                .create_tag(&path, &target_oid, &tag_name)
                .map_err(|e| e.to_string());
            tx.send(AppEvent::NetworkActionCompleted(
                res,
                format!("Created tag '{tag_name_for_event}'"),
            ));
        });
        self.repo.new_branch_name.clear();
        self.new_branch_cursor = 0;
        self.new_branch_selection = None;
        self.nav.active_dialog = ActiveDialog::None;
        cx.notify();
    }

    pub(crate) fn delete_tag(&mut self, tag_name: String, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let tag_name = tag_name.trim().to_string();
        if tag_name.is_empty() {
            self.messages.error_message = "Tag name cannot be empty.".to_string();
            cx.notify();
            return;
        }

        self.messages.status_message = format!("Deleting tag '{tag_name}'...");
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let tag_name_for_event = tag_name.clone();
        thread::spawn(move || {
            let git = GitClient::new();
            let res = git.delete_tag(&path, &tag_name).map_err(|e| e.to_string());
            tx.send(AppEvent::NetworkActionCompleted(
                res,
                format!("Deleted tag '{tag_name_for_event}'"),
            ));
        });
        self.nav.active_dialog = ActiveDialog::None;
        cx.notify();
    }

    pub fn merge_branch(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let target = self.repo.merge_target.trim().to_string();
        if target.is_empty() {
            self.messages.error_message = "Choose a branch to merge.".to_string();
            return;
        }

        self.messages.status_message = format!("Merging '{}'...", target);
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.merge_branch(&path, &target).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::BranchMerged(res, target));
        });
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Commit operations
    // ------------------------------------------------------------------

    fn commit_all(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let summary = self.commit.summary.trim();
        if summary.is_empty() {
            self.messages.error_message = "Commit summary cannot be empty.".to_string();
            return;
        }
        let summary = summary.to_string();

        if let Some(message) = self.missing_identity_message() {
            self.messages.error_message = message.to_string();
            return;
        }

        let message = if self.commit.body.trim().is_empty() {
            summary.clone()
        } else {
            format!("{}\n\n{}", summary, self.commit.body.trim())
        };
        let summary_for_event = summary;
        let included_paths = self.included_commit_paths();

        self.messages.status_message = "Creating commit...".to_string();
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = if let Some(paths) = included_paths {
                git.commit_paths(&path, &paths, &message)
            } else {
                git.commit_all(&path, &message)
            }
            .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::CommitCreated(res, summary_for_event));
        });
        cx.notify();
    }

    pub(crate) fn undo_last_commit(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            return;
        };
        if !self.can_undo_last_commit() {
            self.messages.error_message =
                "Cannot undo the last commit while it has tags.".to_string();
            cx.notify();
            return;
        }
        self.nav.undo_commit = None;
        self.messages.status_message = "Undoing last commit...".to_string();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.undo_last_commit(&path).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::CommitUndone(res));
        });
        cx.notify();
    }

    // ------------------------------------------------------------------
    // AI commit generation
    // ------------------------------------------------------------------

    fn generate_ai_commit(&mut self, cx: &mut Context<Self>) {
        if self.commit.ai_in_flight {
            return;
        }

        let Some(snapshot) = &self.repo.snapshot else {
            self.messages.error_message =
                "Open a repository before generating a commit message.".to_string();
            return;
        };

        let diff = snapshot
            .diffs
            .iter()
            .filter(|entry| !entry.is_binary)
            .map(|entry| format!("FILE: {}\n{}", entry.path, entry.diff))
            .collect::<Vec<_>>()
            .join("\n\n");

        if diff.trim().is_empty() {
            self.messages.error_message =
                "No text diff available for AI commit generation.".to_string();
            return;
        }

        self.messages.status_message = "Generating AI commit suggestion...".to_string();
        self.messages.error_message.clear();
        self.commit.ai_in_flight = true;
        let tx = self.event_tx.clone();
        let ai = AiClient::new();
        let settings = self.settings.ai.clone();
        thread::spawn(move || {
            let res = ai
                .generate_commit_message(&settings, &diff)
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::AiCommitGenerated(res));
        });
        cx.notify();
    }

    fn ensure_openrouter_models(&mut self, cx: &mut Context<Self>) {
        if self.settings.ai.provider != AiProvider::OpenRouter {
            return;
        }

        match self.filters.openrouter_models {
            OpenRouterModelsState::Idle | OpenRouterModelsState::Error(_) => {}
            OpenRouterModelsState::Loading | OpenRouterModelsState::Ready(_) => return,
        }

        self.filters.openrouter_models = OpenRouterModelsState::Loading;
        let tx = self.event_tx.clone();
        let ai = AiClient::new();
        thread::spawn(move || {
            let res = ai.fetch_openrouter_models().map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::OpenRouterModelsLoaded(res));
        });
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Git config
    // ------------------------------------------------------------------

    fn save_git_config(&mut self) {
        if self.settings_has_repository_scope()
            && let Some(path) = self.repo_path().map(PathBuf::from)
        {
            let write_result = if self.repo.use_local_identity {
                if !git_author_name_is_valid(&self.repo.local_identity.user_name) {
                    self.messages.error_message = INVALID_GIT_AUTHOR_NAME_MESSAGE.to_string();
                    self.messages.status_message.clear();
                    return;
                }
                self.git.write_identity(&path, &self.repo.local_identity)
            } else {
                self.git.clear_local_author_identity(&path).and_then(|_| {
                    self.git.write_global_default_branch(
                        self.repo.global_identity.default_branch.as_deref(),
                    )
                })
            };

            match write_result {
                Ok(()) => {
                    if let Err(err) = self
                        .git
                        .write_pull_rebase(&path, self.repo.identity.pull_rebase)
                    {
                        self.messages.error_message = format!(
                            "Saved repository Git identity, but failed to save repository pull behavior: {err}"
                        );
                        return;
                    }

                    self.settings.default_branch =
                        self.active_git_settings_identity().default_branch.clone();
                    self.persist_settings();
                    self.load_identity(&path);
                    self.messages.status_message = "Git config saved.".to_string();
                    self.messages.error_message.clear();
                }
                Err(err) => {
                    self.messages.error_message =
                        format!("Failed to save repository Git config: {err}");
                }
            }
        } else {
            if !git_author_name_is_valid(&self.repo.global_identity.user_name) {
                self.messages.error_message = INVALID_GIT_AUTHOR_NAME_MESSAGE.to_string();
                self.messages.status_message.clear();
                return;
            }
            match self.git.write_global_identity(&self.repo.global_identity) {
                Ok(()) => {
                    if let Some(path) = self.repo_path().map(PathBuf::from) {
                        self.load_identity(&path);
                    } else {
                        self.load_global_identity();
                    }
                    self.settings.default_branch = self.repo.global_identity.default_branch.clone();
                    self.persist_settings();
                    self.messages.status_message = "Git config saved.".to_string();
                    self.messages.error_message.clear();
                }
                Err(err) => {
                    self.messages.error_message =
                        format!("Failed to save global Git config: {err}");
                }
            }
        }
    }

    fn save_remote_settings(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            self.messages.status_message.clear();
            return;
        };
        let Some(remote_name) = self.repo.remote_name.clone() else {
            self.messages.error_message = "This repository does not have a remote.".to_string();
            self.messages.status_message.clear();
            return;
        };
        let remote_url = self.repo.remote_url.trim().to_string();
        if remote_url.is_empty() {
            self.messages.error_message = "Remote URL cannot be empty.".to_string();
            self.messages.status_message.clear();
            return;
        }

        match self.git.set_remote_url(&path, &remote_name, &remote_url) {
            Ok(snapshot) => {
                self.add_recent_repo(snapshot.repo.path.clone());
                self.adopt_snapshot(snapshot);
                self.messages.status_message = "Remote settings saved.".to_string();
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message = format!("Failed to save remote settings: {err}");
                self.messages.status_message.clear();
            }
        }
        cx.notify();
    }

    fn save_ignored_files_settings(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            self.messages.status_message.clear();
            return;
        };

        match self
            .git
            .write_gitignore(&path, &self.repo.ignored_files_text)
        {
            Ok(snapshot) => {
                self.add_recent_repo(snapshot.repo.path.clone());
                self.adopt_snapshot(snapshot);
                self.messages.status_message = "Ignored files saved.".to_string();
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message = format!("Failed to save ignored files: {err}");
                self.messages.status_message.clear();
            }
        }
        cx.notify();
    }

    fn load_identity(&mut self, path: &Path) {
        self.load_global_identity();
        match self.git.read_identity(path) {
            Ok(mut identity) => {
                // Fall back to app settings default branch if git config doesn't have one
                if identity.default_branch.is_none() {
                    identity.default_branch = self.settings.default_branch.clone();
                }
                self.repo.identity = identity;
            }
            Err(err) => {
                self.repo.identity = GitIdentity::default();
                self.messages.error_message = format!("Could not load git config: {err}");
            }
        }

        match self.git.read_local_identity(path) {
            Ok(mut identity) => {
                if identity.default_branch.is_none() {
                    identity.default_branch = self.settings.default_branch.clone();
                }
                self.repo.use_local_identity =
                    !identity.user_name.trim().is_empty() || !identity.user_email.trim().is_empty();
                self.repo.local_identity = identity;
            }
            Err(err) => {
                self.repo.use_local_identity = false;
                self.repo.local_identity = GitIdentity::default();
                self.messages.error_message = format!("Could not load local Git config: {err}");
            }
        }
    }

    fn load_global_identity(&mut self) {
        match self.git.read_global_identity() {
            Ok(mut identity) => {
                if identity.default_branch.is_none() {
                    identity.default_branch = self.settings.default_branch.clone();
                }
                self.repo.global_identity = identity;
            }
            Err(err) => {
                self.repo.global_identity = GitIdentity::default();
                self.messages.error_message = format!("Could not load global Git config: {err}");
            }
        }
    }

    fn load_remote_settings(&mut self, path: &Path) {
        match self.git.primary_remote(path) {
            Ok(Some((name, url))) => {
                self.repo.remote_name = Some(name);
                self.repo.remote_url = url;
                self.settings_modal.remote_url_cursor = self.repo.remote_url.len();
                self.settings_modal.remote_url_selection = None;
            }
            Ok(None) => {
                self.repo.remote_name = None;
                self.repo.remote_url.clear();
                self.settings_modal.remote_url_cursor = 0;
                self.settings_modal.remote_url_selection = None;
            }
            Err(err) => {
                self.repo.remote_name = None;
                self.repo.remote_url.clear();
                self.settings_modal.remote_url_cursor = 0;
                self.settings_modal.remote_url_selection = None;
                self.messages.error_message = format!("Could not load remote settings: {err}");
            }
        }
    }

    fn load_ignored_files_settings(&mut self, path: &Path) {
        match self.git.read_gitignore(path) {
            Ok(text) => {
                self.repo.ignored_files_text = text;
                self.settings_modal.ignored_files_cursor = self.repo.ignored_files_text.len();
                self.settings_modal.ignored_files_selection = None;
            }
            Err(err) => {
                self.repo.ignored_files_text.clear();
                self.settings_modal.ignored_files_cursor = 0;
                self.settings_modal.ignored_files_selection = None;
                self.messages.error_message = format!("Could not load ignored files: {err}");
            }
        }
    }

    // ------------------------------------------------------------------
    // Commit diff / selection
    // ------------------------------------------------------------------

    fn load_commit_diff(&mut self, oid: String) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            return;
        };

        let tx = self.event_tx.clone();
        let git = GitClient::new();

        thread::spawn(move || {
            let res = git.get_commit_diff(&path, &oid).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::CommitDiffLoaded(oid, res));
        });
    }

    /// Expand diff context in-memory for a specific hunk.
    pub fn expand_diff_context(
        &mut self,
        file_path: String,
        hunk_index: usize,
        direction: DiffExpandDirection,
    ) {
        let Some(snapshot) = &mut self.repo.snapshot else {
            return;
        };
        let Some(entry) = snapshot.diffs.iter_mut().find(|d| d.path == file_path) else {
            return;
        };
        let Some(file_lines) = &entry.file_contents else {
            return;
        };

        // Save original diff for collapse (only on first expansion)
        if entry.original_diff.is_none() {
            entry.original_diff = Some(entry.diff.clone());
        }

        let new_diff = crate::ui::workspace::expand_diff_in_memory(
            &entry.diff,
            file_lines,
            hunk_index,
            direction,
        );
        entry.diff = new_diff;
    }

    /// Collapse expanded diff back to original.
    pub fn collapse_diff(&mut self, file_path: String) {
        let Some(snapshot) = &mut self.repo.snapshot else {
            return;
        };
        let Some(entry) = snapshot.diffs.iter_mut().find(|d| d.path == file_path) else {
            return;
        };
        if let Some(original) = entry.original_diff.take() {
            entry.diff = original;
        }
    }

    /// Re-fetch the diff for a single file in the working directory.
    pub fn refresh_file_diff(&mut self, file_path: String) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            return;
        };
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        let fp = file_path.clone();
        thread::spawn(move || {
            let res = git.get_file_diff(&path, &fp).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::FileDiffRefreshed(fp, res));
        });
    }

    pub fn select_commit(&mut self, oid: String, cx: &mut Context<Self>) {
        let already_selected = self.selection.selected_commit.as_deref() == Some(oid.as_str());
        if already_selected && self.selection.commit_diffs.is_some() {
            return;
        }

        self.selection.selected_commit = Some(oid.clone());
        self.selection.selected_commit_file = None;
        self.selection.commit_diffs = None;
        self.load_commit_diff(oid);
        cx.notify();
    }

    pub(crate) fn close_history_context_menu(&mut self) {}

    pub(crate) fn handle_history_context_menu_action_for_oid(
        &mut self,
        oid: String,
        action: HistoryContextMenuAction,
        cx: &mut Context<Self>,
    ) {
        match action {
            HistoryContextMenuAction::CheckoutCommit => {
                let short = short_commit_label(&oid).to_string();
                self.run_commit_repo_action(
                    oid,
                    "Checkout commit".to_string(),
                    format!("Checked out commit {short}."),
                    GitClient::checkout_commit,
                    cx,
                );
            }
            HistoryContextMenuAction::RevertChangesInCommit => {
                let short = short_commit_label(&oid).to_string();
                self.run_commit_repo_action(
                    oid,
                    "Revert commit".to_string(),
                    format!("Reverted commit {short}."),
                    GitClient::revert_commit,
                    cx,
                );
            }
            HistoryContextMenuAction::CherryPickCommit => {
                let short = short_commit_label(&oid).to_string();
                self.repo.pending_cherry_pick_oid = Some(oid);
                self.nav.show_branch_selector = true;
                self.nav.branch_selector_mode = BranchSelectorMode::Switch;
                self.nav.show_repo_selector = false;
                self.nav.show_network_dropdown = false;
                self.filters.branch_filter_text.clear();
                self.branch_filter_cursor = 0;
                self.messages.status_message =
                    format!("Choose a branch to cherry-pick {short} into.");
                self.messages.error_message.clear();
            }
            HistoryContextMenuAction::CopySha => {
                cx.write_to_clipboard(ClipboardItem::new_string(oid.clone()));
                self.messages.status_message = format!("Copied SHA {}.", short_commit_label(&oid));
                self.messages.error_message.clear();
            }
            HistoryContextMenuAction::CopyDiff => self.copy_commit_diff(&oid, cx),
            HistoryContextMenuAction::CopyTag => self.copy_commit_tags(&oid, cx),
            HistoryContextMenuAction::ViewOnGitHub => self.view_commit_on_github(&oid),
            HistoryContextMenuAction::CreateBranchFromCommit => {
                let short = short_commit_label(&oid);
                self.repo.new_branch_name = format!("branch-{short}");
                self.new_branch_cursor = self.repo.new_branch_name.len();
                self.new_branch_selection = None;
                self.repo.new_branch_start_point = Some(oid);
                self.nav.active_dialog = ActiveDialog::CreateBranch;
                self.messages.error_message.clear();
            }
            HistoryContextMenuAction::CreateTag => {
                self.repo.new_branch_name.clear();
                self.new_branch_cursor = 0;
                self.new_branch_selection = None;
                self.nav.active_dialog = ActiveDialog::CreateTag { target_oid: oid };
                self.messages.error_message.clear();
            }
            HistoryContextMenuAction::DeleteTag => {
                let tags = self.commit_tags_for_oid(&oid);
                if let [tag_name] = tags.as_slice() {
                    self.nav.active_dialog = ActiveDialog::DeleteTag {
                        tag_name: tag_name.clone(),
                    };
                    self.messages.error_message.clear();
                } else if tags.is_empty() {
                    self.messages.error_message =
                        format!("Commit {} has no tags.", short_commit_label(&oid));
                } else {
                    self.messages.error_message =
                        "Choose a commit with a single tag to delete it.".to_string();
                }
            }
            HistoryContextMenuAction::ResetToCommit => {
                if self.can_reset_to_commit(&oid) {
                    self.nav.active_dialog = ActiveDialog::ResetToCommit { target_oid: oid };
                    self.messages.error_message.clear();
                }
            }
            HistoryContextMenuAction::ReorderCommit => {}
        }

        cx.notify();
    }

    pub(crate) fn select_branch_from_selector(&mut self, name: String, cx: &mut Context<Self>) {
        if let Some(oid) = self.repo.pending_cherry_pick_oid.take() {
            self.nav.show_branch_selector = false;
            self.nav.branch_selector_mode = BranchSelectorMode::Switch;
            self.cherry_pick_commit_onto_branch(oid, name, cx);
            return;
        }

        if self.nav.branch_selector_mode == BranchSelectorMode::Merge {
            self.nav.show_branch_selector = false;
            self.nav.branch_selector_mode = BranchSelectorMode::Switch;
            self.repo.merge_target = name;
            self.merge_branch(cx);
            return;
        }

        if !self
            .repo
            .snapshot
            .as_ref()
            .map(|s| s.repo.current_branch == name)
            .unwrap_or(false)
        {
            self.repo.branch_target = name;
            self.switch_branch(cx);
        }

        self.nav.show_branch_selector = false;
        self.nav.branch_selector_mode = BranchSelectorMode::Switch;
        cx.notify();
    }

    pub(crate) fn handle_changes_context_action(
        &mut self,
        path: String,
        action: ChangesContextAction,
        cx: &mut Context<Self>,
    ) {
        match action {
            ChangesContextAction::DiscardChanges => {
                self.nav.active_dialog = ActiveDialog::DiscardChanges {
                    paths: vec![path.clone()],
                };
                self.messages.error_message.clear();
            }
            ChangesContextAction::IgnoreFile => {
                self.ignore_path(&path);
            }
            ChangesContextAction::IgnoreFolder(folder) => {
                self.ignore_path(&folder);
            }
            ChangesContextAction::IgnoreExtension => {
                if let Some(ext) = std::path::Path::new(&path)
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                {
                    self.ignore_extension(&ext);
                }
            }
            ChangesContextAction::CopyFilePath => {
                if let Some(repo_path) = self.repo_path() {
                    let full = format!("{}/{}", repo_path.display(), path);
                    cx.write_to_clipboard(ClipboardItem::new_string(full));
                    self.messages.status_message = "Copied file path.".to_string();
                    self.messages.error_message.clear();
                }
            }
            ChangesContextAction::CopyRelativePath => {
                cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
                self.messages.status_message = "Copied relative path.".to_string();
                self.messages.error_message.clear();
            }
            ChangesContextAction::RevealInFinder => {
                self.reveal_in_finder(&path);
            }
            ChangesContextAction::OpenInExternalEditor => {
                self.open_in_external_editor(&path);
            }
            ChangesContextAction::OpenWithDefault => {
                self.open_with_default_program(&path);
            }
            ChangesContextAction::ViewOnGitHub => {
                self.view_file_on_github(&path);
            }
        }
        cx.notify();
    }

    pub(crate) fn handle_branch_context_action(
        &mut self,
        branch_name: String,
        action: BranchContextAction,
        cx: &mut Context<Self>,
    ) {
        match action {
            BranchContextAction::CopyName => {
                cx.write_to_clipboard(ClipboardItem::new_string(branch_name.clone()));
                self.messages.status_message = format!("Copied branch name '{branch_name}'.");
                self.messages.error_message.clear();
            }
            BranchContextAction::Delete => {
                self.nav.active_dialog = ActiveDialog::DeleteBranch { branch_name };
                self.messages.error_message.clear();
            }
            BranchContextAction::ViewOnGitHub => {
                let Some(path) = self.repo_path().map(PathBuf::from) else {
                    self.messages.error_message = "No repository selected.".to_string();
                    return;
                };

                match self.git.github_branch_url(&path, &branch_name) {
                    Ok(Some(url)) => match open_url(&url) {
                        Ok(_) => {
                            self.messages.status_message =
                                format!("Opened branch '{branch_name}' on GitHub.");
                            self.messages.error_message.clear();
                        }
                        Err(err) => {
                            self.messages.error_message =
                                format!("Failed to open branch '{branch_name}' on GitHub: {err}");
                        }
                    },
                    Ok(None) => {
                        self.messages.error_message =
                            "This repository does not have a GitHub remote URL.".to_string();
                    }
                    Err(err) => {
                        self.messages.error_message = format!(
                            "Failed to resolve GitHub URL for branch '{branch_name}': {err}"
                        );
                    }
                }
            }
            BranchContextAction::Rename => {
                self.repo.new_branch_name = branch_name.clone();
                self.new_branch_cursor = self.repo.new_branch_name.len();
                self.new_branch_selection = None;
                self.nav.active_dialog = ActiveDialog::RenameBranch {
                    old_name: branch_name,
                };
            }
        }
        cx.notify();
    }

    pub(crate) fn confirm_delete_branch(&mut self, branch_name: String, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        self.nav.active_dialog = ActiveDialog::None;
        self.messages.status_message = format!("Deleting branch '{branch_name}'...");
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        let name = branch_name.clone();
        thread::spawn(move || {
            let res = git
                .delete_branch_from_current_worktree(&path, &name)
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::NetworkActionCompleted(
                res,
                format!("Deleted branch '{name}'"),
            ));
        });
        cx.notify();
    }

    fn run_commit_repo_action(
        &mut self,
        oid: String,
        action_label: String,
        success_message: String,
        operation: fn(&GitClient, &Path, &str) -> anyhow::Result<RepoSnapshot>,
        _cx: &mut Context<Self>,
    ) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        self.messages.status_message = format!("{action_label} {}...", short_commit_label(&oid));
        self.messages.error_message.clear();

        let tx = self.event_tx.clone();
        let action_label_for_event = action_label.clone();

        thread::spawn(move || {
            let git = GitClient::new();
            let res = operation(&git, &path, &oid).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::RepoOperationCompleted(
                res,
                action_label_for_event,
                success_message,
            ));
        });
    }

    fn cherry_pick_commit_onto_branch(
        &mut self,
        oid: String,
        target_branch: String,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        if self.repo.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.repo.current_branch != target_branch && !snapshot.changes.is_empty()
        }) {
            self.messages.error_message =
                "Cherry-pick target needs a clean working tree before switching branches."
                    .to_string();
            cx.notify();
            return;
        }

        let short = short_commit_label(&oid).to_string();
        let action_label = "Cherry-pick commit".to_string();
        let success_message = format!("Cherry-picked commit {short} into '{target_branch}'.");
        self.messages.status_message = format!("Cherry-picking {short} into '{target_branch}'...");
        self.messages.error_message.clear();

        let tx = self.event_tx.clone();
        thread::spawn(move || {
            let git = GitClient::new();
            let res = git
                .cherry_pick_commit_onto_branch(&path, &oid, &target_branch)
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::RepoOperationCompleted(
                res,
                action_label,
                success_message,
            ));
        });
        cx.notify();
    }

    fn copy_commit_diff(&mut self, oid: &str, cx: &mut Context<Self>) {
        if self.selection.selected_commit.as_deref() == Some(oid)
            && let Some(diffs) = self.selection.commit_diffs.as_ref()
        {
            cx.write_to_clipboard(ClipboardItem::new_string(commit_diff_clipboard_text(diffs)));
            self.messages.status_message = format!("Copied diff for {}.", short_commit_label(oid));
            self.messages.error_message.clear();
            return;
        }

        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let oid = oid.to_string();
        self.messages.status_message = format!("Copying diff for {}...", short_commit_label(&oid));
        self.messages.error_message.clear();

        let tx = self.event_tx.clone();
        thread::spawn(move || {
            let git = GitClient::new();
            let res = git
                .get_commit_diff(&path, &oid)
                .map(|diffs| commit_diff_clipboard_text(&diffs))
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::CommitDiffCopied(oid, res));
        });

        cx.notify();
    }

    fn copy_commit_tags(&mut self, oid: &str, cx: &mut Context<Self>) {
        let tags = self.commit_tags_for_oid(oid);
        if tags.is_empty() {
            self.messages.error_message =
                format!("Commit {} has no tags.", short_commit_label(oid));
            return;
        }

        let copied = tags.join(" ");
        cx.write_to_clipboard(ClipboardItem::new_string(copied));
        self.messages.status_message = if tags.len() == 1 {
            format!("Copied tag {}.", tags[0])
        } else {
            format!("Copied {} tags.", tags.len())
        };
        self.messages.error_message.clear();
    }

    pub(crate) fn reset_to_commit(&mut self, oid: String, cx: &mut Context<Self>) {
        self.nav.active_dialog = ActiveDialog::None;
        self.nav.sidebar_tab = SidebarTab::Changes;
        self.run_commit_repo_action(
            oid.clone(),
            "Reset to commit".to_string(),
            format!("Reset to commit {}.", short_commit_label(&oid)),
            GitClient::reset_to_commit,
            cx,
        );
    }

    fn view_commit_on_github(&mut self, oid: &str) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        match self.git.github_commit_url(&path, oid) {
            Ok(Some(url)) => match open_url(&url) {
                Ok(_) => {
                    self.messages.status_message =
                        format!("Opened commit {} on GitHub.", short_commit_label(oid));
                    self.messages.error_message.clear();
                }
                Err(err) => {
                    self.messages.error_message = format!(
                        "Failed to open commit {} on GitHub: {err}",
                        short_commit_label(oid)
                    );
                }
            },
            Ok(None) => {
                self.messages.error_message =
                    "This repository does not have a GitHub remote URL.".to_string();
            }
            Err(err) => {
                self.messages.error_message = format!(
                    "Failed to resolve GitHub URL for {}: {err}",
                    short_commit_label(oid)
                );
            }
        }
    }

    fn view_file_on_github(&mut self, relative_path: &str) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        match self.git.github_file_url(&path, relative_path) {
            Ok(Some(url)) => match open_url(&url) {
                Ok(_) => {
                    self.messages.status_message = format!("Opened '{}' on GitHub.", relative_path);
                    self.messages.error_message.clear();
                }
                Err(err) => {
                    self.messages.error_message =
                        format!("Failed to open '{}' on GitHub: {err}", relative_path);
                }
            },
            Ok(None) => {
                self.messages.error_message =
                    "This repository does not have a GitHub remote URL.".to_string();
            }
            Err(err) => {
                self.messages.error_message = format!(
                    "Failed to resolve GitHub URL for '{}': {err}",
                    relative_path
                );
            }
        }
    }

    // ------------------------------------------------------------------
    // File operations
    // ------------------------------------------------------------------

    fn discard_change(&mut self, relative_path: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        match self.git.discard_change(&repo_path, relative_path) {
            Ok(snapshot) => {
                self.adopt_snapshot(snapshot);
                self.messages.status_message =
                    format!("Discarded changes for '{}'.", relative_path);
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message =
                    format!("Failed to discard changes for '{}': {err}", relative_path);
            }
        }
    }

    fn ignore_path(&mut self, relative_path: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };
        if Path::new(relative_path)
            .file_name()
            .and_then(|name| name.to_str())
            == Some(".gitignore")
        {
            self.messages.error_message = "Cannot ignore .gitignore.".to_string();
            return;
        }

        let pattern = relative_path.replace('\\', "/");
        match self.git.append_gitignore_pattern(&repo_path, &pattern) {
            Ok(snapshot) => {
                self.adopt_snapshot(snapshot);
                self.messages.status_message = format!("Added '{}' to .gitignore.", relative_path);
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message =
                    format!("Failed to ignore '{}': {err}", relative_path);
            }
        }
    }

    fn ignore_extension(&mut self, ext: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let pattern = format!("*.{ext}");
        match self.git.append_gitignore_pattern(&repo_path, &pattern) {
            Ok(snapshot) => {
                self.adopt_snapshot(snapshot);
                self.messages.status_message = format!("Added '{}' to .gitignore.", pattern);
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message = format!("Failed to ignore '{}': {err}", pattern);
            }
        }
    }

    fn reveal_in_finder(&mut self, relative_path: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };
        let full_path = repo_path.join(relative_path);

        let result = reveal_path(&full_path);

        match result {
            Ok(_) => {
                self.messages.status_message = format!("Revealed '{}' in Finder.", relative_path);
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message =
                    format!("Failed to reveal '{}': {err}", relative_path);
            }
        }
    }

    fn open_in_external_editor(&mut self, relative_path: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let full_path = repo_path.join(relative_path);
        let configured_editor = self
            .git
            .read_config_value(&repo_path, "core.editor")
            .ok()
            .flatten()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                env::var("VISUAL")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
            })
            .or_else(|| {
                env::var("EDITOR")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
            });

        let result = if let Some(editor_cmd) = configured_editor {
            Command::new("sh")
                .arg("-lc")
                .arg(format!(
                    "{} {}",
                    editor_cmd,
                    shell_escape(&full_path.to_string_lossy())
                ))
                .spawn()
                .map(|_| ())
        } else {
            open::that_detached(&full_path)
        };

        match result {
            Ok(_) => {
                self.messages.status_message =
                    format!("Opened '{}' in external editor.", relative_path);
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message = format!(
                    "Failed to open '{}' in external editor: {err}",
                    relative_path
                );
            }
        }
    }

    fn open_with_default_program(&mut self, relative_path: &str) {
        let Some(repo_path) = self.repo_path() else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let full_path = repo_path.join(relative_path);
        match open_with_default_program(&full_path) {
            Ok(_) => {
                self.messages.status_message =
                    format!("Opened '{relative_path}' with the default program.");
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message =
                    format!("Failed to open '{relative_path}' with default program: {err}");
            }
        }
    }

    // ------------------------------------------------------------------
    // Settings persistence
    // ------------------------------------------------------------------

    fn add_recent_repo(&mut self, path: PathBuf) {
        push_recent_repo(&mut self.settings, path);
        self.persist_settings();
    }

    pub fn persist_settings(&mut self) {
        if let Err(err) = save_settings(&self.settings) {
            self.messages.error_message = format!("Failed to save settings: {err}");
        }
    }

    // ------------------------------------------------------------------
    // Snapshot adoption
    // ------------------------------------------------------------------

    fn adopt_snapshot(&mut self, snapshot: RepoSnapshot) {
        let previous_commit = self.selection.selected_commit.clone();
        let current_branch = snapshot.repo.current_branch.clone();
        let has_stash = snapshot.stash_count > 0;
        let changed_paths: Vec<String> = snapshot
            .changes
            .iter()
            .map(|change| change.path.clone())
            .collect();
        self.close_history_context_menu();
        self.selection.selected_change = snapshot.changes.first().map(|change| change.path.clone());
        self.repo.branch_target = current_branch;
        self.repo.merge_target = snapshot
            .branches
            .iter()
            .find(|branch| !branch.is_current && !branch.is_remote)
            .map(|branch| branch.name.clone())
            .unwrap_or_default();
        self.load_identity(&snapshot.repo.path);
        self.load_remote_settings(&snapshot.repo.path);
        self.load_ignored_files_settings(&snapshot.repo.path);
        self.ensure_repo_watch(&snapshot.repo.path);
        self.repo.has_stash = has_stash;
        if has_stash {
            self.repo.stash_files = self
                .git
                .latest_stash_files(&snapshot.repo.path)
                .unwrap_or_default();
        } else {
            self.repo.stash_files.clear();
        }
        self.repo.snapshot = Some(snapshot);
        self.reconcile_commit_inclusions(&changed_paths);

        let next_selected_commit = self.repo.snapshot.as_ref().and_then(|repo| {
            previous_commit
                .filter(|oid| repo.history.iter().any(|commit| commit.oid == *oid))
                .or_else(|| repo.history.first().map(|commit| commit.oid.clone()))
        });

        self.selection.selected_commit = next_selected_commit.clone();
        self.selection.selected_commit_file = None;
        self.selection.commit_diffs = None;

        if let Some(oid) = next_selected_commit {
            self.load_commit_diff(oid);
        }
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    pub fn repo_path(&self) -> Option<&Path> {
        self.repo
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.repo.path.as_path())
    }

    pub(crate) fn repo_has_github_remote(&self) -> bool {
        let Some(path) = self.repo_path() else {
            return false;
        };

        self.git
            .github_repository_url(path)
            .ok()
            .flatten()
            .is_some()
    }

    pub(crate) fn commit_tags_for_oid(&self, oid: &str) -> Vec<String> {
        self.repo
            .snapshot
            .as_ref()
            .and_then(|snapshot| {
                snapshot
                    .history
                    .iter()
                    .find(|commit| commit.oid == oid)
                    .map(|commit| commit.tags.clone())
            })
            .unwrap_or_default()
    }

    pub(crate) fn tag_name_exists(&self, tag_name: &str) -> bool {
        let tag_name = tag_name.trim();
        !tag_name.is_empty()
            && self
                .repo
                .snapshot
                .as_ref()
                .map(|snapshot| {
                    snapshot.tags.iter().any(|tag| tag == tag_name)
                        || snapshot
                            .history
                            .iter()
                            .flat_map(|commit| commit.tags.iter())
                            .any(|tag| tag == tag_name)
                })
                .unwrap_or(false)
    }

    pub(crate) fn branch_name_exists(&self, branch_name: &str) -> bool {
        let branch_name = branch_name.trim();
        !branch_name.is_empty()
            && self
                .repo
                .snapshot
                .as_ref()
                .map(|snapshot| {
                    snapshot
                        .branches
                        .iter()
                        .any(|branch| branch.name == branch_name)
                })
                .unwrap_or(false)
    }

    pub(crate) fn create_branch_validation_message(&self) -> Option<String> {
        let proposed_name = self.repo.new_branch_name.trim();
        if proposed_name.is_empty() {
            return Some("Type a branch name.".to_string());
        }
        let branch_name = sanitized_ref_name(proposed_name);
        if branch_name.is_empty() {
            return Some(format!("{proposed_name} is not a valid name."));
        }
        if self.branch_name_exists(&branch_name) {
            return Some(format!("A branch named {branch_name} already exists."));
        }
        if branch_name != proposed_name {
            return Some(format!(
                "Will be created as {branch_name}. Spaces and invalid characters have been replaced by hyphens."
            ));
        }
        None
    }

    pub(crate) fn can_create_branch_from_dialog(&self) -> bool {
        let proposed_name = self.repo.new_branch_name.trim();
        let branch_name = sanitized_ref_name(proposed_name);
        !proposed_name.is_empty()
            && !branch_name.is_empty()
            && !self.branch_name_exists(&branch_name)
    }

    pub(crate) fn rename_branch_validation_message(&self, old_name: &str) -> Option<String> {
        let proposed_name = self.repo.new_branch_name.trim();
        if proposed_name.is_empty() {
            return Some("Type a new branch name.".to_string());
        }
        let branch_name = sanitized_ref_name(proposed_name);
        if branch_name.is_empty() {
            return Some(format!("{proposed_name} is not a valid name."));
        }
        if branch_name == old_name {
            return Some("Type a different branch name.".to_string());
        }
        if self.branch_name_exists(&branch_name) {
            return Some(format!("A branch named {branch_name} already exists."));
        }
        if branch_name != proposed_name {
            return Some(format!(
                "Will be renamed as {branch_name}. Spaces and invalid characters have been replaced by hyphens."
            ));
        }
        None
    }

    pub(crate) fn can_rename_branch_from_dialog(&self, old_name: &str) -> bool {
        let proposed_name = self.repo.new_branch_name.trim();
        let branch_name = sanitized_ref_name(proposed_name);
        !proposed_name.is_empty()
            && !branch_name.is_empty()
            && branch_name != old_name
            && !self.branch_name_exists(&branch_name)
    }

    pub(crate) fn create_tag_validation_message(&self) -> Option<String> {
        let tag_name = self.repo.new_branch_name.trim();
        if tag_name.is_empty() {
            return Some("Type a tag name.".to_string());
        }
        if let Some(message) = tag_name_length_validation_message(tag_name) {
            return Some(message);
        }
        if self.tag_name_exists(tag_name) {
            return Some(format!("A tag named {tag_name} already exists."));
        }
        None
    }

    pub(crate) fn can_undo_last_commit(&self) -> bool {
        self.nav.undo_commit.is_some()
            && self
                .repo
                .snapshot
                .as_ref()
                .and_then(|snapshot| {
                    snapshot
                        .repo
                        .head_oid
                        .as_ref()
                        .and_then(|head_oid| {
                            snapshot
                                .history
                                .iter()
                                .find(|commit| &commit.oid == head_oid)
                        })
                        .or_else(|| snapshot.history.first())
                })
                .map(|commit| commit.tags.is_empty())
                .unwrap_or(false)
    }

    pub(crate) fn can_reset_to_commit(&self, oid: &str) -> bool {
        let Some(snapshot) = self.repo.snapshot.as_ref() else {
            return false;
        };
        let Some(index) = snapshot.history.iter().position(|commit| commit.oid == oid) else {
            return false;
        };
        if index == 0 {
            return false;
        }
        snapshot.repo.remote_name.is_none() || index <= snapshot.repo.ahead
    }

    fn reconcile_commit_inclusions(&mut self, changed_paths: &[String]) {
        if self.commit.include_all {
            self.commit.included_files.clear();
            return;
        }

        self.commit.included_files.retain(|path| {
            changed_paths
                .iter()
                .any(|changed_path| changed_path == path)
        });

        if !changed_paths.is_empty() && self.commit.included_files.len() == changed_paths.len() {
            self.commit.include_all = true;
            self.commit.included_files.clear();
        }
    }

    pub(crate) fn commit_file_count(&self) -> usize {
        self.repo
            .snapshot
            .as_ref()
            .map(|snapshot| {
                if self.commit.include_all {
                    snapshot.changes.len()
                } else {
                    self.commit.included_files.len()
                }
            })
            .unwrap_or(0)
    }

    fn included_commit_changes(&self) -> Vec<&ChangeEntry> {
        let Some(snapshot) = &self.repo.snapshot else {
            return Vec::new();
        };

        if self.commit.include_all {
            snapshot.changes.iter().collect()
        } else {
            snapshot
                .changes
                .iter()
                .filter(|change| self.commit.included_files.contains(&change.path))
                .collect()
        }
    }

    fn included_commit_paths(&self) -> Option<Vec<String>> {
        if self.commit.include_all {
            None
        } else {
            Some(
                self.included_commit_changes()
                    .into_iter()
                    .map(|change| change.path.clone())
                    .collect(),
            )
        }
    }

    fn default_commit_summary(&self) -> Option<String> {
        if !self.commit.summary.trim().is_empty() {
            return None;
        }

        let changes = self.included_commit_changes();
        if changes.len() != 1 {
            return None;
        }

        Some(default_commit_summary_for_change(changes[0]))
    }

    pub(crate) fn can_commit(&self) -> bool {
        !self.commit.summary.trim().is_empty()
            && self.commit_file_count() > 0
            && self.identity_is_configured()
            && !self.commit.ai_in_flight
    }

    fn identity_is_configured(&self) -> bool {
        self.missing_identity_message().is_none()
    }

    pub(crate) fn missing_identity_message(&self) -> Option<&'static str> {
        let missing_name = self.repo.identity.user_name.trim().is_empty();
        let missing_email = self.repo.identity.user_email.trim().is_empty();

        if !missing_name && !git_author_name_is_valid(&self.repo.identity.user_name) {
            return Some(INVALID_GIT_AUTHOR_NAME_MESSAGE);
        }

        match (missing_name, missing_email) {
            (true, true) => Some("Configure your Git name and email before committing."),
            (true, false) => Some("Configure your Git name before committing."),
            (false, true) => Some("Configure your Git email before committing."),
            (false, false) => None,
        }
    }

    pub(crate) fn identity_settings_focus_field(&self) -> SettingsField {
        let missing_name = self.repo.identity.user_name.trim().is_empty();
        let missing_email = self.repo.identity.user_email.trim().is_empty();

        if missing_name
            || !git_author_name_is_valid(&self.repo.identity.user_name)
            || !missing_email
        {
            SettingsField::GitUserName
        } else {
            SettingsField::GitUserEmail
        }
    }

    pub(crate) fn open_identity_settings_from_warning(&mut self, cx: &mut Context<Self>) {
        let field = self.identity_settings_focus_field();
        if self.repo.use_local_identity {
            self.open_repository_settings_modal(
                Some(crate::ui::ui_state::SettingsSection::Git),
                cx,
            );
        } else {
            self.open_global_settings_modal(Some(crate::ui::ui_state::SettingsSection::Git), cx);
        }
        self.settings_modal.active_field = Some(field);
        self.set_settings_field_cursor(field, self.settings_field_value(field).len());
    }

    #[allow(dead_code)]
    pub fn selected_diff(&self) -> Option<&DiffEntry> {
        let snapshot = self.repo.snapshot.as_ref()?;
        let selected_change = self.selection.selected_change.as_ref()?;
        snapshot
            .diffs
            .iter()
            .find(|diff| &diff.path == selected_change)
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for GitSparkApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Capture window bounds and persist when they change.
        // Skip the first 10 renders to let GPUI settle the window position.
        self.render_count = self.render_count.saturating_add(1);
        if self.render_count > 10 {
            let bounds = window.bounds();
            let new_x = bounds.origin.x / px(1.0);
            let new_y = bounds.origin.y / px(1.0);
            let new_w = bounds.size.width / px(1.0);
            let new_h = bounds.size.height / px(1.0);
            let ws = &self.settings.window_size;
            let changed = !ws.has_position
                || (ws.x - new_x).abs() > 1.0
                || (ws.y - new_y).abs() > 1.0
                || (ws.width - new_w).abs() > 1.0
                || (ws.height - new_h).abs() > 1.0;
            if changed {
                self.settings.window_size.x = new_x;
                self.settings.window_size.y = new_y;
                self.settings.window_size.width = new_w;
                self.settings.window_size.height = new_h;
                self.settings.window_size.has_position = true;
                self.settings.window_size.display_id = window.display(cx).map(|d| d.id().into());
                self.persist_settings();
            }
        }

        // Refresh git changes when window gains focus
        let is_active = window.is_window_active();
        if is_active && !self.was_window_active && self.repo.snapshot.is_some() {
            self.request_repo_refresh(RepoRefreshReason::Focus, cx);
        }
        self.was_window_active = is_active;

        // Clamp cursors to valid positions (e.g. after AI fill or clear)
        self.summary_cursor = self.summary_cursor.min(self.commit.summary.len());
        self.description_cursor = self.description_cursor.min(self.commit.body.len());
        let git_identity = self.active_git_settings_identity();
        let git_user_name_len = git_identity.user_name.len();
        let git_user_email_len = git_identity.user_email.len();
        let git_default_branch_len = git_identity.default_branch.as_deref().unwrap_or("").len();
        self.settings_modal.git_user_name_cursor = self
            .settings_modal
            .git_user_name_cursor
            .min(git_user_name_len);
        self.settings_modal.git_user_email_cursor = self
            .settings_modal
            .git_user_email_cursor
            .min(git_user_email_len);
        self.settings_modal.git_default_branch_cursor = self
            .settings_modal
            .git_default_branch_cursor
            .min(git_default_branch_len);
        self.settings_modal.ai_model_cursor = self
            .settings_modal
            .ai_model_cursor
            .min(self.settings.ai.model.len());
        self.settings_modal.ai_api_key_cursor = self
            .settings_modal
            .ai_api_key_cursor
            .min(self.settings.ai.api_key.len());
        self.settings_modal.ai_system_prompt_cursor = self
            .settings_modal
            .ai_system_prompt_cursor
            .min(self.settings.ai.system_prompt.len());
        self.settings_modal.openrouter_model_filter_cursor = self
            .settings_modal
            .openrouter_model_filter_cursor
            .min(self.filters.openrouter_model_filter.len());

        if self.pending_summary_focus {
            self.pending_summary_focus = false;
            window.focus(&self.summary_focus);
        }

        let summary_focused = self.summary_focus.is_focused(window);
        let description_focused = self.description_focus.is_focused(window);
        let branch_filter_focused = self.branch_filter_focus.is_focused(window);
        let repo_filter_focused = self.repo_filter_focus.is_focused(window);

        // Clamp filter cursors
        self.branch_filter_cursor = self
            .branch_filter_cursor
            .min(self.filters.branch_filter_text.len());
        self.repo_filter_cursor = self
            .repo_filter_cursor
            .min(self.filters.repo_filter_text.len());

        // Build toolbar parts separately — they go into the resizable columns
        let (toolbar_left, toolbar_right) = self.render_toolbar_parts(cx);

        // Left column: repo toolbar section + sidebar (or repo selector)
        let left_column = v_flex().size_full().min_h_0().child(toolbar_left).child(
            if self.nav.show_repo_selector {
                self.render_repo_selector_panel(repo_filter_focused, cx)
                    .into_any_element()
            } else {
                self.render_sidebar(summary_focused, description_focused, cx)
                    .into_any_element()
            },
        );

        // Right column: branch + network toolbar sections + workspace
        // Branch selector overlay lives inside the right column so it aligns naturally
        let show_network_overlay = self.nav.show_network_dropdown
            && self
                .repo
                .snapshot
                .as_ref()
                .map(NetworkAction::from_snapshot)
                .is_some_and(|action| matches!(action, NetworkAction::Pull | NetworkAction::Push));

        let right_column = div()
            .size_full()
            .min_h_0()
            .relative()
            .child(
                v_flex()
                    .size_full()
                    .min_h_0()
                    .child(toolbar_right)
                    .child(self.render_workspace(cx.entity().clone(), cx)),
            )
            .children(if self.nav.show_branch_selector {
                Some(self.render_branch_selector_overlay(branch_filter_focused, cx))
            } else {
                None
            })
            .children(if show_network_overlay {
                Some(self.render_network_dropdown_overlay(cx))
            } else {
                None
            });

        // Apply zoom level
        let zoom_factor = self.rem_size / DEFAULT_REM_SIZE;
        theme::set_zoom(zoom_factor);
        window.set_rem_size(px(self.rem_size));

        // macOS titlebar spacer (traffic lights sit here)
        let titlebar_height = if cfg!(target_os = "macos") { 38.0 } else { 0.0 };

        let titlebar_spacer = {
            let spacer = div()
                .id("window-titlebar-spacer")
                .w_full()
                .h(px(titlebar_height))
                .flex_shrink_0();
            #[cfg(target_os = "macos")]
            let spacer = spacer.on_click(|event: &ClickEvent, window: &mut Window, _| {
                if event.click_count() == 2 {
                    window.titlebar_double_click();
                }
            });
            spacer.bg(theme::panel_bg())
        };

        let mut root = v_flex()
            .size_full()
            .relative()
            .bg(theme::bg())
            .font_family(".SystemUIFont")
            .text_size(theme::z(theme::FONT_SIZE))
            .child(titlebar_spacer) // slightly lighter than bg for titlebar strip
            .child(
                div().w_full().flex_1().min_h_0().child(
                    h_resizable("main-panels")
                        .child(
                            resizable_panel()
                                .size(px(260.0))
                                .size_range(px(200.0)..px(400.0))
                                .child(left_column),
                        )
                        .child(resizable_panel().child(right_column)),
                ),
            )
            .child(self.render_status_bar());

        if self.nav.show_settings {
            root = root.child(settings_modal::render_settings_modal(self, window, cx));
        }

        // Dialogs
        if self.nav.active_dialog != ActiveDialog::None {
            root = root.child(self.render_active_dialog(window, cx));
        }

        if let Some(context_menu) =
            changes_context_menu::render_changes_context_menu_overlay(self, window, cx)
        {
            root = root.child(context_menu);
        }

        root = root
            .on_action(cx.listener(Self::handle_menu_open_repository))
            .on_action(cx.listener(Self::handle_menu_new_repository))
            .on_action(cx.listener(Self::handle_menu_clone_repository))
            .on_action(cx.listener(Self::handle_menu_show_settings))
            .on_action(cx.listener(Self::handle_menu_show_changes))
            .on_action(cx.listener(Self::handle_menu_show_history))
            .on_action(cx.listener(Self::handle_menu_show_repository_list))
            .on_action(cx.listener(Self::handle_menu_show_branches_list))
            .on_action(cx.listener(Self::handle_menu_go_to_summary))
            .on_action(cx.listener(Self::handle_menu_show_stashed_changes))
            .on_action(cx.listener(Self::handle_menu_fetch))
            .on_action(cx.listener(Self::handle_menu_pull))
            .on_action(cx.listener(Self::handle_menu_push))
            .on_action(cx.listener(Self::handle_menu_publish_repository))
            .on_action(cx.listener(Self::handle_menu_open_external_editor))
            .on_action(cx.listener(Self::handle_menu_open_in_terminal))
            .on_action(cx.listener(Self::handle_menu_show_in_finder))
            .on_action(cx.listener(Self::handle_menu_view_on_github))
            .on_action(cx.listener(Self::handle_menu_repository_settings))
            .on_action(cx.listener(Self::handle_menu_new_branch))
            .on_action(cx.listener(Self::handle_menu_rename_branch))
            .on_action(cx.listener(Self::handle_menu_delete_branch))
            .on_action(cx.listener(Self::handle_menu_merge_branch))
            .on_action(cx.listener(Self::handle_menu_view_branch_on_github))
            .on_action(cx.listener(Self::handle_menu_discard_all_changes))
            .on_action(cx.listener(Self::handle_menu_stash_changes))
            .on_action(cx.listener(Self::handle_menu_zoom_in))
            .on_action(cx.listener(Self::handle_menu_zoom_out))
            .on_action(cx.listener(Self::handle_menu_zoom_reset));

        root
    }
}

impl GitSparkApp {
    pub fn menu_open_repository(&mut self, cx: &mut Context<Self>) {
        self.open_repo_dialog(cx);
    }

    pub fn menu_new_repository(&mut self, cx: &mut Context<Self>) {
        self.open_create_repository_dialog(cx);
    }

    pub fn menu_clone_repository(&mut self, cx: &mut Context<Self>) {
        self.open_clone_repository_dialog(cx);
    }

    pub fn menu_show_settings(&mut self, cx: &mut Context<Self>) {
        self.open_global_settings_modal(None, cx);
        cx.notify();
    }

    pub fn menu_show_changes(&mut self, cx: &mut Context<Self>) {
        self.nav.sidebar_tab = SidebarTab::Changes;
        cx.notify();
    }

    pub fn menu_show_history(&mut self, cx: &mut Context<Self>) {
        self.nav.sidebar_tab = SidebarTab::History;
        cx.notify();
    }

    pub fn menu_show_repository_list(&mut self, cx: &mut Context<Self>) {
        self.nav.show_repo_selector = true;
        self.nav.show_branch_selector = false;
        self.nav.branch_selector_mode = BranchSelectorMode::Switch;
        self.nav.show_network_dropdown = false;
        self.close_history_context_menu();
        cx.notify();
    }

    pub fn menu_show_branches_list(&mut self, cx: &mut Context<Self>) {
        if self.repo.snapshot.is_none() {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        }

        self.nav.show_branch_selector = true;
        self.nav.branch_selector_mode = BranchSelectorMode::Switch;
        self.nav.show_repo_selector = false;
        self.nav.show_network_dropdown = false;
        self.repo.pending_cherry_pick_oid = None;
        self.close_history_context_menu();
        cx.notify();
    }

    pub fn menu_go_to_summary(&mut self, cx: &mut Context<Self>) {
        self.nav.sidebar_tab = SidebarTab::Changes;
        self.summary_cursor = self.commit.summary.len();
        self.summary_selection = None;
        self.pending_summary_focus = true;
        cx.notify();
    }

    pub fn menu_show_stashed_changes(&mut self, cx: &mut Context<Self>) {
        if matches!(self.nav.active_dialog, ActiveDialog::RestoreStash) {
            self.nav.active_dialog = ActiveDialog::None;
            cx.notify();
            return;
        }

        let Some(snapshot) = self.repo.snapshot.as_ref() else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        if snapshot.stash_count == 0 {
            self.messages.error_message = "There are no stashed changes.".to_string();
            cx.notify();
            return;
        }

        self.show_restore_stash_dialog(cx);
    }

    pub fn menu_fetch(&mut self, cx: &mut Context<Self>) {
        self.fetch_origin(cx);
    }

    pub fn menu_pull(&mut self, cx: &mut Context<Self>) {
        self.pull_origin(cx);
    }

    pub fn menu_push(&mut self, cx: &mut Context<Self>) {
        self.push_origin(cx);
    }

    pub fn menu_publish_repository(&mut self, cx: &mut Context<Self>) {
        self.run_network_action(NetworkAction::PublishRepository, cx);
    }

    pub fn menu_open_external_editor(&mut self, cx: &mut Context<Self>) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        let configured_editor = self
            .git
            .read_config_value(&repo_path, "core.editor")
            .ok()
            .flatten()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                env::var("VISUAL")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
            })
            .or_else(|| {
                env::var("EDITOR")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
            });

        let result = if let Some(editor_cmd) = configured_editor {
            Command::new("sh")
                .arg("-lc")
                .arg(format!(
                    "{} {}",
                    editor_cmd,
                    shell_escape(&repo_path.to_string_lossy())
                ))
                .spawn()
                .map(|_| ())
        } else {
            open::that_detached(&repo_path)
        };

        match result {
            Ok(_) => {
                self.messages.status_message = "Opened repository in external editor.".to_string();
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message =
                    format!("Failed to open repository in external editor: {err}");
            }
        }
        cx.notify();
    }

    pub fn menu_open_in_terminal(&mut self, cx: &mut Context<Self>) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        #[cfg(target_os = "macos")]
        let result = Command::new("open")
            .arg("-a")
            .arg("Terminal")
            .arg(&repo_path)
            .spawn()
            .map(|_| ());

        #[cfg(not(target_os = "macos"))]
        let result = open::that_detached(&repo_path);

        match result {
            Ok(_) => {
                self.messages.status_message = "Opened repository in Terminal.".to_string();
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message =
                    format!("Failed to open repository in Terminal: {err}");
            }
        }
        cx.notify();
    }

    pub fn menu_show_in_finder(&mut self, cx: &mut Context<Self>) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        match reveal_path(&repo_path) {
            Ok(_) => {
                self.messages.status_message = "Revealed repository in Finder.".to_string();
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message =
                    format!("Failed to reveal repository in Finder: {err}");
            }
        }
        cx.notify();
    }

    pub fn menu_view_on_github(&mut self, cx: &mut Context<Self>) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        match self.git.github_repository_url(&repo_path) {
            Ok(Some(url)) => match open_url(&url) {
                Ok(_) => {
                    self.messages.status_message = "Opened repository page on GitHub.".to_string();
                    self.messages.error_message.clear();
                }
                Err(err) => {
                    self.messages.error_message =
                        format!("Failed to open repository on GitHub: {err}");
                }
            },
            Ok(None) => {
                self.messages.error_message =
                    "This repository does not have a GitHub remote URL.".to_string();
            }
            Err(err) => {
                self.messages.error_message =
                    format!("Failed to resolve repository GitHub URL: {err}");
            }
        }
        cx.notify();
    }

    pub fn menu_repository_settings(&mut self, cx: &mut Context<Self>) {
        if self.repo.snapshot.is_none() {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        }

        self.open_repository_settings_modal(Some(crate::ui::ui_state::SettingsSection::Remote), cx);
        cx.notify();
    }

    pub fn menu_new_branch(&mut self, cx: &mut Context<Self>) {
        if self.repo.snapshot.is_none() {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        }

        self.repo.new_branch_name = self.filters.branch_filter_text.clone();
        self.new_branch_cursor = self.repo.new_branch_name.len();
        self.new_branch_selection = None;
        self.repo.new_branch_start_point = None;
        self.nav.active_dialog = ActiveDialog::CreateBranch;
        cx.notify();
    }

    pub fn menu_rename_current_branch(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = self.repo.snapshot.as_ref() else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        let branch_name = snapshot.repo.current_branch.clone();
        self.repo.new_branch_name = branch_name.clone();
        self.new_branch_cursor = self.repo.new_branch_name.len();
        self.new_branch_selection = None;
        self.nav.active_dialog = ActiveDialog::RenameBranch {
            old_name: branch_name,
        };
        self.messages.error_message.clear();
        cx.notify();
    }

    pub fn menu_delete_current_branch(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = self.repo.snapshot.as_ref() else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        let branch_name = snapshot.repo.current_branch.clone();
        let default_branch = self
            .repo
            .identity
            .default_branch
            .as_deref()
            .or(self.settings.default_branch.as_deref())
            .unwrap_or("main");
        if branch_name == default_branch || branch_name == "main" || branch_name == "master" {
            self.messages.error_message =
                "Cannot delete the default branch from the Branch menu.".to_string();
            cx.notify();
            return;
        }

        if snapshot
            .branches
            .iter()
            .filter(|branch| !branch.is_remote)
            .count()
            <= 1
        {
            self.messages.error_message =
                "Cannot delete the only local branch in this repository.".to_string();
            cx.notify();
            return;
        }

        self.nav.active_dialog = ActiveDialog::DeleteBranch { branch_name };
        self.messages.error_message.clear();
        cx.notify();
    }

    pub fn menu_merge_branch(&mut self, cx: &mut Context<Self>) {
        if self.repo.snapshot.is_none() {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        }

        self.nav.show_branch_selector = true;
        self.nav.branch_selector_mode = BranchSelectorMode::Merge;
        self.repo.pending_cherry_pick_oid = None;
        self.messages.status_message =
            "Choose a branch to merge into the current branch.".to_string();
        cx.notify();
    }

    pub fn menu_view_current_branch_on_github(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = self.repo.snapshot.as_ref() else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        self.handle_branch_context_action(
            snapshot.repo.current_branch.clone(),
            BranchContextAction::ViewOnGitHub,
            cx,
        );
    }

    pub fn menu_discard_all_changes(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = self.repo.snapshot.as_ref() else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        let paths: Vec<String> = snapshot
            .changes
            .iter()
            .map(|change| change.path.clone())
            .collect();
        if paths.is_empty() {
            self.messages.error_message = "There are no local changes to discard.".to_string();
            cx.notify();
            return;
        }

        self.nav.active_dialog = ActiveDialog::DiscardChanges { paths };
        self.messages.error_message.clear();
        cx.notify();
    }

    pub fn menu_stash_changes(&mut self, cx: &mut Context<Self>) {
        self.show_stash_changes_dialog(cx);
    }

    pub fn menu_zoom_in(&mut self, cx: &mut Context<Self>) {
        self.rem_size = (self.rem_size + ZOOM_STEP).min(ZOOM_MAX);
        let pct = ((self.rem_size / DEFAULT_REM_SIZE) * 100.0).round() as i32;
        self.messages.status_message = format!("Zoom: {pct}%");
        cx.notify();
    }

    pub fn menu_zoom_out(&mut self, cx: &mut Context<Self>) {
        self.rem_size = (self.rem_size - ZOOM_STEP).max(ZOOM_MIN);
        let pct = ((self.rem_size / DEFAULT_REM_SIZE) * 100.0).round() as i32;
        self.messages.status_message = format!("Zoom: {pct}%");
        cx.notify();
    }

    pub fn menu_zoom_reset(&mut self, cx: &mut Context<Self>) {
        self.rem_size = DEFAULT_REM_SIZE;
        self.messages.status_message = "Zoom: 100%".to_string();
        cx.notify();
    }

    fn handle_menu_open_repository(
        &mut self,
        _: &crate::MenuOpenRepository,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_repo_dialog(cx);
    }

    fn handle_menu_new_repository(
        &mut self,
        _: &crate::MenuNewRepository,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_create_repository_dialog(cx);
    }

    fn handle_menu_clone_repository(
        &mut self,
        _: &crate::MenuCloneRepository,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_clone_repository_dialog(cx);
    }

    fn handle_menu_show_settings(
        &mut self,
        _: &crate::MenuShowSettings,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_global_settings_modal(None, cx);
        cx.notify();
    }

    fn handle_menu_show_changes(
        &mut self,
        _: &crate::MenuShowChanges,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.nav.sidebar_tab = SidebarTab::Changes;
        cx.notify();
    }

    fn handle_menu_show_history(
        &mut self,
        _: &crate::MenuShowHistory,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.nav.sidebar_tab = SidebarTab::History;
        cx.notify();
    }

    fn handle_menu_show_repository_list(
        &mut self,
        _: &crate::MenuShowRepositoryList,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_show_repository_list(cx);
    }

    fn handle_menu_show_branches_list(
        &mut self,
        _: &crate::MenuShowBranchesList,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_show_branches_list(cx);
    }

    fn handle_menu_go_to_summary(
        &mut self,
        _: &crate::MenuGoToSummary,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_go_to_summary(cx);
        window.focus(&self.summary_focus);
    }

    fn handle_menu_show_stashed_changes(
        &mut self,
        _: &crate::MenuShowStashedChanges,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_show_stashed_changes(cx);
    }

    fn handle_menu_fetch(
        &mut self,
        _: &crate::MenuFetch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.fetch_origin(cx);
    }

    fn handle_menu_pull(
        &mut self,
        _: &crate::MenuPull,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pull_origin(cx);
    }

    fn handle_menu_push(
        &mut self,
        _: &crate::MenuPush,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.push_origin(cx);
    }

    fn handle_menu_publish_repository(
        &mut self,
        _: &crate::MenuPublishRepository,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_network_action(NetworkAction::PublishRepository, cx);
    }

    fn handle_menu_open_external_editor(
        &mut self,
        _: &crate::MenuOpenExternalEditor,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_open_external_editor(cx);
    }

    fn handle_menu_open_in_terminal(
        &mut self,
        _: &crate::MenuOpenInTerminal,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_open_in_terminal(cx);
    }

    fn handle_menu_show_in_finder(
        &mut self,
        _: &crate::MenuShowInFinder,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_show_in_finder(cx);
    }

    fn handle_menu_view_on_github(
        &mut self,
        _: &crate::MenuViewOnGitHub,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_view_on_github(cx);
    }

    fn handle_menu_repository_settings(
        &mut self,
        _: &crate::MenuRepositorySettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_repository_settings(cx);
        if self.nav.show_settings {
            self.activate_settings_field(SettingsField::GitUserName, window, cx);
        }
    }

    fn handle_menu_new_branch(
        &mut self,
        _: &crate::MenuNewBranch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_new_branch(cx);
        if matches!(self.nav.active_dialog, ActiveDialog::CreateBranch) {
            window.focus(&self.new_branch_focus);
        }
    }

    fn handle_menu_rename_branch(
        &mut self,
        _: &crate::MenuRenameBranch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_rename_current_branch(cx);
    }

    fn handle_menu_delete_branch(
        &mut self,
        _: &crate::MenuDeleteBranch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_delete_current_branch(cx);
    }

    fn handle_menu_merge_branch(
        &mut self,
        _: &crate::MenuMergeBranch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_merge_branch(cx);
    }

    fn handle_menu_view_branch_on_github(
        &mut self,
        _: &crate::MenuViewBranchOnGitHub,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_view_current_branch_on_github(cx);
    }

    fn handle_menu_discard_all_changes(
        &mut self,
        _: &crate::MenuDiscardAllChanges,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_discard_all_changes(cx);
    }

    fn handle_menu_stash_changes(
        &mut self,
        _: &crate::MenuStashChanges,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_stash_changes(cx);
    }

    fn handle_menu_zoom_in(
        &mut self,
        _: &crate::MenuZoomIn,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.rem_size = (self.rem_size + ZOOM_STEP).min(ZOOM_MAX);
        let pct = ((self.rem_size / DEFAULT_REM_SIZE) * 100.0).round() as i32;
        self.messages.status_message = format!("Zoom: {pct}%");
        cx.notify();
    }

    fn handle_menu_zoom_out(
        &mut self,
        _: &crate::MenuZoomOut,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.rem_size = (self.rem_size - ZOOM_STEP).max(ZOOM_MIN);
        let pct = ((self.rem_size / DEFAULT_REM_SIZE) * 100.0).round() as i32;
        self.messages.status_message = format!("Zoom: {pct}%");
        cx.notify();
    }

    fn handle_menu_zoom_reset(
        &mut self,
        _: &crate::MenuZoomReset,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.rem_size = DEFAULT_REM_SIZE;
        self.messages.status_message = "Zoom: 100%".to_string();
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Toolbar
    // ------------------------------------------------------------------

    /// Returns (left_toolbar, right_toolbar) so they can go into separate resizable columns.
    fn render_toolbar_parts(&self, cx: &mut Context<Self>) -> (Div, Div) {
        use crate::ui::toolbar;

        let snapshot = self.repo.snapshot.as_ref();
        let repo_name = snapshot
            .map(|s| s.repo.name.as_str())
            .unwrap_or("Choose repository");
        let branch_name = snapshot
            .map(|s| s.repo.current_branch.as_str())
            .unwrap_or("No branch");
        let ahead = snapshot.map(|s| s.repo.ahead).unwrap_or(0);
        let behind = snapshot.map(|s| s.repo.behind).unwrap_or(0);

        let network_action = snapshot
            .map(|s| NetworkAction::from_snapshot(s))
            .unwrap_or(NetworkAction::Fetch);
        let remote_name = snapshot
            .and_then(|s| s.repo.remote_name.as_deref())
            .unwrap_or("origin");
        let is_in_flight = self.network.active_action.is_some();
        let network_label = if let Some(active) = self.network.active_action {
            active.pending_title(remote_name)
        } else {
            network_action.title(remote_name)
        };
        let last_fetched = snapshot.and_then(|s| s.repo.last_fetched.as_deref());
        let network_enabled = snapshot.is_some();

        // --- Left: repo section ---
        // Icon: lock for repos with remote (private-like), folder for local-only
        let repo_icon = if snapshot.and_then(|s| s.repo.remote_name.as_ref()).is_some() {
            toolbar::ToolbarIcon::Svg("icons/lock.svg")
        } else {
            toolbar::ToolbarIcon::Name(IconName::FolderClosed)
        };
        let repo_section = toolbar::render_toolbar_section(
            "section-repo",
            repo_icon,
            "Current Repository",
            repo_name,
            self.nav.show_repo_selector,
            false,
            false,
        )
        .on_click(cx.listener(|app, _evt, _win, cx| {
            app.handle_toolbar_action(ToolbarAction::ToggleRepoSelector, cx);
            if app.nav.show_repo_selector {
                _win.focus(&app.repo_filter_focus);
            }
        }));

        let left = h_flex()
            .w_full()
            .h(theme::z(theme::TOOLBAR_HEIGHT))
            .flex_shrink_0()
            .bg(theme::toolbar_bg())
            .border_b_1()
            .border_color(theme::toolbar_button_border())
            .child(repo_section);

        // --- Right: branch section + divider + network section ---
        let branch_section = toolbar::render_toolbar_section(
            "section-branch",
            toolbar::ToolbarIcon::Svg("icons/git-branch.svg"),
            "Current Branch",
            branch_name,
            self.nav.show_branch_selector,
            false,
            snapshot.is_none(),
        )
        .flex_none()
        .w(px(toolbar::BRANCH_SECTION_WIDTH))
        .on_click(cx.listener(|app, _evt, _win, cx| {
            if app.repo.snapshot.is_none() {
                return;
            }
            app.nav.show_branch_selector = !app.nav.show_branch_selector;
            if !app.nav.show_branch_selector {
                app.repo.pending_cherry_pick_oid = None;
            }
            app.nav.branch_selector_mode = BranchSelectorMode::Switch;
            app.nav.show_repo_selector = false;
            app.nav.show_network_dropdown = false;
            cx.notify();
        }));

        let has_network_dropdown =
            matches!(network_action, NetworkAction::Pull | NetworkAction::Push);
        let show_network_dropdown = self.nav.show_network_dropdown && has_network_dropdown;
        let (network_main, network_caret) = toolbar::render_network_parts(
            &network_label,
            ahead,
            behind,
            last_fetched,
            is_in_flight,
            show_network_dropdown,
            !network_enabled,
        );

        let net_action = network_action;
        let network_main =
            network_main
                .pr(theme::z(10.0))
                .on_click(cx.listener(move |app, _evt, _win, cx| {
                    if app.repo.snapshot.is_some() && app.network.active_action.is_none() {
                        app.nav.show_network_dropdown = false;
                        if net_action == NetworkAction::PublishRepository {
                            app.open_publish_dialog(_win, cx);
                        } else {
                            app.handle_toolbar_action(
                                ToolbarAction::RunNetworkAction(net_action),
                                cx,
                            );
                        }
                    }
                }));
        let network_caret = network_caret.on_click(cx.listener(|app, _evt, _win, cx| {
            if app.repo.snapshot.is_none() {
                return;
            }
            let Some(snapshot) = app.repo.snapshot.as_ref() else {
                return;
            };
            let action = NetworkAction::from_snapshot(snapshot);
            if !matches!(action, NetworkAction::Pull | NetworkAction::Push) {
                app.nav.show_network_dropdown = false;
                cx.notify();
                return;
            }
            app.nav.show_network_dropdown = !app.nav.show_network_dropdown;
            app.nav.show_repo_selector = false;
            app.nav.show_branch_selector = false;
            app.nav.branch_selector_mode = BranchSelectorMode::Switch;
            cx.notify();
        }));

        let right = h_flex()
            .w_full()
            .h(theme::z(theme::TOOLBAR_HEIGHT))
            .flex_shrink_0()
            .bg(theme::toolbar_bg())
            .border_b_1()
            .border_color(theme::toolbar_button_border())
            .child(branch_section)
            .child(toolbar::vertical_divider())
            .child(
                div()
                    .flex_none()
                    .w(px(toolbar::NETWORK_SECTION_WIDTH))
                    .h_full()
                    .child(h_flex().size_full().child(network_main).children(
                        if has_network_dropdown {
                            Some(network_caret)
                        } else {
                            None
                        },
                    )),
            );

        (left, right)
    }

    // ------------------------------------------------------------------
    // Sidebar
    // ------------------------------------------------------------------

    fn render_sidebar(
        &self,
        summary_focused: bool,
        description_focused: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = cx.entity().clone();
        let sidebar_tab = self.nav.sidebar_tab;

        let mut sidebar = crate::ui::sidebar::render_sidebar_interactive(self, view, cx);

        // Commit form with interactive handlers (only on Changes tab)
        if sidebar_tab == SidebarTab::Changes {
            // Undo commit banner (auto-dismiss after 15 seconds)
            if let Some((summary, created_at)) = self
                .nav
                .undo_commit
                .as_ref()
                .filter(|_| self.can_undo_last_commit())
            {
                let elapsed = created_at.elapsed().as_secs();
                if elapsed < 15 {
                    let summary_text = if summary.len() > 30 {
                        format!("{}...", &summary[..27])
                    } else {
                        summary.clone()
                    };
                    sidebar = sidebar.child(
                        h_flex()
                            .w_full()
                            .h(theme::z(32.0))
                            .px(theme::z(10.0))
                            .items_center()
                            .gap(theme::z(6.0))
                            .bg(theme::surface_bg())
                            .border_t_1()
                            .border_color(theme::border())
                            .flex_shrink_0()
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(theme::z(11.0))
                                    .text_color(theme::text_muted())
                                    .overflow_x_hidden()
                                    .whitespace_nowrap()
                                    .child(format!("\u{201C}{summary_text}\u{201D}")),
                            )
                            .child(
                                div()
                                    .id("undo-commit-btn")
                                    .px(theme::z(8.0))
                                    .py(theme::z(2.0))
                                    .rounded(theme::z(4.0))
                                    .bg(theme::accent())
                                    .text_size(theme::z(11.0))
                                    .text_color(gpui::white())
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::commit_button_hover_bg()))
                                    .on_click(cx.listener(|app, _evt, _win, cx| {
                                        app.undo_last_commit(cx);
                                    }))
                                    .child("Undo"),
                            ),
                    );
                } else {
                    // Auto-dismiss
                    // (Can't mutate here in render, but it'll clear on next event)
                }
            }

            let branch_name = self
                .repo
                .snapshot
                .as_ref()
                .map(|s| s.repo.current_branch.clone())
                .unwrap_or_else(|| "main".to_string());
            sidebar = sidebar.child(self.render_commit_form_interactive(
                &branch_name,
                summary_focused,
                description_focused,
                cx,
            ));
        }

        sidebar
    }

    // ------------------------------------------------------------------
    // Text input key handling
    // ------------------------------------------------------------------

    fn handle_summary_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ks = &event.keystroke;
        if ks.modifiers.secondary() {
            match ks.key.as_str() {
                "v" => {
                    if let Some(item) = cx.read_from_clipboard() {
                        if let Some(text) = item.text() {
                            let text = text.replace('\n', " ");
                            self.delete_summary_selection();
                            self.commit.summary.insert_str(self.summary_cursor, &text);
                            self.summary_cursor += text.len();
                            cx.notify();
                        }
                    }
                }
                "a" => {
                    // Select all
                    self.summary_selection = Some(0);
                    self.summary_cursor = self.commit.summary.len();
                    cx.notify();
                }
                "c" => {
                    // Copy selection
                    if let Some(sel) = self.summary_selection {
                        let (start, end) = ordered_range(sel, self.summary_cursor);
                        let selected = &self.commit.summary[start..end];
                        if !selected.is_empty() {
                            cx.write_to_clipboard(ClipboardItem::new_string(selected.to_string()));
                        }
                    }
                }
                "x" => {
                    // Cut selection
                    if let Some(sel) = self.summary_selection {
                        let (start, end) = ordered_range(sel, self.summary_cursor);
                        let selected = &self.commit.summary[start..end];
                        if !selected.is_empty() {
                            cx.write_to_clipboard(ClipboardItem::new_string(selected.to_string()));
                            self.delete_summary_selection();
                            cx.notify();
                        }
                    }
                }
                _ => {}
            }
            return;
        }
        match ks.key.as_str() {
            "backspace" => {
                if self.summary_selection.is_some() {
                    self.delete_summary_selection();
                } else if self.summary_cursor > 0 {
                    let new_pos = prev_char_boundary(&self.commit.summary, self.summary_cursor);
                    self.commit.summary.drain(new_pos..self.summary_cursor);
                    self.summary_cursor = new_pos;
                }
                cx.notify();
            }
            "delete" => {
                if self.summary_selection.is_some() {
                    self.delete_summary_selection();
                } else if self.summary_cursor < self.commit.summary.len() {
                    let end = next_char_boundary(&self.commit.summary, self.summary_cursor);
                    self.commit.summary.drain(self.summary_cursor..end);
                }
                cx.notify();
            }
            "left" => {
                if ks.modifiers.shift {
                    if self.summary_selection.is_none() {
                        self.summary_selection = Some(self.summary_cursor);
                    }
                } else {
                    self.summary_selection = None;
                }
                if self.summary_cursor > 0 {
                    self.summary_cursor =
                        prev_char_boundary(&self.commit.summary, self.summary_cursor);
                    cx.notify();
                }
            }
            "right" => {
                if ks.modifiers.shift {
                    if self.summary_selection.is_none() {
                        self.summary_selection = Some(self.summary_cursor);
                    }
                } else {
                    self.summary_selection = None;
                }
                if self.summary_cursor < self.commit.summary.len() {
                    self.summary_cursor =
                        next_char_boundary(&self.commit.summary, self.summary_cursor);
                    cx.notify();
                }
            }
            "home" => {
                if ks.modifiers.shift {
                    if self.summary_selection.is_none() {
                        self.summary_selection = Some(self.summary_cursor);
                    }
                } else {
                    self.summary_selection = None;
                }
                self.summary_cursor = 0;
                cx.notify();
            }
            "end" => {
                if ks.modifiers.shift {
                    if self.summary_selection.is_none() {
                        self.summary_selection = Some(self.summary_cursor);
                    }
                } else {
                    self.summary_selection = None;
                }
                self.summary_cursor = self.commit.summary.len();
                cx.notify();
            }
            _ => {
                if let Some(ref ch) = ks.key_char {
                    if !ks.modifiers.control && !ch.contains('\n') && !ch.contains('\r') {
                        self.delete_summary_selection();
                        self.commit.summary.insert_str(self.summary_cursor, ch);
                        self.summary_cursor += ch.len();
                        cx.notify();
                    }
                }
            }
        }
    }

    fn delete_summary_selection(&mut self) {
        if let Some(sel) = self.summary_selection.take() {
            let (start, end) = ordered_range(sel, self.summary_cursor);
            if start < end && end <= self.commit.summary.len() {
                self.commit.summary.drain(start..end);
                self.summary_cursor = start;
            }
        }
    }

    fn handle_description_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ks = &event.keystroke;
        if ks.modifiers.secondary() {
            match ks.key.as_str() {
                "v" => {
                    if let Some(item) = cx.read_from_clipboard() {
                        if let Some(text) = item.text() {
                            self.delete_description_selection();
                            self.commit.body.insert_str(self.description_cursor, &text);
                            self.description_cursor += text.len();
                            cx.notify();
                        }
                    }
                }
                "a" => {
                    self.description_selection = Some(0);
                    self.description_cursor = self.commit.body.len();
                    cx.notify();
                }
                "c" => {
                    if let Some(sel) = self.description_selection {
                        let (start, end) = ordered_range(sel, self.description_cursor);
                        let selected = &self.commit.body[start..end];
                        if !selected.is_empty() {
                            cx.write_to_clipboard(ClipboardItem::new_string(selected.to_string()));
                        }
                    }
                }
                "x" => {
                    if let Some(sel) = self.description_selection {
                        let (start, end) = ordered_range(sel, self.description_cursor);
                        let selected = &self.commit.body[start..end];
                        if !selected.is_empty() {
                            cx.write_to_clipboard(ClipboardItem::new_string(selected.to_string()));
                            self.delete_description_selection();
                            cx.notify();
                        }
                    }
                }
                _ => {}
            }
            return;
        }
        match ks.key.as_str() {
            "backspace" => {
                if self.description_selection.is_some() {
                    self.delete_description_selection();
                } else if self.description_cursor > 0 {
                    let new_pos = prev_char_boundary(&self.commit.body, self.description_cursor);
                    self.commit.body.drain(new_pos..self.description_cursor);
                    self.description_cursor = new_pos;
                }
                cx.notify();
            }
            "delete" => {
                if self.description_selection.is_some() {
                    self.delete_description_selection();
                } else if self.description_cursor < self.commit.body.len() {
                    let end = next_char_boundary(&self.commit.body, self.description_cursor);
                    self.commit.body.drain(self.description_cursor..end);
                }
                cx.notify();
            }
            "left" => {
                if ks.modifiers.shift {
                    if self.description_selection.is_none() {
                        self.description_selection = Some(self.description_cursor);
                    }
                } else {
                    self.description_selection = None;
                }
                if self.description_cursor > 0 {
                    self.description_cursor =
                        prev_char_boundary(&self.commit.body, self.description_cursor);
                    cx.notify();
                }
            }
            "right" => {
                if ks.modifiers.shift {
                    if self.description_selection.is_none() {
                        self.description_selection = Some(self.description_cursor);
                    }
                } else {
                    self.description_selection = None;
                }
                if self.description_cursor < self.commit.body.len() {
                    self.description_cursor =
                        next_char_boundary(&self.commit.body, self.description_cursor);
                    cx.notify();
                }
            }
            "home" => {
                if ks.modifiers.shift {
                    if self.description_selection.is_none() {
                        self.description_selection = Some(self.description_cursor);
                    }
                } else {
                    self.description_selection = None;
                }
                self.description_cursor = 0;
                cx.notify();
            }
            "end" => {
                if ks.modifiers.shift {
                    if self.description_selection.is_none() {
                        self.description_selection = Some(self.description_cursor);
                    }
                } else {
                    self.description_selection = None;
                }
                self.description_cursor = self.commit.body.len();
                cx.notify();
            }
            "enter" => {
                self.delete_description_selection();
                self.commit.body.insert_str(self.description_cursor, "\n");
                self.description_cursor += 1;
                cx.notify();
            }
            _ => {
                if let Some(ref ch) = ks.key_char {
                    if !ks.modifiers.control {
                        self.delete_description_selection();
                        self.commit.body.insert_str(self.description_cursor, ch);
                        self.description_cursor += ch.len();
                        cx.notify();
                    }
                }
            }
        }
    }

    fn delete_description_selection(&mut self) {
        if let Some(sel) = self.description_selection.take() {
            let (start, end) = ordered_range(sel, self.description_cursor);
            if start < end && end <= self.commit.body.len() {
                self.commit.body.drain(start..end);
                self.description_cursor = start;
            }
        }
    }

    // ------------------------------------------------------------------
    // Filter input key handling
    // ------------------------------------------------------------------

    fn handle_branch_filter_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ks = &event.keystroke;
        if ks.modifiers.secondary() {
            if ks.key.as_str() == "v" {
                if let Some(item) = cx.read_from_clipboard() {
                    if let Some(text) = item.text() {
                        let text = text.replace('\n', "");
                        self.filters
                            .branch_filter_text
                            .insert_str(self.branch_filter_cursor, &text);
                        self.branch_filter_cursor += text.len();
                        cx.notify();
                    }
                }
            }
            return;
        }
        match ks.key.as_str() {
            "backspace" => {
                if self.branch_filter_cursor > 0 {
                    let new_pos = prev_char_boundary(
                        &self.filters.branch_filter_text,
                        self.branch_filter_cursor,
                    );
                    self.filters
                        .branch_filter_text
                        .drain(new_pos..self.branch_filter_cursor);
                    self.branch_filter_cursor = new_pos;
                    cx.notify();
                }
            }
            "escape" => {
                self.nav.show_branch_selector = false;
                self.nav.branch_selector_mode = BranchSelectorMode::Switch;
                self.repo.pending_cherry_pick_oid = None;
                self.filters.branch_filter_text.clear();
                self.branch_filter_cursor = 0;
                cx.notify();
            }
            "left" => {
                if self.branch_filter_cursor > 0 {
                    self.branch_filter_cursor = prev_char_boundary(
                        &self.filters.branch_filter_text,
                        self.branch_filter_cursor,
                    );
                    cx.notify();
                }
            }
            "right" => {
                if self.branch_filter_cursor < self.filters.branch_filter_text.len() {
                    self.branch_filter_cursor = next_char_boundary(
                        &self.filters.branch_filter_text,
                        self.branch_filter_cursor,
                    );
                    cx.notify();
                }
            }
            _ => {
                if let Some(ref ch) = ks.key_char {
                    if !ks.modifiers.control && !ch.contains('\n') && !ch.contains('\r') {
                        self.filters
                            .branch_filter_text
                            .insert_str(self.branch_filter_cursor, ch);
                        self.branch_filter_cursor += ch.len();
                        cx.notify();
                    }
                }
            }
        }
    }

    fn handle_new_branch_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut state = crate::ui::text_field::TextFieldState {
            cursor: self.new_branch_cursor,
            selection: self.new_branch_selection,
        };
        let handled = crate::ui::text_field::handle_text_key(
            &mut self.repo.new_branch_name,
            &mut state,
            false,
            event,
            cx,
        );
        self.new_branch_cursor = state.cursor;
        self.new_branch_selection = state.selection;
        if handled {
            cx.notify();
        }
    }

    pub(crate) fn handle_publish_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ks = &event.keystroke;
        if ks.key == "escape" {
            self.nav.active_dialog = ActiveDialog::None;
            cx.notify();
            return;
        }
        if ks.key == "tab" {
            self.publish_active_field = Some(match self.publish_active_field {
                Some(PublishField::Name) => PublishField::Description,
                _ => PublishField::Name,
            });
            cx.notify();
            return;
        }

        let Some(field) = self.publish_active_field else {
            return;
        };

        let (value, cursor, selection) = match field {
            PublishField::Name => (
                &mut self.network.publish_name,
                &mut self.publish_name_cursor,
                &mut self.publish_name_selection,
            ),
            PublishField::Description => (
                &mut self.network.publish_description,
                &mut self.publish_description_cursor,
                &mut self.publish_description_selection,
            ),
        };
        let mut state = crate::ui::text_field::TextFieldState {
            cursor: *cursor,
            selection: *selection,
        };
        let handled = crate::ui::text_field::handle_text_key(value, &mut state, false, event, cx);
        *cursor = state.cursor;
        *selection = state.selection;
        if handled {
            cx.notify();
        }
    }

    pub(crate) fn repository_field_value(&self, field: RepositoryField) -> &str {
        match field {
            RepositoryField::CreateName => self.repo.create_repo_name.as_str(),
            RepositoryField::CreateDescription => self.repo.create_repo_description.as_str(),
            RepositoryField::CreatePath => self.repo.create_repo_path.as_str(),
            RepositoryField::CloneUrl => self.repo.clone_repo_url.as_str(),
            RepositoryField::ClonePath => self.repo.clone_repo_path.as_str(),
        }
    }

    pub(crate) fn repository_field_cursor(&self, field: RepositoryField) -> usize {
        match field {
            RepositoryField::CreateName => self.repository_create_name_cursor,
            RepositoryField::CreateDescription => self.repository_create_description_cursor,
            RepositoryField::CreatePath => self.repository_create_path_cursor,
            RepositoryField::CloneUrl => self.repository_clone_url_cursor,
            RepositoryField::ClonePath => self.repository_clone_path_cursor,
        }
    }

    pub(crate) fn repository_field_selection(&self, field: RepositoryField) -> Option<usize> {
        match field {
            RepositoryField::CreateName => self.repository_create_name_selection,
            RepositoryField::CreateDescription => self.repository_create_description_selection,
            RepositoryField::CreatePath => self.repository_create_path_selection,
            RepositoryField::CloneUrl => self.repository_clone_url_selection,
            RepositoryField::ClonePath => self.repository_clone_path_selection,
        }
    }

    pub(crate) fn repository_field_focused(&self, field: RepositoryField, window: &Window) -> bool {
        self.repository_focus.is_focused(window) && self.repository_active_field == Some(field)
    }

    pub(crate) fn activate_repository_field(
        &mut self,
        field: RepositoryField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.repository_active_field = Some(field);
        let cursor = self.repository_field_value(field).len();
        self.set_repository_field_cursor(field, cursor);
        self.set_repository_field_selection(field, None);
        window.focus(&self.repository_focus);
        cx.notify();
    }

    fn set_repository_field_cursor(&mut self, field: RepositoryField, cursor: usize) {
        match field {
            RepositoryField::CreateName => self.repository_create_name_cursor = cursor,
            RepositoryField::CreateDescription => {
                self.repository_create_description_cursor = cursor
            }
            RepositoryField::CreatePath => self.repository_create_path_cursor = cursor,
            RepositoryField::CloneUrl => self.repository_clone_url_cursor = cursor,
            RepositoryField::ClonePath => self.repository_clone_path_cursor = cursor,
        }
    }

    fn set_repository_field_selection(&mut self, field: RepositoryField, selection: Option<usize>) {
        match field {
            RepositoryField::CreateName => self.repository_create_name_selection = selection,
            RepositoryField::CreateDescription => {
                self.repository_create_description_selection = selection
            }
            RepositoryField::CreatePath => self.repository_create_path_selection = selection,
            RepositoryField::CloneUrl => self.repository_clone_url_selection = selection,
            RepositoryField::ClonePath => self.repository_clone_path_selection = selection,
        }
    }

    pub(crate) fn handle_repository_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key == "escape" {
            self.nav.active_dialog = ActiveDialog::None;
            cx.notify();
            return;
        }
        if event.keystroke.key == "tab" {
            let next_field = match self.nav.active_dialog {
                ActiveDialog::CreateRepository => match self.repository_active_field {
                    Some(RepositoryField::CreateName) => RepositoryField::CreateDescription,
                    Some(RepositoryField::CreateDescription) => RepositoryField::CreatePath,
                    _ => RepositoryField::CreateName,
                },
                ActiveDialog::CloneRepository => match self.repository_active_field {
                    Some(RepositoryField::CloneUrl) => RepositoryField::ClonePath,
                    _ => RepositoryField::CloneUrl,
                },
                _ => return,
            };
            self.repository_active_field = Some(next_field);
            self.set_repository_field_cursor(
                next_field,
                self.repository_field_value(next_field).len(),
            );
            self.set_repository_field_selection(next_field, None);
            cx.notify();
            return;
        }

        let Some(field) = self.repository_active_field else {
            return;
        };

        let handled = match field {
            RepositoryField::CreateName => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.repository_create_name_cursor,
                    selection: self.repository_create_name_selection,
                };
                let handled = crate::ui::text_field::handle_text_key(
                    &mut self.repo.create_repo_name,
                    &mut state,
                    false,
                    event,
                    cx,
                );
                self.repository_create_name_cursor = state.cursor;
                self.repository_create_name_selection = state.selection;
                handled
            }
            RepositoryField::CreateDescription => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.repository_create_description_cursor,
                    selection: self.repository_create_description_selection,
                };
                let handled = crate::ui::text_field::handle_text_key(
                    &mut self.repo.create_repo_description,
                    &mut state,
                    false,
                    event,
                    cx,
                );
                self.repository_create_description_cursor = state.cursor;
                self.repository_create_description_selection = state.selection;
                handled
            }
            RepositoryField::CreatePath => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.repository_create_path_cursor,
                    selection: self.repository_create_path_selection,
                };
                let handled = crate::ui::text_field::handle_text_key(
                    &mut self.repo.create_repo_path,
                    &mut state,
                    false,
                    event,
                    cx,
                );
                self.repository_create_path_cursor = state.cursor;
                self.repository_create_path_selection = state.selection;
                handled
            }
            RepositoryField::CloneUrl => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.repository_clone_url_cursor,
                    selection: self.repository_clone_url_selection,
                };
                let handled = crate::ui::text_field::handle_text_key(
                    &mut self.repo.clone_repo_url,
                    &mut state,
                    false,
                    event,
                    cx,
                );
                self.repository_clone_url_cursor = state.cursor;
                self.repository_clone_url_selection = state.selection;
                handled
            }
            RepositoryField::ClonePath => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.repository_clone_path_cursor,
                    selection: self.repository_clone_path_selection,
                };
                let handled = crate::ui::text_field::handle_text_key(
                    &mut self.repo.clone_repo_path,
                    &mut state,
                    false,
                    event,
                    cx,
                );
                self.repository_clone_path_cursor = state.cursor;
                self.repository_clone_path_selection = state.selection;
                handled
            }
        };
        if handled {
            cx.notify();
        }
    }

    #[allow(dead_code)]
    fn handle_repo_filter_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ks = &event.keystroke;
        if ks.modifiers.secondary() {
            if ks.key.as_str() == "v" {
                if let Some(item) = cx.read_from_clipboard() {
                    if let Some(text) = item.text() {
                        let text = text.replace('\n', "");
                        self.filters
                            .repo_filter_text
                            .insert_str(self.repo_filter_cursor, &text);
                        self.repo_filter_cursor += text.len();
                        cx.notify();
                    }
                }
            }
            return;
        }
        match ks.key.as_str() {
            "backspace" => {
                if self.repo_filter_cursor > 0 {
                    let new_pos =
                        prev_char_boundary(&self.filters.repo_filter_text, self.repo_filter_cursor);
                    self.filters
                        .repo_filter_text
                        .drain(new_pos..self.repo_filter_cursor);
                    self.repo_filter_cursor = new_pos;
                    cx.notify();
                }
            }
            "escape" => {
                self.nav.show_repo_selector = false;
                self.filters.repo_filter_text.clear();
                self.repo_filter_cursor = 0;
                cx.notify();
            }
            "left" => {
                if self.repo_filter_cursor > 0 {
                    self.repo_filter_cursor =
                        prev_char_boundary(&self.filters.repo_filter_text, self.repo_filter_cursor);
                    cx.notify();
                }
            }
            "right" => {
                if self.repo_filter_cursor < self.filters.repo_filter_text.len() {
                    self.repo_filter_cursor =
                        next_char_boundary(&self.filters.repo_filter_text, self.repo_filter_cursor);
                    cx.notify();
                }
            }
            _ => {
                if let Some(ref ch) = ks.key_char {
                    if !ks.modifiers.control && !ch.contains('\n') && !ch.contains('\r') {
                        self.filters
                            .repo_filter_text
                            .insert_str(self.repo_filter_cursor, ch);
                        self.repo_filter_cursor += ch.len();
                        cx.notify();
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Settings modal input handling
    // ------------------------------------------------------------------

    pub(crate) fn close_settings_modal(&mut self) {
        self.nav.show_settings = false;
        self.settings_modal.active_field = None;
        self.close_history_context_menu();
    }

    pub(crate) fn open_settings_modal(
        &mut self,
        section: Option<crate::ui::ui_state::SettingsSection>,
        cx: &mut Context<Self>,
    ) {
        let scope = if self.repo.snapshot.is_some() {
            SettingsScope::Repository
        } else {
            SettingsScope::Global
        };
        self.open_settings_modal_with_scope(section, scope, cx);
    }

    pub(crate) fn open_global_settings_modal(
        &mut self,
        section: Option<crate::ui::ui_state::SettingsSection>,
        cx: &mut Context<Self>,
    ) {
        self.open_settings_modal_with_scope(section, SettingsScope::Global, cx);
    }

    pub(crate) fn open_repository_settings_modal(
        &mut self,
        section: Option<crate::ui::ui_state::SettingsSection>,
        cx: &mut Context<Self>,
    ) {
        self.open_settings_modal_with_scope(section, SettingsScope::Repository, cx);
    }

    fn open_settings_modal_with_scope(
        &mut self,
        section: Option<crate::ui::ui_state::SettingsSection>,
        scope: SettingsScope,
        cx: &mut Context<Self>,
    ) {
        if let Some(section) = section {
            self.nav.settings_section = section;
        } else if scope == SettingsScope::Global
            && matches!(
                self.nav.settings_section,
                crate::ui::ui_state::SettingsSection::Remote
                    | crate::ui::ui_state::SettingsSection::IgnoredFiles
            )
        {
            self.nav.settings_section = crate::ui::ui_state::SettingsSection::Git;
        } else if scope == SettingsScope::Repository
            && self.nav.settings_section == crate::ui::ui_state::SettingsSection::Ai
        {
            self.nav.settings_section = crate::ui::ui_state::SettingsSection::Remote;
        }

        self.close_history_context_menu();
        self.nav.settings_scope = scope;
        self.nav.show_settings = true;
        if self.nav.settings_section == crate::ui::ui_state::SettingsSection::Remote
            && let Some(path) = self.repo_path().map(PathBuf::from)
        {
            self.load_remote_settings(&path);
        } else if self.nav.settings_section == crate::ui::ui_state::SettingsSection::IgnoredFiles
            && let Some(path) = self.repo_path().map(PathBuf::from)
        {
            self.load_ignored_files_settings(&path);
        } else if self.nav.settings_section == crate::ui::ui_state::SettingsSection::Git {
            if self.settings_has_repository_scope()
                && let Some(path) = self.repo_path().map(PathBuf::from)
            {
                self.load_identity(&path);
            } else {
                self.load_global_identity();
            }
        }
        let field = if self.nav.settings_section == crate::ui::ui_state::SettingsSection::Ai
            && self.settings.ai.provider == AiProvider::OpenRouter
        {
            SettingsField::OpenRouterModelFilter
        } else {
            settings_modal::default_settings_field(self.nav.settings_section)
        };
        self.settings_modal.active_field = Some(field);
        self.set_settings_field_cursor(field, self.settings_field_value(field).len());

        if self.nav.settings_section == crate::ui::ui_state::SettingsSection::Ai
            && self.settings.ai.provider == AiProvider::OpenRouter
        {
            self.ensure_openrouter_models(cx);
        }
    }

    pub(crate) fn settings_field_value(&self, field: SettingsField) -> &str {
        match field {
            SettingsField::RemoteUrl => self.repo.remote_url.as_str(),
            SettingsField::IgnoredFiles => self.repo.ignored_files_text.as_str(),
            SettingsField::GitUserName => self.active_git_settings_identity().user_name.as_str(),
            SettingsField::GitUserEmail => self.active_git_settings_identity().user_email.as_str(),
            SettingsField::GitDefaultBranch => self
                .active_git_settings_identity()
                .default_branch
                .as_deref()
                .unwrap_or(""),
            SettingsField::AiModel => self.settings.ai.model.as_str(),
            SettingsField::AiEndpoint => self.settings.ai.endpoint.as_str(),
            SettingsField::AiApiKey => self.settings.ai.api_key.as_str(),
            SettingsField::AiSystemPrompt => self.settings.ai.system_prompt.as_str(),
            SettingsField::OpenRouterModelFilter => self.filters.openrouter_model_filter.as_str(),
        }
    }

    pub(crate) fn active_git_settings_identity(&self) -> &GitIdentity {
        if self.settings_has_repository_scope() {
            if self.repo.use_local_identity {
                &self.repo.local_identity
            } else {
                &self.repo.global_identity
            }
        } else {
            &self.repo.global_identity
        }
    }

    pub(crate) fn active_git_settings_identity_mut(&mut self) -> &mut GitIdentity {
        if self.settings_has_repository_scope() {
            if self.repo.use_local_identity {
                &mut self.repo.local_identity
            } else {
                &mut self.repo.global_identity
            }
        } else {
            &mut self.repo.global_identity
        }
    }

    pub(crate) fn settings_field_read_only(&self, field: SettingsField) -> bool {
        self.settings_has_repository_scope()
            && !self.repo.use_local_identity
            && matches!(
                field,
                SettingsField::GitUserName | SettingsField::GitUserEmail
            )
    }

    pub(crate) fn settings_has_repository_scope(&self) -> bool {
        self.nav.settings_scope == SettingsScope::Repository && self.repo.snapshot.is_some()
    }

    pub(crate) fn settings_field_cursor(&self, field: SettingsField) -> usize {
        match field {
            SettingsField::RemoteUrl => self.settings_modal.remote_url_cursor,
            SettingsField::IgnoredFiles => self.settings_modal.ignored_files_cursor,
            SettingsField::GitUserName => self.settings_modal.git_user_name_cursor,
            SettingsField::GitUserEmail => self.settings_modal.git_user_email_cursor,
            SettingsField::GitDefaultBranch => self.settings_modal.git_default_branch_cursor,
            SettingsField::AiModel => self.settings_modal.ai_model_cursor,
            SettingsField::AiEndpoint => self.settings_modal.ai_endpoint_cursor,
            SettingsField::AiApiKey => self.settings_modal.ai_api_key_cursor,
            SettingsField::AiSystemPrompt => self.settings_modal.ai_system_prompt_cursor,
            SettingsField::OpenRouterModelFilter => {
                self.settings_modal.openrouter_model_filter_cursor
            }
        }
    }

    pub(crate) fn settings_field_selection(&self, field: SettingsField) -> Option<usize> {
        match field {
            SettingsField::RemoteUrl => self.settings_modal.remote_url_selection,
            SettingsField::IgnoredFiles => self.settings_modal.ignored_files_selection,
            SettingsField::GitUserName => self.settings_modal.git_user_name_selection,
            SettingsField::GitUserEmail => self.settings_modal.git_user_email_selection,
            SettingsField::GitDefaultBranch => self.settings_modal.git_default_branch_selection,
            SettingsField::AiModel => self.settings_modal.ai_model_selection,
            SettingsField::AiEndpoint => self.settings_modal.ai_endpoint_selection,
            SettingsField::AiApiKey => self.settings_modal.ai_api_key_selection,
            SettingsField::AiSystemPrompt => self.settings_modal.ai_system_prompt_selection,
            SettingsField::OpenRouterModelFilter => {
                self.settings_modal.openrouter_model_filter_selection
            }
        }
    }

    pub(crate) fn settings_field_focused(&self, field: SettingsField, window: &Window) -> bool {
        self.settings_modal.focus.is_focused(window)
            && self.settings_modal.active_field == Some(field)
    }

    pub(crate) fn activate_settings_field(
        &mut self,
        field: SettingsField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cursor = self.settings_field_value(field).len();
        self.settings_modal.active_field = Some(field);
        self.set_settings_field_cursor(field, cursor);
        window.focus(&self.settings_modal.focus);
        cx.notify();
    }

    pub(crate) fn handle_settings_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key == "escape" {
            self.close_settings_modal();
            cx.notify();
            return;
        }

        let Some(field) = self.settings_modal.active_field else {
            return;
        };
        if self.settings_field_read_only(field) {
            return;
        }

        let multiline = matches!(
            field,
            SettingsField::IgnoredFiles | SettingsField::AiSystemPrompt
        );

        // Get mutable references to the value, cursor, and selection for the active field
        let handled = match field {
            SettingsField::RemoteUrl => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.settings_modal.remote_url_cursor,
                    selection: self.settings_modal.remote_url_selection,
                };
                let h = crate::ui::text_field::handle_text_key(
                    &mut self.repo.remote_url,
                    &mut state,
                    multiline,
                    event,
                    cx,
                );
                self.settings_modal.remote_url_cursor = state.cursor;
                self.settings_modal.remote_url_selection = state.selection;
                h
            }
            SettingsField::IgnoredFiles => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.settings_modal.ignored_files_cursor,
                    selection: self.settings_modal.ignored_files_selection,
                };
                let h = crate::ui::text_field::handle_text_key(
                    &mut self.repo.ignored_files_text,
                    &mut state,
                    multiline,
                    event,
                    cx,
                );
                self.settings_modal.ignored_files_cursor = state.cursor;
                self.settings_modal.ignored_files_selection = state.selection;
                h
            }
            SettingsField::GitUserName => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.settings_modal.git_user_name_cursor,
                    selection: self.settings_modal.git_user_name_selection,
                };
                let mut value = self.active_git_settings_identity().user_name.clone();
                let h = crate::ui::text_field::handle_text_key(
                    &mut value, &mut state, multiline, event, cx,
                );
                self.active_git_settings_identity_mut().user_name = value;
                self.settings_modal.git_user_name_cursor = state.cursor;
                self.settings_modal.git_user_name_selection = state.selection;
                h
            }
            SettingsField::GitUserEmail => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.settings_modal.git_user_email_cursor,
                    selection: self.settings_modal.git_user_email_selection,
                };
                let mut value = self.active_git_settings_identity().user_email.clone();
                let h = crate::ui::text_field::handle_text_key(
                    &mut value, &mut state, multiline, event, cx,
                );
                self.active_git_settings_identity_mut().user_email = value;
                self.settings_modal.git_user_email_cursor = state.cursor;
                self.settings_modal.git_user_email_selection = state.selection;
                h
            }
            SettingsField::GitDefaultBranch => {
                let mut value = self
                    .active_git_settings_identity()
                    .default_branch
                    .clone()
                    .unwrap_or_default();
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.settings_modal.git_default_branch_cursor,
                    selection: self.settings_modal.git_default_branch_selection,
                };
                let h = crate::ui::text_field::handle_text_key(
                    &mut value, &mut state, multiline, event, cx,
                );
                self.active_git_settings_identity_mut().default_branch = if value.trim().is_empty()
                {
                    None
                } else {
                    Some(value)
                };
                self.settings_modal.git_default_branch_cursor = state.cursor;
                self.settings_modal.git_default_branch_selection = state.selection;
                h
            }
            SettingsField::AiModel => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.settings_modal.ai_model_cursor,
                    selection: self.settings_modal.ai_model_selection,
                };
                let h = crate::ui::text_field::handle_text_key(
                    &mut self.settings.ai.model,
                    &mut state,
                    multiline,
                    event,
                    cx,
                );
                self.settings_modal.ai_model_cursor = state.cursor;
                self.settings_modal.ai_model_selection = state.selection;
                h
            }
            SettingsField::AiEndpoint => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.settings_modal.ai_endpoint_cursor,
                    selection: self.settings_modal.ai_endpoint_selection,
                };
                let h = crate::ui::text_field::handle_text_key(
                    &mut self.settings.ai.endpoint,
                    &mut state,
                    multiline,
                    event,
                    cx,
                );
                self.settings_modal.ai_endpoint_cursor = state.cursor;
                self.settings_modal.ai_endpoint_selection = state.selection;
                h
            }
            SettingsField::AiApiKey => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.settings_modal.ai_api_key_cursor,
                    selection: self.settings_modal.ai_api_key_selection,
                };
                let h = crate::ui::text_field::handle_text_key(
                    &mut self.settings.ai.api_key,
                    &mut state,
                    multiline,
                    event,
                    cx,
                );
                self.settings_modal.ai_api_key_cursor = state.cursor;
                self.settings_modal.ai_api_key_selection = state.selection;
                h
            }
            SettingsField::AiSystemPrompt => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.settings_modal.ai_system_prompt_cursor,
                    selection: self.settings_modal.ai_system_prompt_selection,
                };
                let h = crate::ui::text_field::handle_text_key(
                    &mut self.settings.ai.system_prompt,
                    &mut state,
                    multiline,
                    event,
                    cx,
                );
                self.settings_modal.ai_system_prompt_cursor = state.cursor;
                self.settings_modal.ai_system_prompt_selection = state.selection;
                h
            }
            SettingsField::OpenRouterModelFilter => {
                let mut state = crate::ui::text_field::TextFieldState {
                    cursor: self.settings_modal.openrouter_model_filter_cursor,
                    selection: self.settings_modal.openrouter_model_filter_selection,
                };
                let h = crate::ui::text_field::handle_text_key(
                    &mut self.filters.openrouter_model_filter,
                    &mut state,
                    multiline,
                    event,
                    cx,
                );
                self.settings_modal.openrouter_model_filter_cursor = state.cursor;
                self.settings_modal.openrouter_model_filter_selection = state.selection;
                h
            }
        };
        if handled {
            cx.notify();
        }
    }

    fn set_settings_field_cursor(&mut self, field: SettingsField, cursor: usize) {
        match field {
            SettingsField::RemoteUrl => self.settings_modal.remote_url_cursor = cursor,
            SettingsField::IgnoredFiles => self.settings_modal.ignored_files_cursor = cursor,
            SettingsField::GitUserName => self.settings_modal.git_user_name_cursor = cursor,
            SettingsField::GitUserEmail => self.settings_modal.git_user_email_cursor = cursor,
            SettingsField::GitDefaultBranch => {
                self.settings_modal.git_default_branch_cursor = cursor
            }
            SettingsField::AiModel => self.settings_modal.ai_model_cursor = cursor,
            SettingsField::AiEndpoint => self.settings_modal.ai_endpoint_cursor = cursor,
            SettingsField::AiApiKey => self.settings_modal.ai_api_key_cursor = cursor,
            SettingsField::AiSystemPrompt => self.settings_modal.ai_system_prompt_cursor = cursor,
            SettingsField::OpenRouterModelFilter => {
                self.settings_modal.openrouter_model_filter_cursor = cursor
            }
        }
    }

    // ------------------------------------------------------------------
    // Text field renderer
    // ------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn render_text_field(
        &self,
        id: &str,
        value: &str,
        placeholder: &str,
        cursor: usize,
        selection: Option<usize>,
        focused: bool,
        multiline: bool,
        focus_handle: &FocusHandle,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let focused = focused && enabled;
        let is_empty = value.trim().is_empty();
        let border = if focused {
            theme::accent() // --focus-color: $blue
        } else {
            theme::surface_bg_alt() // --contrast-border in dark
        };

        // Build the display text with cursor
        let text_child: Div = if is_empty && !focused {
            // Placeholder (unfocused)
            div()
                .text_size(theme::z(12.0))
                .text_color(theme::text_muted())
                .child(placeholder.to_string())
        } else if is_empty && focused {
            // Placeholder with cursor (focused but empty)
            h_flex()
                .items_center()
                .text_size(theme::z(12.0))
                .child(
                    div()
                        .w(px(1.0))
                        .h(px(14.0))
                        .bg(theme::text_main())
                        .flex_shrink_0(),
                )
                .child(
                    div()
                        .text_color(theme::text_muted())
                        .child(placeholder.to_string()),
                )
        } else if focused {
            // Editable: show text with cursor and optional selection highlight
            let cursor_pos = cursor.min(value.len());
            let sel_highlight = theme::text_selection_bg();

            if let Some(sel_anchor) = selection {
                // Has selection: render before_sel + selected + after_sel with cursor
                let (sel_start, sel_end) = ordered_range(sel_anchor.min(value.len()), cursor_pos);
                let before_sel = &value[..sel_start];
                let selected = &value[sel_start..sel_end];
                let after_sel = &value[sel_end..];

                let nowrap = !multiline;
                let mut row = if multiline {
                    h_flex().items_start().text_size(theme::z(12.0)).flex_wrap()
                } else {
                    h_flex()
                        .items_center()
                        .overflow_x_hidden()
                        .text_size(theme::z(12.0))
                };

                if !before_sel.is_empty() {
                    let mut el = div()
                        .text_color(theme::text_main())
                        .child(before_sel.to_string());
                    if nowrap {
                        el = el.whitespace_nowrap();
                    }
                    row = row.child(el);
                }

                // Cursor at start of selection (if cursor < anchor)
                if cursor_pos == sel_start {
                    row = row.child(
                        div()
                            .w(px(1.0))
                            .h(px(14.0))
                            .bg(theme::text_main())
                            .flex_shrink_0(),
                    );
                }

                // Selected text with highlight
                if !selected.is_empty() {
                    let mut el = div()
                        .text_color(gpui::white())
                        .bg(sel_highlight)
                        .child(selected.to_string());
                    if nowrap {
                        el = el.whitespace_nowrap();
                    }
                    row = row.child(el);
                }

                // Cursor at end of selection (if cursor > anchor)
                if cursor_pos == sel_end {
                    row = row.child(
                        div()
                            .w(px(1.0))
                            .h(px(14.0))
                            .bg(theme::text_main())
                            .flex_shrink_0(),
                    );
                }

                if !after_sel.is_empty() {
                    let mut el = div()
                        .text_color(theme::text_main())
                        .child(after_sel.to_string());
                    if nowrap {
                        el = el.whitespace_nowrap();
                    }
                    row = row.child(el);
                }
                row
            } else {
                // No selection: just cursor
                let before = &value[..cursor_pos];
                let after = &value[cursor_pos..];

                if multiline {
                    let mut row = h_flex().items_start().text_size(theme::z(12.0));
                    row = row.child(
                        div()
                            .text_color(theme::text_main())
                            .child(before.to_string()),
                    );
                    row = row.child(
                        div()
                            .w(px(1.0))
                            .h(px(14.0))
                            .bg(theme::text_main())
                            .flex_shrink_0(),
                    );
                    row = row.child(
                        div()
                            .text_color(theme::text_main())
                            .child(after.to_string()),
                    );
                    row
                } else {
                    h_flex()
                        .items_center()
                        .overflow_x_hidden()
                        .text_size(theme::z(12.0))
                        .child(
                            div()
                                .text_color(theme::text_main())
                                .whitespace_nowrap()
                                .child(before.to_string()),
                        )
                        .child(
                            div()
                                .w(px(1.0))
                                .h(px(14.0))
                                .bg(theme::text_main())
                                .flex_shrink_0(),
                        )
                        .child(
                            div()
                                .text_color(theme::text_main())
                                .whitespace_nowrap()
                                .child(after.to_string()),
                        )
                }
            }
        } else {
            // Has text, not focused
            if multiline {
                div()
                    .text_size(theme::z(12.0))
                    .text_color(if enabled {
                        theme::text_main()
                    } else {
                        theme::text_muted()
                    })
                    .child(value.to_string())
            } else {
                div()
                    .text_size(theme::z(12.0))
                    .text_color(if enabled {
                        theme::text_main()
                    } else {
                        theme::text_muted()
                    })
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .child(value.to_string())
            }
        };

        let is_summary = id == "commit-summary-field";

        let mut field = if multiline {
            // Description: scrollable container with inner content that grows
            div()
                .id(SharedString::from(id.to_string()))
                .w_full()
                .h(px(80.0))
                .bg(theme::bg())
                .border_1()
                .border_color(border)
                .rounded_t(theme::z(theme::CORNER_RADIUS))
                .rounded_b_none()
                .border_b_0()
                .overflow_y_scroll()
                .when(!enabled, |s| s.opacity(0.65))
                .when(enabled, |s| {
                    s.track_focus(focus_handle)
                        .key_context("text-field")
                        .cursor_text()
                })
                .child(div().w_full().px(px(8.0)).py(px(6.0)).child(text_child))
        } else {
            // Summary: single line, vertically centered
            div()
                .id(SharedString::from(id.to_string()))
                .w_full()
                .h(px(25.0))
                .flex()
                .items_center()
                .bg(theme::bg())
                .border_1()
                .border_color(border)
                .px(px(8.0))
                .rounded(theme::z(theme::CORNER_RADIUS))
                .when(!enabled, |s| s.opacity(0.65))
                .when(enabled, |s| {
                    s.track_focus(focus_handle)
                        .key_context("text-field")
                        .cursor_text()
                })
                .child(text_child)
        };

        if !enabled {
            return field;
        }

        if is_summary {
            field = field
                .on_key_down(cx.listener(Self::handle_summary_key))
                .on_click(cx.listener(|app, evt: &ClickEvent, _win, cx| {
                    if evt.click_count() == 2 && !app.commit.summary.is_empty() {
                        // Select all on double-click
                        app.summary_selection = Some(0);
                        app.summary_cursor = app.commit.summary.len();
                        cx.notify();
                    }
                }));
        } else {
            field = field
                .on_key_down(cx.listener(Self::handle_description_key))
                .on_click(cx.listener(|app, evt: &ClickEvent, _win, cx| {
                    if evt.click_count() == 2 && !app.commit.body.is_empty() {
                        // Select all on double-click
                        app.description_selection = Some(0);
                        app.description_cursor = app.commit.body.len();
                        cx.notify();
                    }
                }));
        }

        field
    }

    // ------------------------------------------------------------------
    // Commit form (interactive)
    // ------------------------------------------------------------------

    fn render_commit_form_interactive(
        &self,
        branch_name: &str,
        summary_focused: bool,
        description_focused: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let file_count = self.commit_file_count();
        let commit_inputs_enabled = self.repo.snapshot.is_some();
        let can_generate_ai =
            self.repo.snapshot.is_some() && file_count > 0 && !self.commit.ai_in_flight;

        // Action bar buttons (below description, matching GitHub Desktop layout)
        let action_bar_btn = |id: &str, icon: IconName| -> Stateful<Div> {
            div()
                .id(SharedString::from(id.to_string()))
                .flex_shrink_0()
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover_bg()))
                .rounded(px(3.0))
                .w(px(18.0))
                .h(px(17.0))
                .items_center()
                .justify_center()
                .child(
                    Icon::new(icon)
                        .size(px(14.0))
                        .text_color(theme::text_muted()),
                )
        };

        let sparkle_button = div()
            .id("ai-generate-btn")
            .flex_shrink_0()
            .rounded(px(3.0))
            .w(px(18.0))
            .h(px(17.0))
            .items_center()
            .justify_center()
            .when(!can_generate_ai, |s| s.opacity(0.45))
            .when(can_generate_ai, |s| {
                s.cursor_pointer().hover(|s| s.bg(theme::hover_bg()))
            })
            .child(
                svg()
                    .path("icons/sparkles.svg")
                    .size(px(14.0))
                    .text_color(theme::text_muted()),
            )
            .on_click(cx.listener(|app, _evt, _win, cx| {
                if app.repo.snapshot.is_none()
                    || app.commit_file_count() == 0
                    || app.commit.ai_in_flight
                {
                    return;
                }
                app.generate_ai_commit(cx);
            }));

        let settings_button = action_bar_btn("commit-settings-btn", IconName::Settings).on_click(
            cx.listener(|app, _evt, window, cx| {
                app.open_settings_modal(Some(crate::ui::ui_state::SettingsSection::Ai), cx);
                app.activate_settings_field(
                    settings_modal::default_settings_field(
                        crate::ui::ui_state::SettingsSection::Ai,
                    ),
                    window,
                    cx,
                );
            }),
        );

        // Action bar — sits below the description textarea
        let action_bar = h_flex()
            .w_full()
            .h(px(26.0))
            .px(px(5.0))
            .items_center()
            .gap(px(2.0))
            .bg(theme::surface_bg())
            .border_1()
            .border_t_0()
            .border_color(theme::surface_bg_alt())
            .rounded_b(theme::z(theme::CORNER_RADIUS))
            .child(sparkle_button)
            .child(
                div()
                    .w(px(1.0))
                    .h(px(12.0))
                    .bg(theme::surface_bg_alt())
                    .mx(px(2.0)),
            )
            .child(settings_button);

        // Auto-placeholder: "Update filename" for a single included file, else generic
        let summary_placeholder = if self.commit.summary.is_empty() {
            self.default_commit_summary()
                .unwrap_or_else(|| "Summary (required)".to_string())
        } else {
            "Summary (required)".to_string()
        };

        // Summary field — editable single-line input
        let summary_field = self.render_text_field(
            "commit-summary-field",
            &self.commit.summary,
            &summary_placeholder,
            self.summary_cursor,
            self.summary_selection,
            summary_focused,
            false,
            &self.summary_focus,
            commit_inputs_enabled,
            cx,
        );

        // Description field — editable multi-line input (no bottom radius, action bar attaches)
        let description_field = self.render_text_field(
            "commit-description-field",
            &self.commit.body,
            "Description",
            self.description_cursor,
            self.description_selection,
            description_focused,
            true,
            &self.description_focus,
            commit_inputs_enabled,
            cx,
        );

        // Description + action bar grouped together (shared border)
        let description_group = v_flex().w_full().child(description_field).child(action_bar);

        let commit_label = if self.commit.ai_in_flight {
            "Generating commit details\u{2026}".to_string()
        } else if file_count > 0 {
            format!(
                "Commit {} to {branch_name}",
                crate::ui::labels::commit_files(file_count)
            )
        } else {
            format!("Commit to {branch_name}")
        };

        let can_commit = self.can_commit();
        let identity_warning = self
            .repo
            .snapshot
            .as_ref()
            .and_then(|_| self.missing_identity_message())
            .map(|message| {
                h_flex()
                    .id("commit-identity-warning")
                    .w_full()
                    .gap(px(8.0))
                    .items_center()
                    .px(px(8.0))
                    .py(px(7.0))
                    .rounded(theme::z(theme::CORNER_RADIUS))
                    .bg(theme::surface_bg())
                    .border_1()
                    .border_color(theme::warning())
                    .child(
                        Icon::new(IconName::TriangleAlert)
                            .size(px(13.0))
                            .text_color(theme::warning()),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .text_size(px(12.0))
                            .text_color(theme::text_main())
                            .child(message),
                    )
                    .child(
                        div()
                            .id("commit-identity-settings")
                            .px(px(8.0))
                            .py(px(4.0))
                            .rounded(px(3.0))
                            .bg(theme::surface_bg_alt())
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::toolbar_hover_bg()))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme::text_main())
                                    .child("Git Settings"),
                            )
                            .on_click(cx.listener(|app, _evt, window, cx| {
                                let field = app.identity_settings_focus_field();
                                app.open_identity_settings_from_warning(cx);
                                app.activate_settings_field(field, window, cx);
                            })),
                    )
            });

        // Summary length hint (> 50 chars)
        let summary_hint = if self.commit.summary.len() > 50 {
            div()
                .flex_shrink_0()
                .child(
                    Icon::new(IconName::Info)
                        .size(px(12.0))
                        .text_color(theme::warning()),
                )
                .into_any_element()
        } else {
            div().into_any_element()
        };

        v_flex()
            .w_full()
            .border_t_1()
            .border_color(theme::toolbar_button_border())
            .bg(theme::panel_bg())
            .p(px(10.0))
            .gap(px(10.0))
            .child(
                h_flex()
                    .gap(px(4.0))
                    .child(summary_field)
                    .child(summary_hint),
            )
            .child(description_group)
            .when_some(identity_warning, |this, warning| this.child(warning))
            .child(
                Button::new("commit-btn")
                    .label(commit_label)
                    .small()
                    .disabled(!can_commit)
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .color(if can_commit {
                                theme::commit_button_bg()
                            } else {
                                theme::surface_bg_alt()
                            })
                            .foreground(if can_commit {
                                theme::commit_button_text()
                            } else {
                                theme::text_muted()
                            })
                            .hover(theme::commit_button_hover_bg())
                            .active(theme::commit_button_hover_bg()),
                    )
                    .on_click(cx.listener(|app, _evt, _win, cx| {
                        app.commit_all(cx);
                    })),
            )
    }

    // ------------------------------------------------------------------
    // Workspace
    // ------------------------------------------------------------------

    fn render_workspace(&self, view: Entity<Self>, cx: &mut Context<Self>) -> AnyElement {
        let sidebar_tab = self.nav.sidebar_tab;

        if self.repo.snapshot.is_none() {
            return h_resizable("workspace-panels")
                .child(
                    resizable_panel()
                        .child(crate::ui::sidebar::render_no_repository_state(&view, cx)),
                )
                .into_any_element();
        }

        // Determine the active file list and selected file based on tab.
        let (diffs, selected_file): (&[DiffEntry], Option<&str>) = match sidebar_tab {
            SidebarTab::Changes => {
                let diffs = self
                    .repo
                    .snapshot
                    .as_ref()
                    .map(|s| s.diffs.as_slice())
                    .unwrap_or(&[]);
                let sel = self.selection.selected_change.as_deref();
                (diffs, sel)
            }
            SidebarTab::History => {
                let diffs = self.selection.commit_diffs.as_deref().unwrap_or(&[]);
                let sel = self.selection.selected_commit_file.as_deref();
                (diffs, sel)
            }
        };

        // Find the diff entry for the selected file.
        let selected_diff = selected_file.and_then(|path| diffs.iter().find(|d| d.path == path));

        // No local changes — show suggestion cards in the workspace
        if sidebar_tab == SidebarTab::Changes && diffs.is_empty() {
            let snapshot = self.repo.snapshot.as_ref();
            let ahead = snapshot.map(|s| s.repo.ahead).unwrap_or(0);
            let behind = snapshot.map(|s| s.repo.behind).unwrap_or(0);
            let remote = snapshot.and_then(|s| s.repo.remote_name.as_deref());
            let has_github_remote = snapshot.map(|s| s.repo.has_github_remote).unwrap_or(false);
            return h_resizable("workspace-panels")
                .child(
                    resizable_panel().child(crate::ui::sidebar::render_no_changes_state(
                        &view,
                        ahead,
                        behind,
                        remote,
                        has_github_remote,
                        cx,
                    )),
                )
                .into_any_element();
        }

        // Show file list panel on History tab (Changes tab has sidebar file list)
        if sidebar_tab == SidebarTab::History {
            let selected_commit = self.repo.snapshot.as_ref().and_then(|snapshot| {
                self.selection
                    .selected_commit
                    .as_deref()
                    .and_then(|oid| snapshot.history.iter().find(|commit| commit.oid == oid))
            });
            let commit_header = self.render_commit_detail_header(selected_commit, diffs, cx);
            let file_list = self.render_commit_file_list(diffs, selected_file, sidebar_tab, cx);

            v_flex()
                .size_full()
                .min_h_0()
                .child(commit_header)
                .child(
                    div().w_full().flex_1().min_h_0().child(
                        h_resizable("workspace-panels")
                            .child(
                                resizable_panel()
                                    .size(px(200.0))
                                    .size_range(px(120.0)..px(350.0))
                                    .child(file_list),
                            )
                            .child(resizable_panel().child(
                                crate::ui::workspace::render_workspace(
                                    selected_file,
                                    selected_diff,
                                    None, // History diffs are read-only, no expand controls
                                ),
                            )),
                    ),
                )
                .into_any_element()
        } else {
            h_resizable("workspace-panels")
                .child(
                    resizable_panel().child(crate::ui::workspace::render_workspace(
                        selected_file,
                        selected_diff,
                        Some(&view),
                    )),
                )
                .into_any_element()
        }
    }

    fn render_commit_detail_header(
        &self,
        commit: Option<&CommitInfo>,
        diffs: &[DiffEntry],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(commit) = commit else {
            return div()
                .id("history-commit-detail-header")
                .h(px(0.0))
                .into_any_element();
        };

        let oid = commit.oid.clone();
        let short_oid = commit.short_oid.clone();
        let (added, deleted) = diff_line_stats(diffs);

        h_flex()
            .id("history-commit-detail-header")
            .w_full()
            .h(px(58.0))
            .flex_shrink_0()
            .px(px(12.0))
            .py(px(8.0))
            .items_center()
            .justify_between()
            .bg(theme::panel_bg())
            .border_b_1()
            .border_color(theme::border())
            .child(
                v_flex()
                    .min_w_0()
                    .gap(px(4.0))
                    .child(
                        h_flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .min_w_0()
                                    .text_size(theme::z(13.0))
                                    .text_color(theme::text_main())
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .whitespace_nowrap()
                                    .overflow_x_hidden()
                                    .child(commit.summary.clone()),
                            )
                            .children(if commit.is_head {
                                Some(Tag::primary().xsmall().child("HEAD"))
                            } else {
                                None
                            }),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_size(theme::z(12.0))
                                    .text_color(theme::text_muted())
                                    .whitespace_nowrap()
                                    .child(commit.author_name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(theme::z(12.0))
                                    .text_color(theme::text_muted())
                                    .child(short_oid.clone()),
                            )
                            .child(
                                div()
                                    .id("history-copy-sha-button")
                                    .px(px(5.0))
                                    .py(px(2.0))
                                    .rounded(theme::z(theme::CORNER_RADIUS))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::toolbar_hover_bg()))
                                    .on_click(cx.listener(move |app, _evt, _win, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            oid.clone(),
                                        ));
                                        app.messages.status_message =
                                            "Copied commit SHA.".to_string();
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .text_size(theme::z(12.0))
                                            .text_color(theme::text_muted())
                                            .child("⧉"),
                                    ),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(theme::z(12.0))
                            .text_color(theme::success())
                            .child(format!("+{added}")),
                    )
                    .child(
                        div()
                            .text_size(theme::z(12.0))
                            .text_color(theme::danger())
                            .child(format!("-{deleted}")),
                    ),
            )
            .into_any_element()
    }

    fn render_commit_file_list(
        &self,
        diffs: &[DiffEntry],
        selected_file: Option<&str>,
        tab: SidebarTab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = cx.entity().clone();
        let diffs_snapshot: Vec<DiffEntry> = diffs.iter().cloned().collect();
        let sel = selected_file.map(String::from);

        let file_list = uniform_list(
            "commit-file-list",
            diffs_snapshot.len(),
            move |range, _win, _cx| {
                let sel = sel.clone();
                range
                    .map(|ix| {
                        let entry = &diffs_snapshot[ix];
                        let is_selected = sel.as_deref() == Some(entry.path.as_str());
                        let text_color = if is_selected {
                            gpui::white().into()
                        } else {
                            theme::text_main()
                        };

                        let path = entry.path.clone();
                        let id_path = stable_id_slug(&entry.path);
                        let vh = view.clone();

                        h_flex()
                            .id(SharedString::from(format!("commit-file-{id_path}")))
                            .w_full()
                            .h(theme::z(28.0))
                            .px(theme::z(10.0))
                            .items_center()
                            .bg(if is_selected {
                                theme::accent()
                            } else {
                                gpui::transparent_black()
                            })
                            // Blue left border for selected file
                            .border_l_2()
                            .border_color(if is_selected {
                                theme::accent()
                            } else {
                                gpui::transparent_black()
                            })
                            .cursor_pointer()
                            .hover(move |s| {
                                s.bg(if is_selected {
                                    theme::accent()
                                } else {
                                    theme::list_hover_bg()
                                })
                            })
                            .on_click(move |_evt, _win, cx| {
                                let path = path.clone();
                                vh.update(cx, |app, cx| {
                                    match tab {
                                        SidebarTab::Changes => {
                                            app.selection.selected_change = Some(path);
                                        }
                                        SidebarTab::History => {
                                            app.selection.selected_commit_file = Some(path);
                                        }
                                    }
                                    cx.notify();
                                });
                            })
                            .child(
                                div().flex_1().overflow_x_hidden().child(
                                    div()
                                        .text_size(theme::z(12.0))
                                        .text_color(text_color)
                                        .whitespace_nowrap()
                                        .child(entry.path.clone()),
                                ),
                            )
                    })
                    .collect()
            },
        )
        .w_full()
        .with_sizing_behavior(ListSizingBehavior::Infer);

        v_flex()
            .size_full()
            .items_start()
            .bg(theme::panel_bg())
            .border_r_1()
            .border_color(theme::border())
            .child(
                h_flex()
                    .w_full()
                    .h(theme::z(32.0))
                    .px(theme::z(10.0))
                    .items_center()
                    .bg(theme::surface_bg_muted())
                    .border_b_1()
                    .border_color(theme::border())
                    .child(
                        div()
                            .text_size(theme::z(12.0))
                            .text_color(theme::text_muted())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(crate::ui::labels::changed_files(diffs.len())),
                    ),
            )
            .child(
                div()
                    .id("commit-file-list-viewport")
                    .w_full()
                    .flex_1()
                    .overflow_hidden()
                    .child(file_list),
            )
    }

    // ------------------------------------------------------------------
    // Status bar
    // ------------------------------------------------------------------

    fn render_status_bar(&self) -> impl IntoElement {
        let branch = self
            .repo
            .snapshot
            .as_ref()
            .map(|s| s.repo.current_branch.as_str());
        let change_count = self
            .repo
            .snapshot
            .as_ref()
            .map(|s| s.changes.len())
            .unwrap_or(0);
        crate::ui::status_bar::render_status_bar(
            &self.messages.status_message,
            &self.messages.error_message,
            branch,
            change_count,
        )
    }

    // ------------------------------------------------------------------
    // Repo selector overlay
    // ------------------------------------------------------------------

    fn render_active_dialog(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        // Backdrop
        let backdrop = div()
            .id("dialog-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .bg(gpui::hsla(0.0, 0.0, 0.0, 0.5))
            .on_click(cx.listener(|app, _evt, _win, cx| {
                app.nav.active_dialog = ActiveDialog::None;
                cx.notify();
            }));

        let (dialog_width, dialog_height) = match &self.nav.active_dialog {
            ActiveDialog::CreateBranch => (400.0, 230.0),
            ActiveDialog::RenameBranch { .. } => (400.0, 230.0),
            ActiveDialog::DeleteBranch { .. } => (440.0, 220.0),
            ActiveDialog::CreateTag { .. } => (400.0, 230.0),
            ActiveDialog::DeleteTag { .. } => (420.0, 220.0),
            ActiveDialog::ResetToCommit { .. } => (500.0, 240.0),
            ActiveDialog::CreateRepository => (560.0, 390.0),
            ActiveDialog::CloneRepository => (560.0, 330.0),
            ActiveDialog::DiscardChanges { .. } => (420.0, 230.0),
            ActiveDialog::StashAndSwitch { .. } => (576.0, 360.0),
            ActiveDialog::StashChanges => (500.0, 360.0),
            ActiveDialog::RestoreStash => (500.0, 360.0),
            ActiveDialog::DiscardStash => (500.0, 400.0),
            ActiveDialog::PublishRepository => (
                crate::ui::publish_dialog::PUBLISH_DIALOG_WIDTH,
                crate::ui::publish_dialog::PUBLISH_DIALOG_HEIGHT,
            ),
            ActiveDialog::None => (0.0, 0.0),
        };
        let bounds = window.bounds();
        let window_width = bounds.size.width / px(1.0);
        let window_height = bounds.size.height / px(1.0);
        let dialog_left = ((window_width - dialog_width) / 2.0).max(16.0);
        let dialog_top = ((window_height - dialog_height) / 2.0).max(16.0);

        let dialog_content = match &self.nav.active_dialog {
            ActiveDialog::CreateBranch => {
                let branch_name = &self.repo.new_branch_name;
                let validation_message = self.create_branch_validation_message();
                let show_validation_message = !branch_name.trim().is_empty();
                let can_create = self.can_create_branch_from_dialog();
                let current = self
                    .repo
                    .snapshot
                    .as_ref()
                    .map(|s| s.repo.current_branch.as_str())
                    .unwrap_or("main");
                let starting_point = self
                    .repo
                    .new_branch_start_point
                    .as_deref()
                    .map(|oid| format!("Based on commit: {}", short_commit_label(oid)))
                    .unwrap_or_else(|| format!("Based on current branch: {current}"));

                v_flex()
                    .w(px(400.0))
                    .bg(theme::panel_bg())
                    .rounded(theme::z(theme::CORNER_RADIUS))
                    .border_1()
                    .border_color(theme::border())
                    .shadow_lg()
                    // Header
                    .child(
                        h_flex()
                            .w_full()
                            .px(theme::z(16.0))
                            .py(theme::z(12.0))
                            .items_center()
                            .justify_between()
                            .border_b_1()
                            .border_color(theme::border())
                            .child(
                                div()
                                    .text_size(theme::z(14.0))
                                    .text_color(theme::text_main())
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Create a Branch"),
                            )
                            .child(
                                div()
                                    .id("dialog-close")
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::hover_bg()))
                                    .rounded(px(4.0))
                                    .p(px(4.0))
                                    .child(
                                        Icon::new(IconName::Close)
                                            .size(px(14.0))
                                            .text_color(theme::text_muted()),
                                    )
                                    .on_click(cx.listener(|app, _evt, _win, cx| {
                                        app.nav.active_dialog = ActiveDialog::None;
                                        cx.notify();
                                    })),
                            ),
                    )
                    // Body
                    .child(
                        v_flex()
                            .w_full()
                            .p(theme::z(16.0))
                            .gap(theme::z(12.0))
                            // Name field
                            .child(
                                v_flex()
                                    .gap(theme::z(4.0))
                                    .child(
                                        div()
                                            .text_size(theme::z(12.0))
                                            .text_color(theme::text_muted())
                                            .child("Name"),
                                    )
                                    .child(
                                        div()
                                            .id("new-branch-name-input")
                                            .track_focus(&self.new_branch_focus)
                                            .key_context("text-field")
                                            .on_key_down(cx.listener(Self::handle_new_branch_key))
                                            .w_full()
                                            .h(theme::z(28.0))
                                            .px(theme::z(8.0))
                                            .flex()
                                            .items_center()
                                            .rounded(theme::z(theme::CORNER_RADIUS))
                                            .bg(theme::bg())
                                            .border_1()
                                            .border_color(theme::accent())
                                            .cursor_text()
                                            .child(
                                                div()
                                                    .text_size(theme::z(12.0))
                                                    .text_color(if branch_name.is_empty() {
                                                        theme::text_muted()
                                                    } else {
                                                        theme::text_main()
                                                    })
                                                    .child(if branch_name.is_empty() {
                                                        "branch-name".to_string()
                                                    } else {
                                                        branch_name.clone()
                                                    }),
                                            )
                                            .on_click(cx.listener(|app, _evt, window, cx| {
                                                window.focus(&app.new_branch_focus);
                                                app.new_branch_cursor =
                                                    app.repo.new_branch_name.len();
                                                app.new_branch_selection = None;
                                                cx.notify();
                                            })),
                                    ),
                            )
                            // Starting point
                            .child(
                                div()
                                    .text_size(theme::z(11.0))
                                    .text_color(theme::text_muted())
                                    .child(starting_point),
                            )
                            .children(
                                validation_message
                                    .as_ref()
                                    .filter(|_| show_validation_message)
                                    .map(|message| {
                                        div()
                                            .id("create-branch-validation-message")
                                            .text_size(theme::z(11.0))
                                            .text_color(branch_validation_message_color(message))
                                            .child(message.clone())
                                    }),
                            ),
                    )
                    // Footer
                    .child(
                        h_flex()
                            .w_full()
                            .px(theme::z(16.0))
                            .py(theme::z(12.0))
                            .justify_end()
                            .gap(theme::z(8.0))
                            .border_t_1()
                            .border_color(theme::border())
                            .child(
                                div()
                                    .id("dialog-cancel")
                                    .px(theme::z(12.0))
                                    .py(theme::z(6.0))
                                    .rounded(theme::z(theme::CORNER_RADIUS))
                                    .bg(theme::surface_bg())
                                    .border_1()
                                    .border_color(theme::surface_bg_alt())
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::toolbar_hover_bg()))
                                    .child(
                                        div()
                                            .text_size(theme::z(12.0))
                                            .text_color(theme::text_main())
                                            .child("Cancel"),
                                    )
                                    .on_click(cx.listener(|app, _evt, _win, cx| {
                                        app.nav.active_dialog = ActiveDialog::None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("dialog-create-branch")
                                    .px(theme::z(12.0))
                                    .py(theme::z(6.0))
                                    .rounded(theme::z(theme::CORNER_RADIUS))
                                    .bg(if can_create {
                                        theme::commit_button_bg()
                                    } else {
                                        theme::surface_bg()
                                    })
                                    .border_1()
                                    .border_color(if can_create {
                                        theme::commit_button_bg()
                                    } else {
                                        theme::surface_bg_alt()
                                    })
                                    .when(can_create, |el| {
                                        el.cursor_pointer()
                                            .hover(|s| s.bg(theme::commit_button_hover_bg()))
                                    })
                                    .child(
                                        div()
                                            .text_size(theme::z(12.0))
                                            .text_color(if can_create {
                                                theme::commit_button_text()
                                            } else {
                                                theme::text_muted()
                                            })
                                            .child("Create Branch"),
                                    )
                                    .on_click(cx.listener(|app, _evt, _win, cx| {
                                        if !app.can_create_branch_from_dialog() {
                                            cx.notify();
                                            return;
                                        }
                                        app.create_branch(cx);
                                    })),
                            ),
                    )
            }
            ActiveDialog::RenameBranch { old_name } => {
                let branch_name = &self.repo.new_branch_name;
                let old_name_for_click = old_name.clone();
                let validation_message = self.rename_branch_validation_message(old_name);
                let show_validation_message =
                    !branch_name.trim().is_empty() && branch_name.trim() != old_name;
                let can_rename = self.can_rename_branch_from_dialog(old_name);

                v_flex()
                    .w(px(400.0))
                    .bg(theme::panel_bg())
                    .rounded(theme::z(theme::CORNER_RADIUS))
                    .border_1()
                    .border_color(theme::border())
                    .shadow_lg()
                    .child(
                        h_flex()
                            .w_full()
                            .px(theme::z(16.0))
                            .py(theme::z(12.0))
                            .items_center()
                            .justify_between()
                            .border_b_1()
                            .border_color(theme::border())
                            .child(
                                div()
                                    .text_size(theme::z(14.0))
                                    .text_color(theme::text_main())
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Rename Branch"),
                            )
                            .child(
                                div()
                                    .id("rename-branch-close")
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::hover_bg()))
                                    .rounded(px(4.0))
                                    .p(px(4.0))
                                    .child(
                                        Icon::new(IconName::Close)
                                            .size(px(14.0))
                                            .text_color(theme::text_muted()),
                                    )
                                    .on_click(cx.listener(|app, _evt, _win, cx| {
                                        app.nav.active_dialog = ActiveDialog::None;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        v_flex()
                            .w_full()
                            .p(theme::z(16.0))
                            .gap(theme::z(12.0))
                            .child(
                                v_flex()
                                    .gap(theme::z(4.0))
                                    .child(
                                        div()
                                            .text_size(theme::z(12.0))
                                            .text_color(theme::text_muted())
                                            .child("Name"),
                                    )
                                    .child(
                                        div()
                                            .id("rename-branch-name-input")
                                            .track_focus(&self.new_branch_focus)
                                            .key_context("text-field")
                                            .on_key_down(cx.listener(Self::handle_new_branch_key))
                                            .w_full()
                                            .h(theme::z(28.0))
                                            .px(theme::z(8.0))
                                            .flex()
                                            .items_center()
                                            .rounded(theme::z(theme::CORNER_RADIUS))
                                            .bg(theme::bg())
                                            .border_1()
                                            .border_color(theme::accent())
                                            .cursor_text()
                                            .child(
                                                div()
                                                    .text_size(theme::z(12.0))
                                                    .text_color(if branch_name.is_empty() {
                                                        theme::text_muted()
                                                    } else {
                                                        theme::text_main()
                                                    })
                                                    .child(if branch_name.is_empty() {
                                                        "branch-name".to_string()
                                                    } else {
                                                        branch_name.clone()
                                                    }),
                                            )
                                            .on_click(cx.listener(|app, _evt, window, cx| {
                                                window.focus(&app.new_branch_focus);
                                                app.new_branch_cursor =
                                                    app.repo.new_branch_name.len();
                                                app.new_branch_selection = None;
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(theme::z(11.0))
                                    .text_color(theme::text_muted())
                                    .child(format!("Current branch name: {old_name}")),
                            ),
                    )
                    .children(
                        validation_message
                            .as_ref()
                            .filter(|_| show_validation_message)
                            .map(|message| {
                                div()
                                    .id("rename-branch-validation-message")
                                    .px(theme::z(16.0))
                                    .pb(theme::z(12.0))
                                    .text_size(theme::z(11.0))
                                    .text_color(branch_validation_message_color(message))
                                    .child(message.clone())
                            }),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .px(theme::z(16.0))
                            .py(theme::z(12.0))
                            .justify_end()
                            .gap(theme::z(8.0))
                            .border_t_1()
                            .border_color(theme::border())
                            .child(
                                div()
                                    .id("rename-branch-cancel")
                                    .px(theme::z(12.0))
                                    .py(theme::z(6.0))
                                    .rounded(theme::z(theme::CORNER_RADIUS))
                                    .bg(theme::surface_bg())
                                    .border_1()
                                    .border_color(theme::surface_bg_alt())
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::toolbar_hover_bg()))
                                    .child(
                                        div()
                                            .text_size(theme::z(12.0))
                                            .text_color(theme::text_main())
                                            .child("Cancel"),
                                    )
                                    .on_click(cx.listener(|app, _evt, _win, cx| {
                                        app.nav.active_dialog = ActiveDialog::None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("rename-branch-confirm")
                                    .px(theme::z(12.0))
                                    .py(theme::z(6.0))
                                    .rounded(theme::z(theme::CORNER_RADIUS))
                                    .bg(if can_rename {
                                        theme::commit_button_bg()
                                    } else {
                                        theme::surface_bg()
                                    })
                                    .border_1()
                                    .border_color(if can_rename {
                                        theme::commit_button_bg()
                                    } else {
                                        theme::surface_bg_alt()
                                    })
                                    .when(can_rename, |el| {
                                        el.cursor_pointer()
                                            .hover(|s| s.bg(theme::commit_button_hover_bg()))
                                    })
                                    .child(
                                        div()
                                            .text_size(theme::z(12.0))
                                            .text_color(if can_rename {
                                                theme::commit_button_text()
                                            } else {
                                                theme::text_muted()
                                            })
                                            .child("Rename Branch"),
                                    )
                                    .on_click(cx.listener(move |app, _evt, _win, cx| {
                                        if !app.can_rename_branch_from_dialog(&old_name_for_click) {
                                            cx.notify();
                                            return;
                                        }
                                        app.rename_branch(old_name_for_click.clone(), cx);
                                    })),
                            ),
                    )
            }
            ActiveDialog::DeleteBranch { branch_name } => {
                crate::ui::delete_branch_dialog::render_delete_branch_dialog(branch_name, cx)
            }
            ActiveDialog::StashChanges => {
                let files = self
                    .repo
                    .snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.changes.clone())
                    .unwrap_or_default();
                crate::ui::stash_changes_dialog::render_stash_changes_dialog(Arc::new(files), cx)
            }
            ActiveDialog::DiscardStash => {
                crate::ui::discard_stash_dialog::render_discard_stash_dialog(
                    Arc::new(self.repo.stash_files.clone()),
                    cx,
                )
            }
            ActiveDialog::CreateTag { target_oid } => {
                let tag_name = &self.repo.new_branch_name;
                let target_oid_for_click = target_oid.clone();
                let short_oid = short_commit_label(target_oid);
                let validation_message = self.create_tag_validation_message();
                let show_validation_message = !tag_name.trim().is_empty();
                let can_create = validation_message.is_none();

                v_flex()
                    .w(px(400.0))
                    .bg(theme::panel_bg())
                    .rounded(theme::z(theme::CORNER_RADIUS))
                    .border_1()
                    .border_color(theme::border())
                    .shadow_lg()
                    .child(
                        h_flex()
                            .w_full()
                            .px(theme::z(16.0))
                            .py(theme::z(12.0))
                            .items_center()
                            .justify_between()
                            .border_b_1()
                            .border_color(theme::border())
                            .child(
                                div()
                                    .text_size(theme::z(14.0))
                                    .text_color(theme::text_main())
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Create a Tag"),
                            )
                            .child(
                                div()
                                    .id("create-tag-close")
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::hover_bg()))
                                    .rounded(px(4.0))
                                    .p(px(4.0))
                                    .child(
                                        Icon::new(IconName::Close)
                                            .size(px(14.0))
                                            .text_color(theme::text_muted()),
                                    )
                                    .on_click(cx.listener(|app, _evt, _win, cx| {
                                        app.nav.active_dialog = ActiveDialog::None;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        v_flex()
                            .w_full()
                            .p(theme::z(16.0))
                            .gap(theme::z(12.0))
                            .child(
                                v_flex()
                                    .gap(theme::z(4.0))
                                    .child(
                                        div()
                                            .text_size(theme::z(12.0))
                                            .text_color(theme::text_muted())
                                            .child("Name"),
                                    )
                                    .child(
                                        div()
                                            .id("create-tag-name-input")
                                            .track_focus(&self.new_branch_focus)
                                            .key_context("text-field")
                                            .on_key_down(cx.listener(Self::handle_new_branch_key))
                                            .w_full()
                                            .h(theme::z(28.0))
                                            .px(theme::z(8.0))
                                            .flex()
                                            .items_center()
                                            .rounded(theme::z(theme::CORNER_RADIUS))
                                            .bg(theme::bg())
                                            .border_1()
                                            .border_color(theme::accent())
                                            .cursor_text()
                                            .child(
                                                div()
                                                    .text_size(theme::z(12.0))
                                                    .text_color(if tag_name.is_empty() {
                                                        theme::text_muted()
                                                    } else {
                                                        theme::text_main()
                                                    })
                                                    .child(if tag_name.is_empty() {
                                                        "v1.0.0".to_string()
                                                    } else {
                                                        tag_name.clone()
                                                    }),
                                            )
                                            .on_click(cx.listener(|app, _evt, window, cx| {
                                                window.focus(&app.new_branch_focus);
                                                app.new_branch_cursor =
                                                    app.repo.new_branch_name.len();
                                                app.new_branch_selection = None;
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(theme::z(11.0))
                                    .text_color(theme::text_muted())
                                    .child(format!("Target commit: {short_oid}")),
                            )
                            .children(
                                validation_message
                                    .as_ref()
                                    .filter(|_| show_validation_message)
                                    .map(|message| {
                                        div()
                                            .id("create-tag-validation-message")
                                            .text_size(theme::z(11.0))
                                            .text_color(theme::danger())
                                            .child(message.clone())
                                    }),
                            ),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .px(theme::z(16.0))
                            .py(theme::z(12.0))
                            .justify_end()
                            .gap(theme::z(8.0))
                            .border_t_1()
                            .border_color(theme::border())
                            .child(
                                div()
                                    .id("create-tag-cancel")
                                    .px(theme::z(12.0))
                                    .py(theme::z(6.0))
                                    .rounded(theme::z(theme::CORNER_RADIUS))
                                    .bg(theme::surface_bg())
                                    .border_1()
                                    .border_color(theme::surface_bg_alt())
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::toolbar_hover_bg()))
                                    .child(
                                        div()
                                            .text_size(theme::z(12.0))
                                            .text_color(theme::text_main())
                                            .child("Cancel"),
                                    )
                                    .on_click(cx.listener(|app, _evt, _win, cx| {
                                        app.nav.active_dialog = ActiveDialog::None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("create-tag-confirm")
                                    .px(theme::z(12.0))
                                    .py(theme::z(6.0))
                                    .rounded(theme::z(theme::CORNER_RADIUS))
                                    .bg(if can_create {
                                        theme::commit_button_bg()
                                    } else {
                                        theme::surface_bg()
                                    })
                                    .border_1()
                                    .border_color(if can_create {
                                        theme::commit_button_bg()
                                    } else {
                                        theme::surface_bg_alt()
                                    })
                                    .when(can_create, |el| {
                                        el.cursor_pointer()
                                            .hover(|s| s.bg(theme::commit_button_hover_bg()))
                                    })
                                    .child(
                                        div()
                                            .text_size(theme::z(12.0))
                                            .text_color(if can_create {
                                                theme::commit_button_text()
                                            } else {
                                                theme::text_muted()
                                            })
                                            .child("Create Tag"),
                                    )
                                    .on_click(cx.listener(move |app, _evt, _win, cx| {
                                        if app.create_tag_validation_message().is_some() {
                                            cx.notify();
                                            return;
                                        }
                                        app.create_tag(target_oid_for_click.clone(), cx);
                                    })),
                            ),
                    )
            }
            ActiveDialog::DeleteTag { tag_name } => {
                let tag_name_for_click = tag_name.clone();
                v_flex()
                    .w(px(420.0))
                    .bg(theme::panel_bg())
                    .rounded(theme::z(theme::CORNER_RADIUS))
                    .border_1()
                    .border_color(theme::border())
                    .shadow_lg()
                    .child(
                        h_flex()
                            .w_full()
                            .px(theme::z(16.0))
                            .py(theme::z(12.0))
                            .items_center()
                            .justify_between()
                            .border_b_1()
                            .border_color(theme::border())
                            .child(
                                div()
                                    .text_size(theme::z(14.0))
                                    .text_color(theme::text_main())
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Delete Tag"),
                            )
                            .child(
                                div()
                                    .id("delete-tag-close")
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::hover_bg()))
                                    .rounded(px(4.0))
                                    .p(px(4.0))
                                    .child(
                                        Icon::new(IconName::Close)
                                            .size(px(14.0))
                                            .text_color(theme::text_muted()),
                                    )
                                    .on_click(cx.listener(|app, _evt, _win, cx| {
                                        app.nav.active_dialog = ActiveDialog::None;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        v_flex()
                            .w_full()
                            .p(theme::z(16.0))
                            .gap(theme::z(10.0))
                            .child(
                                div()
                                    .id("delete-tag-confirmation")
                                    .text_size(theme::z(12.0))
                                    .line_height(theme::z(18.0))
                                    .text_color(theme::text_main())
                                    .child(format!(
                                        "Are you sure you want to delete the tag '{tag_name}'?"
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(theme::z(11.0))
                                    .text_color(theme::text_muted())
                                    .child("This removes the local tag from this repository."),
                            ),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .px(theme::z(16.0))
                            .py(theme::z(12.0))
                            .justify_end()
                            .gap(theme::z(8.0))
                            .border_t_1()
                            .border_color(theme::border())
                            .child(
                                div()
                                    .id("delete-tag-cancel")
                                    .px(theme::z(12.0))
                                    .py(theme::z(6.0))
                                    .rounded(theme::z(theme::CORNER_RADIUS))
                                    .bg(theme::surface_bg())
                                    .border_1()
                                    .border_color(theme::surface_bg_alt())
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::toolbar_hover_bg()))
                                    .child(
                                        div()
                                            .text_size(theme::z(12.0))
                                            .text_color(theme::text_main())
                                            .child("Cancel"),
                                    )
                                    .on_click(cx.listener(|app, _evt, _win, cx| {
                                        app.nav.active_dialog = ActiveDialog::None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("delete-tag-confirm")
                                    .px(theme::z(12.0))
                                    .py(theme::z(6.0))
                                    .rounded(theme::z(theme::CORNER_RADIUS))
                                    .bg(theme::danger())
                                    .border_1()
                                    .border_color(theme::danger())
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::danger_hover()))
                                    .child(
                                        div()
                                            .text_size(theme::z(12.0))
                                            .text_color(theme::commit_button_text())
                                            .child("Delete"),
                                    )
                                    .on_click(cx.listener(move |app, _evt, _win, cx| {
                                        app.delete_tag(tag_name_for_click.clone(), cx);
                                    })),
                            ),
                    )
            }
            ActiveDialog::ResetToCommit { target_oid } => {
                crate::ui::reset_dialog::render_reset_to_commit_dialog(target_oid, cx)
            }
            ActiveDialog::CreateRepository => {
                crate::ui::repository_dialog::render_create_repository_dialog(self, window, cx)
            }
            ActiveDialog::CloneRepository => {
                crate::ui::repository_dialog::render_clone_repository_dialog(self, window, cx)
            }
            ActiveDialog::DiscardChanges { paths } => {
                let file_list = if paths.len() <= 10 {
                    paths
                        .iter()
                        .map(|p| format!("  \u{2022} {p}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    let shown: Vec<_> = paths
                        .iter()
                        .take(10)
                        .map(|p| format!("  \u{2022} {p}"))
                        .collect();
                    format!("{}\n  ...and {} more", shown.join("\n"), paths.len() - 10)
                };
                let path_count = paths.len();

                v_flex()
                    .w(px(420.0))
                    .bg(theme::panel_bg())
                    .rounded(theme::z(theme::CORNER_RADIUS))
                    .border_1()
                    .border_color(theme::border())
                    .shadow_lg()
                    .child(
                        h_flex()
                            .w_full()
                            .px(theme::z(16.0))
                            .py(theme::z(12.0))
                            .items_center()
                            .gap(theme::z(8.0))
                            .border_b_1()
                            .border_color(theme::border())
                            .child(
                                Icon::new(IconName::TriangleAlert)
                                    .size(px(16.0))
                                    .text_color(theme::warning()),
                            )
                            .child(
                                div()
                                    .text_size(theme::z(14.0))
                                    .text_color(theme::text_main())
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Confirm Discard Changes"),
                            ),
                    )
                    .child(
                        v_flex()
                            .w_full()
                            .p(theme::z(16.0))
                            .gap(theme::z(8.0))
                            .child(
                                div()
                                    .text_size(theme::z(12.0))
                                    .text_color(theme::text_main())
                                    .child(format!(
                                        "Are you sure you want to discard all changes to {path_count} file{}?",
                                        if path_count == 1 { "" } else { "s" }
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(theme::z(11.0))
                                    .text_color(theme::text_muted())
                                    .whitespace_nowrap()
                                    .child(file_list),
                            ),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .px(theme::z(16.0))
                            .py(theme::z(12.0))
                            .justify_end()
                            .gap(theme::z(8.0))
                            .border_t_1()
                            .border_color(theme::border())
                            .child(
                                div()
                                    .id("discard-cancel")
                                    .px(theme::z(12.0))
                                    .py(theme::z(6.0))
                                    .rounded(theme::z(theme::CORNER_RADIUS))
                                    .bg(theme::surface_bg())
                                    .border_1()
                                    .border_color(theme::surface_bg_alt())
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::toolbar_hover_bg()))
                                    .child(div().text_size(theme::z(12.0)).text_color(theme::text_main()).child("Cancel"))
                                    .on_click(cx.listener(|app, _evt, _win, cx| {
                                        app.nav.active_dialog = ActiveDialog::None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("discard-confirm")
                                    .px(theme::z(12.0))
                                    .py(theme::z(6.0))
                                    .rounded(theme::z(theme::CORNER_RADIUS))
                                    .bg(theme::danger())
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::danger_hover()))
                                    .child(div().text_size(theme::z(12.0)).text_color(gpui::white()).child("Discard Changes"))
                                    .on_click(cx.listener(|app, _evt, _win, cx| {
                                        if let ActiveDialog::DiscardChanges { paths } = &app.nav.active_dialog {
                                            let paths = paths.clone();
                                            for path in &paths {
                                                app.discard_change(path);
                                            }
                                        }
                                        app.nav.active_dialog = ActiveDialog::None;
                                        cx.notify();
                                    })),
                            ),
                    )
            }
            ActiveDialog::StashAndSwitch { target_branch } => {
                let target = target_branch.clone();
                let bring_changes = self.repo.switch_branch_bring_changes;
                let files_to_stash = Arc::new(
                    self.repo
                        .snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.changes.clone())
                        .unwrap_or_default(),
                );
                let file_count = files_to_stash.len();
                let current_branch = self
                    .repo
                    .snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.repo.current_branch.as_str())
                    .unwrap_or("this branch");
                v_flex()
                    .w(px(576.0))
                    .bg(theme::panel_bg())
                    .rounded(theme::z(theme::CORNER_RADIUS))
                    .border_1()
                    .border_color(theme::border())
                    .shadow_lg()
                    .child(
                        h_flex()
                            .w_full()
                            .px(theme::z(16.0))
                            .py(theme::z(12.0))
                            .items_center()
                            .border_b_1()
                            .border_color(theme::border())
                            .child(
                                div()
                                    .text_size(theme::z(14.0))
                                    .text_color(theme::text_main())
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Switch Branch"),
                            ),
                    )
                    .child(
                        v_flex()
                            .w_full()
                            .p(theme::z(16.0))
                            .gap(theme::z(10.0))
                            .child(
                                div()
                                    .text_size(theme::z(12.0))
                                    .text_color(theme::text_main())
                                    .child("You have changes on this branch. What would you like to do with them?"),
                            )
                            .child(
                                v_flex()
                                    .w_full()
                                    .gap(theme::z(6.0))
                                    .child(
                                        h_flex()
                                            .w_full()
                                            .items_center()
                                            .justify_between()
                                            .child(
                                                div()
                                                    .text_size(theme::z(12.0))
                                                    .text_color(theme::text_main())
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child("Files affected"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(theme::z(11.0))
                                                    .text_color(theme::text_muted())
                                                    .child(pluralize_files(file_count)),
                                            ),
                                    )
                                    .child(render_stash_file_list(
                                        "branch-switch-file-list",
                                        "branch-switch-files",
                                        "branch-switch-file",
                                        files_to_stash.clone(),
                                        "No file list is available for these changes.",
                                    )),
                            )
                            .child(render_branch_switch_option(
                                "branch-switch-stash-option",
                                !bring_changes,
                                format!("Leave my changes on {current_branch}"),
                                "Your in-progress work will be stashed on this branch for you to return to later",
                                false,
                                cx,
                            ))
                            .child(render_branch_switch_option(
                                "branch-switch-bring-option",
                                bring_changes,
                                &format!("Bring my changes to {target}"),
                                "Your in-progress work will follow you to the new branch",
                                true,
                                cx,
                            )),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .px(theme::z(16.0))
                            .py(theme::z(12.0))
                            .justify_end()
                            .gap(theme::z(8.0))
                            .border_t_1()
                            .border_color(theme::border())
                            .child(
                                div()
                                    .id("stash-cancel")
                                    .px(theme::z(12.0))
                                    .py(theme::z(6.0))
                                    .rounded(theme::z(theme::CORNER_RADIUS))
                                    .bg(theme::surface_bg())
                                    .border_1()
                                    .border_color(theme::surface_bg_alt())
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::toolbar_hover_bg()))
                                    .child(
                                        div()
                                            .text_size(theme::z(12.0))
                                            .text_color(theme::text_main())
                                            .child("Cancel"),
                                    )
                                    .on_click(cx.listener(|app, _evt, _win, cx| {
                                        app.nav.active_dialog = ActiveDialog::None;
                                        app.repo.switch_branch_bring_changes = false;
                                        cx.notify();
                                    })),
                            )
                            .child({
                                let target = target.clone();
                                div()
                                    .id("stash-switch")
                                    .px(theme::z(12.0))
                                    .py(theme::z(6.0))
                                    .rounded(theme::z(theme::CORNER_RADIUS))
                                    .bg(theme::commit_button_bg())
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::commit_button_hover_bg()))
                                    .child(
                                        div()
                                            .text_size(theme::z(12.0))
                                            .text_color(theme::commit_button_text())
                                            .child("Switch Branch"),
                                    )
                                    .on_click(cx.listener(move |app, _evt, _win, cx| {
                                        if app.repo.switch_branch_bring_changes {
                                            app.switch_branch_with_changes(target.clone(), cx);
                                        } else {
                                            app.stash_and_switch_branch(target.clone(), cx);
                                        }
                                    }))
                            }),
                    )
            }
            ActiveDialog::RestoreStash => {
                crate::ui::restore_stash_dialog::render_restore_stash_dialog(
                    Arc::new(self.repo.stash_files.clone()),
                    cx,
                )
            }
            ActiveDialog::PublishRepository => {
                crate::ui::publish_dialog::render_publish_dialog(self, window, cx)
            }
            _ => div(),
        };

        // Center the dialog
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(backdrop)
            .child(
                div()
                    .id("dialog-container")
                    .on_click(|_evt, _win, cx| cx.stop_propagation())
                    .absolute()
                    .left(px(dialog_left))
                    .top(px(dialog_top))
                    .child(dialog_content),
            )
    }

    fn render_network_dropdown_overlay(&self, cx: &mut Context<Self>) -> Div {
        let backdrop = div()
            .id("network-dropdown-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .on_click(cx.listener(|app, _evt, _win, cx| {
                app.nav.show_network_dropdown = false;
                cx.stop_propagation();
                cx.notify();
            }));

        let panel = self
            .render_network_dropdown(cx)
            .id("network-dropdown-panel")
            .on_click(|_evt, _win, cx| cx.stop_propagation());

        // Position using h_flex: spacer pushes panel to align under the network section
        let positioned = h_flex()
            .absolute()
            .top(theme::z(theme::TOOLBAR_HEIGHT))
            .left_0()
            .w_full()
            .child(
                div()
                    .flex_none()
                    .w(px(crate::ui::toolbar::network_dropdown_left_offset())),
            )
            .child(panel);

        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(backdrop)
            .child(positioned)
    }

    fn render_network_dropdown(&self, cx: &mut Context<Self>) -> Div {
        let snapshot = self.repo.snapshot.as_ref();
        let remote_name = snapshot
            .and_then(|s| s.repo.remote_name.as_deref())
            .unwrap_or("origin");

        let fetch_title = format!("Fetch {remote_name}");
        let fetch_desc = format!("Fetch the latest changes from {remote_name}");

        v_flex()
            .w(px(crate::ui::toolbar::NETWORK_DROPDOWN_WIDTH))
            .bg(theme::panel_bg())
            .border_1()
            .border_color(theme::toolbar_button_border())
            .rounded_b(theme::z(theme::CORNER_RADIUS))
            .shadow_lg()
            .child(
                h_flex()
                    .id("net-fetch")
                    .w_full()
                    .p(px(12.0))
                    .gap(px(10.0))
                    .items_center()
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::hover_bg()))
                    .child(
                        gpui::svg()
                            .path("icons/rotate-cw.svg")
                            .size(px(20.0))
                            .text_color(theme::text_main())
                            .flex_shrink_0(),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(theme::z(14.0))
                                    .text_color(theme::text_main())
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(fetch_title),
                            )
                            .child(
                                div()
                                    .text_size(theme::z(12.0))
                                    .text_color(theme::text_muted())
                                    .child(fetch_desc),
                            ),
                    )
                    .on_click(cx.listener(|app, _evt, _win, cx| {
                        cx.stop_propagation();
                        app.nav.show_network_dropdown = false;
                        app.handle_toolbar_action(
                            ToolbarAction::RunNetworkAction(NetworkAction::Fetch),
                            cx,
                        );
                    })),
            )
    }

    fn render_repo_selector_panel(&self, repo_filter_focused: bool, cx: &mut Context<Self>) -> Div {
        let recent_repos = self.settings.recent_repos.clone();
        let current_repo = self
            .repo
            .snapshot
            .as_ref()
            .map(|s| s.repo.name.clone())
            .unwrap_or_default();

        // --- Header: current repo + caret up ---
        let _header = h_flex()
            .id("repo-selector-header")
            .w_full()
            .h(theme::z(theme::TOOLBAR_HEIGHT))
            .flex_shrink_0()
            .bg(theme::toolbar_bg())
            .border_b_1()
            .border_color(theme::toolbar_button_border())
            .px(px(10.0))
            .gap(px(10.0))
            .items_center()
            .cursor_pointer()
            .on_click(cx.listener(|app, _evt, _win, cx| {
                app.nav.show_repo_selector = false;
                cx.notify();
            }))
            // Repo icon
            .child(
                div().flex_shrink_0().child(
                    gpui_component::Icon::new(gpui_component::IconName::FolderOpen)
                        .size(px(16.0))
                        .text_color(theme::text_main()),
                ),
            )
            // Text stack
            .child(
                v_flex()
                    .flex_1()
                    .gap(px(2.0))
                    .overflow_hidden()
                    .child(
                        div()
                            .text_size(theme::z(theme::FONT_SIZE_SM))
                            .text_color(theme::text_muted())
                            .child("Current Repository"),
                    )
                    .child(
                        div()
                            .text_size(theme::z(theme::FONT_SIZE))
                            .text_color(theme::text_main())
                            .font_weight(FontWeight::SEMIBOLD)
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .child(current_repo),
                    ),
            )
            // Caret up
            .child(
                div().flex_shrink_0().child(
                    gpui_component::Icon::new(gpui_component::IconName::ChevronUp)
                        .size(px(10.0))
                        .text_color(theme::text_muted()),
                ),
            );

        // --- Filter bar ---
        let filter_text = &self.filters.repo_filter_text;
        let cursor = self.repo_filter_cursor.min(filter_text.len());
        let repo_filter_child: AnyElement = if filter_text.is_empty() && !repo_filter_focused {
            div()
                .text_size(theme::z(theme::FONT_SIZE))
                .text_color(theme::text_muted())
                .child("Filter")
                .into_any_element()
        } else {
            let before = &filter_text[..cursor];
            let after = &filter_text[cursor..];
            h_flex()
                .items_center()
                .overflow_x_hidden()
                .text_size(theme::z(theme::FONT_SIZE))
                .child(
                    div()
                        .text_color(theme::text_main())
                        .whitespace_nowrap()
                        .child(before.to_string()),
                )
                .child(if repo_filter_focused {
                    div()
                        .w(px(1.0))
                        .h(px(14.0))
                        .bg(theme::text_main())
                        .flex_shrink_0()
                        .into_any_element()
                } else {
                    div().into_any_element()
                })
                .child(
                    div()
                        .text_color(theme::text_main())
                        .whitespace_nowrap()
                        .child(after.to_string()),
                )
                .into_any_element()
        };

        let filter_bar = h_flex()
            .w_full()
            .flex_shrink_0()
            .px(px(10.0))
            .py(px(10.0))
            .gap(px(8.0))
            .items_center()
            .child(
                h_flex()
                    .id("repo-filter-input")
                    .track_focus(&self.repo_filter_focus)
                    .key_context("text-field")
                    .on_key_down(cx.listener(Self::handle_repo_filter_key))
                    .flex_1()
                    .h(px(28.0))
                    .px(px(8.0))
                    .items_center()
                    .gap(px(6.0))
                    .rounded(theme::z(theme::CORNER_RADIUS))
                    .border_1()
                    .border_color(if repo_filter_focused {
                        theme::accent()
                    } else {
                        theme::surface_bg_alt()
                    })
                    .bg(theme::bg())
                    .cursor_text()
                    .child(
                        Icon::new(IconName::Search)
                            .size(px(14.0))
                            .text_color(theme::text_muted()),
                    )
                    .child(repo_filter_child),
            )
            // Add button
            .child(
                h_flex()
                    .id("repo-add-btn")
                    .flex_shrink_0()
                    .h(px(28.0))
                    .px(px(12.0))
                    .items_center()
                    .justify_center()
                    .rounded(theme::z(theme::CORNER_RADIUS))
                    .bg(theme::surface_bg())
                    .border_1()
                    .border_color(theme::surface_bg_alt())
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::toolbar_hover_bg()))
                    .on_click(cx.listener(|app, _evt, _win, cx| {
                        app.open_repo_dialog(cx);
                    }))
                    .child(
                        h_flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(theme::z(theme::FONT_SIZE))
                                    .text_color(theme::text_main())
                                    .child("Add"),
                            )
                            .child(
                                Icon::new(IconName::ChevronDown)
                                    .size(px(8.0))
                                    .text_color(theme::text_muted()),
                            ),
                    ),
            );

        // --- Repo list ---
        // Filter repos by search text
        let repo_filter = self.filters.repo_filter_text.to_lowercase();
        let repos_snapshot: Vec<_> = recent_repos
            .iter()
            .filter(|p| {
                repo_filter.is_empty()
                    || p.file_name()
                        .map(|n| n.to_string_lossy().to_lowercase().contains(&repo_filter))
                        .unwrap_or(false)
            })
            .cloned()
            .collect();
        let repo_list = if repos_snapshot.is_empty() {
            let empty_message = if repo_filter.is_empty() {
                "No recent repositories"
            } else {
                "Sorry, I can't find that repository"
            };
            div().flex_1().child(
                v_flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .px(px(14.0))
                    .child(
                        div()
                            .text_size(theme::z(12.0))
                            .text_color(theme::text_muted())
                            .text_align(gpui::TextAlign::Center)
                            .child(empty_message),
                    ),
            )
        } else {
            let count = repos_snapshot.len();
            div().flex_1().child(
                uniform_list("repo-list", count, {
                    let repos = repos_snapshot.clone();
                    let current = self
                        .repo
                        .snapshot
                        .as_ref()
                        .map(|s| s.repo.path.clone())
                        .unwrap_or_default();
                    let view = cx.entity().clone();
                    move |range, _win, _cx| {
                        range
                            .map(|ix| {
                                let repo_path = &repos[ix];
                                let display_name = repo_path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| repo_path.to_string_lossy().to_string());
                                let is_current =
                                    repo_path.to_string_lossy() == current.to_string_lossy();
                                let repo_id = stable_id_slug(&repo_path.to_string_lossy());
                                let path_clone = repo_path.clone();
                                let vh = view.clone();

                                h_flex()
                                    .id(SharedString::from(format!("repo-{repo_id}")))
                                    .w_full()
                                    .h(px(40.0))
                                    .px(px(10.0))
                                    .items_center()
                                    .gap(px(8.0))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::hover_bg()))
                                    .bg(if is_current {
                                        theme::surface_bg_alt()
                                    } else {
                                        gpui::transparent_black()
                                    })
                                    // Repo icon
                                    .child(
                                        Icon::new(IconName::FolderClosed)
                                            .size(px(16.0))
                                            .text_color(theme::text_muted()),
                                    )
                                    // Repo name
                                    .child(
                                        div().flex_1().overflow_x_hidden().child(
                                            div()
                                                .text_size(theme::z(theme::FONT_SIZE))
                                                .text_color(theme::text_main())
                                                .whitespace_nowrap()
                                                .child(display_name),
                                        ),
                                    )
                                    .child(if is_current {
                                        div()
                                            .id(SharedString::from(format!(
                                                "repo-current-indicator-{}",
                                                repo_path.to_string_lossy()
                                            )))
                                            .w(px(7.0))
                                            .h(px(7.0))
                                            .rounded(px(999.0))
                                            .bg(theme::accent())
                                            .into_any_element()
                                    } else {
                                        div().w(px(7.0)).h(px(7.0)).into_any_element()
                                    })
                                    .on_click(move |_evt, _win, cx| {
                                        let p = path_clone.clone();
                                        vh.update(cx, |app, cx| {
                                            app.open_repo_with_notify(p, cx);
                                        });
                                    })
                                    .into_any_element()
                            })
                            .collect()
                    }
                })
                .flex_1()
                .with_sizing_behavior(ListSizingBehavior::Infer),
            )
        };

        // --- Section header: "Recent" ---
        let section_header = div().w_full().px(px(10.0)).py(px(8.0)).child(
            div()
                .text_size(theme::z(theme::FONT_SIZE))
                .text_color(theme::text_main())
                .font_weight(FontWeight::BOLD)
                .child("Recent"),
        );

        // --- Fill the sidebar panel ---
        v_flex()
            .size_full()
            .bg(theme::panel_bg())
            .border_r_1()
            .border_color(theme::border())
            .child(filter_bar)
            .child(section_header)
            .child(repo_list)
    }

    // ------------------------------------------------------------------
    // Branch selector (full-width panel)
    // ------------------------------------------------------------------

    fn render_branch_selector_overlay(
        &self,
        branch_filter_focused: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        // Backdrop — starts below toolbar so toolbar clicks still work
        let backdrop = div()
            .id("branch-selector-backdrop")
            .absolute()
            .top(theme::z(theme::TOOLBAR_HEIGHT))
            .left_0()
            .w_full()
            .bottom_0()
            .on_click(cx.listener(|app, _evt, _win, cx| {
                app.nav.show_branch_selector = false;
                app.nav.branch_selector_mode = BranchSelectorMode::Switch;
                app.repo.pending_cherry_pick_oid = None;
                cx.notify();
            }));

        // Panel drops down from the toolbar, left-aligned within the right column
        // The id + on_click stops propagation so clicks inside don't hit the backdrop
        let panel = self
            .render_branch_selector_panel(branch_filter_focused, cx)
            .id("branch-selector-panel")
            .on_click(|_evt, _win, cx| cx.stop_propagation())
            .absolute()
            .top(theme::z(theme::TOOLBAR_HEIGHT))
            .left_0()
            .w(px(360.0))
            .h(px(486.0))
            .shadow_lg();

        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(backdrop)
            .child(panel)
    }

    fn render_branch_selector_panel(
        &self,
        branch_filter_focused: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let snapshot = self.repo.snapshot.as_ref();
        let current_branch = snapshot
            .map(|s| s.repo.current_branch.clone())
            .unwrap_or_else(|| "main".to_string());
        let branches: Vec<BranchInfo> = snapshot.map(|s| s.branches.clone()).unwrap_or_default();

        // Separate local branches, filtered by search text
        let filter = self.filters.branch_filter_text.to_lowercase();
        let merge_mode = self.nav.branch_selector_mode == BranchSelectorMode::Merge;
        let local_branches: Vec<&BranchInfo> = branches
            .iter()
            .filter(|b| !b.is_remote)
            .filter(|b| !merge_mode || !b.is_current)
            .filter(|b| filter.is_empty() || b.name.to_lowercase().contains(&filter))
            .collect();

        // Find default branch (current one)
        let default_branch = local_branches
            .iter()
            .find(|b| b.is_current)
            .map(|b| b.name.clone())
            .unwrap_or_else(|| current_branch.clone());

        // --- Header: Current Branch + caret up ---
        let _header = h_flex()
            .id("branch-selector-header")
            .w_full()
            .h(theme::z(theme::TOOLBAR_HEIGHT))
            .flex_shrink_0()
            .bg(theme::toolbar_bg())
            .border_b_1()
            .border_color(theme::toolbar_button_border())
            .px(px(10.0))
            .gap(px(10.0))
            .items_center()
            .cursor_pointer()
            .on_click(cx.listener(|app, _evt, _win, cx| {
                app.nav.show_branch_selector = false;
                app.nav.branch_selector_mode = BranchSelectorMode::Switch;
                app.repo.pending_cherry_pick_oid = None;
                cx.notify();
            }))
            // Branch icon
            .child(
                div().flex_shrink_0().child(
                    Icon::new(IconName::GitHub)
                        .size(px(16.0))
                        .text_color(theme::text_main()),
                ),
            )
            // Text
            .child(
                v_flex()
                    .flex_1()
                    .gap(px(2.0))
                    .overflow_hidden()
                    .child(
                        div()
                            .text_size(theme::z(theme::FONT_SIZE_SM))
                            .text_color(theme::text_muted())
                            .child("Current Branch"),
                    )
                    .child(
                        div()
                            .text_size(theme::z(theme::FONT_SIZE))
                            .text_color(theme::text_main())
                            .font_weight(FontWeight::SEMIBOLD)
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .child(current_branch.clone()),
                    ),
            )
            // Caret up
            .child(
                div().flex_shrink_0().child(
                    gpui_component::Icon::new(gpui_component::IconName::ChevronUp)
                        .size(px(10.0))
                        .text_color(theme::text_muted()),
                ),
            );

        // --- Filter bar ---
        let filter_bar = h_flex()
            .w_full()
            .flex_shrink_0()
            .px(px(10.0))
            .py(px(10.0))
            .gap(px(8.0))
            .items_center()
            .child({
                let filter_text = &self.filters.branch_filter_text;
                let cursor = self.branch_filter_cursor;
                let focused = branch_filter_focused;

                let border_color = if focused {
                    theme::accent()
                } else {
                    theme::surface_bg_alt()
                };

                let text_child = if filter_text.is_empty() && !focused {
                    div()
                        .text_size(theme::z(theme::FONT_SIZE))
                        .text_color(theme::text_muted())
                        .child("Filter")
                } else {
                    let pos = cursor.min(filter_text.len());
                    let before = &filter_text[..pos];
                    let after = &filter_text[pos..];
                    h_flex()
                        .items_center()
                        .overflow_x_hidden()
                        .text_size(theme::z(theme::FONT_SIZE))
                        .child(
                            div()
                                .text_color(theme::text_main())
                                .whitespace_nowrap()
                                .child(before.to_string()),
                        )
                        .child(if focused {
                            div()
                                .w(px(1.0))
                                .h(px(14.0))
                                .bg(theme::text_main())
                                .flex_shrink_0()
                                .into_any_element()
                        } else {
                            div().into_any_element()
                        })
                        .child(
                            div()
                                .text_color(theme::text_main())
                                .whitespace_nowrap()
                                .child(after.to_string()),
                        )
                };

                h_flex()
                    .id("branch-filter-input")
                    .track_focus(&self.branch_filter_focus)
                    .key_context("text-field")
                    .on_key_down(cx.listener(Self::handle_branch_filter_key))
                    .flex_1()
                    .h(px(28.0))
                    .px(px(8.0))
                    .items_center()
                    .gap(px(6.0))
                    .rounded(theme::z(theme::CORNER_RADIUS))
                    .border_1()
                    .border_color(border_color)
                    .bg(theme::bg())
                    .cursor_text()
                    .child(
                        Icon::new(IconName::Search)
                            .size(px(14.0))
                            .text_color(theme::text_muted()),
                    )
                    .child(text_child)
            })
            .children(if merge_mode {
                None
            } else {
                Some(
                    h_flex()
                        .id("branch-new-btn")
                        .flex_shrink_0()
                        .h(px(28.0))
                        .px(px(12.0))
                        .items_center()
                        .justify_center()
                        .rounded(theme::z(theme::CORNER_RADIUS))
                        .bg(theme::surface_bg())
                        .border_1()
                        .border_color(theme::surface_bg_alt())
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::toolbar_hover_bg()))
                        .on_click(cx.listener(|app, _evt, _win, cx| {
                            app.menu_new_branch(cx);
                            if matches!(app.nav.active_dialog, ActiveDialog::CreateBranch) {
                                _win.focus(&app.new_branch_focus);
                            }
                        }))
                        .child(
                            div()
                                .text_size(theme::z(theme::FONT_SIZE))
                                .text_color(theme::text_main())
                                .child("New Branch"),
                        ),
                )
            });

        // --- Grouped branch list: Default Branch + Other Branches ---
        // Separate into default (current) and others
        let mut default_branches: Vec<BranchInfo> = Vec::new();
        let mut other_branches: Vec<BranchInfo> = Vec::new();
        for b in &local_branches {
            if b.is_current || b.name == "main" || b.name == "master" {
                default_branches.push((*b).clone());
            } else {
                other_branches.push((*b).clone());
            }
        }

        // Flatten into a single list with section markers
        // We'll use a flat Vec<(Option<&str>, BranchInfo)> for the uniform_list
        #[derive(Clone)]
        enum BranchListItem {
            SectionHeader(String),
            Branch(BranchInfo),
        }

        let mut items: Vec<BranchListItem> = Vec::new();
        if !default_branches.is_empty() {
            items.push(BranchListItem::SectionHeader("Default Branch".to_string()));
            for b in &default_branches {
                items.push(BranchListItem::Branch(b.clone()));
            }
        }
        if !other_branches.is_empty() {
            items.push(BranchListItem::SectionHeader("Other Branches".to_string()));
            for b in &other_branches {
                items.push(BranchListItem::Branch(b.clone()));
            }
        }

        let branch_list = if items.is_empty() {
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .w_full()
                .child(
                    v_flex()
                        .size_full()
                        .items_center()
                        .justify_center()
                        .px(px(16.0))
                        .child(
                            div()
                                .text_size(theme::z(12.0))
                                .text_color(theme::text_muted())
                                .text_align(gpui::TextAlign::Center)
                                .child("Sorry, I can't find that branch"),
                        ),
                )
                .into_any_element()
        } else {
            let count = items.len();
            let view = cx.entity().clone();
            div()
                .id("branch-list-scroll")
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .w_full()
                .overflow_y_scrollbar()
                .child(
                    uniform_list("branch-list", count, {
                        move |range, _win, _cx| {
                            range
                                .map(|ix| match &items[ix] {
                                    BranchListItem::SectionHeader(title) => div()
                                        .id(SharedString::from(format!("branch-section-{ix}")))
                                        .w_full()
                                        .px(px(10.0))
                                        .py(px(8.0))
                                        .child(
                                            div()
                                                .text_size(theme::z(theme::FONT_SIZE))
                                                .text_color(theme::text_main())
                                                .font_weight(FontWeight::BOLD)
                                                .child(title.clone()),
                                        )
                                        .into_any_element(),
                                    BranchListItem::Branch(branch) => {
                                        let is_current = branch.is_current;
                                        let name = branch.name.clone();
                                        let ctx_name = branch.name.clone();
                                        let updated = branch.updated.clone();
                                        let branch_id = stable_id_slug(&branch.name);
                                        let vh = view.clone();

                                        let row = h_flex()
                                            .id(SharedString::from(format!("branch-{branch_id}")))
                                            .w_full()
                                            .h(px(36.0))
                                            .px(px(10.0))
                                            .items_center()
                                            .gap(px(8.0))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(theme::hover_bg()))
                                            .bg(if is_current {
                                                theme::hover_bg()
                                            } else {
                                                gpui::transparent_black()
                                            })
                                            .child({
                                                let mut check_slot = div()
                                                    .w(px(20.0))
                                                    .flex_shrink_0()
                                                    .items_center()
                                                    .justify_center();
                                                if is_current {
                                                    check_slot = check_slot.child(
                                                        Icon::new(IconName::Check)
                                                            .size(px(14.0))
                                                            .text_color(theme::text_main()),
                                                    );
                                                }
                                                check_slot
                                            })
                                            .child(
                                                div().flex_1().overflow_x_hidden().child(
                                                    div()
                                                        .text_size(theme::z(theme::FONT_SIZE))
                                                        .text_color(theme::text_main())
                                                        .whitespace_nowrap()
                                                        .child(branch.name.clone()),
                                                ),
                                            )
                                            .children(updated.map(|updated| {
                                                div()
                                                    .flex_shrink_0()
                                                    .text_size(theme::z(12.0))
                                                    .text_color(theme::text_muted())
                                                    .child(updated)
                                            }))
                                            .on_click(move |_evt, _win, cx| {
                                                let name = name.clone();
                                                vh.update(cx, |app, cx| {
                                                    app.select_branch_from_selector(name, cx);
                                                });
                                            });

                                        crate::ui::branch_context_menu::bind_branch_context_click(
                                            row,
                                            view.clone(),
                                            ctx_name,
                                        )
                                        .into_any_element()
                                    }
                                })
                                .collect()
                        }
                    })
                    .flex_1()
                    .with_sizing_behavior(ListSizingBehavior::Infer),
                )
                .into_any_element()
        };

        let branch_selector_footer = if self.repo.pending_cherry_pick_oid.is_some() {
            "Choose a branch to cherry-pick into"
        } else if self.nav.branch_selector_mode == BranchSelectorMode::Merge {
            "Choose a branch to merge into"
        } else {
            "Choose a branch to switch to"
        };
        let show_branch_selector_target = self.repo.pending_cherry_pick_oid.is_some()
            || self.nav.branch_selector_mode == BranchSelectorMode::Merge;

        // --- Branch selector prompt ---
        let bottom_bar = h_flex()
            .id("branch-selector-merge-bar")
            .w_full()
            .h(px(52.0))
            .flex_shrink_0()
            .border_t_1()
            .border_color(theme::toolbar_button_border())
            .px(px(10.0))
            .bg(theme::surface_bg())
            .items_center()
            .justify_center()
            .child(
                h_flex()
                    .id("branch-selector-merge-button")
                    .w_full()
                    .h(px(32.0))
                    .items_center()
                    .justify_center()
                    .gap(px(6.0))
                    .rounded(theme::z(theme::CORNER_RADIUS))
                    .border_1()
                    .border_color(theme::surface_bg_alt())
                    .bg(theme::panel_bg())
                    .child(
                        div()
                            .text_size(theme::z(14.0))
                            .text_color(theme::text_main())
                            .child("⑂"),
                    )
                    .child(
                        div()
                            .text_size(theme::z(theme::FONT_SIZE))
                            .text_color(theme::text_muted())
                            .child(branch_selector_footer),
                    )
                    .when(show_branch_selector_target, |el| {
                        el.child(
                            div()
                                .text_size(theme::z(theme::FONT_SIZE))
                                .text_color(theme::text_main())
                                .font_weight(FontWeight::BOLD)
                                .child(default_branch),
                        )
                    }),
            );

        v_flex()
            .size_full()
            .bg(theme::panel_bg())
            .child(filter_bar)
            .child(
                div()
                    .id("branch-selector-list-viewport")
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .child(branch_list),
            )
            .child(bottom_bar)
    }
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

fn render_branch_switch_option(
    id: &'static str,
    selected: bool,
    title: impl Into<String>,
    description: &'static str,
    bring_changes: bool,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
    h_flex()
        .id(id)
        .w_full()
        .min_h(theme::z(72.0))
        .p(theme::z(12.0))
        .gap(theme::z(10.0))
        .items_start()
        .rounded(theme::z(theme::CORNER_RADIUS))
        .border_1()
        .border_color(if selected {
            theme::accent()
        } else {
            theme::border()
        })
        .bg(if selected {
            theme::surface_bg()
        } else {
            theme::surface_bg_muted()
        })
        .cursor_pointer()
        .hover(|s| s.bg(theme::surface_bg()))
        .child(
            div()
                .w(theme::z(16.0))
                .h(theme::z(16.0))
                .mt(theme::z(2.0))
                .rounded_full()
                .border_1()
                .border_color(if selected {
                    theme::accent()
                } else {
                    theme::text_muted()
                })
                .flex()
                .items_center()
                .justify_center()
                .when(selected, |el| {
                    el.child(
                        div()
                            .w(theme::z(8.0))
                            .h(theme::z(8.0))
                            .rounded_full()
                            .bg(theme::accent()),
                    )
                }),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(theme::z(4.0))
                .child(
                    div()
                        .text_size(theme::z(13.0))
                        .text_color(theme::text_main())
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.into()),
                )
                .child(
                    div()
                        .text_size(theme::z(12.0))
                        .text_color(theme::text_muted())
                        .child(description),
                ),
        )
        .on_click(cx.listener(move |app, _evt, _win, cx| {
            app.repo.switch_branch_bring_changes = bring_changes;
            cx.notify();
        }))
}

fn short_commit_label(oid: &str) -> &str {
    &oid[..oid.len().min(7)]
}

fn tag_name_length_validation_message(tag_name: &str) -> Option<String> {
    (tag_name.len() > MAX_TAG_NAME_LENGTH)
        .then(|| format!("The tag name cannot be longer than {MAX_TAG_NAME_LENGTH} characters"))
}

fn sanitized_ref_name(name: &str) -> String {
    let mut sanitized = name
        .chars()
        .map(|ch| {
            if ch <= '\u{20}'
                || ch == '\u{7f}'
                || matches!(
                    ch,
                    '~' | '^' | ':' | '?' | '*' | '[' | '\\' | '|' | '"' | '<' | '>'
                )
            {
                '-'
            } else {
                ch
            }
        })
        .collect::<String>();

    while sanitized.contains("@{") {
        sanitized = sanitized.replace("@{", "-");
    }
    while sanitized.contains("..") {
        sanitized = sanitized.replace("..", "-");
    }
    if sanitized.starts_with('.') {
        sanitized.replace_range(..1, "-");
    }
    if sanitized.ends_with(".lock") {
        let start = sanitized.len() - ".lock".len();
        sanitized.replace_range(start.., "-");
    }
    if sanitized.ends_with('.') || sanitized.ends_with('/') {
        let start = sanitized
            .char_indices()
            .last()
            .map(|(index, _)| index)
            .unwrap_or(0);
        sanitized.replace_range(start.., "-");
    }

    sanitized
        .trim_start_matches(|ch| matches!(ch, '-' | '+'))
        .to_string()
}

fn branch_validation_message_color(message: &str) -> Hsla {
    if message.starts_with("Will be ") {
        theme::warning()
    } else {
        theme::danger()
    }
}

fn default_commit_summary_for_change(change: &ChangeEntry) -> String {
    let filename = change.path.rsplit('/').next().unwrap_or(&change.path);
    let verb = if change.status.contains('?') || change.status.contains('A') {
        "Create"
    } else if change.status.contains('D') {
        "Delete"
    } else {
        "Update"
    };
    format!("{verb} {filename}")
}

fn pluralize_files(count: usize) -> String {
    match count {
        0 => "no listed files".to_string(),
        1 => "1 file".to_string(),
        count => format!("{count} files"),
    }
}

fn diff_line_stats(diffs: &[DiffEntry]) -> (usize, usize) {
    let mut added = 0;
    let mut deleted = 0;

    for diff in diffs {
        for line in diff.diff.lines() {
            if line.starts_with("+++") || line.starts_with("---") {
                continue;
            }
            if line.starts_with('+') {
                added += 1;
            } else if line.starts_with('-') {
                deleted += 1;
            }
        }
    }

    (added, deleted)
}

fn commit_diff_clipboard_text(diffs: &[DiffEntry]) -> String {
    diffs
        .iter()
        .map(|entry| {
            let body = entry.diff.trim_end();
            if body.starts_with("diff --git ")
                || body.starts_with("--- ")
                || body.starts_with("Binary file")
                || body.starts_with("Binary files")
            {
                body.to_string()
            } else {
                format!("FILE: {}\n{body}", entry.path)
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn external_command_from_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn spawn_shell_path_command(command: &str, path: &Path) -> std::io::Result<()> {
    spawn_shell_arg_command(command, &path.to_string_lossy())
}

fn spawn_shell_arg_command(command: &str, arg: &str) -> std::io::Result<()> {
    Command::new("sh")
        .arg("-lc")
        .arg(format!("{} {}", command, shell_escape(arg)))
        .spawn()
        .map(|_| ())
}

fn branch_switch_needs_stash(error: &str) -> bool {
    let normalized = error.to_lowercase();
    normalized.contains("would be overwritten by checkout")
        || normalized.contains("would be overwritten by merge")
        || normalized.contains("please commit your changes or stash them")
}

fn reveal_path(path: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(command) = external_command_from_env("GITSPARK_REVEAL_COMMAND") {
        return spawn_shell_path_command(&command, path).map_err(Into::into);
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(Into::into)
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(open::that_detached(path)?)
    }
}

fn open_with_default_program(path: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(command) = external_command_from_env("GITSPARK_OPEN_COMMAND") {
        return spawn_shell_path_command(&command, path).map_err(Into::into);
    }

    Ok(open::that(path)?)
}

fn open_url(url: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(command) = external_command_from_env("GITSPARK_OPEN_URL_COMMAND") {
        return spawn_shell_arg_command(&command, url).map_err(Into::into);
    }

    Ok(open::that_detached(url)?)
}

// ---------------------------------------------------------------------------
// Text input helpers
// ---------------------------------------------------------------------------

fn ordered_range(a: usize, b: usize) -> (usize, usize) {
    if a <= b { (a, b) } else { (b, a) }
}

fn prev_char_boundary(s: &str, pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }
    let mut p = pos - 1;
    while p > 0 && !s.is_char_boundary(p) {
        p -= 1;
    }
    p
}

fn next_char_boundary(s: &str, pos: usize) -> usize {
    if pos >= s.len() {
        return s.len();
    }
    let mut p = pos + 1;
    while p < s.len() && !s.is_char_boundary(p) {
        p += 1;
    }
    p
}

#[cfg(test)]
mod tests {
    use super::{MAX_TAG_NAME_LENGTH, sanitized_ref_name, tag_name_length_validation_message};

    #[test]
    fn validates_github_desktop_tag_name_length_limit() {
        assert!(tag_name_length_validation_message(&"x".repeat(MAX_TAG_NAME_LENGTH)).is_none());
        assert_eq!(
            tag_name_length_validation_message(&"x".repeat(MAX_TAG_NAME_LENGTH + 1)),
            Some("The tag name cannot be longer than 245 characters".to_string())
        );
    }

    #[test]
    fn sanitizes_branch_names_like_github_desktop() {
        assert_eq!(sanitized_ref_name("feature branch?"), "feature-branch-");
        assert_eq!(sanitized_ref_name("+ bad/name.lock"), "bad/name-");
        assert_eq!(sanitized_ref_name(".@{bad..name."), "bad-name-");
        assert_eq!(sanitized_ref_name("////"), "///-");
        assert_eq!(sanitized_ref_name("   "), "");
    }
}

#[allow(dead_code)]
fn edit_string_field(
    value: &mut String,
    cursor: &mut usize,
    multiline: bool,
    event: &KeyDownEvent,
    cx: &mut Context<GitSparkApp>,
) {
    let ks = &event.keystroke;

    if ks.modifiers.secondary() {
        match ks.key.as_str() {
            "v" => {
                if let Some(item) = cx.read_from_clipboard() {
                    if let Some(text) = item.text() {
                        let text = if multiline {
                            text.to_string()
                        } else {
                            text.replace(['\n', '\r'], " ")
                        };
                        value.insert_str(*cursor, &text);
                        *cursor += text.len();
                        cx.notify();
                    }
                }
            }
            "a" => {
                *cursor = value.len();
                cx.notify();
            }
            _ => {}
        }
        return;
    }

    match ks.key.as_str() {
        "backspace" => {
            if *cursor > 0 {
                let new_pos = prev_char_boundary(value, *cursor);
                value.drain(new_pos..*cursor);
                *cursor = new_pos;
                cx.notify();
            }
        }
        "delete" => {
            if *cursor < value.len() {
                let end = next_char_boundary(value, *cursor);
                value.drain(*cursor..end);
                cx.notify();
            }
        }
        "left" => {
            if *cursor > 0 {
                *cursor = prev_char_boundary(value, *cursor);
                cx.notify();
            }
        }
        "right" => {
            if *cursor < value.len() {
                *cursor = next_char_boundary(value, *cursor);
                cx.notify();
            }
        }
        "home" => {
            *cursor = 0;
            cx.notify();
        }
        "end" => {
            *cursor = value.len();
            cx.notify();
        }
        "enter" if multiline => {
            value.insert(*cursor, '\n');
            *cursor += 1;
            cx.notify();
        }
        _ => {
            if let Some(ref ch) = ks.key_char {
                if !ks.modifiers.control
                    && (multiline || (!ch.contains('\n') && !ch.contains('\r')))
                {
                    value.insert_str(*cursor, ch);
                    *cursor += ch.len();
                    cx.notify();
                }
            }
        }
    }
}
