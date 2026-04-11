use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;
use std::{env, process::Command};

use gpui::*;
use gpui_component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_component::resizable::{h_resizable, resizable_panel};
use gpui_component::{Disableable, Icon, IconName, Sizable, h_flex, v_flex};
use rfd::FileDialog;

use crate::ai::AiClient;
use crate::git::GitClient;
use crate::models::{
    AiProvider, AppSettings, BranchInfo, CommitSuggestion, DiffEntry, GitIdentity,
    RemoteModelOption, RepoSnapshot,
};
use crate::storage::{push_recent_repo, save_settings};
use crate::ui::domain_state::{
    CommitState, NetworkAction, NetworkState, RepoState, SelectionState,
};
use crate::ui::branch_context_menu::BranchContextAction;
use crate::ui::changes_context_menu::ChangesContextAction;
use crate::ui::history_context_menu::HistoryContextMenuAction;
use crate::ui::settings_modal::{self, SettingsField, SettingsModalState};
use crate::ui::theme;
use crate::ui::ui_state::{
    ActiveDialog, FilterState, MessageState, NavState, OpenRouterModelsState, SidebarTab,
};

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum RepoRefreshReason {
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
    CommitCreated(Result<RepoSnapshot, String>),
    NetworkActionCompleted(Result<RepoSnapshot, String>, String),
    AiCommitGenerated(Result<CommitSuggestion, String>),
    OpenRouterModelsLoaded(Result<Vec<RemoteModelOption>, String>),
    CommitDiffLoaded(String, Result<Vec<DiffEntry>, String>),
    RepoOperationCompleted(Result<RepoSnapshot, String>, String, String),
    CommitDiffCopied(String, Result<String, String>),
}

// ---------------------------------------------------------------------------
// Actions (dispatched by child views via gpui action system)
// ---------------------------------------------------------------------------

// Toolbar
#[derive(Clone)]
pub enum ToolbarAction {
    ToggleRepoSelector,
    SwitchBranch(String),
    RunNetworkAction(NetworkAction),
    FetchOrigin,
    PullOrigin,
    PushOrigin,
}

// Sidebar
#[derive(Clone)]
pub enum SidebarAction {
    OpenRepoDialog,
    OpenRepo(PathBuf),
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
    ShowSettings,
    CommitAll,
}

// Settings
#[derive(Clone)]
pub enum SettingsAction {
    SaveGitConfig,
    SaveAiSettings,
    ChangeProvider(AiProvider),
    SelectOpenRouterModel(String),
    RetryOpenRouterModels,
    Close,
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
    description_cursor: usize,
    // Filter input state
    branch_filter_focus: FocusHandle,
    branch_filter_cursor: usize,
    repo_filter_focus: FocusHandle,
    repo_filter_cursor: usize,
    pub(crate) settings_modal: SettingsModalState,
    // Zoom
    rem_size: f32,
}

impl GitSparkApp {
    pub fn new(settings: AppSettings, cx: &mut Context<Self>) -> Self {
        let (tx, event_rx) = mpsc::channel();
        let event_pending = Arc::new(AtomicBool::new(false));
        let event_tx = NotifySender {
            tx,
            pending: Arc::clone(&event_pending),
        };

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
            description_cursor: 0,
            branch_filter_focus: cx.focus_handle(),
            branch_filter_cursor: 0,
            repo_filter_focus: cx.focus_handle(),
            repo_filter_cursor: 0,
            settings_modal: SettingsModalState::new(cx),
            rem_size: DEFAULT_REM_SIZE,
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
                        app.nav.show_settings = !app.nav.show_settings;
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
                        app.nav.active_dialog = ActiveDialog::CreateBranch;
                        cx.notify();
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

    fn process_events(&mut self, cx: &mut Context<Self>) {
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
                AppEvent::BranchSwitched(Err(err), _) => {
                    self.messages.error_message = format!("Branch switch failed: {err}");
                }
                AppEvent::BranchMerged(Ok(snapshot), branch) => {
                    self.adopt_snapshot(snapshot);
                    self.messages.status_message = format!("Merged '{branch}'.");
                    self.messages.error_message.clear();
                }
                AppEvent::BranchMerged(Err(err), _) => {
                    self.messages.error_message = format!("Merge failed: {err}");
                }
                AppEvent::CommitCreated(Ok(snapshot)) => {
                    let summary = self.commit.summary.clone();
                    self.adopt_snapshot(snapshot);
                    self.commit.summary.clear();
                    self.commit.body.clear();
                    self.summary_cursor = 0;
                    self.description_cursor = 0;
                    self.commit.ai_preview = None;
                    self.messages.status_message = "Commit created.".to_string();
                    self.messages.error_message.clear();
                    // Undo commit banner
                    self.nav.undo_commit =
                        Some((summary, std::time::Instant::now()));
                }
                AppEvent::CommitCreated(Err(err)) => {
                    self.messages.error_message = format!("Commit failed: {err}");
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
                AppEvent::RepoOperationCompleted(Ok(snapshot), _action_label, success_message) => {
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
                    self.messages.error_message =
                        format!("Failed to copy diff for {}: {err}", short_commit_label(&oid));
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
            SidebarAction::OpenWithDefault(path) => {
                if let Some(repo_path) = self.repo_path() {
                    let full_path = repo_path.join(&path);
                    match open::that(&full_path) {
                        Ok(_) => {
                            self.messages.status_message =
                                format!("Opened '{path}' with the default program.");
                            self.messages.error_message.clear();
                        }
                        Err(err) => {
                            self.messages.error_message =
                                format!("Failed to open '{path}' with default program: {err}");
                        }
                    }
                }
            }
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
            SettingsAction::SaveGitConfig => self.save_git_config(),
            SettingsAction::SaveAiSettings => {
                self.settings.ai.endpoint =
                    self.settings.ai.provider.default_endpoint().to_string();
                self.persist_settings();
                if self.messages.error_message.is_empty() {
                    self.messages.status_message = "AI settings saved.".to_string();
                }
            }
            SettingsAction::ChangeProvider(provider) => {
                self.settings.ai.provider = provider;
                self.settings.ai.endpoint =
                    self.settings.ai.provider.default_endpoint().to_string();
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
        self.run_network_action(NetworkAction::Push, cx);
    }

    fn run_network_action(&mut self, action: NetworkAction, cx: &mut Context<Self>) {
        if self.network.active_action.is_some() {
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
                NetworkAction::PublishRepository => {
                    Err(anyhow::anyhow!("Publish repository is not yet implemented"))
                }
            }
            .map_err(|e| e.to_string());

            let _ = tx.send(AppEvent::NetworkActionCompleted(
                res,
                action_label_for_event,
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

    fn create_branch(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        // Use filter text as branch name if new_branch_name is empty
        let name = if self.repo.new_branch_name.trim().is_empty() {
            self.filters.branch_filter_text.trim().to_string()
        } else {
            self.repo.new_branch_name.trim().to_string()
        };
        if name.is_empty() {
            self.messages.error_message = "Type a branch name in the filter field, then click New Branch.".to_string();
            cx.notify();
            return;
        }

        self.messages.status_message = format!("Creating branch '{name}'...");
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.create_branch(&path, &name).map_err(|e| e.to_string());
            tx.send(AppEvent::BranchSwitched(res, name));
        });
        self.repo.new_branch_name.clear();
        self.filters.branch_filter_text.clear();
        self.branch_filter_cursor = 0;
        self.nav.show_branch_selector = false;
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

        if self.commit.summary.trim().is_empty() {
            self.messages.error_message = "Commit summary cannot be empty.".to_string();
            return;
        }

        let message = if self.commit.body.trim().is_empty() {
            self.commit.summary.trim().to_string()
        } else {
            format!(
                "{}\n\n{}",
                self.commit.summary.trim(),
                self.commit.body.trim()
            )
        };

        self.messages.status_message = "Creating commit...".to_string();
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.commit_all(&path, &message).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::CommitCreated(res));
        });
        cx.notify();
    }

    fn undo_last_commit(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            return;
        };
        self.nav.undo_commit = None;
        self.messages.status_message = "Undoing last commit...".to_string();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.undo_last_commit(&path).map_err(|e| e.to_string());
            tx.send(AppEvent::CommitCreated(res));
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
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        match self.git.write_identity(&path, &self.repo.identity) {
            Ok(()) => {
                self.messages.status_message = "Git config saved.".to_string();
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message = format!("Failed to save git config: {err}");
            }
        }
    }

    fn load_identity(&mut self, path: &Path) {
        match self.git.read_identity(path) {
            Ok(identity) => {
                self.repo.identity = identity;
            }
            Err(err) => {
                self.repo.identity = GitIdentity::default();
                self.messages.error_message = format!("Could not load git config: {err}");
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
                self.run_commit_repo_action(
                    oid,
                    "Cherry-pick commit".to_string(),
                    format!("Cherry-picked commit {short}."),
                    GitClient::cherry_pick_commit,
                    cx,
                );
            }
            HistoryContextMenuAction::CopySha => {
                cx.write_to_clipboard(ClipboardItem::new_string(oid.clone()));
                self.messages.status_message =
                    format!("Copied SHA {}.", short_commit_label(&oid));
                self.messages.error_message.clear();
            }
            HistoryContextMenuAction::CopyDiff => self.copy_commit_diff(&oid, cx),
            HistoryContextMenuAction::ViewOnGitHub => self.view_commit_on_github(&oid),
            HistoryContextMenuAction::ResetToCommit
            | HistoryContextMenuAction::ReorderCommit
            | HistoryContextMenuAction::CreateBranchFromCommit
            | HistoryContextMenuAction::CreateTag
            | HistoryContextMenuAction::CopyTag => {}
        }

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
            }
            ChangesContextAction::IgnoreFile => {
                self.ignore_path(&path);
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
                }
            }
            ChangesContextAction::CopyRelativePath => {
                cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
                self.messages.status_message = "Copied relative path.".to_string();
            }
            ChangesContextAction::RevealInFinder => {
                self.reveal_in_finder(&path);
            }
            ChangesContextAction::OpenInExternalEditor => {
                self.open_in_external_editor(&path);
            }
        }
        self.messages.error_message.clear();
        cx.notify();
    }

    pub(crate) fn handle_branch_context_action(
        &mut self,
        branch_name: String,
        action: BranchContextAction,
        cx: &mut Context<Self>,
    ) {
        match action {
            BranchContextAction::Delete => {
                let Some(path) = self.repo_path().map(PathBuf::from) else { return };
                self.messages.status_message = format!("Deleting branch '{branch_name}'...");
                self.messages.error_message.clear();
                let tx = self.event_tx.clone();
                let git = GitClient::new();
                let name = branch_name.clone();
                thread::spawn(move || {
                    let res = git.delete_branch(&path, &name).map_err(|e| e.to_string());
                    tx.send(AppEvent::NetworkActionCompleted(
                        res,
                        format!("Deleted branch '{name}'"),
                    ));
                });
            }
            BranchContextAction::ViewOnGitHub => {
                if let Some(snapshot) = &self.repo.snapshot {
                    if let Some(remote) = &snapshot.repo.remote_name {
                        // Try to construct GitHub URL
                        let repo_name = &snapshot.repo.name;
                        let url = format!("https://github.com/{repo_name}/tree/{branch_name}");
                        let _ = open::that_detached(&url);
                    }
                }
            }
            BranchContextAction::Rename => {
                // Not yet implemented
                self.messages.status_message = "Branch rename not yet implemented.".to_string();
            }
        }
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

    fn copy_commit_diff(&mut self, oid: &str, cx: &mut Context<Self>) {
        if self.selection.selected_commit.as_deref() == Some(oid)
            && let Some(diffs) = self.selection.commit_diffs.as_ref()
        {
            cx.write_to_clipboard(ClipboardItem::new_string(commit_diff_clipboard_text(diffs)));
            self.messages.status_message =
                format!("Copied diff for {}.", short_commit_label(oid));
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

    fn view_commit_on_github(&mut self, oid: &str) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        match self.git.github_commit_url(&path, oid) {
            Ok(Some(url)) => match open::that_detached(&url) {
                Ok(_) => {
                    self.messages.status_message =
                        format!("Opened commit {} on GitHub.", short_commit_label(oid));
                    self.messages.error_message.clear();
                }
                Err(err) => {
                    self.messages.error_message =
                        format!("Failed to open commit {} on GitHub: {err}", short_commit_label(oid));
                }
            },
            Ok(None) => {
                self.messages.error_message =
                    "This repository does not have a GitHub remote URL.".to_string();
            }
            Err(err) => {
                self.messages.error_message =
                    format!("Failed to resolve GitHub URL for {}: {err}", short_commit_label(oid));
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

        #[cfg(target_os = "macos")]
        let result = Command::new("open")
            .arg("-R")
            .arg(&full_path)
            .spawn()
            .map(|_| ());

        #[cfg(not(target_os = "macos"))]
        let result = open::that_detached(&full_path);

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

    // ------------------------------------------------------------------
    // Settings persistence
    // ------------------------------------------------------------------

    fn add_recent_repo(&mut self, path: PathBuf) {
        push_recent_repo(&mut self.settings, path);
        self.persist_settings();
    }

    fn persist_settings(&mut self) {
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
        self.ensure_repo_watch(&snapshot.repo.path);
        self.repo.snapshot = Some(snapshot);

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
        // Clamp cursors to valid positions (e.g. after AI fill or clear)
        self.summary_cursor = self.summary_cursor.min(self.commit.summary.len());
        self.description_cursor = self.description_cursor.min(self.commit.body.len());
        self.settings_modal.git_user_name_cursor = self
            .settings_modal
            .git_user_name_cursor
            .min(self.repo.identity.user_name.len());
        self.settings_modal.git_user_email_cursor = self
            .settings_modal
            .git_user_email_cursor
            .min(self.repo.identity.user_email.len());
        self.settings_modal.git_default_branch_cursor =
            self.settings_modal.git_default_branch_cursor.min(
                self.repo
                    .identity
                    .default_branch
                    .as_deref()
                    .unwrap_or("")
                    .len(),
            );
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

        let summary_focused = self.summary_focus.is_focused(window);
        let description_focused = self.description_focus.is_focused(window);
        let branch_filter_focused = self.branch_filter_focus.is_focused(window);
        let repo_filter_focused = self.repo_filter_focus.is_focused(window);

        // Clamp filter cursors
        self.branch_filter_cursor = self.branch_filter_cursor.min(self.filters.branch_filter_text.len());
        self.repo_filter_cursor = self.repo_filter_cursor.min(self.filters.repo_filter_text.len());

        // Build toolbar parts separately — they go into the resizable columns
        let (toolbar_left, toolbar_right) = self.render_toolbar_parts(cx);

        // Left column: repo toolbar section + sidebar (or repo selector)
        let left_column =
            v_flex()
                .size_full()
                .child(toolbar_left)
                .child(if self.nav.show_repo_selector {
                    self.render_repo_selector_panel(cx).into_any_element()
                } else {
                    self.render_sidebar(summary_focused, description_focused, cx)
                        .into_any_element()
                });

        // Right column: branch + network toolbar sections + workspace
        // Branch selector overlay lives inside the right column so it aligns naturally
        let right_column = div()
            .size_full()
            .relative()
            .child(
                v_flex()
                    .size_full()
                    .child(toolbar_right)
                    .child(self.render_workspace(cx)),
            )
            .children(if self.nav.show_branch_selector {
                Some(self.render_branch_selector_overlay(branch_filter_focused, cx))
            } else {
                None
            });

        // Apply zoom level
        let zoom_factor = self.rem_size / DEFAULT_REM_SIZE;
        theme::set_zoom(zoom_factor);
        window.set_rem_size(px(self.rem_size));

        // macOS titlebar spacer (traffic lights sit here)
        let titlebar_height = if cfg!(target_os = "macos") { 38.0 } else { 0.0 };

        let mut root = v_flex()
            .size_full()
            .relative()
            .bg(theme::bg())
            .font_family(".SystemUIFont")
            .text_size(theme::z(theme::FONT_SIZE))
            .child(
                div()
                    .w_full()
                    .h(px(titlebar_height))
                    .flex_shrink_0()
                    .bg(theme::panel_bg()), // slightly lighter than bg for titlebar strip
            )
            .child(
                h_resizable("main-panels")
                    .child(
                        resizable_panel()
                            .size(px(260.0))
                            .size_range(px(200.0)..px(400.0))
                            .child(left_column),
                    )
                    .child(resizable_panel().child(right_column)),
            )
            .child(self.render_status_bar());

        if self.nav.show_settings {
            root = root.child(settings_modal::render_settings_modal(self, window, cx));
        }

        // Dialogs
        if self.nav.active_dialog != ActiveDialog::None {
            root = root.child(self.render_active_dialog(cx));
        }

        root
    }
}

impl GitSparkApp {
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
        let network_label = if is_in_flight {
            network_action.pending_title(remote_name)
        } else {
            network_action.title(remote_name)
        };
        let last_fetched = snapshot.and_then(|s| s.repo.last_fetched.as_deref());

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
        )
        .on_click(cx.listener(|app, _evt, _win, cx| {
            app.handle_toolbar_action(ToolbarAction::ToggleRepoSelector, cx);
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
        )
        .flex_none()
        .w(px(300.0))
        .on_click(cx.listener(|app, _evt, _win, cx| {
            app.nav.show_branch_selector = !app.nav.show_branch_selector;
            app.nav.show_repo_selector = false;
            app.nav.show_network_dropdown = false;
            cx.notify();
        }));

        let (network_main, network_caret) = toolbar::render_network_parts(
            &network_label,
            ahead,
            behind,
            last_fetched,
            is_in_flight,
            self.nav.show_network_dropdown,
        );

        let net_action = network_action;
        let network_main = network_main.on_click(cx.listener(move |app, _evt, _win, cx| {
            if app.network.active_action.is_none() {
                app.nav.show_network_dropdown = false;
                app.handle_toolbar_action(ToolbarAction::RunNetworkAction(net_action), cx);
            }
        }));
        let network_caret = network_caret.on_click(cx.listener(|app, _evt, _win, cx| {
            app.nav.show_network_dropdown = !app.nav.show_network_dropdown;
            app.nav.show_repo_selector = false;
            app.nav.show_branch_selector = false;
            cx.notify();
        }));

        let mut network_dropdown = div();
        if self.nav.show_network_dropdown {
            network_dropdown = self.render_network_dropdown(cx);
        }

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
                    .w(px(300.0))
                    .h_full()
                    .relative()
                    .child(
                        h_flex()
                            .size_full()
                            .child(network_main)
                            .child(network_caret),
                    )
                    .child(network_dropdown),
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
            if let Some((ref summary, created_at)) = self.nav.undo_commit {
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
                            self.commit.summary.insert_str(self.summary_cursor, &text);
                            self.summary_cursor += text.len();
                            cx.notify();
                        }
                    }
                }
                "a" => {
                    self.summary_cursor = self.commit.summary.len();
                    cx.notify();
                }
                _ => {}
            }
            return;
        }
        match ks.key.as_str() {
            "backspace" => {
                if self.summary_cursor > 0 {
                    let new_pos = prev_char_boundary(&self.commit.summary, self.summary_cursor);
                    self.commit.summary.drain(new_pos..self.summary_cursor);
                    self.summary_cursor = new_pos;
                    cx.notify();
                }
            }
            "delete" => {
                if self.summary_cursor < self.commit.summary.len() {
                    let end = next_char_boundary(&self.commit.summary, self.summary_cursor);
                    self.commit.summary.drain(self.summary_cursor..end);
                    cx.notify();
                }
            }
            "left" => {
                if self.summary_cursor > 0 {
                    self.summary_cursor =
                        prev_char_boundary(&self.commit.summary, self.summary_cursor);
                    cx.notify();
                }
            }
            "right" => {
                if self.summary_cursor < self.commit.summary.len() {
                    self.summary_cursor =
                        next_char_boundary(&self.commit.summary, self.summary_cursor);
                    cx.notify();
                }
            }
            "home" => {
                self.summary_cursor = 0;
                cx.notify();
            }
            "end" => {
                self.summary_cursor = self.commit.summary.len();
                cx.notify();
            }
            _ => {
                if let Some(ref ch) = ks.key_char {
                    if !ks.modifiers.control && !ch.contains('\n') && !ch.contains('\r') {
                        self.commit.summary.insert_str(self.summary_cursor, ch);
                        self.summary_cursor += ch.len();
                        cx.notify();
                    }
                }
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
                            self.commit.body.insert_str(self.description_cursor, &text);
                            self.description_cursor += text.len();
                            cx.notify();
                        }
                    }
                }
                "a" => {
                    self.description_cursor = self.commit.body.len();
                    cx.notify();
                }
                _ => {}
            }
            return;
        }
        match ks.key.as_str() {
            "backspace" => {
                if self.description_cursor > 0 {
                    let new_pos = prev_char_boundary(&self.commit.body, self.description_cursor);
                    self.commit.body.drain(new_pos..self.description_cursor);
                    self.description_cursor = new_pos;
                    cx.notify();
                }
            }
            "delete" => {
                if self.description_cursor < self.commit.body.len() {
                    let end = next_char_boundary(&self.commit.body, self.description_cursor);
                    self.commit.body.drain(self.description_cursor..end);
                    cx.notify();
                }
            }
            "left" => {
                if self.description_cursor > 0 {
                    self.description_cursor =
                        prev_char_boundary(&self.commit.body, self.description_cursor);
                    cx.notify();
                }
            }
            "right" => {
                if self.description_cursor < self.commit.body.len() {
                    self.description_cursor =
                        next_char_boundary(&self.commit.body, self.description_cursor);
                    cx.notify();
                }
            }
            "home" => {
                self.description_cursor = 0;
                cx.notify();
            }
            "end" => {
                self.description_cursor = self.commit.body.len();
                cx.notify();
            }
            "enter" => {
                self.commit.body.insert_str(self.description_cursor, "\n");
                self.description_cursor += 1;
                cx.notify();
            }
            _ => {
                if let Some(ref ch) = ks.key_char {
                    if !ks.modifiers.control {
                        self.commit.body.insert_str(self.description_cursor, ch);
                        self.description_cursor += ch.len();
                        cx.notify();
                    }
                }
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
                        self.filters.branch_filter_text.insert_str(self.branch_filter_cursor, &text);
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
                    let new_pos = prev_char_boundary(&self.filters.branch_filter_text, self.branch_filter_cursor);
                    self.filters.branch_filter_text.drain(new_pos..self.branch_filter_cursor);
                    self.branch_filter_cursor = new_pos;
                    cx.notify();
                }
            }
            "escape" => {
                self.nav.show_branch_selector = false;
                self.filters.branch_filter_text.clear();
                self.branch_filter_cursor = 0;
                cx.notify();
            }
            "left" => {
                if self.branch_filter_cursor > 0 {
                    self.branch_filter_cursor = prev_char_boundary(&self.filters.branch_filter_text, self.branch_filter_cursor);
                    cx.notify();
                }
            }
            "right" => {
                if self.branch_filter_cursor < self.filters.branch_filter_text.len() {
                    self.branch_filter_cursor = next_char_boundary(&self.filters.branch_filter_text, self.branch_filter_cursor);
                    cx.notify();
                }
            }
            _ => {
                if let Some(ref ch) = ks.key_char {
                    if !ks.modifiers.control && !ch.contains('\n') && !ch.contains('\r') {
                        self.filters.branch_filter_text.insert_str(self.branch_filter_cursor, ch);
                        self.branch_filter_cursor += ch.len();
                        cx.notify();
                    }
                }
            }
        }
    }

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
                        self.filters.repo_filter_text.insert_str(self.repo_filter_cursor, &text);
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
                    let new_pos = prev_char_boundary(&self.filters.repo_filter_text, self.repo_filter_cursor);
                    self.filters.repo_filter_text.drain(new_pos..self.repo_filter_cursor);
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
                    self.repo_filter_cursor = prev_char_boundary(&self.filters.repo_filter_text, self.repo_filter_cursor);
                    cx.notify();
                }
            }
            "right" => {
                if self.repo_filter_cursor < self.filters.repo_filter_text.len() {
                    self.repo_filter_cursor = next_char_boundary(&self.filters.repo_filter_text, self.repo_filter_cursor);
                    cx.notify();
                }
            }
            _ => {
                if let Some(ref ch) = ks.key_char {
                    if !ks.modifiers.control && !ch.contains('\n') && !ch.contains('\r') {
                        self.filters.repo_filter_text.insert_str(self.repo_filter_cursor, ch);
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

    fn open_settings_modal(
        &mut self,
        section: Option<crate::ui::ui_state::SettingsSection>,
        cx: &mut Context<Self>,
    ) {
        if let Some(section) = section {
            self.nav.settings_section = section;
        }

        self.close_history_context_menu();
        self.nav.show_settings = true;
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
            SettingsField::GitUserName => self.repo.identity.user_name.as_str(),
            SettingsField::GitUserEmail => self.repo.identity.user_email.as_str(),
            SettingsField::GitDefaultBranch => {
                self.repo.identity.default_branch.as_deref().unwrap_or("")
            }
            SettingsField::AiModel => self.settings.ai.model.as_str(),
            SettingsField::AiApiKey => self.settings.ai.api_key.as_str(),
            SettingsField::AiSystemPrompt => self.settings.ai.system_prompt.as_str(),
            SettingsField::OpenRouterModelFilter => self.filters.openrouter_model_filter.as_str(),
        }
    }

    pub(crate) fn settings_field_cursor(&self, field: SettingsField) -> usize {
        match field {
            SettingsField::GitUserName => self.settings_modal.git_user_name_cursor,
            SettingsField::GitUserEmail => self.settings_modal.git_user_email_cursor,
            SettingsField::GitDefaultBranch => self.settings_modal.git_default_branch_cursor,
            SettingsField::AiModel => self.settings_modal.ai_model_cursor,
            SettingsField::AiApiKey => self.settings_modal.ai_api_key_cursor,
            SettingsField::AiSystemPrompt => self.settings_modal.ai_system_prompt_cursor,
            SettingsField::OpenRouterModelFilter => {
                self.settings_modal.openrouter_model_filter_cursor
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

        let multiline = matches!(field, SettingsField::AiSystemPrompt);

        match field {
            SettingsField::GitUserName => edit_string_field(
                &mut self.repo.identity.user_name,
                &mut self.settings_modal.git_user_name_cursor,
                multiline,
                event,
                cx,
            ),
            SettingsField::GitUserEmail => edit_string_field(
                &mut self.repo.identity.user_email,
                &mut self.settings_modal.git_user_email_cursor,
                multiline,
                event,
                cx,
            ),
            SettingsField::GitDefaultBranch => {
                let value = self
                    .repo
                    .identity
                    .default_branch
                    .get_or_insert_with(String::new);
                edit_string_field(
                    value,
                    &mut self.settings_modal.git_default_branch_cursor,
                    multiline,
                    event,
                    cx,
                );
            }
            SettingsField::AiModel => edit_string_field(
                &mut self.settings.ai.model,
                &mut self.settings_modal.ai_model_cursor,
                multiline,
                event,
                cx,
            ),
            SettingsField::AiApiKey => edit_string_field(
                &mut self.settings.ai.api_key,
                &mut self.settings_modal.ai_api_key_cursor,
                multiline,
                event,
                cx,
            ),
            SettingsField::AiSystemPrompt => edit_string_field(
                &mut self.settings.ai.system_prompt,
                &mut self.settings_modal.ai_system_prompt_cursor,
                multiline,
                event,
                cx,
            ),
            SettingsField::OpenRouterModelFilter => edit_string_field(
                &mut self.filters.openrouter_model_filter,
                &mut self.settings_modal.openrouter_model_filter_cursor,
                multiline,
                event,
                cx,
            ),
        }
    }

    fn set_settings_field_cursor(&mut self, field: SettingsField, cursor: usize) {
        match field {
            SettingsField::GitUserName => self.settings_modal.git_user_name_cursor = cursor,
            SettingsField::GitUserEmail => self.settings_modal.git_user_email_cursor = cursor,
            SettingsField::GitDefaultBranch => {
                self.settings_modal.git_default_branch_cursor = cursor
            }
            SettingsField::AiModel => self.settings_modal.ai_model_cursor = cursor,
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
        focused: bool,
        multiline: bool,
        focus_handle: &FocusHandle,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let is_empty = value.trim().is_empty();
        let border = if focused {
            theme::accent() // --focus-color: $blue
        } else {
            theme::surface_bg_alt() // --contrast-border in dark
        };

        // Build the display text with cursor
        let text_child: Div = if is_empty && !focused {
            // Placeholder
            div()
                .text_size(theme::z(12.0))
                .text_color(theme::text_muted())
                .child(placeholder.to_string())
        } else if focused {
            // Editable: show text with cursor indicator
            let cursor_pos = cursor.min(value.len());
            let before = &value[..cursor_pos];
            let after = &value[cursor_pos..];

            if multiline {
                // For multiline, render with line wrapping
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
        } else {
            // Has text, not focused
            if multiline {
                div()
                    .text_size(theme::z(12.0))
                    .text_color(theme::text_main())
                    .child(value.to_string())
            } else {
                div()
                    .text_size(theme::z(12.0))
                    .text_color(theme::text_main())
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .child(value.to_string())
            }
        };

        let height = if multiline { px(80.0) } else { px(25.0) }; // --text-field-height: 25px

        let is_summary = id == "commit-summary-field";

        let mut field = div()
            .id(SharedString::from(id.to_string()))
            .track_focus(focus_handle)
            .key_context("text-field")
            .w_full()
            .h(height)
            .bg(theme::bg()) // --background-color (dark canvas)
            .border_1()
            .border_color(border)
            .px(px(8.0))
            .cursor_text()
            .child(text_child);

        if multiline {
            // Description: top corners rounded, bottom corners flat (action bar attaches)
            field = field
                .rounded_t(theme::z(theme::CORNER_RADIUS))
                .rounded_b_none()
                .border_b_0()
                .py(px(6.0))
                .overflow_hidden();
        } else {
            // Summary: fully rounded
            field = field.rounded(theme::z(theme::CORNER_RADIUS)).items_center();
        }

        if is_summary {
            field = field.on_key_down(cx.listener(Self::handle_summary_key));
        } else {
            field = field.on_key_down(cx.listener(Self::handle_description_key));
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
            .cursor_pointer()
            .hover(|s| s.bg(theme::hover_bg()))
            .rounded(px(3.0))
            .w(px(18.0))
            .h(px(17.0))
            .items_center()
            .justify_center()
            .child(
                svg()
                    .path("icons/sparkles.svg")
                    .size(px(14.0))
                    .text_color(theme::text_muted()),
            )
            .on_click(cx.listener(|app, _evt, _win, cx| {
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

        // Auto-placeholder: "Update filename" for single file, else generic
        let summary_placeholder = if self.commit.summary.is_empty() {
            if let Some(snapshot) = &self.repo.snapshot {
                if snapshot.changes.len() == 1 {
                    let path = &snapshot.changes[0].path;
                    let filename = path.rsplit('/').next().unwrap_or(path);
                    let status = &snapshot.changes[0].status;
                    let verb = if status.contains('?') || status.contains('A') {
                        "Create"
                    } else if status.contains('D') {
                        "Delete"
                    } else {
                        "Update"
                    };
                    format!("{verb} {filename}")
                } else {
                    "Summary (required)".to_string()
                }
            } else {
                "Summary (required)".to_string()
            }
        } else {
            "Summary (required)".to_string()
        };

        // Summary field — editable single-line input
        let summary_field = self.render_text_field(
            "commit-summary-field",
            &self.commit.summary,
            &summary_placeholder,
            self.summary_cursor,
            summary_focused,
            false,
            &self.summary_focus,
            cx,
        );

        // Description field — editable multi-line input (no bottom radius, action bar attaches)
        let description_field = self.render_text_field(
            "commit-description-field",
            &self.commit.body,
            "Description",
            self.description_cursor,
            description_focused,
            true,
            &self.description_focus,
            cx,
        );

        // Description + action bar grouped together (shared border)
        let description_group = v_flex().w_full().child(description_field).child(action_bar);

        // Commit button label with file count
        let file_count = self
            .repo
            .snapshot
            .as_ref()
            .map(|s| {
                if self.commit.include_all {
                    s.changes.len()
                } else {
                    self.commit.included_files.len()
                }
            })
            .unwrap_or(0);

        let commit_label = if self.commit.ai_in_flight {
            "Generating commit details\u{2026}".to_string()
        } else if file_count > 0 {
            format!("Commit {file_count} files to {branch_name}")
        } else {
            format!("Commit to {branch_name}")
        };

        let can_commit = !self.commit.summary.trim().is_empty()
            && file_count > 0
            && !self.commit.ai_in_flight;

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

    fn render_workspace(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let sidebar_tab = self.nav.sidebar_tab;

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

        // Show file list panel only on History tab (Changes tab has sidebar file list)
        if sidebar_tab == SidebarTab::History && !diffs.is_empty() {
            let file_list = self.render_commit_file_list(diffs, selected_file, sidebar_tab, cx);
            h_resizable("workspace-panels")
                .child(
                    resizable_panel()
                        .size(px(200.0))
                        .size_range(px(120.0)..px(350.0))
                        .child(file_list),
                )
                .child(
                    resizable_panel().child(crate::ui::workspace::render_workspace(
                        selected_file,
                        selected_diff,
                    )),
                )
        } else {
            h_resizable("workspace-panels").child(resizable_panel().child(
                crate::ui::workspace::render_workspace(selected_file, selected_diff),
            ))
        }
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
                        let vh = view.clone();

                        h_flex()
                            .id(SharedString::from(format!("commit-file-{}", entry.path)))
                            .w_full()
                            .h(theme::z(28.0))
                            .px(theme::z(10.0))
                            .items_center()
                            .bg(if is_selected {
                                theme::hover_bg()
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
                            .hover(|s| s.bg(theme::hover_bg()))
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
                            .child(format!("{} changed files", diffs.len())),
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

    fn render_active_dialog(&self, cx: &mut Context<Self>) -> Div {
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

        let dialog_content = match &self.nav.active_dialog {
            ActiveDialog::CreateBranch => {
                let branch_name = &self.repo.new_branch_name;
                let current = self
                    .repo
                    .snapshot
                    .as_ref()
                    .map(|s| s.repo.current_branch.as_str())
                    .unwrap_or("main");

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
                                            .w_full()
                                            .h(theme::z(28.0))
                                            .px(theme::z(8.0))
                                            .items_center()
                                            .rounded(theme::z(theme::CORNER_RADIUS))
                                            .bg(theme::bg())
                                            .border_1()
                                            .border_color(theme::accent())
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
                                            ),
                                    ),
                            )
                            // Starting point
                            .child(
                                div()
                                    .text_size(theme::z(11.0))
                                    .text_color(theme::text_muted())
                                    .child(format!("Based on current branch: {current}")),
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
                                    .bg(theme::commit_button_bg())
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::commit_button_hover_bg()))
                                    .child(
                                        div()
                                            .text_size(theme::z(12.0))
                                            .text_color(theme::commit_button_text())
                                            .child("Create Branch"),
                                    )
                                    .on_click(cx.listener(|app, _evt, _win, cx| {
                                        app.nav.active_dialog = ActiveDialog::None;
                                        app.create_branch(cx);
                                    })),
                            ),
                    )
            }
            ActiveDialog::DiscardChanges { paths } => {
                let file_list = if paths.len() <= 10 {
                    paths.iter().map(|p| format!("  \u{2022} {p}")).collect::<Vec<_>>().join("\n")
                } else {
                    let shown: Vec<_> = paths.iter().take(10).map(|p| format!("  \u{2022} {p}")).collect();
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
                                    .hover(|s| s.bg(gpui::Hsla::from(gpui::rgb(0xff6961))))
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
                            .gap(theme::z(8.0))
                            .child(
                                div()
                                    .text_size(theme::z(12.0))
                                    .text_color(theme::text_main())
                                    .child("You have uncommitted changes. What would you like to do?"),
                            )
                            .child(
                                div()
                                    .text_size(theme::z(12.0))
                                    .text_color(theme::text_muted())
                                    .child("Your changes will be stashed before switching."),
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
                                    .id("stash-cancel")
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
                                    .child(div().text_size(theme::z(12.0)).text_color(theme::commit_button_text()).child("Stash & Switch"))
                                    .on_click(cx.listener(move |app, _evt, _win, cx| {
                                        app.nav.active_dialog = ActiveDialog::None;
                                        // Stash then switch
                                        if let Some(path) = app.repo_path().map(PathBuf::from) {
                                            let tx = app.event_tx.clone();
                                            let git = GitClient::new();
                                            let branch = target.clone();
                                            thread::spawn(move || {
                                                let _ = git.stash_all(&path);
                                                let res = git.switch_branch(&path, &branch).map_err(|e| e.to_string());
                                                tx.send(AppEvent::BranchSwitched(res, branch));
                                            });
                                        }
                                        cx.notify();
                                    }))
                            }),
                    )
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
                    .top_0()
                    .left_0()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .child(dialog_content),
            )
    }

    fn render_network_dropdown(&self, cx: &mut Context<Self>) -> Div {
        let snapshot = self.repo.snapshot.as_ref();
        let remote_name = snapshot
            .and_then(|s| s.repo.remote_name.as_deref())
            .unwrap_or("origin");

        let actions: Vec<(NetworkAction, IconName, String)> = vec![
            (
                NetworkAction::Fetch,
                IconName::Loader,
                format!("Fetch {remote_name}"),
            ),
            (
                NetworkAction::Pull,
                IconName::ArrowDown,
                format!("Pull {remote_name}"),
            ),
            (
                NetworkAction::Push,
                IconName::ArrowUp,
                format!("Push {remote_name}"),
            ),
        ];

        let mut dropdown = v_flex()
            .absolute()
            .top(theme::z(theme::TOOLBAR_HEIGHT))
            .right_0()
            .w(px(220.0))
            .bg(theme::panel_bg())
            .border_1()
            .border_color(theme::toolbar_button_border())
            .rounded_b(theme::z(theme::CORNER_RADIUS))
            .shadow_lg();

        for (action, icon, label) in actions {
            let is_current = snapshot
                .map(|s| NetworkAction::from_snapshot(s) == action)
                .unwrap_or(action == NetworkAction::Fetch);

            dropdown = dropdown.child(
                h_flex()
                    .id(SharedString::from(format!("net-{label}")))
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
                    .child(
                        Icon::new(icon)
                            .size(px(14.0))
                            .text_color(theme::text_main()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(theme::z(theme::FONT_SIZE))
                            .text_color(theme::text_main())
                            .child(label),
                    )
                    .on_click(cx.listener(move |app, _evt, _win, cx| {
                        app.nav.show_network_dropdown = false;
                        app.handle_toolbar_action(ToolbarAction::RunNetworkAction(action), cx);
                    })),
            );
        }

        dropdown
    }

    fn render_repo_selector_panel(&self, cx: &mut Context<Self>) -> Div {
        let recent_repos = self.settings.recent_repos.clone();
        let current_repo = self
            .repo
            .snapshot
            .as_ref()
            .map(|s| s.repo.name.clone())
            .unwrap_or_default();

        // --- Header: current repo + caret up ---
        let header = h_flex()
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
        let filter_bar = h_flex()
            .w_full()
            .flex_shrink_0()
            .px(px(10.0))
            .py(px(10.0))
            .gap(px(8.0))
            .items_center()
            // Filter input placeholder
            .child(
                h_flex()
                    .flex_1()
                    .h(px(28.0))
                    .px(px(8.0))
                    .items_center()
                    .gap(px(6.0))
                    .rounded(theme::z(theme::CORNER_RADIUS))
                    .border_1()
                    .border_color(theme::accent())
                    .bg(theme::bg())
                    .child(
                        Icon::new(IconName::Search)
                            .size(px(14.0))
                            .text_color(theme::text_muted()),
                    )
                    .child(
                        div()
                            .text_size(theme::z(theme::FONT_SIZE))
                            .text_color(theme::text_muted())
                            .child("Filter"),
                    ),
            )
            // Add button
            .child(
                div()
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
            div().flex_1().child(
                div()
                    .w_full()
                    .py(px(20.0))
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_size(theme::z(12.0))
                            .text_color(theme::text_muted())
                            .child("No recent repositories"),
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
                                let path_clone = repo_path.clone();
                                let vh = view.clone();

                                h_flex()
                                    .id(SharedString::from(format!(
                                        "repo-{}",
                                        repo_path.to_string_lossy()
                                    )))
                                    .w_full()
                                    .h(px(40.0))
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
            .child(header)
            .child(filter_bar)
            .child(section_header)
            .child(repo_list)
    }

    // ------------------------------------------------------------------
    // Branch selector (full-width panel)
    // ------------------------------------------------------------------

    fn render_branch_selector_overlay(&self, branch_filter_focused: bool, cx: &mut Context<Self>) -> Div {
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
            .w(px(300.0))
            .bottom_0()
            .shadow_lg();

        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(backdrop)
            .child(panel)
    }

    fn render_branch_selector_panel(&self, branch_filter_focused: bool, cx: &mut Context<Self>) -> Div {
        let snapshot = self.repo.snapshot.as_ref();
        let current_branch = snapshot
            .map(|s| s.repo.current_branch.clone())
            .unwrap_or_else(|| "main".to_string());
        let branches: Vec<BranchInfo> = snapshot.map(|s| s.branches.clone()).unwrap_or_default();

        // Separate local branches, filtered by search text
        let filter = self.filters.branch_filter_text.to_lowercase();
        let local_branches: Vec<&BranchInfo> = branches
            .iter()
            .filter(|b| !b.is_remote)
            .filter(|b| filter.is_empty() || b.name.to_lowercase().contains(&filter))
            .collect();

        // Find default branch (current one)
        let default_branch = local_branches
            .iter()
            .find(|b| b.is_current)
            .map(|b| b.name.clone())
            .unwrap_or_else(|| current_branch.clone());

        // --- Header: Current Branch + caret up ---
        let header = h_flex()
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

        // --- Tab bar: Branches | Pull Requests ---
        let tab_bar = h_flex()
            .w_full()
            .flex_shrink_0()
            .border_b_1()
            .border_color(theme::toolbar_button_border())
            .child(
                h_flex()
                    .flex_1()
                    .h(px(34.0))
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .border_b_2()
                    .border_color(theme::accent())
                    .hover(|s| s.bg(theme::hover_bg()))
                    .child(
                        div()
                            .text_size(theme::z(theme::FONT_SIZE))
                            .text_color(theme::text_main())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Branches"),
                    ),
            )
            ;

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

                let border_color = if focused { theme::accent() } else { theme::surface_bg_alt() };

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
                        .child(div().text_color(theme::text_main()).whitespace_nowrap().child(before.to_string()))
                        .child(if focused {
                            div().w(px(1.0)).h(px(14.0)).bg(theme::text_main()).flex_shrink_0().into_any_element()
                        } else {
                            div().into_any_element()
                        })
                        .child(div().text_color(theme::text_main()).whitespace_nowrap().child(after.to_string()))
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
            .child(
                div()
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
                        // Pre-fill the new branch name from filter text
                        app.repo.new_branch_name = app.filters.branch_filter_text.clone();
                        app.nav.active_dialog = ActiveDialog::CreateBranch;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_size(theme::z(theme::FONT_SIZE))
                            .text_color(theme::text_main())
                            .child("New Branch"),
                    ),
            );

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
            div().flex_1().child(
                div()
                    .w_full()
                    .py(px(20.0))
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_size(theme::z(12.0))
                            .text_color(theme::text_muted())
                            .child("No branches"),
                    ),
            )
        } else {
            let count = items.len();
            let view = cx.entity().clone();
            div().flex_1().min_h_0().child(
                uniform_list("branch-list", count, {
                    move |range, _win, _cx| {
                        range
                            .map(|ix| {
                                match &items[ix] {
                                    BranchListItem::SectionHeader(title) => {
                                        div()
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
                                            .into_any_element()
                                    }
                                    BranchListItem::Branch(branch) => {
                                        let is_current = branch.is_current;
                                        let name = branch.name.clone();
                                        let ctx_name = branch.name.clone();
                                        let vh = view.clone();

                                        let row = h_flex()
                                            .id(SharedString::from(format!("branch-{}", branch.name)))
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
                                            .on_click(move |_evt, _win, cx| {
                                                let name = name.clone();
                                                vh.update(cx, |app, cx| {
                                                    if !app
                                                        .repo
                                                        .snapshot
                                                        .as_ref()
                                                        .map(|s| s.repo.current_branch == name)
                                                        .unwrap_or(false)
                                                    {
                                                        app.repo.branch_target = name;
                                                        app.switch_branch(cx);
                                                    }
                                                    app.nav.show_branch_selector = false;
                                                    cx.notify();
                                                });
                                            });

                                        crate::ui::branch_context_menu::bind_branch_context_click(
                                            row,
                                            view.clone(),
                                            ctx_name,
                                        )
                                        .into_any_element()
                                    }
                                }
                            })
                            .collect()
                    }
                })
                .flex_1()
                .with_sizing_behavior(ListSizingBehavior::Infer),
            )
        };

        // --- Bottom bar: merge prompt ---
        let bottom_bar = h_flex()
            .w_full()
            .h(px(40.0))
            .flex_shrink_0()
            .border_t_1()
            .border_color(theme::toolbar_button_border())
            .px(px(10.0))
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .child(
                Icon::new(IconName::GitHub)
                    .size(px(16.0))
                    .text_color(theme::text_muted()),
            )
            .child(
                h_flex()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(theme::z(theme::FONT_SIZE))
                            .text_color(theme::text_muted())
                            .child("Choose a branch to merge into"),
                    )
                    .child(
                        div()
                            .text_size(theme::z(theme::FONT_SIZE))
                            .text_color(theme::text_main())
                            .font_weight(FontWeight::BOLD)
                            .child(default_branch),
                    ),
            );

        v_flex()
            .size_full()
            .bg(theme::panel_bg())
            .child(header)
            .child(tab_bar)
            .child(filter_bar)
            .child(branch_list)
            .child(bottom_bar)
    }
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

fn short_commit_label(oid: &str) -> &str {
    &oid[..oid.len().min(7)]
}

fn commit_diff_clipboard_text(diffs: &[DiffEntry]) -> String {
    diffs.iter()
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

// ---------------------------------------------------------------------------
// Text input helpers
// ---------------------------------------------------------------------------

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
