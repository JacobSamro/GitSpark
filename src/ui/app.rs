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
use crate::git::{GitClient, inferred_clone_directory_name, safe_repository_directory_name};
use crate::models::{
    AiProvider, AppSettings, BranchComparison, ChangeEntry, CommitInfo, CommitSuggestion,
    CreateRepositoryOptions, DiffEntry, GitIdentity, GitOperationKind,
    INVALID_GIT_AUTHOR_NAME_MESSAGE, RemoteModelOption, RepoSnapshot, git_author_name_is_valid,
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

mod branch_dialogs;
mod branch_selector;
mod change_dialogs;
mod dialogs;
mod helpers;
mod operations;
mod repo_selector;
mod tag_dialogs;
mod ui_shell;
pub(crate) use helpers::diff_line_stats;

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
    GitOperationControlCompleted(Result<RepoSnapshot, String>, String),
    CommitDiffCopied(String, Result<String, String>),
    Automation(automation::AutomationRequest),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RepositoryField {
    CreateName,
    CreateDescription,
    CreatePath,
    CreateBranchName,
    CloneUrl,
    CloneName,
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

/// Sender wrapper that sets an atomic flag after queueing,
/// so the poll timer can skip acquiring the app lock when idle.
#[derive(Clone)]
pub(crate) struct NotifySender {
    tx: Sender<AppEvent>,
    pending: Arc<AtomicBool>,
}

impl NotifySender {
    pub(crate) fn send(&self, event: AppEvent) {
        let _ = self.tx.send(event);
        self.pending.store(true, Ordering::Release);
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
    pub(crate) repository_create_branch_cursor: usize,
    pub(crate) repository_create_branch_selection: Option<usize>,
    pub(crate) repository_clone_url_cursor: usize,
    pub(crate) repository_clone_url_selection: Option<usize>,
    pub(crate) repository_clone_name_cursor: usize,
    pub(crate) repository_clone_name_selection: Option<usize>,
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
            repository_create_branch_cursor: 0,
            repository_create_branch_selection: None,
            repository_clone_url_cursor: 0,
            repository_clone_url_selection: None,
            repository_clone_name_cursor: 0,
            repository_clone_name_selection: None,
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
}
