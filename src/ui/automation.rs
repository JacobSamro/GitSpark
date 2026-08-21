use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use gpui::{Context, KeyDownEvent, Keystroke, Modifiers};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::git::inferred_clone_directory_name;
use crate::models::{AiProvider, GitOperationKind};
use crate::ui::app::SettingsAction;
use crate::ui::app::{AppEvent, GitSparkApp, NotifySender, SidebarAction, ToolbarAction};
use crate::ui::branch_context_menu::BranchContextAction;
use crate::ui::changes_context_menu::ChangesContextAction;
use crate::ui::diff_line_selection::DiffLineSelection;
use crate::ui::domain_state::NetworkAction;
use crate::ui::history_context_menu::HistoryContextMenuAction;
use crate::ui::ids::stable_id_slug;
use crate::ui::theme;
use crate::ui::settings_modal::SettingsField;
use crate::ui::ui_state::{ActiveDialog, BranchSelectorMode, SidebarTab};
use crate::ui::ui_state::{OpenRouterModelsState, SettingsScope, SettingsSection};

const DEFAULT_ADDR: &str = "127.0.0.1:7878";
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct AutomationRequest {
    pub command: AutomationCommand,
    pub respond_to: mpsc::Sender<AutomationResponse>,
}

pub(crate) struct AutomationHandle {
    shutdown: Arc<AtomicBool>,
    _thread: JoinHandle<()>,
}

impl Drop for AutomationHandle {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
    }
}

#[derive(Clone, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub(crate) enum AutomationCommand {
    Ping,
    Snapshot,
    TestTree,
    ClipboardText,
    Query {
        selector: AutomationSelector,
    },
    Click {
        selector: AutomationSelector,
    },
    Fill {
        selector: AutomationSelector,
        text: String,
    },
    TypeText {
        selector: AutomationSelector,
        text: String,
    },
    PressKeys {
        selector: AutomationSelector,
        keys: Vec<String>,
    },
    OpenRepo {
        path: PathBuf,
    },
    RefreshRepo,
    SelectTab {
        tab: AutomationSidebarTab,
    },
    SelectChange {
        path: String,
    },
    SelectCommit {
        oid: String,
    },
    SetCommitMessage {
        summary: String,
        #[serde(default)]
        body: String,
    },
    CommitAll,
    UndoLastCommit,
    StashAll,
    StashPop,
    ShowSettings {
        show: bool,
    },
    ShowGlobalSettings {
        show: bool,
    },
    ShowRepositorySettings {
        show: bool,
    },
    ShowCreateRepository,
    ShowCloneRepository,
    ShowRepoSelector {
        show: bool,
    },
    SetRepoFilter {
        text: String,
    },
    SetBranchFilter {
        text: String,
    },
    SetSettingsSection {
        section: AutomationSettingsSection,
    },
    SetSettingsField {
        field: AutomationSettingsField,
        text: String,
    },
    SaveSettings {
        section: AutomationSettingsSection,
    },
    ChangeAiProvider {
        provider: AutomationAiProvider,
    },
    GenerateAiCommit,
    ChangeAction {
        path: String,
        action: AutomationChangeAction,
    },
    HistoryAction {
        oid: String,
        action: AutomationHistoryAction,
    },
    BranchAction {
        name: String,
        action: AutomationBranchAction,
    },
    CreateBranch {
        name: String,
    },
    MergeBranch {
        name: String,
    },
    RebaseBranch {
        name: String,
    },
    UpdateFromDefaultBranch,
    CompareBranch {
        name: String,
    },
    CompareCurrentBranchOnGithub,
    NetworkAction {
        action: AutomationNetworkAction,
    },
    ContinueGitOperation,
    SkipRebaseOperation,
    AbortGitOperation,
}

#[derive(Clone, Deserialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub(crate) enum AutomationSelector {
    TestId { value: String },
    Text { value: String },
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AutomationSidebarTab {
    Changes,
    History,
}

impl From<AutomationSidebarTab> for SidebarTab {
    fn from(tab: AutomationSidebarTab) -> Self {
        match tab {
            AutomationSidebarTab::Changes => Self::Changes,
            AutomationSidebarTab::History => Self::History,
        }
    }
}

impl From<SidebarTab> for AutomationSidebarTab {
    fn from(tab: SidebarTab) -> Self {
        match tab {
            SidebarTab::Changes => Self::Changes,
            SidebarTab::History => Self::History,
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AutomationNetworkAction {
    Fetch,
    Pull,
    Push,
    PublishRepository,
}

impl From<AutomationNetworkAction> for NetworkAction {
    fn from(action: AutomationNetworkAction) -> Self {
        match action {
            AutomationNetworkAction::Fetch => Self::Fetch,
            AutomationNetworkAction::Pull => Self::Pull,
            AutomationNetworkAction::Push => Self::Push,
            AutomationNetworkAction::PublishRepository => Self::PublishRepository,
        }
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AutomationSettingsSection {
    Remote,
    IgnoredFiles,
    Git,
    Ai,
    Appearance,
    Integrations,
}

impl From<AutomationSettingsSection> for SettingsSection {
    fn from(section: AutomationSettingsSection) -> Self {
        match section {
            AutomationSettingsSection::Remote => Self::Remote,
            AutomationSettingsSection::IgnoredFiles => Self::IgnoredFiles,
            AutomationSettingsSection::Git => Self::Git,
            AutomationSettingsSection::Ai => Self::Ai,
            AutomationSettingsSection::Appearance => Self::Appearance,
            AutomationSettingsSection::Integrations => Self::Integrations,
        }
    }
}

impl From<SettingsSection> for AutomationSettingsSection {
    fn from(section: SettingsSection) -> Self {
        match section {
            SettingsSection::Remote => Self::Remote,
            SettingsSection::IgnoredFiles => Self::IgnoredFiles,
            SettingsSection::Git => Self::Git,
            SettingsSection::Ai => Self::Ai,
            SettingsSection::Appearance => Self::Appearance,
            SettingsSection::Integrations => Self::Integrations,
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AutomationSettingsField {
    RemoteUrl,
    IgnoredFiles,
    GitUserName,
    GitUserEmail,
    GitDefaultBranch,
    AiModel,
    AiEndpoint,
    AiApiKey,
    AiSystemPrompt,
    OpenRouterModelFilter,
}

impl From<AutomationSettingsField> for SettingsField {
    fn from(field: AutomationSettingsField) -> Self {
        match field {
            AutomationSettingsField::RemoteUrl => Self::RemoteUrl,
            AutomationSettingsField::IgnoredFiles => Self::IgnoredFiles,
            AutomationSettingsField::GitUserName => Self::GitUserName,
            AutomationSettingsField::GitUserEmail => Self::GitUserEmail,
            AutomationSettingsField::GitDefaultBranch => Self::GitDefaultBranch,
            AutomationSettingsField::AiModel => Self::AiModel,
            AutomationSettingsField::AiEndpoint => Self::AiEndpoint,
            AutomationSettingsField::AiApiKey => Self::AiApiKey,
            AutomationSettingsField::AiSystemPrompt => Self::AiSystemPrompt,
            AutomationSettingsField::OpenRouterModelFilter => Self::OpenRouterModelFilter,
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AutomationAiProvider {
    OpenRouter,
    OpenAiCompatible,
}

impl From<AutomationAiProvider> for AiProvider {
    fn from(provider: AutomationAiProvider) -> Self {
        match provider {
            AutomationAiProvider::OpenRouter => Self::OpenRouter,
            AutomationAiProvider::OpenAiCompatible => Self::OpenAICompatible,
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AutomationChangeAction {
    Discard,
    PromptDiscard,
    IgnorePath,
    IgnoreFolder,
    IgnoreExtension,
    CopyFullPath,
    CopyRelativePath,
    RevealInFinder,
    OpenInEditor,
    OpenWithDefault,
    ViewOnGithub,
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AutomationHistoryAction {
    ResetToCommit,
    CheckoutCommit,
    RevertChangesInCommit,
    CreateBranchFromCommit,
    CreateTag,
    DeleteTag,
    CherryPickCommit,
    CopySha,
    CopyDiff,
    CopyTag,
    ViewOnGithub,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AutomationBranchAction {
    Rename,
    CopyName,
    Delete,
    ViewOnGithub,
}

#[derive(Serialize)]
pub(crate) struct AutomationResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl AutomationResponse {
    fn success<T: Serialize>(value: T) -> Self {
        match serde_json::to_value(value) {
            Ok(result) => Self {
                ok: true,
                result: Some(result),
                error: None,
            },
            Err(err) => Self::failure(format!("failed to encode automation response: {err}")),
        }
    }

    fn failure(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            result: None,
            error: Some(error.into()),
        }
    }
}

#[derive(Serialize)]
struct AutomationSnapshot {
    repo: Option<AutomationRepoSnapshot>,
    test_tree: AutomationNode,
    sidebar_tab: AutomationSidebarTab,
    selected_change: Option<String>,
    selected_commit: Option<String>,
    selected_commit_file: Option<String>,
    diff_hide_whitespace_changes: bool,
    diff_show_side_by_side: bool,
    selected_diff_visible_line_count: Option<usize>,
    selected_diff_selectable_line_count: usize,
    selected_diff_selected_line_count: usize,
    selected_diff_is_image: bool,
    selected_diff_is_submodule: bool,
    show_settings: bool,
    show_repo_selector: bool,
    show_branch_selector: bool,
    show_network_dropdown: bool,
    active_dialog: String,
    network_action: Option<String>,
    ai_in_flight: bool,
    commit_summary: String,
    commit_body: String,
    repo_filter_text: String,
    branch_filter_text: String,
    status_message: String,
    error_message: String,
    compare: Option<AutomationCompareSnapshot>,
    operation: Option<AutomationOperationSnapshot>,
    settings_scope: AutomationSettingsScope,
    settings_section: AutomationSettingsSection,
    git_user_name: String,
    git_user_email: String,
    git_default_branch: Option<String>,
    git_pull_rebase: Option<bool>,
    ai_provider: String,
    ai_model: String,
    ai_endpoint: String,
    ai_system_prompt: String,
    repo_remote_name: Option<String>,
    repo_remote_url: String,
    repo_ignored_files_text: String,
    menu_availability: AutomationMenuAvailability,
}

#[derive(Serialize)]
struct AutomationMenuAvailability {
    has_repository: bool,
    fetch: bool,
    pull: bool,
    push: bool,
    publish_repository: bool,
    view_repository_on_github: bool,
    create_branch: bool,
    modify_current_branch: bool,
    compare_on_github: bool,
    change_worktree: bool,
}

impl From<crate::MenuAvailability> for AutomationMenuAvailability {
    fn from(availability: crate::MenuAvailability) -> Self {
        Self {
            has_repository: availability.has_repository,
            fetch: availability.fetch,
            pull: availability.pull,
            push: availability.push,
            publish_repository: availability.publish_repository,
            view_repository_on_github: availability.view_repository_on_github,
            create_branch: availability.create_branch,
            modify_current_branch: availability.modify_current_branch,
            compare_on_github: availability.compare_on_github,
            change_worktree: availability.change_worktree,
        }
    }
}

#[derive(Serialize)]
struct AutomationCompareSnapshot {
    current_branch: String,
    target_branch: String,
    ahead: usize,
    behind: usize,
    commits: Vec<AutomationCommit>,
    files: Vec<AutomationChange>,
}

#[derive(Serialize)]
struct AutomationOperationSnapshot {
    kind: String,
    current_branch: String,
    target_branch: Option<String>,
    conflicted_files: Vec<AutomationChange>,
    can_continue: bool,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum AutomationSettingsScope {
    Global,
    Repository,
}

impl From<SettingsScope> for AutomationSettingsScope {
    fn from(scope: SettingsScope) -> Self {
        match scope {
            SettingsScope::Global => Self::Global,
            SettingsScope::Repository => Self::Repository,
        }
    }
}

#[derive(Serialize)]
struct AutomationRepoSnapshot {
    path: String,
    name: String,
    current_branch: String,
    head_oid: Option<String>,
    remote_name: Option<String>,
    has_github_remote: bool,
    ahead: usize,
    behind: usize,
    stash_count: usize,
    changes: Vec<AutomationChange>,
    branches: Vec<AutomationBranch>,
    history: Vec<AutomationCommit>,
    tags: Vec<String>,
}

#[derive(Serialize)]
struct AutomationChange {
    path: String,
    status: String,
}

#[derive(Serialize)]
struct AutomationBranch {
    name: String,
    is_current: bool,
    is_remote: bool,
}

#[derive(Serialize)]
struct AutomationCommit {
    oid: String,
    short_oid: String,
    summary: String,
    author_name: String,
    date: String,
    is_head: bool,
    tags: Vec<String>,
}

#[derive(Clone, Serialize)]
struct AutomationNode {
    id: String,
    role: AutomationRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    test_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    visible: bool,
    enabled: bool,
    selected: bool,
    #[serde(skip_serializing)]
    action: Option<AutomationNodeAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    children: Vec<AutomationNode>,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum AutomationRole {
    App,
    Button,
    List,
    ListItem,
    Status,
    Tab,
    Textbox,
}

#[derive(Clone)]
enum AutomationNodeAction {
    CommitAll,
    Network(NetworkAction),
    SelectChange(String),
    SelectCommit(String),
    SelectTab(SidebarTab),
    RevealSelectedBinaryInFinder,
    OpenSelectedBinaryWithDefaultProgram,
    RevealSelectedImageInFinder,
    OpenSelectedImageWithDefaultProgram,
    OpenSelectedSubmodule,
    RevealSelectedSubmodule,
    ToggleDiffLine(DiffLineSelection),
    DiscardSelectedDiffLines,
    ToggleDiffOptionsMenu,
    ShowUnifiedDiff,
    ToggleSideBySideDiff,
    ToggleHideWhitespaceChanges,
    SetBranchFilter,
    SetCommitBody,
    SetCommitSummary,
    OpenIdentitySettings,
    SetRepoFilter,
    OpenRecentRepo(PathBuf),
    SetSettingsField(SettingsField),
    SetSettingsSection(SettingsSection),
    ShowBranchSelector(bool),
    ShowRepoSelector(bool),
    ShowSettings(bool),
    SwitchBranch(String),
    StartCreateBranch,
    SetNewBranchName,
    SetBranchSwitchMode(bool),
    ConfirmCreateBranch,
    ConfirmRenameBranch,
    ConfirmDeleteBranch,
    ConfirmCreateTag,
    SelectTagToDelete(String),
    ConfirmDeleteTag,
    ConfirmResetToCommit,
    ConfirmStashChanges,
    ConfirmStashAndSwitch,
    ShowRestoreStash,
    RestoreStash,
    ShowDiscardStash,
    ConfirmDiscardStash,
    SetCreateRepositoryName,
    SetCreateRepositoryDescription,
    SetCreateRepositoryPath,
    SetCreateRepositoryBranch,
    SetCreateRepositoryGitignore(String),
    SetCreateRepositoryLicense(String),
    ToggleCreateRepositoryReadme,
    ToggleCreateRepositoryInitialCommit,
    ConfirmCreateRepository,
    SetCloneRepositoryUrl,
    SetCloneRepositoryName,
    SetCloneRepositoryPath,
    ConfirmCloneRepository,
    SetPublishName,
    SetPublishDescription,
    TogglePublishPrivate,
    ConfirmPublishRepository,
    SaveRemoteSettings,
    SaveIgnoredFilesSettings,
    SaveGitSettings,
    SaveAiSettings,
    SetGitConfigScope(bool),
    TogglePullRebase,
    ChangeAiProvider(AiProvider),
    SetAppearance(theme::Appearance),
    ShowOpenRouterModelPicker,
    SelectOpenRouterModel(String),
    GenerateAiCommit,
    UndoLastCommit,
    ExitCompare,
    MergeComparedBranch,
    ContinueGitOperation,
    SkipRebaseOperation,
    AbortGitOperation,
    OpenConflictInEditor(String),
    RevealConflictFile(String),
    MarkConflictResolved(String),
    CancelDialog,
    ConfirmDiscardChanges,
    ChangeFile(String, AutomationChangeAction),
    History(String, AutomationHistoryAction),
    Branch(String, AutomationBranchAction),
}

pub(crate) fn maybe_start(event_tx: NotifySender) -> Option<AutomationHandle> {
    let config = match AutomationConfig::from_env() {
        Ok(Some(config)) => config,
        Ok(None) => return None,
        Err(err) => {
            eprintln!("GitSpark automation disabled: {err}");
            return None;
        }
    };

    match start_server(config, event_tx) {
        Ok(handle) => Some(handle),
        Err(err) => {
            eprintln!("GitSpark automation failed to start: {err}");
            None
        }
    }
}

impl GitSparkApp {
    pub(crate) fn handle_automation_command(
        &mut self,
        command: AutomationCommand,
        cx: &mut Context<Self>,
    ) -> AutomationResponse {
        self.process_events(cx);

        match command {
            AutomationCommand::Ping => AutomationResponse::success(json!({ "pong": true })),
            AutomationCommand::Snapshot => AutomationResponse::success(self.automation_snapshot()),
            AutomationCommand::TestTree => AutomationResponse::success(self.automation_test_tree()),
            AutomationCommand::ClipboardText => {
                let text = cx.read_from_clipboard().and_then(|item| item.text());
                AutomationResponse::success(json!({ "text": text }))
            }
            AutomationCommand::Query { selector } => {
                AutomationResponse::success(self.query_automation_nodes(&selector))
            }
            AutomationCommand::Click { selector } => self.click_automation_node(selector, cx),
            AutomationCommand::Fill { selector, text } => {
                self.fill_automation_node(selector, text, cx)
            }
            AutomationCommand::TypeText { selector, text } => {
                self.type_text_automation_node(selector, text, cx)
            }
            AutomationCommand::PressKeys { selector, keys } => {
                self.press_keys_automation_node(selector, keys, cx)
            }
            AutomationCommand::OpenRepo { path } => {
                self.handle_sidebar_action(SidebarAction::OpenRepo(path), cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::RefreshRepo => {
                self.refresh_repo(cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::SelectTab { tab } => {
                self.nav.sidebar_tab = tab.into();
                cx.notify();
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::SelectChange { path } => {
                self.handle_sidebar_action(SidebarAction::SelectChange(path), cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::SelectCommit { oid } => {
                self.handle_sidebar_action(SidebarAction::SelectCommit(oid), cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::SetCommitMessage { summary, body } => {
                self.commit.summary = summary;
                self.commit.body = body;
                cx.notify();
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::CommitAll => {
                self.handle_sidebar_action(SidebarAction::CommitAll, cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::UndoLastCommit => {
                self.undo_last_commit(cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::StashAll => {
                self.show_stash_changes_dialog(cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::StashPop => {
                self.show_restore_stash_dialog(cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::ShowSettings { show } => {
                if show {
                    self.open_global_settings_modal(None, cx);
                } else {
                    self.close_settings_modal();
                }
                cx.notify();
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::ShowGlobalSettings { show } => {
                if show {
                    self.open_global_settings_modal(None, cx);
                } else {
                    self.close_settings_modal();
                }
                cx.notify();
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::ShowRepositorySettings { show } => {
                if show {
                    self.open_repository_settings_modal(None, cx);
                } else {
                    self.close_settings_modal();
                }
                cx.notify();
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::ShowCreateRepository => {
                self.open_create_repository_dialog(cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::ShowCloneRepository => {
                self.open_clone_repository_dialog(cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::ShowRepoSelector { show } => {
                self.nav.show_repo_selector = show;
                if show {
                    self.nav.show_branch_selector = false;
                    self.nav.branch_selector_mode = BranchSelectorMode::Switch;
                    self.repo.pending_cherry_pick_oid = None;
                    self.nav.show_network_dropdown = false;
                }
                cx.notify();
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::SetRepoFilter { text } => {
                self.filters.repo_filter_text = text;
                cx.notify();
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::SetBranchFilter { text } => {
                self.filters.branch_filter_text = text;
                cx.notify();
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::SetSettingsSection { section } => {
                self.set_automation_settings_section(section.into(), cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::SetSettingsField { field, text } => {
                self.set_automation_settings_field(field.into(), text, cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::SaveSettings { section } => {
                match section {
                    AutomationSettingsSection::Git => {
                        self.handle_settings_action(SettingsAction::SaveGitConfig, cx);
                    }
                    AutomationSettingsSection::Remote => {
                        self.handle_settings_action(SettingsAction::SaveRemote, cx);
                    }
                    AutomationSettingsSection::IgnoredFiles => {
                        self.handle_settings_action(SettingsAction::SaveIgnoredFiles, cx);
                    }
                    AutomationSettingsSection::Ai => {
                        self.handle_settings_action(SettingsAction::SaveAiSettings, cx);
                    }
                    AutomationSettingsSection::Appearance
                    | AutomationSettingsSection::Integrations => {
                        return AutomationResponse::failure(
                            "settings section does not have a save action",
                        );
                    }
                }
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::ChangeAiProvider { provider } => {
                self.handle_settings_action(SettingsAction::ChangeProvider(provider.into()), cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::GenerateAiCommit => {
                self.handle_sidebar_action(SidebarAction::GenerateAiCommit, cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::ChangeAction { path, action } => {
                self.perform_change_action(path, action, cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::HistoryAction { oid, action } => {
                self.perform_history_action(oid, action, cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::BranchAction { name, action } => {
                self.perform_branch_action(name, action, cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::CreateBranch { name } => {
                self.repo.new_branch_name = name;
                self.create_branch(cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::MergeBranch { name } => {
                self.repo.merge_target = name;
                self.merge_branch(cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::RebaseBranch { name } => {
                self.repo.merge_target = name;
                self.rebase_branch(cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::UpdateFromDefaultBranch => {
                self.update_from_default_branch(cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::CompareBranch { name } => {
                self.compare_branch(name, cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::CompareCurrentBranchOnGithub => {
                self.menu_compare_current_branch_on_github(cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::NetworkAction { action } => {
                self.handle_toolbar_action(ToolbarAction::RunNetworkAction(action.into()), cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::ContinueGitOperation => {
                self.continue_git_operation(cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::SkipRebaseOperation => {
                self.skip_rebase_operation(cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::AbortGitOperation => {
                self.abort_git_operation(cx);
                AutomationResponse::success(self.automation_snapshot())
            }
        }
    }

    fn automation_snapshot(&self) -> AutomationSnapshot {
        AutomationSnapshot {
            repo: self
                .repo
                .snapshot
                .as_ref()
                .map(|snapshot| AutomationRepoSnapshot {
                    path: snapshot.repo.path.to_string_lossy().to_string(),
                    name: snapshot.repo.name.clone(),
                    current_branch: snapshot.repo.current_branch.clone(),
                    head_oid: snapshot.repo.head_oid.clone(),
                    remote_name: snapshot.repo.remote_name.clone(),
                    has_github_remote: snapshot.repo.has_github_remote,
                    ahead: snapshot.repo.ahead,
                    behind: snapshot.repo.behind,
                    stash_count: snapshot.stash_count,
                    changes: snapshot
                        .changes
                        .iter()
                        .map(|change| AutomationChange {
                            path: change.path.clone(),
                            status: change.status.clone(),
                        })
                        .collect(),
                    branches: snapshot
                        .branches
                        .iter()
                        .map(|branch| AutomationBranch {
                            name: branch.name.clone(),
                            is_current: branch.is_current,
                            is_remote: branch.is_remote,
                        })
                        .collect(),
                    history: snapshot
                        .history
                        .iter()
                        .map(|commit| AutomationCommit {
                            oid: commit.oid.clone(),
                            short_oid: commit.short_oid.clone(),
                            summary: commit.summary.clone(),
                            author_name: commit.author_name.clone(),
                            date: commit.date.clone(),
                            is_head: commit.is_head,
                            tags: commit.tags.clone(),
                        })
                        .collect(),
                    tags: snapshot.tags.clone(),
                }),
            test_tree: self.automation_test_tree(),
            sidebar_tab: self.nav.sidebar_tab.into(),
            selected_change: self.selection.selected_change.clone(),
            selected_commit: self.selection.selected_commit.clone(),
            selected_commit_file: self.selection.selected_commit_file.clone(),
            diff_hide_whitespace_changes: self.nav.diff_options.hide_whitespace_changes,
            diff_show_side_by_side: self.nav.diff_options.show_side_by_side,
            selected_diff_visible_line_count: self.selected_diff().map(|diff| {
                crate::ui::workspace::visible_diff_line_count(
                    &diff.diff,
                    self.nav.diff_options.hide_whitespace_changes,
                )
            }),
            selected_diff_selectable_line_count: self
                .selected_diff()
                .map(|diff| {
                    crate::ui::workspace::selectable_diff_line_targets(
                        &diff.path,
                        &diff.diff,
                        self.nav.diff_options.hide_whitespace_changes,
                    )
                    .len()
                })
                .unwrap_or(0),
            selected_diff_selected_line_count: self
                .selected_diff()
                .map(|diff| {
                    crate::ui::workspace::selectable_diff_line_targets(
                        &diff.path,
                        &diff.diff,
                        self.nav.diff_options.hide_whitespace_changes,
                    )
                    .into_iter()
                    .filter(|target| !self.selection.selected_diff_lines.contains(target))
                    .count()
                })
                .unwrap_or(0),
            selected_diff_is_image: self.selected_diff().is_some_and(|diff| diff.is_image),
            selected_diff_is_submodule: self.selected_diff().is_some_and(|diff| diff.is_submodule),
            show_settings: self.nav.show_settings,
            show_repo_selector: self.nav.show_repo_selector,
            show_branch_selector: self.nav.show_branch_selector,
            show_network_dropdown: self.nav.show_network_dropdown
                && self.network_dropdown_available(),
            active_dialog: active_dialog_name(&self.nav.active_dialog).to_string(),
            network_action: self.network.active_action.map(network_action_name),
            ai_in_flight: self.commit.ai_in_flight,
            commit_summary: self.commit.summary.clone(),
            commit_body: self.commit.body.clone(),
            repo_filter_text: self.filters.repo_filter_text.clone(),
            branch_filter_text: self.filters.branch_filter_text.clone(),
            status_message: self.messages.status_message.clone(),
            error_message: self.messages.error_message.clone(),
            compare: self
                .repo
                .comparison
                .as_ref()
                .map(|comparison| AutomationCompareSnapshot {
                    current_branch: comparison.current_branch.clone(),
                    target_branch: comparison.target_branch.clone(),
                    ahead: comparison.ahead,
                    behind: comparison.behind,
                    commits: comparison
                        .commits
                        .iter()
                        .map(|commit| AutomationCommit {
                            oid: commit.oid.clone(),
                            short_oid: commit.short_oid.clone(),
                            summary: commit.summary.clone(),
                            author_name: commit.author_name.clone(),
                            date: commit.date.clone(),
                            is_head: commit.is_head,
                            tags: commit.tags.clone(),
                        })
                        .collect(),
                    files: comparison
                        .diffs
                        .iter()
                        .map(|diff| AutomationChange {
                            path: diff.path.clone(),
                            status: String::new(),
                        })
                        .collect(),
                }),
            operation: self
                .repo
                .operation
                .as_ref()
                .map(|operation| AutomationOperationSnapshot {
                    kind: git_operation_kind_name(&operation.kind).to_string(),
                    current_branch: operation.current_branch.clone(),
                    target_branch: operation.target_branch.clone(),
                    conflicted_files: operation
                        .conflicted_files
                        .iter()
                        .map(|file| AutomationChange {
                            path: file.path.clone(),
                            status: file.status.clone(),
                        })
                        .collect(),
                    can_continue: operation.can_continue,
                    message: operation.message.clone(),
                }),
            settings_scope: self.nav.settings_scope.into(),
            settings_section: self.nav.settings_section.into(),
            git_user_name: self.active_git_settings_identity().user_name.clone(),
            git_user_email: self.active_git_settings_identity().user_email.clone(),
            git_default_branch: self.active_git_settings_identity().default_branch.clone(),
            git_pull_rebase: self.repo.identity.pull_rebase,
            ai_provider: ai_provider_name(&self.settings.ai.provider).to_string(),
            ai_model: self.settings.ai.model.clone(),
            ai_endpoint: self.settings.ai.endpoint.clone(),
            ai_system_prompt: self.settings.ai.system_prompt.clone(),
            repo_remote_name: self.repo.remote_name.clone(),
            repo_remote_url: self.repo.remote_url.clone(),
            repo_ignored_files_text: self.repo.ignored_files_text.clone(),
            menu_availability: self.native_menu_availability().into(),
        }
    }

    fn automation_test_tree(&self) -> AutomationNode {
        let mut children = vec![
            automation_node(
                "tab-changes",
                AutomationRole::Tab,
                Some("tab-changes"),
                Some("Changes"),
                Some(AutomationNodeAction::SelectTab(SidebarTab::Changes)),
            )
            .selected(self.nav.sidebar_tab == SidebarTab::Changes),
            automation_node(
                "tab-history",
                AutomationRole::Tab,
                Some("tab-history"),
                Some("History"),
                Some(AutomationNodeAction::SelectTab(SidebarTab::History)),
            )
            .selected(self.nav.sidebar_tab == SidebarTab::History),
            automation_node(
                "commit-summary",
                AutomationRole::Textbox,
                Some("input-commit-summary"),
                Some(self.commit.summary.as_str()),
                Some(AutomationNodeAction::SetCommitSummary),
            )
            .enabled(self.repo.snapshot.is_some()),
            automation_node(
                "commit-body",
                AutomationRole::Textbox,
                Some("input-commit-body"),
                Some(self.commit.body.as_str()),
                Some(AutomationNodeAction::SetCommitBody),
            )
            .enabled(self.repo.snapshot.is_some()),
            automation_node(
                "commit-all",
                AutomationRole::Button,
                Some("button-commit-all"),
                Some("Commit"),
                Some(AutomationNodeAction::CommitAll),
            )
            .enabled(self.can_commit()),
            automation_node(
                "commit-identity-warning",
                AutomationRole::Status,
                Some("commit-identity-warning"),
                self.missing_identity_message(),
                None,
            )
            .visible(self.repo.snapshot.is_some() && self.missing_identity_message().is_some()),
            automation_node(
                "commit-identity-settings",
                AutomationRole::Button,
                Some("commit-identity-settings"),
                Some("Git Settings"),
                Some(AutomationNodeAction::OpenIdentitySettings),
            )
            .visible(self.repo.snapshot.is_some() && self.missing_identity_message().is_some()),
            automation_node(
                "undo-last-commit",
                AutomationRole::Button,
                Some("button-undo-last-commit"),
                Some("Undo"),
                Some(AutomationNodeAction::UndoLastCommit),
            )
            .visible(self.can_undo_last_commit())
            .enabled(self.can_undo_last_commit()),
            automation_node(
                "settings-toggle",
                AutomationRole::Button,
                Some("button-settings"),
                Some("Global Settings"),
                Some(AutomationNodeAction::ShowSettings(!self.nav.show_settings)),
            ),
            automation_node(
                "generate-ai-commit",
                AutomationRole::Button,
                Some("button-generate-ai-commit"),
                Some("Generate AI commit"),
                Some(AutomationNodeAction::GenerateAiCommit),
            )
            .enabled(
                self.repo.snapshot.is_some()
                    && self.commit_file_count() > 0
                    && !self.commit.ai_in_flight,
            ),
            automation_node(
                "repo-selector-toggle",
                AutomationRole::Button,
                Some("button-repo-selector"),
                Some("Repository selector"),
                Some(AutomationNodeAction::ShowRepoSelector(
                    !self.nav.show_repo_selector,
                )),
            ),
            automation_node(
                "branch-selector-toggle",
                AutomationRole::Button,
                Some("button-branch-selector"),
                Some("Branch selector"),
                Some(AutomationNodeAction::ShowBranchSelector(
                    !self.nav.show_branch_selector,
                )),
            )
            .enabled(self.repo.snapshot.is_some()),
            automation_node(
                "repo-filter",
                AutomationRole::Textbox,
                Some("input-repo-filter"),
                Some(self.filters.repo_filter_text.as_str()),
                Some(AutomationNodeAction::SetRepoFilter),
            )
            .visible(self.nav.show_repo_selector),
            automation_node(
                "branch-filter",
                AutomationRole::Textbox,
                Some("input-branch-filter"),
                Some(self.filters.branch_filter_text.as_str()),
                Some(AutomationNodeAction::SetBranchFilter),
            )
            .visible(self.nav.show_branch_selector),
            automation_node(
                "diff-options-menu",
                AutomationRole::Button,
                Some("diff-options-menu"),
                Some("Diff Settings"),
                Some(AutomationNodeAction::ToggleDiffOptionsMenu),
            )
            .visible(
                self.nav.sidebar_tab == SidebarTab::Changes
                    && self.selection.selected_change.is_some()
                    && self.selected_diff().is_some_and(|diff| {
                        !diff.is_binary && !diff.is_image && !diff.is_submodule
                    }),
            ),
            automation_node(
                "diff-option-unified",
                AutomationRole::Button,
                Some("diff-option-unified"),
                Some("Unified"),
                Some(AutomationNodeAction::ShowUnifiedDiff),
            )
            .visible(
                self.nav.sidebar_tab == SidebarTab::Changes
                    && self.selection.selected_change.is_some()
                    && self.nav.show_diff_options_menu
                    && self.selected_diff().is_some_and(|diff| {
                        !diff.is_binary && !diff.is_image && !diff.is_submodule
                    }),
            )
            .selected(!self.nav.diff_options.show_side_by_side),
            automation_node(
                "diff-option-side-by-side",
                AutomationRole::Button,
                Some("diff-option-side-by-side"),
                Some("Split"),
                Some(AutomationNodeAction::ToggleSideBySideDiff),
            )
            .visible(
                self.nav.sidebar_tab == SidebarTab::Changes
                    && self.selection.selected_change.is_some()
                    && self.nav.show_diff_options_menu
                    && self.selected_diff().is_some_and(|diff| {
                        !diff.is_binary && !diff.is_image && !diff.is_submodule
                    }),
            )
            .selected(self.nav.diff_options.show_side_by_side),
            automation_node(
                "diff-option-hide-whitespace",
                AutomationRole::Button,
                Some("diff-option-hide-whitespace"),
                Some("Hide whitespace"),
                Some(AutomationNodeAction::ToggleHideWhitespaceChanges),
            )
            .visible(
                self.nav.sidebar_tab == SidebarTab::Changes
                    && self.selection.selected_change.is_some()
                    && self.nav.show_diff_options_menu
                    && self
                        .selected_diff()
                        .is_some_and(|diff| !diff.is_image && !diff.is_submodule),
            )
            .selected(self.nav.diff_options.hide_whitespace_changes),
            automation_node(
                "diff-binary-reveal",
                AutomationRole::Button,
                Some("diff-binary-reveal"),
                Some(crate::ui::labels::reveal_in_file_manager_menu()),
                Some(AutomationNodeAction::RevealSelectedBinaryInFinder),
            )
            .visible(
                self.nav.sidebar_tab == SidebarTab::Changes
                    && self
                        .selected_diff()
                        .is_some_and(|diff| diff.is_binary && !diff.is_image),
            ),
            automation_node(
                "diff-image-preview",
                AutomationRole::Status,
                Some("diff-image-preview"),
                Some("Image preview"),
                None,
            )
            .visible(
                self.nav.sidebar_tab == SidebarTab::Changes
                    && self.selected_diff().is_some_and(|diff| diff.is_image),
            ),
            automation_node(
                "diff-image-reveal",
                AutomationRole::Button,
                Some("diff-image-reveal"),
                Some(crate::ui::labels::reveal_in_file_manager_menu()),
                Some(AutomationNodeAction::RevealSelectedImageInFinder),
            )
            .visible(
                self.nav.sidebar_tab == SidebarTab::Changes
                    && self.selected_diff().is_some_and(|diff| diff.is_image),
            ),
            automation_node(
                "diff-image-open-default",
                AutomationRole::Button,
                Some("diff-image-open-default"),
                Some("Open Image"),
                Some(AutomationNodeAction::OpenSelectedImageWithDefaultProgram),
            )
            .visible(
                self.nav.sidebar_tab == SidebarTab::Changes
                    && self.selected_diff().is_some_and(|diff| diff.is_image),
            ),
            automation_node(
                "diff-submodule-open",
                AutomationRole::Button,
                Some("diff-submodule-open"),
                Some("Open Submodule"),
                Some(AutomationNodeAction::OpenSelectedSubmodule),
            )
            .visible(
                self.nav.sidebar_tab == SidebarTab::Changes
                    && self.selected_diff().is_some_and(|diff| diff.is_submodule),
            ),
            automation_node(
                "diff-submodule-reveal",
                AutomationRole::Button,
                Some("diff-submodule-reveal"),
                Some(crate::ui::labels::reveal_in_file_manager_menu()),
                Some(AutomationNodeAction::RevealSelectedSubmodule),
            )
            .visible(
                self.nav.sidebar_tab == SidebarTab::Changes
                    && self.selected_diff().is_some_and(|diff| diff.is_submodule),
            ),
            automation_node(
                "diff-binary-open-default",
                AutomationRole::Button,
                Some("diff-binary-open-default"),
                Some("Open Anyway"),
                Some(AutomationNodeAction::OpenSelectedBinaryWithDefaultProgram),
            )
            .visible(
                self.nav.sidebar_tab == SidebarTab::Changes
                    && self
                        .selected_diff()
                        .is_some_and(|diff| diff.is_binary && !diff.is_image),
            ),
            automation_node(
                "status-message",
                AutomationRole::Status,
                Some("status-message"),
                Some(self.messages.status_message.as_str()),
                None,
            ),
        ];

        if self.nav.sidebar_tab == SidebarTab::Changes
            && let Some(diff) = self.selected_diff()
            && !diff.is_binary
            && !diff.is_image
            && !diff.is_submodule
        {
            children.extend(
                crate::ui::workspace::selectable_diff_line_targets(
                    &diff.path,
                    &diff.diff,
                    self.nav.diff_options.hide_whitespace_changes,
                )
                .into_iter()
                .map(|target| {
                    let id = target.id();
                    let selected = !self.selection.selected_diff_lines.contains(&target);
                    automation_node(
                        id.clone(),
                        AutomationRole::ListItem,
                        Some(id),
                        Some("Diff line"),
                        Some(AutomationNodeAction::ToggleDiffLine(target)),
                    )
                    .selected(selected)
                }),
            );
        }

        children.push(
            automation_node(
                "diff-discard-selected-lines",
                AutomationRole::Button,
                Some("diff-discard-selected-lines"),
                Some("Discard selected lines"),
                Some(AutomationNodeAction::DiscardSelectedDiffLines),
            )
            .visible(false),
        );

        if self.nav.show_settings {
            children.extend(settings_automation_nodes(self));
        }

        if !self.messages.error_message.is_empty() {
            children.push(automation_node(
                "error-message",
                AutomationRole::Status,
                Some("error-message"),
                Some(self.messages.error_message.as_str()),
                None,
            ));
        }

        if let Some(operation) = self.repo.operation.as_ref() {
            let mut operation_children = vec![
                automation_node(
                    "operation-continue",
                    AutomationRole::Button,
                    Some("operation-continue"),
                    Some("Continue operation"),
                    Some(AutomationNodeAction::ContinueGitOperation),
                )
                .enabled(operation.can_continue),
                automation_node(
                    "operation-abort",
                    AutomationRole::Button,
                    Some("operation-abort"),
                    Some("Abort operation"),
                    Some(AutomationNodeAction::AbortGitOperation),
                ),
            ];
            if operation.kind == GitOperationKind::Rebase {
                operation_children.push(automation_node(
                    "operation-skip",
                    AutomationRole::Button,
                    Some("operation-skip"),
                    Some("Skip rebase commit"),
                    Some(AutomationNodeAction::SkipRebaseOperation),
                ));
            }
            operation_children.push(
                automation_node(
                    "operation-conflict-files",
                    AutomationRole::List,
                    Some("operation-conflict-files"),
                    Some("Conflicted files"),
                    None,
                )
                .children(
                    operation
                        .conflicted_files
                        .iter()
                        .map(|file| {
                            let slug = stable_test_slug(&file.path);
                            automation_node(
                                format!("operation-conflict-file-{slug}"),
                                AutomationRole::ListItem,
                                Some(format!("operation-conflict-file-{slug}")),
                                Some(file.path.as_str()),
                                None,
                            )
                            .children(vec![
                                automation_node(
                                    format!("operation-conflict-open-editor-{slug}"),
                                    AutomationRole::Button,
                                    Some(format!("operation-conflict-open-editor-{slug}")),
                                    Some("Open"),
                                    Some(AutomationNodeAction::OpenConflictInEditor(
                                        file.path.clone(),
                                    )),
                                ),
                                automation_node(
                                    format!("operation-conflict-reveal-{slug}"),
                                    AutomationRole::Button,
                                    Some(format!("operation-conflict-reveal-{slug}")),
                                    Some("Reveal"),
                                    Some(AutomationNodeAction::RevealConflictFile(
                                        file.path.clone(),
                                    )),
                                ),
                                automation_node(
                                    format!("operation-conflict-mark-resolved-{slug}"),
                                    AutomationRole::Button,
                                    Some(format!("operation-conflict-mark-resolved-{slug}")),
                                    Some("Mark Resolved"),
                                    Some(AutomationNodeAction::MarkConflictResolved(
                                        file.path.clone(),
                                    )),
                                ),
                            ])
                        })
                        .collect(),
                ),
            );
            children.push(
                automation_node(
                    "operation-conflict-banner",
                    AutomationRole::Status,
                    Some("operation-conflict-banner"),
                    Some(operation.kind.title()),
                    None,
                )
                .children(operation_children),
            );
        }

        if self.repo.snapshot.is_none() {
            children.extend([
                automation_node(
                    "no-repository-state",
                    AutomationRole::Status,
                    Some("no-repository-state"),
                    Some("No repository selected"),
                    None,
                ),
                automation_node(
                    "no-repository-choose",
                    AutomationRole::Button,
                    Some("no-repository-choose"),
                    Some("Show Repository List"),
                    Some(AutomationNodeAction::ShowRepoSelector(true)),
                ),
                automation_node(
                    "no-repository-add-local",
                    AutomationRole::Button,
                    Some("no-repository-add-local"),
                    Some("Add Local Repository…"),
                    None::<AutomationNodeAction>,
                ),
            ]);
        }

        if self.nav.show_repo_selector {
            children.extend(repo_selector_nodes(self));
        }

        if self.nav.show_branch_selector {
            let branch_selector_target_mode = matches!(
                self.nav.branch_selector_mode,
                BranchSelectorMode::Merge
                    | BranchSelectorMode::Rebase
                    | BranchSelectorMode::Compare
            );
            children.push(
                automation_node(
                    "branch-new",
                    AutomationRole::Button,
                    Some("button-branch-new"),
                    Some("New Branch"),
                    Some(AutomationNodeAction::StartCreateBranch),
                )
                .visible(!branch_selector_target_mode)
                .enabled(!branch_selector_target_mode),
            );
            children.extend(branch_selector_nodes(self));
        }

        if matches!(self.nav.active_dialog, ActiveDialog::CreateBranch) {
            let branch_validation = self.create_branch_validation_message();
            let show_branch_validation = !self.repo.new_branch_name.trim().is_empty();
            children.extend([
                automation_node(
                    "new-branch-name",
                    AutomationRole::Textbox,
                    Some("input-new-branch-name"),
                    Some(self.repo.new_branch_name.as_str()),
                    Some(AutomationNodeAction::SetNewBranchName),
                ),
                automation_node(
                    "dialog-cancel",
                    AutomationRole::Button,
                    Some("dialog-cancel"),
                    Some("Cancel"),
                    Some(AutomationNodeAction::CancelDialog),
                ),
                automation_node(
                    "dialog-create-branch",
                    AutomationRole::Button,
                    Some("dialog-create-branch"),
                    Some("Create Branch"),
                    Some(AutomationNodeAction::ConfirmCreateBranch),
                )
                .enabled(self.can_create_branch_from_dialog()),
            ]);
            if show_branch_validation {
                if let Some(message) = branch_validation {
                    children.push(automation_node(
                        "create-branch-validation-message",
                        AutomationRole::Status,
                        Some("create-branch-validation-message"),
                        Some(message.as_str()),
                        None::<AutomationNodeAction>,
                    ));
                }
            }
        }

        if matches!(self.nav.active_dialog, ActiveDialog::CreateRepository) {
            let validation = self.create_repository_validation_message();
            children.extend([
                automation_node(
                    "create-repository-name-input",
                    AutomationRole::Textbox,
                    Some("create-repository-name-input"),
                    Some(self.repo.create_repo_name.as_str()),
                    Some(AutomationNodeAction::SetCreateRepositoryName),
                ),
                automation_node(
                    "create-repository-description-input",
                    AutomationRole::Textbox,
                    Some("create-repository-description-input"),
                    Some(self.repo.create_repo_description.as_str()),
                    Some(AutomationNodeAction::SetCreateRepositoryDescription),
                ),
                automation_node(
                    "create-repository-path-input",
                    AutomationRole::Textbox,
                    Some("create-repository-path-input"),
                    Some(self.repo.create_repo_path.as_str()),
                    Some(AutomationNodeAction::SetCreateRepositoryPath),
                ),
                automation_node(
                    "create-repository-branch-input",
                    AutomationRole::Textbox,
                    Some("create-repository-branch-input"),
                    Some(self.repo.create_repo_branch_name.as_str()),
                    Some(AutomationNodeAction::SetCreateRepositoryBranch),
                ),
                automation_node(
                    "create-repository-gitignore-none",
                    AutomationRole::Button,
                    Some("create-repository-gitignore-none"),
                    Some("Git ignore: None"),
                    Some(AutomationNodeAction::SetCreateRepositoryGitignore(
                        String::new(),
                    )),
                )
                .selected(self.repo.create_repo_gitignore_template.is_empty()),
                automation_node(
                    "create-repository-gitignore-rust",
                    AutomationRole::Button,
                    Some("create-repository-gitignore-rust"),
                    Some("Git ignore: Rust"),
                    Some(AutomationNodeAction::SetCreateRepositoryGitignore(
                        "Rust".to_string(),
                    )),
                )
                .selected(self.repo.create_repo_gitignore_template == "Rust"),
                automation_node(
                    "create-repository-gitignore-node",
                    AutomationRole::Button,
                    Some("create-repository-gitignore-node"),
                    Some("Git ignore: Node"),
                    Some(AutomationNodeAction::SetCreateRepositoryGitignore(
                        "Node".to_string(),
                    )),
                )
                .selected(self.repo.create_repo_gitignore_template == "Node"),
                automation_node(
                    "create-repository-gitignore-python",
                    AutomationRole::Button,
                    Some("create-repository-gitignore-python"),
                    Some("Git ignore: Python"),
                    Some(AutomationNodeAction::SetCreateRepositoryGitignore(
                        "Python".to_string(),
                    )),
                )
                .selected(self.repo.create_repo_gitignore_template == "Python"),
                automation_node(
                    "create-repository-license-none",
                    AutomationRole::Button,
                    Some("create-repository-license-none"),
                    Some("License: None"),
                    Some(AutomationNodeAction::SetCreateRepositoryLicense(
                        String::new(),
                    )),
                )
                .selected(self.repo.create_repo_license_template.is_empty()),
                automation_node(
                    "create-repository-license-mit",
                    AutomationRole::Button,
                    Some("create-repository-license-mit"),
                    Some("License: MIT"),
                    Some(AutomationNodeAction::SetCreateRepositoryLicense(
                        "MIT".to_string(),
                    )),
                )
                .selected(self.repo.create_repo_license_template == "MIT"),
                automation_node(
                    "create-repository-license-apache-2-0",
                    AutomationRole::Button,
                    Some("create-repository-license-apache-2-0"),
                    Some("License: Apache-2.0"),
                    Some(AutomationNodeAction::SetCreateRepositoryLicense(
                        "Apache-2.0".to_string(),
                    )),
                )
                .selected(self.repo.create_repo_license_template == "Apache-2.0"),
                automation_node(
                    "create-repository-license-gpl-3-0",
                    AutomationRole::Button,
                    Some("create-repository-license-gpl-3-0"),
                    Some("License: GPL-3.0"),
                    Some(AutomationNodeAction::SetCreateRepositoryLicense(
                        "GPL-3.0".to_string(),
                    )),
                )
                .selected(self.repo.create_repo_license_template == "GPL-3.0"),
                automation_node(
                    "create-repository-readme-checkbox",
                    AutomationRole::Button,
                    Some("create-repository-readme-checkbox"),
                    Some("Initialize this repository with a README"),
                    Some(AutomationNodeAction::ToggleCreateRepositoryReadme),
                )
                .selected(self.repo.create_repo_initialize_readme),
                automation_node(
                    "create-repository-initial-commit-checkbox",
                    AutomationRole::Button,
                    Some("create-repository-initial-commit-checkbox"),
                    Some("Create initial commit"),
                    Some(AutomationNodeAction::ToggleCreateRepositoryInitialCommit),
                )
                .selected(self.repo.create_repo_initial_commit),
                automation_node(
                    "create-repository-cancel",
                    AutomationRole::Button,
                    Some("create-repository-cancel"),
                    Some("Cancel"),
                    Some(AutomationNodeAction::CancelDialog),
                ),
                automation_node(
                    "create-repository-confirm",
                    AutomationRole::Button,
                    Some("create-repository-confirm"),
                    Some("Create Repository"),
                    Some(AutomationNodeAction::ConfirmCreateRepository),
                )
                .enabled(validation.is_none()),
            ]);
            if let Some(message) = validation {
                children.push(automation_node(
                    "create-repository-validation-message",
                    AutomationRole::Status,
                    Some("create-repository-validation-message"),
                    Some(message.as_str()),
                    None::<AutomationNodeAction>,
                ));
            }
        }

        if matches!(self.nav.active_dialog, ActiveDialog::CloneRepository) {
            let validation = self.clone_repository_validation_message();
            children.extend([
                automation_node(
                    "clone-repository-url-input",
                    AutomationRole::Textbox,
                    Some("clone-repository-url-input"),
                    Some(self.repo.clone_repo_url.as_str()),
                    Some(AutomationNodeAction::SetCloneRepositoryUrl),
                ),
                automation_node(
                    "clone-repository-path-input",
                    AutomationRole::Textbox,
                    Some("clone-repository-path-input"),
                    Some(self.repo.clone_repo_path.as_str()),
                    Some(AutomationNodeAction::SetCloneRepositoryPath),
                ),
                automation_node(
                    "clone-repository-name-input",
                    AutomationRole::Textbox,
                    Some("clone-repository-name-input"),
                    Some(self.repo.clone_repo_name.as_str()),
                    Some(AutomationNodeAction::SetCloneRepositoryName),
                ),
                automation_node(
                    "clone-repository-cancel",
                    AutomationRole::Button,
                    Some("clone-repository-cancel"),
                    Some("Cancel"),
                    Some(AutomationNodeAction::CancelDialog),
                ),
                automation_node(
                    "clone-repository-confirm",
                    AutomationRole::Button,
                    Some("clone-repository-confirm"),
                    Some("Clone"),
                    Some(AutomationNodeAction::ConfirmCloneRepository),
                )
                .enabled(validation.is_none()),
            ]);
            if let Some(message) = validation {
                children.push(automation_node(
                    "clone-repository-validation-message",
                    AutomationRole::Status,
                    Some("clone-repository-validation-message"),
                    Some(message.as_str()),
                    None::<AutomationNodeAction>,
                ));
            }
        }

        if matches!(self.nav.active_dialog, ActiveDialog::DiscardChanges { .. }) {
            children.extend([
                automation_node(
                    "discard-cancel",
                    AutomationRole::Button,
                    Some("discard-cancel"),
                    Some("Cancel"),
                    Some(AutomationNodeAction::CancelDialog),
                ),
                automation_node(
                    "discard-confirm",
                    AutomationRole::Button,
                    Some("discard-confirm"),
                    Some("Discard Changes"),
                    Some(AutomationNodeAction::ConfirmDiscardChanges),
                ),
            ]);
        }

        if matches!(self.nav.active_dialog, ActiveDialog::StashAndSwitch { .. }) {
            children.push(automation_node(
                "branch-switch-file-list",
                AutomationRole::List,
                Some("branch-switch-file-list"),
                Some("Files affected"),
                None,
            ));
            if let Some(snapshot) = &self.repo.snapshot {
                children.extend(snapshot.changes.iter().map(|file| {
                    let id = format!("branch-switch-file-{}", stable_test_slug(&file.path));
                    automation_node(
                        id.clone(),
                        AutomationRole::ListItem,
                        Some(id),
                        Some(file.path.as_str()),
                        None,
                    )
                }));
            }
            children.extend([
                automation_node(
                    "branch-switch-stash-option",
                    AutomationRole::Button,
                    Some("branch-switch-stash-option"),
                    Some("Leave my changes on current branch"),
                    Some(AutomationNodeAction::SetBranchSwitchMode(false)),
                ),
                automation_node(
                    "branch-switch-bring-option",
                    AutomationRole::Button,
                    Some("branch-switch-bring-option"),
                    Some("Bring my changes to target branch"),
                    Some(AutomationNodeAction::SetBranchSwitchMode(true)),
                ),
                automation_node(
                    "stash-cancel",
                    AutomationRole::Button,
                    Some("stash-cancel"),
                    Some("Cancel"),
                    Some(AutomationNodeAction::CancelDialog),
                ),
                automation_node(
                    "stash-switch",
                    AutomationRole::Button,
                    Some("stash-switch"),
                    Some("Switch Branch"),
                    Some(AutomationNodeAction::ConfirmStashAndSwitch),
                ),
            ]);
        }

        if matches!(self.nav.active_dialog, ActiveDialog::StashChanges) {
            children.push(automation_node(
                "stash-changes-file-list",
                AutomationRole::List,
                Some("stash-changes-file-list"),
                Some("Files to stash"),
                None,
            ));
            children.push(
                automation_node(
                    "stash-changes-replace-warning",
                    AutomationRole::Status,
                    Some("stash-changes-replace-warning"),
                    Some("Stashing will replace the existing GitSpark stash for this branch."),
                    None::<AutomationNodeAction>,
                )
                .visible(self.repo.has_stash),
            );
            if let Some(snapshot) = &self.repo.snapshot {
                children.extend(snapshot.changes.iter().map(|file| {
                    let id = format!("stash-changes-file-{}", stable_test_slug(&file.path));
                    automation_node(
                        id.clone(),
                        AutomationRole::ListItem,
                        Some(id),
                        Some(file.path.as_str()),
                        None,
                    )
                }));
            }
            children.extend([
                automation_node(
                    "stash-changes-cancel",
                    AutomationRole::Button,
                    Some("stash-changes-cancel"),
                    Some("Cancel"),
                    Some(AutomationNodeAction::CancelDialog),
                ),
                automation_node(
                    "stash-changes-confirm",
                    AutomationRole::Button,
                    Some("stash-changes-confirm"),
                    Some("Stash Changes"),
                    Some(AutomationNodeAction::ConfirmStashChanges),
                ),
            ]);
        }

        if matches!(self.nav.active_dialog, ActiveDialog::RestoreStash) {
            children.push(automation_node(
                "restore-stash-file-list",
                AutomationRole::List,
                Some("restore-stash-file-list"),
                Some("Stash files"),
                None,
            ));
            children.extend(self.repo.stash_files.iter().map(|file| {
                let id = format!("restore-stash-file-{}", stable_test_slug(&file.path));
                automation_node(
                    id.clone(),
                    AutomationRole::ListItem,
                    Some(id),
                    Some(file.path.as_str()),
                    None,
                )
            }));
            children.extend([
                automation_node(
                    "restore-stash-close",
                    AutomationRole::Button,
                    Some("restore-stash-close"),
                    Some("Close"),
                    Some(AutomationNodeAction::CancelDialog),
                ),
                automation_node(
                    "restore-stash-cancel",
                    AutomationRole::Button,
                    Some("restore-stash-cancel"),
                    Some("Cancel"),
                    Some(AutomationNodeAction::CancelDialog),
                ),
                automation_node(
                    "restore-stash-discard",
                    AutomationRole::Button,
                    Some("restore-stash-discard"),
                    Some("Discard Stash"),
                    Some(AutomationNodeAction::ShowDiscardStash),
                ),
                automation_node(
                    "restore-stash-confirm",
                    AutomationRole::Button,
                    Some("restore-stash-confirm"),
                    Some("Restore Stash"),
                    Some(AutomationNodeAction::RestoreStash),
                )
                .enabled(!self.repo.stash_files.is_empty()),
            ]);
        }

        if matches!(self.nav.active_dialog, ActiveDialog::DiscardStash) {
            children.push(automation_node(
                "discard-stash-file-list",
                AutomationRole::List,
                Some("discard-stash-file-list"),
                Some("Stash files"),
                None,
            ));
            children.extend(self.repo.stash_files.iter().map(|file| {
                let id = format!("discard-stash-file-{}", stable_test_slug(&file.path));
                automation_node(
                    id.clone(),
                    AutomationRole::ListItem,
                    Some(id),
                    Some(file.path.as_str()),
                    None,
                )
            }));
            children.extend([
                automation_node(
                    "discard-stash-cancel",
                    AutomationRole::Button,
                    Some("discard-stash-cancel"),
                    Some("Cancel"),
                    Some(AutomationNodeAction::CancelDialog),
                ),
                automation_node(
                    "discard-stash-confirm",
                    AutomationRole::Button,
                    Some("discard-stash-confirm"),
                    Some("Discard Stash"),
                    Some(AutomationNodeAction::ConfirmDiscardStash),
                )
                .enabled(!self.repo.stash_files.is_empty()),
            ]);
        }

        if let ActiveDialog::RenameBranch { old_name } = &self.nav.active_dialog {
            let rename_validation = self.rename_branch_validation_message(old_name);
            let show_rename_validation = !self.repo.new_branch_name.trim().is_empty()
                && self.repo.new_branch_name.trim() != old_name;
            children.extend([
                automation_node(
                    "rename-branch-name-input",
                    AutomationRole::Textbox,
                    Some("rename-branch-name-input"),
                    Some(self.repo.new_branch_name.as_str()),
                    Some(AutomationNodeAction::SetNewBranchName),
                ),
                automation_node(
                    "rename-branch-cancel",
                    AutomationRole::Button,
                    Some("rename-branch-cancel"),
                    Some("Cancel"),
                    Some(AutomationNodeAction::CancelDialog),
                ),
                automation_node(
                    "rename-branch-confirm",
                    AutomationRole::Button,
                    Some("rename-branch-confirm"),
                    Some("Rename Branch"),
                    Some(AutomationNodeAction::ConfirmRenameBranch),
                )
                .enabled(self.can_rename_branch_from_dialog(old_name)),
            ]);
            if show_rename_validation {
                if let Some(message) = rename_validation {
                    children.push(automation_node(
                        "rename-branch-validation-message",
                        AutomationRole::Status,
                        Some("rename-branch-validation-message"),
                        Some(message.as_str()),
                        None::<AutomationNodeAction>,
                    ));
                }
            }
        }

        if matches!(self.nav.active_dialog, ActiveDialog::DeleteBranch { .. }) {
            children.extend([
                automation_node(
                    "delete-branch-cancel",
                    AutomationRole::Button,
                    Some("delete-branch-cancel"),
                    Some("Cancel"),
                    Some(AutomationNodeAction::CancelDialog),
                ),
                automation_node(
                    "delete-branch-confirm",
                    AutomationRole::Button,
                    Some("delete-branch-confirm"),
                    Some("Delete"),
                    Some(AutomationNodeAction::ConfirmDeleteBranch),
                ),
            ]);
        }

        if matches!(self.nav.active_dialog, ActiveDialog::CreateTag { .. }) {
            let tag_validation = self.create_tag_validation_message();
            let show_tag_validation = !self.repo.new_branch_name.trim().is_empty();
            children.extend([
                automation_node(
                    "create-tag-name-input",
                    AutomationRole::Textbox,
                    Some("create-tag-name-input"),
                    Some(self.repo.new_branch_name.as_str()),
                    Some(AutomationNodeAction::SetNewBranchName),
                ),
                automation_node(
                    "create-tag-cancel",
                    AutomationRole::Button,
                    Some("create-tag-cancel"),
                    Some("Cancel"),
                    Some(AutomationNodeAction::CancelDialog),
                ),
                automation_node(
                    "create-tag-confirm",
                    AutomationRole::Button,
                    Some("create-tag-confirm"),
                    Some("Create Tag"),
                    Some(AutomationNodeAction::ConfirmCreateTag),
                )
                .enabled(tag_validation.is_none()),
            ]);
            if show_tag_validation {
                if let Some(message) = tag_validation {
                    children.push(automation_node(
                        "create-tag-validation-message",
                        AutomationRole::Status,
                        Some("create-tag-validation-message"),
                        Some(message.as_str()),
                        None::<AutomationNodeAction>,
                    ));
                }
            }
        }

        if matches!(self.nav.active_dialog, ActiveDialog::DeleteTag { .. }) {
            children.extend([
                automation_node(
                    "delete-tag-confirmation",
                    AutomationRole::Status,
                    Some("delete-tag-confirmation"),
                    Some("Delete tag confirmation"),
                    None::<AutomationNodeAction>,
                ),
                automation_node(
                    "delete-tag-cancel",
                    AutomationRole::Button,
                    Some("delete-tag-cancel"),
                    Some("Cancel"),
                    Some(AutomationNodeAction::CancelDialog),
                ),
                automation_node(
                    "delete-tag-confirm",
                    AutomationRole::Button,
                    Some("delete-tag-confirm"),
                    Some("Delete"),
                    Some(AutomationNodeAction::ConfirmDeleteTag),
                ),
            ]);
        }

        if let ActiveDialog::ChooseTagToDelete { target_oid } = &self.nav.active_dialog {
            let tags = self.commit_tags_for_oid(target_oid);
            children.push(automation_node(
                "choose-delete-tag-description",
                AutomationRole::Status,
                Some("choose-delete-tag-description"),
                Some("Choose tag to delete"),
                None::<AutomationNodeAction>,
            ));
            children.extend(tags.into_iter().map(|tag_name| {
                let tag_id = stable_test_slug(&tag_name);
                let label = tag_name.clone();
                automation_node(
                    format!("choose-delete-tag-{tag_id}"),
                    AutomationRole::Button,
                    Some(format!("choose-delete-tag-{tag_id}")),
                    Some(label.as_str()),
                    Some(AutomationNodeAction::SelectTagToDelete(tag_name)),
                )
            }));
            children.push(automation_node(
                "choose-delete-tag-cancel",
                AutomationRole::Button,
                Some("choose-delete-tag-cancel"),
                Some("Cancel"),
                Some(AutomationNodeAction::CancelDialog),
            ));
        }

        if matches!(self.nav.active_dialog, ActiveDialog::PublishRepository) {
            let publish_enabled = !self.network.publish_name.trim().is_empty()
                && self.network.active_action.is_none();
            children.extend([
                automation_node(
                    "publish-repo-name",
                    AutomationRole::Textbox,
                    Some("publish-repo-name"),
                    Some(self.network.publish_name.as_str()),
                    Some(AutomationNodeAction::SetPublishName),
                ),
                automation_node(
                    "publish-repo-description",
                    AutomationRole::Textbox,
                    Some("publish-repo-description"),
                    Some(self.network.publish_description.as_str()),
                    Some(AutomationNodeAction::SetPublishDescription),
                ),
                automation_node(
                    "publish-repo-private",
                    AutomationRole::Button,
                    Some("publish-repo-private"),
                    Some("Keep this code private"),
                    Some(AutomationNodeAction::TogglePublishPrivate),
                )
                .selected(self.network.publish_private),
                automation_node(
                    "publish-cancel",
                    AutomationRole::Button,
                    Some("publish-cancel"),
                    Some("Cancel"),
                    Some(AutomationNodeAction::CancelDialog),
                ),
                automation_node(
                    "publish-confirm",
                    AutomationRole::Button,
                    Some("publish-confirm"),
                    Some("Publish Repository"),
                    Some(AutomationNodeAction::ConfirmPublishRepository),
                )
                .enabled(publish_enabled),
            ]);
        }

        if matches!(self.nav.active_dialog, ActiveDialog::ResetToCommit { .. }) {
            children.extend([
                automation_node(
                    "reset-to-commit-cancel",
                    AutomationRole::Button,
                    Some("reset-to-commit-cancel"),
                    Some("Cancel"),
                    Some(AutomationNodeAction::CancelDialog),
                ),
                automation_node(
                    "reset-to-commit-confirm",
                    AutomationRole::Button,
                    Some("reset-to-commit-confirm"),
                    Some("Continue"),
                    Some(AutomationNodeAction::ConfirmResetToCommit),
                ),
            ]);
        }

        if let Some(snapshot) = &self.repo.snapshot {
            let has_github_remote = self.repo_has_github_remote();
            children.push(
                automation_node(
                    "changes-list",
                    AutomationRole::List,
                    Some("changes-list"),
                    Some("Changes"),
                    None,
                )
                .children(
                    snapshot
                        .changes
                        .iter()
                        .map(|change| {
                            automation_node(
                                format!("change-{}", stable_test_slug(&change.path)),
                                AutomationRole::ListItem,
                                Some(format!("change-{}", stable_test_slug(&change.path))),
                                Some(change.path.as_str()),
                                Some(AutomationNodeAction::SelectChange(change.path.clone())),
                            )
                            .selected(
                                self.selection.selected_change.as_deref()
                                    == Some(change.path.as_str()),
                            )
                        })
                        .collect(),
                ),
            );

            for change in &snapshot.changes {
                children.extend(change_action_nodes(
                    change.path.as_str(),
                    change.status.as_str(),
                    has_github_remote,
                ));
            }

            if snapshot.changes.is_empty() {
                children.extend(no_changes_action_nodes(
                    has_github_remote,
                    snapshot.repo.remote_name.is_some(),
                    snapshot.repo.ahead,
                    snapshot.repo.behind,
                ));
            }

            children.push(
                automation_node(
                    "history-list",
                    AutomationRole::List,
                    Some("history-list"),
                    Some("History"),
                    None,
                )
                .children(
                    self.repo
                        .comparison
                        .as_ref()
                        .map(|comparison| comparison.commits.as_slice())
                        .unwrap_or(snapshot.history.as_slice())
                        .iter()
                        .map(|commit| {
                            automation_node(
                                format!("commit-{}", stable_test_slug(&commit.short_oid)),
                                AutomationRole::ListItem,
                                Some(format!("commit-{}", stable_test_slug(&commit.short_oid))),
                                Some(commit.summary.as_str()),
                                Some(AutomationNodeAction::SelectCommit(commit.oid.clone())),
                            )
                            .selected(
                                self.selection.selected_commit.as_deref()
                                    == Some(commit.oid.as_str()),
                            )
                        })
                        .collect(),
                ),
            );

            let history_actions = self
                .repo
                .comparison
                .as_ref()
                .map(|comparison| comparison.commits.as_slice())
                .unwrap_or(snapshot.history.as_slice());
            for commit in history_actions {
                children.extend(history_action_nodes(
                    commit.short_oid.as_str(),
                    commit.oid.as_str(),
                    &commit.tags,
                    self.can_reset_to_commit(&commit.oid),
                    has_github_remote,
                ));
            }

            if self.nav.sidebar_tab == SidebarTab::History {
                if let Some(comparison) = self.repo.comparison.as_ref() {
                    children.extend([
                        automation_node(
                            "compare-exit-button",
                            AutomationRole::Button,
                            Some("compare-exit-button"),
                            Some("Exit Compare"),
                            Some(AutomationNodeAction::ExitCompare),
                        ),
                        automation_node(
                            "compare-merge-button",
                            AutomationRole::Button,
                            Some("compare-merge-button"),
                            Some("Merge compared branch"),
                            Some(AutomationNodeAction::MergeComparedBranch),
                        )
                        .enabled(comparison.behind > 0),
                    ]);
                }
                children.push(automation_node(
                    "commit-file-list-viewport",
                    AutomationRole::List,
                    Some("commit-file-list-viewport"),
                    Some("Commit files"),
                    None::<AutomationNodeAction>,
                ));

                let visible_diffs = self
                    .repo
                    .comparison
                    .as_ref()
                    .map(|comparison| comparison.diffs.as_slice())
                    .or_else(|| self.selection.commit_diffs.as_deref());
                if self.selection.selected_commit.as_ref().is_some()
                    && let Some(diffs) = visible_diffs
                {
                    children.extend(diffs.iter().map(|entry| {
                        let id = format!("commit-file-{}", stable_test_slug(&entry.path));
                        automation_node(
                            id.clone(),
                            AutomationRole::ListItem,
                            Some(id),
                            Some(entry.path.as_str()),
                            None::<AutomationNodeAction>,
                        )
                        .selected(
                            self.selection.selected_commit_file.as_deref()
                                == Some(entry.path.as_str()),
                        )
                    }));
                }
            }

            if snapshot.stash_count > 0 {
                children.push(automation_node(
                    "stash-indicator",
                    AutomationRole::Button,
                    Some("stash-indicator"),
                    Some("Stashed Changes"),
                    Some(AutomationNodeAction::ShowRestoreStash),
                ));
            }

            for branch in snapshot
                .branches
                .iter()
                .filter(|branch| !branch.is_current && !branch.is_remote)
            {
                children.extend(branch_action_nodes(branch.name.as_str(), has_github_remote));
            }

            children.push(
                automation_node(
                    "branch-list",
                    AutomationRole::List,
                    Some("branch-list"),
                    Some("Branches"),
                    None,
                )
                .children(
                    snapshot
                        .branches
                        .iter()
                        .filter(|branch| !branch.is_remote)
                        .filter(|branch| {
                            !matches!(
                                self.nav.branch_selector_mode,
                                BranchSelectorMode::Merge
                                    | BranchSelectorMode::Rebase
                                    | BranchSelectorMode::Compare
                            ) || !branch.is_current
                        })
                        .map(|branch| {
                            automation_node(
                                format!("branch-{}", stable_test_slug(&branch.name)),
                                AutomationRole::ListItem,
                                Some(format!("branch-{}", stable_test_slug(&branch.name))),
                                Some(branch.name.as_str()),
                                Some(AutomationNodeAction::SwitchBranch(branch.name.clone())),
                            )
                            .selected(branch.is_current)
                        })
                        .collect(),
                ),
            );
        }

        children.extend(self.network_action_nodes());

        automation_node(
            "gitspark-root",
            AutomationRole::App,
            Some("gitspark-root"),
            Some("GitSpark"),
            None,
        )
        .children(children)
    }

    fn network_action_nodes(&self) -> Vec<AutomationNode> {
        let Some(snapshot) = self.repo.snapshot.as_ref() else {
            return vec![
                automation_node(
                    "network-primary",
                    AutomationRole::Button,
                    Some("button-network-primary"),
                    Some("Fetch"),
                    None,
                )
                .enabled(false),
                automation_node(
                    "network-caret",
                    AutomationRole::Button,
                    Some("network-caret"),
                    Some("Network options"),
                    None,
                )
                .visible(false)
                .enabled(false),
            ];
        };

        let remote_name = snapshot.repo.remote_name.as_deref().unwrap_or("origin");
        let primary_action = NetworkAction::from_snapshot(snapshot);
        let primary_label = primary_action.title(remote_name);
        let has_dropdown = matches!(primary_action, NetworkAction::Pull | NetworkAction::Push);
        let actions_enabled = self.network.active_action.is_none();

        let mut nodes = vec![
            automation_node(
                "network-primary",
                AutomationRole::Button,
                Some("button-network-primary"),
                Some(primary_label.as_str()),
                Some(AutomationNodeAction::Network(primary_action)),
            )
            .enabled(actions_enabled),
            automation_node(
                "network-caret",
                AutomationRole::Button,
                Some("network-caret"),
                Some("Network options"),
                None,
            )
            .visible(has_dropdown)
            .enabled(has_dropdown && actions_enabled),
            automation_node(
                "network-fetch",
                AutomationRole::Button,
                Some("button-network-fetch"),
                Some("Fetch"),
                Some(AutomationNodeAction::Network(NetworkAction::Fetch)),
            )
            .visible(primary_action == NetworkAction::Fetch)
            .enabled(primary_action == NetworkAction::Fetch && actions_enabled),
            automation_node(
                "network-pull",
                AutomationRole::Button,
                Some("button-network-pull"),
                Some("Pull"),
                Some(AutomationNodeAction::Network(NetworkAction::Pull)),
            )
            .visible(primary_action == NetworkAction::Pull)
            .enabled(primary_action == NetworkAction::Pull && actions_enabled),
            automation_node(
                "network-push",
                AutomationRole::Button,
                Some("button-network-push"),
                Some("Push"),
                Some(AutomationNodeAction::Network(NetworkAction::Push)),
            )
            .visible(primary_action == NetworkAction::Push)
            .enabled(primary_action == NetworkAction::Push && actions_enabled),
        ];

        if self.nav.show_network_dropdown && has_dropdown {
            let fetch_label = format!("Fetch {remote_name}");
            nodes.push(
                automation_node(
                    "network-dropdown",
                    AutomationRole::List,
                    Some("network-dropdown"),
                    Some("Network options"),
                    None,
                )
                .children(vec![
                    automation_node(
                        "network-dropdown-fetch",
                        AutomationRole::Button,
                        Some("network-dropdown-fetch"),
                        Some(fetch_label.as_str()),
                        Some(AutomationNodeAction::Network(NetworkAction::Fetch)),
                    )
                    .enabled(actions_enabled),
                ]),
            );
        }

        nodes
    }

    fn network_dropdown_available(&self) -> bool {
        self.repo
            .snapshot
            .as_ref()
            .map(NetworkAction::from_snapshot)
            .is_some_and(|action| matches!(action, NetworkAction::Pull | NetworkAction::Push))
    }

    fn query_automation_nodes(&self, selector: &AutomationSelector) -> Vec<AutomationNode> {
        let mut matches = Vec::new();
        collect_matching_nodes(&self.automation_test_tree(), selector, &mut matches);
        matches
    }

    fn click_automation_node(
        &mut self,
        selector: AutomationSelector,
        cx: &mut Context<Self>,
    ) -> AutomationResponse {
        let Some(node) = self
            .query_automation_nodes(&selector)
            .into_iter()
            .find(|node| node.visible && node.enabled)
        else {
            return AutomationResponse::failure("no visible enabled node matched selector");
        };

        let Some(action) = node.action else {
            return AutomationResponse::failure(format!("node '{}' is not clickable", node.id));
        };

        self.perform_automation_action(action, None, cx)
    }

    fn fill_automation_node(
        &mut self,
        selector: AutomationSelector,
        text: String,
        cx: &mut Context<Self>,
    ) -> AutomationResponse {
        let Some(node) = self
            .query_automation_nodes(&selector)
            .into_iter()
            .find(|node| node.visible && node.enabled)
        else {
            return AutomationResponse::failure("no visible enabled node matched selector");
        };

        let Some(action) = node.action else {
            return AutomationResponse::failure(format!("node '{}' is not fillable", node.id));
        };

        self.perform_automation_action(action, Some(text), cx)
    }

    fn type_text_automation_node(
        &mut self,
        selector: AutomationSelector,
        text: String,
        cx: &mut Context<Self>,
    ) -> AutomationResponse {
        let Some(action) = self
            .query_automation_nodes(&selector)
            .into_iter()
            .find(|node| node.visible && node.enabled)
            .and_then(|node| node.action)
        else {
            return AutomationResponse::failure("no visible enabled node matched selector");
        };

        let keystrokes = text.chars().map(typed_keystroke_for_char).collect();
        self.dispatch_real_keystrokes(action, keystrokes, cx)
    }

    fn press_keys_automation_node(
        &mut self,
        selector: AutomationSelector,
        keys: Vec<String>,
        cx: &mut Context<Self>,
    ) -> AutomationResponse {
        let Some(action) = self
            .query_automation_nodes(&selector)
            .into_iter()
            .find(|node| node.visible && node.enabled)
            .and_then(|node| node.action)
        else {
            return AutomationResponse::failure("no visible enabled node matched selector");
        };

        let mut keystrokes = Vec::with_capacity(keys.len());
        for key in keys {
            match Keystroke::parse(&key) {
                Ok(keystroke) => keystrokes.push(keystroke),
                Err(err) => return AutomationResponse::failure(err.to_string()),
            }
        }

        self.dispatch_real_keystrokes(action, keystrokes, cx)
    }

    fn dispatch_real_keystrokes(
        &mut self,
        action: AutomationNodeAction,
        keystrokes: Vec<Keystroke>,
        cx: &mut Context<Self>,
    ) -> AutomationResponse {
        self.prepare_automation_text_action(&action);
        cx.notify();

        for keystroke in keystrokes {
            let event = KeyDownEvent {
                keystroke: keystroke.with_simulated_ime(),
                is_held: false,
            };
            self.apply_automation_text_key(&action, &event, cx);
            self.process_events(cx);
        }

        AutomationResponse::success(self.automation_snapshot())
    }

    fn prepare_automation_text_action(&mut self, action: &AutomationNodeAction) {
        match action {
            AutomationNodeAction::SetCommitSummary => {
                self.prepare_commit_summary_field_for_automation();
            }
            AutomationNodeAction::SetCommitBody => {
                self.prepare_commit_body_field_for_automation();
            }
            AutomationNodeAction::SetBranchFilter => {
                self.prepare_branch_filter_field_for_automation();
            }
            AutomationNodeAction::SetRepoFilter => {
                self.prepare_repo_filter_field_for_automation();
            }
            AutomationNodeAction::SetNewBranchName => {
                self.prepare_new_branch_field_for_automation();
            }
            AutomationNodeAction::SetSettingsField(field) => {
                self.activate_settings_field_for_automation(*field);
            }
            AutomationNodeAction::SetCreateRepositoryName => {
                self.activate_repository_field_for_automation(
                    crate::ui::app::RepositoryField::CreateName,
                );
            }
            AutomationNodeAction::SetCreateRepositoryDescription => {
                self.activate_repository_field_for_automation(
                    crate::ui::app::RepositoryField::CreateDescription,
                );
            }
            AutomationNodeAction::SetCreateRepositoryPath => {
                self.activate_repository_field_for_automation(
                    crate::ui::app::RepositoryField::CreatePath,
                );
            }
            AutomationNodeAction::SetCreateRepositoryBranch => {
                self.activate_repository_field_for_automation(
                    crate::ui::app::RepositoryField::CreateBranchName,
                );
            }
            AutomationNodeAction::SetCloneRepositoryUrl => {
                self.activate_repository_field_for_automation(
                    crate::ui::app::RepositoryField::CloneUrl,
                );
            }
            AutomationNodeAction::SetCloneRepositoryName => {
                self.activate_repository_field_for_automation(
                    crate::ui::app::RepositoryField::CloneName,
                );
            }
            AutomationNodeAction::SetCloneRepositoryPath => {
                self.activate_repository_field_for_automation(
                    crate::ui::app::RepositoryField::ClonePath,
                );
            }
            _ => {}
        }
    }

    fn apply_automation_text_key(
        &mut self,
        action: &AutomationNodeAction,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.modifiers.secondary()
            && event.keystroke.key == "enter"
            && self.nav.active_dialog == ActiveDialog::None
            && !self.nav.show_settings
            && !self.commit.summary.trim().is_empty()
        {
            self.commit_all(cx);
            return;
        }

        match action {
            AutomationNodeAction::SetCommitSummary => {
                self.apply_summary_key_for_automation(event, cx);
            }
            AutomationNodeAction::SetCommitBody => {
                self.apply_description_key_for_automation(event, cx);
            }
            AutomationNodeAction::SetBranchFilter => {
                self.apply_branch_filter_key_for_automation(event, cx);
            }
            AutomationNodeAction::SetRepoFilter => {
                self.apply_repo_filter_key_for_automation(event, cx);
            }
            AutomationNodeAction::SetNewBranchName => {
                self.apply_new_branch_key_for_automation(event, cx);
            }
            AutomationNodeAction::SetSettingsField(_) => {
                self.apply_settings_key_for_automation(event, cx);
            }
            AutomationNodeAction::SetCreateRepositoryName
            | AutomationNodeAction::SetCreateRepositoryDescription
            | AutomationNodeAction::SetCreateRepositoryPath
            | AutomationNodeAction::SetCreateRepositoryBranch
            | AutomationNodeAction::SetCloneRepositoryUrl
            | AutomationNodeAction::SetCloneRepositoryName
            | AutomationNodeAction::SetCloneRepositoryPath => {
                self.apply_repository_key_for_automation(event, cx);
            }
            _ => {}
        }
    }

    fn perform_automation_action(
        &mut self,
        action: AutomationNodeAction,
        fill_text: Option<String>,
        cx: &mut Context<Self>,
    ) -> AutomationResponse {
        match action {
            AutomationNodeAction::CommitAll => {
                self.handle_sidebar_action(SidebarAction::CommitAll, cx);
            }
            AutomationNodeAction::Network(action) => {
                self.handle_toolbar_action(ToolbarAction::RunNetworkAction(action), cx);
            }
            AutomationNodeAction::SelectChange(path) => {
                self.handle_sidebar_action(SidebarAction::SelectChange(path), cx);
            }
            AutomationNodeAction::SelectCommit(oid) => {
                self.handle_sidebar_action(SidebarAction::SelectCommit(oid), cx);
            }
            AutomationNodeAction::OpenSelectedBinaryWithDefaultProgram => {
                let Some(path) = self.selection.selected_change.clone() else {
                    return AutomationResponse::failure("no selected binary file");
                };
                if !self.selected_diff().is_some_and(|diff| diff.is_binary) {
                    return AutomationResponse::failure("selected file is not binary");
                }
                self.open_with_default_program(&path);
                cx.notify();
            }
            AutomationNodeAction::RevealSelectedBinaryInFinder => {
                let Some(path) = self.selection.selected_change.clone() else {
                    return AutomationResponse::failure("no selected binary file");
                };
                if !self.selected_diff().is_some_and(|diff| diff.is_binary) {
                    return AutomationResponse::failure("selected file is not binary");
                }
                self.reveal_in_finder(&path);
                cx.notify();
            }
            AutomationNodeAction::RevealSelectedImageInFinder => {
                let Some(path) = self.selection.selected_change.clone() else {
                    return AutomationResponse::failure("no selected image file");
                };
                if !self.selected_diff().is_some_and(|diff| diff.is_image) {
                    return AutomationResponse::failure("selected file is not an image");
                }
                self.reveal_in_finder(&path);
                cx.notify();
            }
            AutomationNodeAction::OpenSelectedImageWithDefaultProgram => {
                let Some(path) = self.selection.selected_change.clone() else {
                    return AutomationResponse::failure("no selected image file");
                };
                if !self.selected_diff().is_some_and(|diff| diff.is_image) {
                    return AutomationResponse::failure("selected file is not an image");
                }
                self.open_with_default_program(&path);
                cx.notify();
            }
            AutomationNodeAction::OpenSelectedSubmodule => {
                let Some(path) = self.selection.selected_change.clone() else {
                    return AutomationResponse::failure("no selected submodule");
                };
                if !self.selected_diff().is_some_and(|diff| diff.is_submodule) {
                    return AutomationResponse::failure("selected file is not a submodule");
                }
                self.open_submodule_repository(&path, cx);
            }
            AutomationNodeAction::RevealSelectedSubmodule => {
                let Some(path) = self.selection.selected_change.clone() else {
                    return AutomationResponse::failure("no selected submodule");
                };
                if !self.selected_diff().is_some_and(|diff| diff.is_submodule) {
                    return AutomationResponse::failure("selected file is not a submodule");
                }
                self.reveal_in_finder(&path);
                cx.notify();
            }
            AutomationNodeAction::ToggleDiffLine(target) => {
                self.toggle_diff_line_selection(target, cx);
            }
            AutomationNodeAction::DiscardSelectedDiffLines => {
                self.discard_selected_diff_lines(cx);
            }
            AutomationNodeAction::ToggleDiffOptionsMenu => {
                self.nav.show_diff_options_menu = !self.nav.show_diff_options_menu;
                cx.notify();
            }
            AutomationNodeAction::ShowUnifiedDiff => {
                self.nav.diff_options.show_side_by_side = false;
                self.nav.show_diff_options_menu = false;
                cx.notify();
            }
            AutomationNodeAction::ToggleSideBySideDiff => {
                self.toggle_side_by_side_diff(cx);
                self.nav.show_diff_options_menu = false;
            }
            AutomationNodeAction::ToggleHideWhitespaceChanges => {
                self.toggle_hide_whitespace_changes(cx);
                self.nav.show_diff_options_menu = false;
            }
            AutomationNodeAction::SelectTab(tab) => {
                self.nav.sidebar_tab = tab;
                if tab == SidebarTab::Changes {
                    self.repo.comparison = None;
                }
                cx.notify();
            }
            AutomationNodeAction::SetBranchFilter => {
                let Some(text) = fill_text else {
                    return AutomationResponse::failure("fill text is required");
                };
                self.filters.branch_filter_text = text;
                cx.notify();
            }
            AutomationNodeAction::SetCommitBody => {
                let Some(text) = fill_text else {
                    return AutomationResponse::failure("fill text is required");
                };
                self.commit.body = text;
                cx.notify();
            }
            AutomationNodeAction::SetCommitSummary => {
                let Some(text) = fill_text else {
                    return AutomationResponse::failure("fill text is required");
                };
                self.commit.summary = text;
                cx.notify();
            }
            AutomationNodeAction::OpenIdentitySettings => {
                self.open_identity_settings_from_warning(cx);
                cx.notify();
            }
            AutomationNodeAction::SetRepoFilter => {
                let Some(text) = fill_text else {
                    return AutomationResponse::failure("fill text is required");
                };
                self.filters.repo_filter_text = text;
                cx.notify();
            }
            AutomationNodeAction::OpenRecentRepo(path) => {
                self.handle_sidebar_action(SidebarAction::OpenRepo(path), cx);
            }
            AutomationNodeAction::SetSettingsField(field) => {
                if self.settings_field_read_only(field) {
                    return AutomationResponse::failure("settings field is read-only");
                }
                let Some(text) = fill_text else {
                    return AutomationResponse::failure("fill text is required");
                };
                self.set_automation_settings_field(field, text, cx);
            }
            AutomationNodeAction::SetSettingsSection(section) => {
                self.set_automation_settings_section(section, cx);
            }
            AutomationNodeAction::ShowBranchSelector(show) => {
                self.nav.show_branch_selector = show;
                self.nav.branch_selector_mode = BranchSelectorMode::Switch;
                if !show {
                    self.repo.pending_cherry_pick_oid = None;
                }
                if show {
                    self.nav.show_repo_selector = false;
                    self.nav.show_network_dropdown = false;
                }
                cx.notify();
            }
            AutomationNodeAction::ShowRepoSelector(show) => {
                self.nav.show_repo_selector = show;
                if show {
                    self.nav.show_branch_selector = false;
                    self.nav.branch_selector_mode = BranchSelectorMode::Switch;
                    self.repo.pending_cherry_pick_oid = None;
                    self.nav.show_network_dropdown = false;
                }
                cx.notify();
            }
            AutomationNodeAction::ShowSettings(show) => {
                if show {
                    self.open_global_settings_modal(None, cx);
                } else {
                    self.close_settings_modal();
                }
                cx.notify();
            }
            AutomationNodeAction::SwitchBranch(name) => {
                self.select_branch_from_selector(name, cx);
            }
            AutomationNodeAction::StartCreateBranch => {
                self.repo.new_branch_name = self.filters.branch_filter_text.clone();
                self.new_branch_cursor = self.repo.new_branch_name.len();
                self.new_branch_selection = None;
                self.repo.new_branch_start_point = None;
                self.nav.show_branch_selector = false;
                self.nav.branch_selector_mode = BranchSelectorMode::Switch;
                self.nav.active_dialog = ActiveDialog::CreateBranch;
                cx.notify();
            }
            AutomationNodeAction::SetNewBranchName => {
                self.repo.new_branch_name = fill_text.unwrap_or_default();
                self.new_branch_cursor = self.repo.new_branch_name.len();
                self.new_branch_selection = None;
                cx.notify();
            }
            AutomationNodeAction::SetBranchSwitchMode(bring_changes) => {
                self.repo.switch_branch_bring_changes = bring_changes;
                cx.notify();
            }
            AutomationNodeAction::ConfirmCreateBranch => {
                self.create_branch(cx);
            }
            AutomationNodeAction::ConfirmRenameBranch => {
                let old_name = match &self.nav.active_dialog {
                    ActiveDialog::RenameBranch { old_name } => old_name.clone(),
                    _ => return AutomationResponse::failure("rename branch dialog is not active"),
                };
                if !self.can_rename_branch_from_dialog(&old_name) {
                    return AutomationResponse::failure("rename branch name is invalid");
                }
                self.rename_branch(old_name, cx);
            }
            AutomationNodeAction::ConfirmDeleteBranch => {
                let branch_name = match &self.nav.active_dialog {
                    ActiveDialog::DeleteBranch { branch_name } => branch_name.clone(),
                    _ => return AutomationResponse::failure("delete branch dialog is not active"),
                };
                self.confirm_delete_branch(branch_name, cx);
            }
            AutomationNodeAction::ConfirmCreateTag => {
                let target_oid = match &self.nav.active_dialog {
                    ActiveDialog::CreateTag { target_oid } => target_oid.clone(),
                    _ => return AutomationResponse::failure("create tag dialog is not active"),
                };
                self.create_tag(target_oid, cx);
            }
            AutomationNodeAction::SelectTagToDelete(tag_name) => {
                if !matches!(
                    self.nav.active_dialog,
                    ActiveDialog::ChooseTagToDelete { .. }
                ) {
                    return AutomationResponse::failure("choose tag dialog is not active");
                }
                self.nav.active_dialog = ActiveDialog::DeleteTag { tag_name };
                cx.notify();
            }
            AutomationNodeAction::ConfirmDeleteTag => {
                let tag_name = match &self.nav.active_dialog {
                    ActiveDialog::DeleteTag { tag_name } => tag_name.clone(),
                    _ => return AutomationResponse::failure("delete tag dialog is not active"),
                };
                self.delete_tag(tag_name, cx);
            }
            AutomationNodeAction::ConfirmResetToCommit => {
                let target_oid = match &self.nav.active_dialog {
                    ActiveDialog::ResetToCommit { target_oid } => target_oid.clone(),
                    _ => {
                        return AutomationResponse::failure("reset to commit dialog is not active");
                    }
                };
                self.reset_to_commit(target_oid, cx);
            }
            AutomationNodeAction::ConfirmStashAndSwitch => {
                let target_branch = match &self.nav.active_dialog {
                    ActiveDialog::StashAndSwitch { target_branch } => target_branch.clone(),
                    _ => {
                        return AutomationResponse::failure(
                            "stash-and-switch dialog is not active",
                        );
                    }
                };
                if self.repo.switch_branch_bring_changes {
                    self.switch_branch_with_changes(target_branch, cx);
                } else {
                    self.stash_and_switch_branch(target_branch, cx);
                }
            }
            AutomationNodeAction::ConfirmStashChanges => {
                if !matches!(self.nav.active_dialog, ActiveDialog::StashChanges) {
                    return AutomationResponse::failure("stash changes dialog is not active");
                }
                self.stash_changes(cx);
            }
            AutomationNodeAction::ShowRestoreStash => {
                self.show_restore_stash_dialog(cx);
            }
            AutomationNodeAction::RestoreStash => {
                self.nav.active_dialog = ActiveDialog::None;
                self.restore_stash(cx);
            }
            AutomationNodeAction::ShowDiscardStash => {
                self.show_discard_stash_dialog(cx);
            }
            AutomationNodeAction::ConfirmDiscardStash => {
                if !matches!(self.nav.active_dialog, ActiveDialog::DiscardStash) {
                    return AutomationResponse::failure("discard stash dialog is not active");
                }
                self.discard_stash(cx);
            }
            AutomationNodeAction::SetCreateRepositoryName => {
                self.repo.create_repo_name = fill_text.unwrap_or_default();
                self.repository_create_name_cursor = self.repo.create_repo_name.len();
                self.repository_create_name_selection = None;
                cx.notify();
            }
            AutomationNodeAction::SetCreateRepositoryDescription => {
                self.repo.create_repo_description = fill_text.unwrap_or_default();
                self.repository_create_description_cursor = self.repo.create_repo_description.len();
                self.repository_create_description_selection = None;
                cx.notify();
            }
            AutomationNodeAction::SetCreateRepositoryPath => {
                self.repo.create_repo_path = fill_text.unwrap_or_default();
                self.repository_create_path_cursor = self.repo.create_repo_path.len();
                self.repository_create_path_selection = None;
                cx.notify();
            }
            AutomationNodeAction::SetCreateRepositoryBranch => {
                self.repo.create_repo_branch_name = fill_text.unwrap_or_default();
                self.repository_create_branch_cursor = self.repo.create_repo_branch_name.len();
                self.repository_create_branch_selection = None;
                cx.notify();
            }
            AutomationNodeAction::SetCreateRepositoryGitignore(template) => {
                self.repo.create_repo_gitignore_template = template;
                cx.notify();
            }
            AutomationNodeAction::SetCreateRepositoryLicense(template) => {
                self.repo.create_repo_license_template = template;
                cx.notify();
            }
            AutomationNodeAction::ToggleCreateRepositoryReadme => {
                self.repo.create_repo_initialize_readme = !self.repo.create_repo_initialize_readme;
                cx.notify();
            }
            AutomationNodeAction::ToggleCreateRepositoryInitialCommit => {
                self.repo.create_repo_initial_commit = !self.repo.create_repo_initial_commit;
                cx.notify();
            }
            AutomationNodeAction::ConfirmCreateRepository => {
                if !matches!(self.nav.active_dialog, ActiveDialog::CreateRepository) {
                    return AutomationResponse::failure("create repository dialog is not active");
                }
                if self.create_repository_validation_message().is_some() {
                    return AutomationResponse::failure("create repository form is invalid");
                }
                self.create_repository(cx);
            }
            AutomationNodeAction::SetCloneRepositoryUrl => {
                let previous_inferred_name =
                    inferred_clone_directory_name(&self.repo.clone_repo_url);
                let should_update_inferred_name = self.repo.clone_repo_name.trim().is_empty()
                    || self.repo.clone_repo_name == previous_inferred_name;
                self.repo.clone_repo_url = fill_text.unwrap_or_default();
                self.repository_clone_url_cursor = self.repo.clone_repo_url.len();
                self.repository_clone_url_selection = None;
                if should_update_inferred_name {
                    self.repo.clone_repo_name =
                        inferred_clone_directory_name(&self.repo.clone_repo_url);
                    self.repository_clone_name_cursor = self.repo.clone_repo_name.len();
                    self.repository_clone_name_selection = None;
                }
                cx.notify();
            }
            AutomationNodeAction::SetCloneRepositoryName => {
                self.repo.clone_repo_name = fill_text.unwrap_or_default();
                self.repository_clone_name_cursor = self.repo.clone_repo_name.len();
                self.repository_clone_name_selection = None;
                cx.notify();
            }
            AutomationNodeAction::SetCloneRepositoryPath => {
                self.repo.clone_repo_path = fill_text.unwrap_or_default();
                self.repository_clone_path_cursor = self.repo.clone_repo_path.len();
                self.repository_clone_path_selection = None;
                cx.notify();
            }
            AutomationNodeAction::ConfirmCloneRepository => {
                if !matches!(self.nav.active_dialog, ActiveDialog::CloneRepository) {
                    return AutomationResponse::failure("clone repository dialog is not active");
                }
                if self.clone_repository_validation_message().is_some() {
                    return AutomationResponse::failure("clone repository form is invalid");
                }
                self.clone_repository(cx);
            }
            AutomationNodeAction::SetPublishName => {
                self.network.publish_name = fill_text.unwrap_or_default();
                self.publish_name_cursor = self.network.publish_name.len();
                self.publish_name_selection = None;
                cx.notify();
            }
            AutomationNodeAction::SetPublishDescription => {
                self.network.publish_description = fill_text.unwrap_or_default();
                self.publish_description_cursor = self.network.publish_description.len();
                self.publish_description_selection = None;
                cx.notify();
            }
            AutomationNodeAction::TogglePublishPrivate => {
                self.network.publish_private = !self.network.publish_private;
                cx.notify();
            }
            AutomationNodeAction::ConfirmPublishRepository => {
                if !matches!(self.nav.active_dialog, ActiveDialog::PublishRepository) {
                    return AutomationResponse::failure("publish dialog is not active");
                }
                if self.network.publish_name.trim().is_empty() {
                    return AutomationResponse::failure("publish repository name is required");
                }
                self.publish_repository(cx);
            }
            AutomationNodeAction::SaveGitSettings => {
                self.handle_settings_action(SettingsAction::SaveGitConfig, cx);
            }
            AutomationNodeAction::SaveRemoteSettings => {
                self.handle_settings_action(SettingsAction::SaveRemote, cx);
            }
            AutomationNodeAction::SaveIgnoredFilesSettings => {
                self.handle_settings_action(SettingsAction::SaveIgnoredFiles, cx);
            }
            AutomationNodeAction::SaveAiSettings => {
                self.handle_settings_action(SettingsAction::SaveAiSettings, cx);
            }
            AutomationNodeAction::SetGitConfigScope(use_local) => {
                self.handle_settings_action(SettingsAction::SetGitConfigScope(use_local), cx);
            }
            AutomationNodeAction::TogglePullRebase => {
                if self.settings_has_repository_scope() {
                    let next = !self.repo.identity.pull_rebase.unwrap_or(false);
                    self.repo.identity.pull_rebase = Some(next);
                    cx.notify();
                }
            }
            AutomationNodeAction::ChangeAiProvider(provider) => {
                self.handle_settings_action(SettingsAction::ChangeProvider(provider), cx);
            }
            AutomationNodeAction::SetAppearance(pref) => {
                self.set_appearance(pref, None, cx);
            }
            AutomationNodeAction::ShowOpenRouterModelPicker => {
                self.settings_modal.show_model_picker = true;
                self.ensure_openrouter_models(cx);
                cx.notify();
            }
            AutomationNodeAction::SelectOpenRouterModel(model_id) => {
                self.handle_settings_action(SettingsAction::SelectOpenRouterModel(model_id), cx);
            }
            AutomationNodeAction::GenerateAiCommit => {
                self.handle_sidebar_action(SidebarAction::GenerateAiCommit, cx);
            }
            AutomationNodeAction::UndoLastCommit => {
                self.undo_last_commit(cx);
            }
            AutomationNodeAction::ExitCompare => {
                self.repo.comparison = None;
                self.selection.selected_commit_file = None;
                cx.notify();
            }
            AutomationNodeAction::MergeComparedBranch => {
                let Some(comparison) = self.repo.comparison.as_ref() else {
                    return AutomationResponse::failure("compare view is not active");
                };
                if comparison.behind == 0 {
                    return AutomationResponse::failure("compared branch has no commits to merge");
                }
                self.repo.merge_target = comparison.target_branch.clone();
                self.merge_branch(cx);
            }
            AutomationNodeAction::ContinueGitOperation => {
                self.continue_git_operation(cx);
            }
            AutomationNodeAction::SkipRebaseOperation => {
                self.skip_rebase_operation(cx);
            }
            AutomationNodeAction::AbortGitOperation => {
                self.abort_git_operation(cx);
            }
            AutomationNodeAction::OpenConflictInEditor(path) => {
                self.open_conflict_in_editor(path, cx);
            }
            AutomationNodeAction::RevealConflictFile(path) => {
                self.reveal_conflict_file(path, cx);
            }
            AutomationNodeAction::MarkConflictResolved(path) => {
                self.mark_conflict_resolved(path, cx);
            }
            AutomationNodeAction::CancelDialog => {
                self.nav.active_dialog = ActiveDialog::None;
                cx.notify();
            }
            AutomationNodeAction::ConfirmDiscardChanges => {
                if let ActiveDialog::DiscardChanges { paths } = &self.nav.active_dialog {
                    let paths = paths.clone();
                    for path in paths {
                        self.handle_sidebar_action(SidebarAction::DiscardChange(path), cx);
                    }
                }
                self.nav.active_dialog = ActiveDialog::None;
                cx.notify();
            }
            AutomationNodeAction::ChangeFile(path, action) => {
                self.perform_change_action(path, action, cx);
            }
            AutomationNodeAction::History(oid, action) => {
                self.perform_history_action(oid, action, cx);
            }
            AutomationNodeAction::Branch(name, action) => {
                self.perform_branch_action(name, action, cx);
            }
        }

        AutomationResponse::success(self.automation_snapshot())
    }

    fn set_automation_settings_section(
        &mut self,
        section: SettingsSection,
        cx: &mut Context<Self>,
    ) {
        self.nav.show_settings = true;
        self.nav.settings_section = self.nav.settings_scope.normalize_section(section);
        let field = crate::ui::settings_modal::default_settings_field(self.nav.settings_section);
        self.settings_modal.active_field = Some(field);
        self.set_automation_settings_cursor(field, self.settings_field_value(field).len());
        cx.notify();
    }

    fn set_automation_settings_field(
        &mut self,
        field: SettingsField,
        text: String,
        cx: &mut Context<Self>,
    ) {
        match field {
            SettingsField::RemoteUrl => {
                self.repo.remote_url = text;
                self.settings_modal.remote_url_selection = None;
            }
            SettingsField::IgnoredFiles => {
                self.repo.ignored_files_text = text;
                self.settings_modal.ignored_files_selection = None;
            }
            SettingsField::GitUserName => {
                self.active_git_settings_identity_mut().user_name = text;
                self.settings_modal.git_user_name_selection = None;
            }
            SettingsField::GitUserEmail => {
                self.active_git_settings_identity_mut().user_email = text;
                self.settings_modal.git_user_email_selection = None;
            }
            SettingsField::GitDefaultBranch => {
                self.active_git_settings_identity_mut().default_branch = if text.trim().is_empty() {
                    None
                } else {
                    Some(text)
                };
                self.settings_modal.git_default_branch_selection = None;
            }
            SettingsField::AiModel => {
                self.settings.ai.model = text;
                self.settings_modal.ai_model_selection = None;
            }
            SettingsField::AiEndpoint => {
                self.settings.ai.endpoint = text;
                self.settings_modal.ai_endpoint_selection = None;
            }
            SettingsField::AiApiKey => {
                self.settings.ai.api_key = text;
                self.settings_modal.ai_api_key_selection = None;
            }
            SettingsField::AiSystemPrompt => {
                self.settings.ai.system_prompt = text;
                self.settings_modal.ai_system_prompt_selection = None;
            }
            SettingsField::OpenRouterModelFilter => {
                self.filters.openrouter_model_filter = text;
                self.settings_modal.openrouter_model_filter_selection = None;
            }
        }
        self.settings_modal.active_field = Some(field);
        self.set_automation_settings_cursor(field, self.settings_field_value(field).len());
        cx.notify();
    }

    fn set_automation_settings_cursor(&mut self, field: SettingsField, cursor: usize) {
        match field {
            SettingsField::RemoteUrl => self.settings_modal.remote_url_cursor = cursor,
            SettingsField::IgnoredFiles => self.settings_modal.ignored_files_cursor = cursor,
            SettingsField::GitUserName => self.settings_modal.git_user_name_cursor = cursor,
            SettingsField::GitUserEmail => self.settings_modal.git_user_email_cursor = cursor,
            SettingsField::GitDefaultBranch => {
                self.settings_modal.git_default_branch_cursor = cursor;
            }
            SettingsField::AiModel => self.settings_modal.ai_model_cursor = cursor,
            SettingsField::AiEndpoint => self.settings_modal.ai_endpoint_cursor = cursor,
            SettingsField::AiApiKey => self.settings_modal.ai_api_key_cursor = cursor,
            SettingsField::AiSystemPrompt => self.settings_modal.ai_system_prompt_cursor = cursor,
            SettingsField::OpenRouterModelFilter => {
                self.settings_modal.openrouter_model_filter_cursor = cursor;
            }
        }
    }

    fn perform_change_action(
        &mut self,
        path: String,
        action: AutomationChangeAction,
        cx: &mut Context<Self>,
    ) {
        let sidebar_action = match action {
            AutomationChangeAction::Discard => SidebarAction::DiscardChange(path),
            AutomationChangeAction::PromptDiscard => {
                self.handle_changes_context_action(path, ChangesContextAction::DiscardChanges, cx);
                return;
            }
            AutomationChangeAction::IgnorePath => SidebarAction::IgnorePath(path),
            AutomationChangeAction::IgnoreFolder => {
                let folder = parent_folder_pattern(&path).unwrap_or(path);
                SidebarAction::IgnorePath(folder)
            }
            AutomationChangeAction::IgnoreExtension => {
                let ext = std::path::Path::new(&path)
                    .extension()
                    .map(|ext| ext.to_string_lossy().to_string())
                    .unwrap_or_default();
                SidebarAction::IgnoreExtension(ext)
            }
            AutomationChangeAction::CopyFullPath => SidebarAction::CopyFullPath(path),
            AutomationChangeAction::CopyRelativePath => SidebarAction::CopyRelativePath(path),
            AutomationChangeAction::RevealInFinder => SidebarAction::RevealInFinder(path),
            AutomationChangeAction::OpenInEditor => SidebarAction::OpenInEditor(path),
            AutomationChangeAction::OpenWithDefault => SidebarAction::OpenWithDefault(path),
            AutomationChangeAction::ViewOnGithub => {
                self.handle_changes_context_action(path, ChangesContextAction::ViewOnGitHub, cx);
                return;
            }
        };
        self.handle_sidebar_action(sidebar_action, cx);
    }

    fn perform_history_action(
        &mut self,
        oid: String,
        action: AutomationHistoryAction,
        cx: &mut Context<Self>,
    ) {
        let action = match action {
            AutomationHistoryAction::ResetToCommit => HistoryContextMenuAction::ResetToCommit,
            AutomationHistoryAction::CheckoutCommit => HistoryContextMenuAction::CheckoutCommit,
            AutomationHistoryAction::RevertChangesInCommit => {
                HistoryContextMenuAction::RevertChangesInCommit
            }
            AutomationHistoryAction::CreateBranchFromCommit => {
                HistoryContextMenuAction::CreateBranchFromCommit
            }
            AutomationHistoryAction::CreateTag => HistoryContextMenuAction::CreateTag,
            AutomationHistoryAction::DeleteTag => HistoryContextMenuAction::DeleteTag,
            AutomationHistoryAction::CherryPickCommit => HistoryContextMenuAction::CherryPickCommit,
            AutomationHistoryAction::CopySha => HistoryContextMenuAction::CopySha,
            AutomationHistoryAction::CopyDiff => HistoryContextMenuAction::CopyDiff,
            AutomationHistoryAction::CopyTag => HistoryContextMenuAction::CopyTag,
            AutomationHistoryAction::ViewOnGithub => HistoryContextMenuAction::ViewOnGitHub,
        };
        self.handle_history_context_menu_action_for_oid(oid, action, cx);
    }

    fn perform_branch_action(
        &mut self,
        name: String,
        action: AutomationBranchAction,
        cx: &mut Context<Self>,
    ) {
        let action = match action {
            AutomationBranchAction::Delete => BranchContextAction::Delete,
            AutomationBranchAction::Rename => BranchContextAction::Rename,
            AutomationBranchAction::CopyName => BranchContextAction::CopyName,
            AutomationBranchAction::ViewOnGithub => BranchContextAction::ViewOnGitHub,
        };
        self.handle_branch_context_action(name, action, cx);
    }
}

impl AutomationNode {
    fn children(mut self, children: Vec<AutomationNode>) -> Self {
        self.children = children;
        self
    }

    fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }
}

fn automation_node(
    id: impl Into<String>,
    role: AutomationRole,
    test_id: Option<impl Into<String>>,
    text: Option<&str>,
    action: Option<AutomationNodeAction>,
) -> AutomationNode {
    AutomationNode {
        id: id.into(),
        role,
        test_id: test_id.map(Into::into),
        text: text.map(str::to_string),
        visible: true,
        enabled: true,
        selected: false,
        action,
        children: Vec::new(),
    }
}

fn no_changes_action_nodes(
    has_github_remote: bool,
    has_remote: bool,
    ahead: usize,
    behind: usize,
) -> Vec<AutomationNode> {
    let mut nodes = Vec::new();

    if !has_remote {
        nodes.push(automation_node(
            "no-changes-publish",
            AutomationRole::Button,
            Some("no-changes-publish"),
            Some("Publish repository"),
            Some(AutomationNodeAction::Network(
                NetworkAction::PublishRepository,
            )),
        ));
    }

    if has_remote && behind > 0 {
        nodes.push(automation_node(
            "no-changes-pull",
            AutomationRole::Button,
            Some("no-changes-pull"),
            Some("Pull"),
            Some(AutomationNodeAction::Network(NetworkAction::Pull)),
        ));
    } else if has_remote && ahead > 0 {
        nodes.push(automation_node(
            "no-changes-push",
            AutomationRole::Button,
            Some("no-changes-push"),
            Some("Push"),
            Some(AutomationNodeAction::Network(NetworkAction::Push)),
        ));
    }

    nodes.extend([
        automation_node(
            "no-changes-editor",
            AutomationRole::Button,
            Some("no-changes-editor"),
            Some("Open in External Editor"),
            None::<AutomationNodeAction>,
        ),
        automation_node(
            "no-changes-finder",
            AutomationRole::Button,
            Some("no-changes-finder"),
            Some("Show in Finder"),
            None::<AutomationNodeAction>,
        ),
    ]);

    if has_github_remote {
        nodes.push(automation_node(
            "no-changes-github",
            AutomationRole::Button,
            Some("no-changes-github"),
            Some("View on GitHub"),
            None::<AutomationNodeAction>,
        ));
    }

    nodes
}

fn repo_selector_nodes(app: &GitSparkApp) -> Vec<AutomationNode> {
    let filter = app.filters.repo_filter_text.to_ascii_lowercase();
    let current_repo = app
        .repo
        .snapshot
        .as_ref()
        .map(|snapshot| snapshot.repo.path.clone());
    let repos = app
        .settings
        .recent_repos
        .iter()
        .filter(|path| {
            filter.is_empty()
                || path
                    .file_name()
                    .map(|name| {
                        name.to_string_lossy()
                            .to_ascii_lowercase()
                            .contains(&filter)
                    })
                    .unwrap_or(false)
        })
        .cloned()
        .collect::<Vec<_>>();

    let mut nodes = vec![automation_node(
        "repo-selector-add",
        AutomationRole::Button,
        Some("repo-add-btn"),
        Some("Add"),
        None::<AutomationNodeAction>,
    )];

    if repos.is_empty() {
        let empty_message = if filter.is_empty() {
            "No recent repositories"
        } else {
            "Sorry, I can't find that repository"
        };
        nodes.push(automation_node(
            "repo-selector-empty",
            AutomationRole::Status,
            Some("repo-selector-empty"),
            Some(empty_message),
            None::<AutomationNodeAction>,
        ));
        return nodes;
    }

    let mut repo_children = Vec::new();
    for repo_path in repos {
        let label = repo_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| repo_path.to_string_lossy().to_string());
        let id = stable_test_slug(&repo_path.to_string_lossy());
        repo_children.push(
            automation_node(
                format!("repo-{id}"),
                AutomationRole::ListItem,
                Some(format!("repo-{id}")),
                Some(label.as_str()),
                Some(AutomationNodeAction::OpenRecentRepo(repo_path.clone())),
            )
            .selected(current_repo.as_ref() == Some(&repo_path)),
        );
    }

    nodes.push(
        automation_node(
            "repo-list",
            AutomationRole::List,
            Some("repo-list"),
            Some("Recent repositories"),
            None::<AutomationNodeAction>,
        )
        .children(repo_children),
    );

    nodes
}

fn branch_selector_nodes(app: &GitSparkApp) -> Vec<AutomationNode> {
    let Some(snapshot) = app.repo.snapshot.as_ref() else {
        return Vec::new();
    };

    let filter = app.filters.branch_filter_text.to_ascii_lowercase();
    let visible_branches = snapshot
        .branches
        .iter()
        .filter(|branch| !branch.is_remote)
        .filter(|branch| filter.is_empty() || branch.name.to_ascii_lowercase().contains(&filter))
        .collect::<Vec<_>>();

    if !visible_branches.is_empty() {
        return Vec::new();
    }

    vec![automation_node(
        "branch-selector-empty",
        AutomationRole::Status,
        Some("branch-selector-empty"),
        Some("Sorry, I can't find that branch"),
        None::<AutomationNodeAction>,
    )]
}

fn settings_automation_nodes(app: &GitSparkApp) -> Vec<AutomationNode> {
    let sections = if app.settings_has_repository_scope() {
        vec![
            (SettingsSection::Remote, "settings-tab-remote", "Remote"),
            (
                SettingsSection::IgnoredFiles,
                "settings-tab-ignored-files",
                "Ignored Files",
            ),
            (SettingsSection::Git, "settings-tab-git", "Git"),
        ]
    } else {
        vec![
            (SettingsSection::Git, "settings-tab-git", "Git"),
            (SettingsSection::Ai, "settings-tab-ai", "AI Commit"),
            (
                SettingsSection::Appearance,
                "settings-tab-appearance",
                "Appearance",
            ),
            (
                SettingsSection::Integrations,
                "settings-tab-integrations",
                "Integrations",
            ),
        ]
    };

    let mut nodes = sections
        .into_iter()
        .map(|(section, test_id, label)| {
            automation_node(
                test_id,
                AutomationRole::Tab,
                Some(test_id),
                Some(label),
                Some(AutomationNodeAction::SetSettingsSection(section)),
            )
            .selected(app.nav.settings_section == section)
        })
        .collect::<Vec<_>>();

    match app.nav.settings_section {
        SettingsSection::Remote => {
            if app.repo.remote_name.is_some() {
                nodes.extend([
                    settings_field_node(
                        app,
                        "settings-remote-url",
                        "Primary Remote Repository URL",
                        SettingsField::RemoteUrl,
                        app.repo.remote_url.as_str(),
                    ),
                    automation_node(
                        "settings-save-remote",
                        AutomationRole::Button,
                        Some("settings-save-remote"),
                        Some("Save Remote"),
                        Some(AutomationNodeAction::SaveRemoteSettings),
                    ),
                ]);
            } else {
                nodes.extend([
                    automation_node(
                        "settings-remote-empty",
                        AutomationRole::Status,
                        Some("settings-remote-empty"),
                        Some("No remote configured"),
                        None::<AutomationNodeAction>,
                    ),
                    automation_node(
                        "settings-remote-publish",
                        AutomationRole::Button,
                        Some("settings-remote-publish"),
                        Some("Publish Repository"),
                        Some(AutomationNodeAction::Network(
                            NetworkAction::PublishRepository,
                        )),
                    ),
                ]);
            }
        }
        SettingsSection::IgnoredFiles => {
            nodes.extend([
                settings_field_node(
                    app,
                    "settings-ignored-files-text",
                    "Ignored files",
                    SettingsField::IgnoredFiles,
                    app.repo.ignored_files_text.as_str(),
                ),
                automation_node(
                    "settings-save-ignored-files",
                    AutomationRole::Button,
                    Some("settings-save-ignored-files"),
                    Some("Save"),
                    Some(AutomationNodeAction::SaveIgnoredFilesSettings),
                ),
            ]);
        }
        SettingsSection::Git => {
            nodes.extend([
                automation_node(
                    "settings-git-scope-global",
                    AutomationRole::Button,
                    Some("settings-git-scope-global"),
                    Some("Use my global Git config"),
                    Some(AutomationNodeAction::SetGitConfigScope(false)),
                )
                .visible(app.settings_has_repository_scope())
                .selected(!app.repo.use_local_identity),
                automation_node(
                    "settings-git-scope-local",
                    AutomationRole::Button,
                    Some("settings-git-scope-local"),
                    Some("Use a local Git config"),
                    Some(AutomationNodeAction::SetGitConfigScope(true)),
                )
                .visible(app.settings_has_repository_scope())
                .selected(app.repo.use_local_identity),
                settings_field_node(
                    app,
                    "settings-git-user-name",
                    "User Name",
                    SettingsField::GitUserName,
                    app.active_git_settings_identity().user_name.as_str(),
                ),
                settings_field_node(
                    app,
                    "settings-git-user-email",
                    "User Email",
                    SettingsField::GitUserEmail,
                    app.active_git_settings_identity().user_email.as_str(),
                ),
                settings_field_node(
                    app,
                    "settings-git-default-branch",
                    "Default Branch",
                    SettingsField::GitDefaultBranch,
                    app.active_git_settings_identity()
                        .default_branch
                        .as_deref()
                        .unwrap_or(""),
                ),
                automation_node(
                    "settings-pull-rebase",
                    AutomationRole::Button,
                    Some("settings-pull-rebase"),
                    Some("Use pull.rebase"),
                    Some(AutomationNodeAction::TogglePullRebase),
                )
                .visible(app.settings_has_repository_scope())
                .enabled(app.settings_has_repository_scope())
                .selected(app.repo.identity.pull_rebase.unwrap_or(false)),
                automation_node(
                    "settings-save-git",
                    AutomationRole::Button,
                    Some("settings-save-git"),
                    Some("Save Git Config"),
                    Some(AutomationNodeAction::SaveGitSettings),
                ),
            ]);
        }
        SettingsSection::Ai => {
            nodes.push(
                automation_node(
                    "settings-provider-openrouter",
                    AutomationRole::Button,
                    Some("settings-provider-openrouter"),
                    Some("OpenRouter"),
                    Some(AutomationNodeAction::ChangeAiProvider(
                        AiProvider::OpenRouter,
                    )),
                )
                .selected(app.settings.ai.provider == AiProvider::OpenRouter),
            );
            nodes.push(
                automation_node(
                    "settings-provider-openai-compatible",
                    AutomationRole::Button,
                    Some("settings-provider-openai-compatible"),
                    Some("OpenAI Compatible"),
                    Some(AutomationNodeAction::ChangeAiProvider(
                        AiProvider::OpenAICompatible,
                    )),
                )
                .selected(app.settings.ai.provider == AiProvider::OpenAICompatible),
            );

            if app.settings.ai.provider == AiProvider::OpenRouter {
                nodes.push(openrouter_model_picker_node(app));
            } else {
                nodes.push(settings_field_node(
                    app,
                    "settings-ai-model",
                    "Model",
                    SettingsField::AiModel,
                    app.settings.ai.model.as_str(),
                ));
            }

            if app.settings.ai.provider == AiProvider::OpenRouter {
                nodes.push(automation_node(
                    "settings-ai-endpoint",
                    AutomationRole::Status,
                    Some("settings-ai-endpoint"),
                    Some(app.settings.ai.endpoint.as_str()),
                    None,
                ));
            } else {
                nodes.push(settings_field_node(
                    app,
                    "settings-ai-endpoint",
                    "Endpoint",
                    SettingsField::AiEndpoint,
                    app.settings.ai.endpoint.as_str(),
                ));
            }

            nodes.extend([
                settings_field_node(
                    app,
                    "settings-ai-api-key",
                    "API Key",
                    SettingsField::AiApiKey,
                    app.settings.ai.api_key.as_str(),
                ),
                settings_field_node(
                    app,
                    "settings-ai-system-prompt",
                    "System Prompt",
                    SettingsField::AiSystemPrompt,
                    app.settings.ai.system_prompt.as_str(),
                ),
                automation_node(
                    "settings-save-ai",
                    AutomationRole::Button,
                    Some("settings-save-ai"),
                    Some("Save AI Settings"),
                    Some(AutomationNodeAction::SaveAiSettings),
                ),
            ]);
        }
        SettingsSection::Appearance => {
            let current = theme::appearance();
            for (pref, id, label) in [
                (theme::Appearance::Light, "settings-theme-light", "Light"),
                (theme::Appearance::Dark, "settings-theme-dark", "Dark"),
                (theme::Appearance::System, "settings-theme-system", "System"),
            ] {
                nodes.push(
                    automation_node(
                        id,
                        AutomationRole::Button,
                        Some(id),
                        Some(label),
                        Some(AutomationNodeAction::SetAppearance(pref)),
                    )
                    .selected(pref == current),
                );
            }
        }
        SettingsSection::Integrations => {}
    }

    nodes
}

fn openrouter_model_picker_node(app: &GitSparkApp) -> AutomationNode {
    let selected_model = app.settings.ai.model.as_str();
    let selected_model_name = match &app.filters.openrouter_models {
        OpenRouterModelsState::Ready(models) => models
            .iter()
            .find(|model| model.id == selected_model)
            .map(|model| model.name.as_str())
            .unwrap_or(selected_model),
        _ => selected_model,
    };

    let mut node = automation_node(
        "settings-ai-model",
        AutomationRole::Button,
        Some("settings-ai-model"),
        Some(selected_model_name),
        Some(AutomationNodeAction::ShowOpenRouterModelPicker),
    );

    if app.settings_modal.show_model_picker {
        let filter = app
            .filters
            .openrouter_model_filter
            .trim()
            .to_ascii_lowercase();
        let mut children = vec![settings_field_node(
            app,
            "settings-openrouter-model-filter",
            "Search Models",
            SettingsField::OpenRouterModelFilter,
            app.filters.openrouter_model_filter.as_str(),
        )];

        match &app.filters.openrouter_models {
            OpenRouterModelsState::Idle | OpenRouterModelsState::Loading => {
                children.push(automation_node(
                    "settings-openrouter-model-loading",
                    AutomationRole::Status,
                    Some("settings-openrouter-model-loading"),
                    Some("Loading OpenRouter models..."),
                    None::<AutomationNodeAction>,
                ));
            }
            OpenRouterModelsState::Error(message) => {
                children.push(automation_node(
                    "settings-openrouter-model-error",
                    AutomationRole::Status,
                    Some("settings-openrouter-model-error"),
                    Some(message.as_str()),
                    None::<AutomationNodeAction>,
                ));
            }
            OpenRouterModelsState::Ready(models) => {
                let mut list_children = Vec::new();
                let filtered = models
                    .iter()
                    .filter(|model| {
                        filter.is_empty()
                            || model.id.to_ascii_lowercase().contains(&filter)
                            || model.name.to_ascii_lowercase().contains(&filter)
                    })
                    .collect::<Vec<_>>();
                for model in filtered.iter().take(24) {
                    let model = *model;
                    list_children.push(
                        automation_node(
                            format!("settings-model-{}", stable_test_slug(&model.id)),
                            AutomationRole::ListItem,
                            Some(format!("settings-model-{}", stable_test_slug(&model.id))),
                            Some(model.name.as_str()),
                            Some(AutomationNodeAction::SelectOpenRouterModel(
                                model.id.clone(),
                            )),
                        )
                        .selected(model.id == selected_model),
                    );
                }
                children.push(
                    automation_node(
                        "settings-openrouter-model-list",
                        AutomationRole::List,
                        Some("settings-openrouter-model-list"),
                        Some(format!("OpenRouter models ({})", filtered.len()).as_str()),
                        None::<AutomationNodeAction>,
                    )
                    .children(list_children),
                );
            }
        }

        node = node.children(children);
    }

    node
}

fn settings_field_node(
    app: &GitSparkApp,
    test_id: &'static str,
    label: &'static str,
    field: SettingsField,
    value: &str,
) -> AutomationNode {
    automation_node(
        test_id,
        AutomationRole::Textbox,
        Some(test_id),
        Some(value),
        Some(AutomationNodeAction::SetSettingsField(field)),
    )
    .enabled(!app.settings_field_read_only(field))
    .selected(app.settings_modal.active_field == Some(field))
    .children(vec![automation_node(
        format!("{test_id}-label"),
        AutomationRole::Status,
        None::<String>,
        Some(label),
        None,
    )])
}

fn change_action_nodes(path: &str, status: &str, has_github_remote: bool) -> Vec<AutomationNode> {
    let slug = stable_test_slug(path);
    let extension = std::path::Path::new(path)
        .extension()
        .map(|ext| ext.to_string_lossy().to_string())
        .unwrap_or_default();
    let folder = parent_folder_pattern(path);
    let basename = std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let ignore_enabled = basename != ".gitignore";
    let file_action_enabled =
        !(status.contains('D') && !status.contains('A') && !status.contains('?'));
    let mut actions = vec![
        (
            "discard",
            crate::ui::labels::discard_changes_menu().to_string(),
            AutomationChangeAction::Discard,
            true,
        ),
        (
            "prompt-discard",
            "Prompt discard change".to_string(),
            AutomationChangeAction::PromptDiscard,
            true,
        ),
        (
            "ignore-path",
            crate::ui::labels::ignore_file_menu().to_string(),
            AutomationChangeAction::IgnorePath,
            ignore_enabled,
        ),
        (
            "copy-full-path",
            crate::ui::labels::copy_file_path_menu().to_string(),
            AutomationChangeAction::CopyFullPath,
            true,
        ),
        (
            "copy-relative-path",
            crate::ui::labels::copy_relative_file_path_menu().to_string(),
            AutomationChangeAction::CopyRelativePath,
            true,
        ),
        (
            "reveal-in-finder",
            crate::ui::labels::reveal_in_file_manager_menu().to_string(),
            AutomationChangeAction::RevealInFinder,
            file_action_enabled,
        ),
        (
            "open-in-editor",
            crate::ui::labels::open_in_external_editor_menu().to_string(),
            AutomationChangeAction::OpenInEditor,
            file_action_enabled,
        ),
        (
            "open-with-default",
            crate::ui::labels::open_with_default_program_menu().to_string(),
            AutomationChangeAction::OpenWithDefault,
            file_action_enabled,
        ),
    ];

    if folder.is_some() {
        actions.insert(
            3,
            (
                "ignore-folder",
                crate::ui::labels::ignore_folder_menu().to_string(),
                AutomationChangeAction::IgnoreFolder,
                ignore_enabled,
            ),
        );
    }

    if !extension.is_empty() {
        actions.insert(
            if folder.is_some() { 4 } else { 3 },
            (
                "ignore-extension",
                crate::ui::labels::ignore_all_extension_menu(&extension),
                AutomationChangeAction::IgnoreExtension,
                ignore_enabled,
            ),
        );
    }

    if has_github_remote {
        actions.push((
            "view-on-github",
            "View on GitHub".to_string(),
            AutomationChangeAction::ViewOnGithub,
            true,
        ));
    }

    actions
        .into_iter()
        .map(|(suffix, label, action, enabled)| {
            automation_node(
                format!("change-{slug}-{suffix}"),
                AutomationRole::Button,
                Some(format!("change-{slug}-{suffix}")),
                Some(label.as_str()),
                Some(AutomationNodeAction::ChangeFile(path.to_string(), action)),
            )
            .enabled(enabled)
        })
        .collect()
}

fn parent_folder_pattern(path: &str) -> Option<String> {
    std::path::Path::new(path).parent().and_then(|parent| {
        let folder = parent.to_string_lossy().replace('\\', "/");
        if folder.is_empty() {
            None
        } else {
            Some(format!("{folder}/"))
        }
    })
}

fn history_action_nodes(
    short_oid: &str,
    oid: &str,
    tags: &[String],
    can_reset_to_commit: bool,
    has_github_remote: bool,
) -> Vec<AutomationNode> {
    let slug = stable_test_slug(short_oid);
    let mut actions = vec![
        (
            "reset",
            crate::ui::labels::reset_to_commit_menu().to_string(),
            AutomationHistoryAction::ResetToCommit,
        ),
        (
            "checkout",
            crate::ui::labels::checkout_commit_menu().to_string(),
            AutomationHistoryAction::CheckoutCommit,
        ),
        (
            "revert",
            crate::ui::labels::revert_changes_in_commit_menu().to_string(),
            AutomationHistoryAction::RevertChangesInCommit,
        ),
        (
            "cherry-pick",
            crate::ui::labels::cherry_pick_commit_menu().to_string(),
            AutomationHistoryAction::CherryPickCommit,
        ),
        (
            "create-branch",
            crate::ui::labels::create_branch_from_commit_menu().to_string(),
            AutomationHistoryAction::CreateBranchFromCommit,
        ),
        (
            "create-tag",
            crate::ui::labels::create_tag_menu().to_string(),
            AutomationHistoryAction::CreateTag,
        ),
        (
            "copy-sha",
            "Copy SHA".to_string(),
            AutomationHistoryAction::CopySha,
        ),
        (
            "copy-diff",
            "Copy diff".to_string(),
            AutomationHistoryAction::CopyDiff,
        ),
    ];

    let copy_tag_label = if tags.len() > 1 {
        "Copy Tags"
    } else {
        "Copy Tag"
    };
    actions.push((
        "copy-tag",
        copy_tag_label.to_string(),
        AutomationHistoryAction::CopyTag,
    ));
    actions.push((
        "delete-tag",
        crate::ui::history_context_menu::delete_tag_label(tags),
        AutomationHistoryAction::DeleteTag,
    ));

    if has_github_remote {
        actions.push((
            "view-on-github",
            "View on GitHub".to_string(),
            AutomationHistoryAction::ViewOnGithub,
        ));
    }

    actions
        .into_iter()
        .map(|(suffix, label, action)| {
            let enabled = match action {
                AutomationHistoryAction::CopyTag => !tags.is_empty(),
                AutomationHistoryAction::DeleteTag => !tags.is_empty(),
                AutomationHistoryAction::ResetToCommit => can_reset_to_commit,
                _ => true,
            };
            automation_node(
                format!("commit-{slug}-{suffix}"),
                AutomationRole::Button,
                Some(format!("commit-{slug}-{suffix}")),
                Some(label.as_str()),
                Some(AutomationNodeAction::History(oid.to_string(), action)),
            )
            .enabled(enabled)
        })
        .collect()
}

fn branch_action_nodes(name: &str, has_github_remote: bool) -> Vec<AutomationNode> {
    let slug = stable_test_slug(name);
    let mut actions = vec![
        (
            "rename",
            crate::ui::labels::rename_branch_context_menu(),
            AutomationBranchAction::Rename,
        ),
        (
            "copy-name",
            crate::ui::labels::copy_branch_name_menu(),
            AutomationBranchAction::CopyName,
        ),
        (
            "delete",
            crate::ui::labels::delete_branch_context_menu(),
            AutomationBranchAction::Delete,
        ),
    ];

    if has_github_remote {
        actions.push((
            "view-on-github",
            crate::ui::labels::view_branch_on_github_menu(),
            AutomationBranchAction::ViewOnGithub,
        ));
    }

    actions
        .into_iter()
        .map(|(suffix, label, action)| {
            automation_node(
                format!("branch-{slug}-{suffix}"),
                AutomationRole::Button,
                Some(format!("branch-{slug}-{suffix}")),
                Some(label),
                Some(AutomationNodeAction::Branch(name.to_string(), action)),
            )
        })
        .collect()
}

fn collect_matching_nodes(
    node: &AutomationNode,
    selector: &AutomationSelector,
    matches: &mut Vec<AutomationNode>,
) {
    if node_matches_selector(node, selector) {
        matches.push(node.clone());
    }

    for child in &node.children {
        collect_matching_nodes(child, selector, matches);
    }
}

fn node_matches_selector(node: &AutomationNode, selector: &AutomationSelector) -> bool {
    match selector {
        AutomationSelector::TestId { value } => node.test_id.as_deref() == Some(value.as_str()),
        AutomationSelector::Text { value } => node.text.as_deref() == Some(value.as_str()),
    }
}

fn stable_test_slug(value: &str) -> String {
    stable_id_slug(value)
}

fn typed_keystroke_for_char(ch: char) -> Keystroke {
    let key = match ch {
        ' ' => "space".to_string(),
        '\n' => "enter".to_string(),
        '\t' => "tab".to_string(),
        ch => ch.to_string(),
    };
    let key_char = match ch {
        '\n' => "\n".to_string(),
        '\t' => "\t".to_string(),
        ch => ch.to_string(),
    };
    Keystroke {
        modifiers: Modifiers::none(),
        key,
        key_char: Some(key_char),
    }
}

struct AutomationConfig {
    addr: String,
    ready_file: Option<PathBuf>,
}

impl AutomationConfig {
    fn from_env() -> Result<Option<Self>, String> {
        let enabled = env::var("GITSPARK_AUTOMATION").unwrap_or_default();
        let addr_override = env::var("GITSPARK_AUTOMATION_ADDR").ok();

        if !automation_enabled(&enabled) && addr_override.is_none() {
            return Ok(None);
        }

        let addr = addr_override.unwrap_or_else(|| automation_addr_from_value(&enabled));
        let ready_file = env::var_os("GITSPARK_AUTOMATION_READY_FILE").map(PathBuf::from);

        Ok(Some(Self { addr, ready_file }))
    }
}

fn automation_enabled(value: &str) -> bool {
    let value = value.trim();
    !(value.is_empty()
        || value == "0"
        || value.eq_ignore_ascii_case("false")
        || value.eq_ignore_ascii_case("off"))
}

fn automation_addr_from_value(value: &str) -> String {
    let value = value.trim();
    if value.contains(':') {
        value.to_string()
    } else if value.chars().all(|ch| ch.is_ascii_digit()) && !value.is_empty() && value != "1" {
        format!("127.0.0.1:{value}")
    } else {
        DEFAULT_ADDR.to_string()
    }
}

fn start_server(config: AutomationConfig, event_tx: NotifySender) -> io::Result<AutomationHandle> {
    let listener = TcpListener::bind(&config.addr)?;
    listener.set_nonblocking(true)?;
    let bound_addr = listener.local_addr()?;
    if let Some(path) = config.ready_file {
        fs::write(path, bound_addr.to_string())?;
    }

    eprintln!("GitSpark automation listening on {bound_addr}");

    let shutdown = Arc::new(AtomicBool::new(false));
    let thread_shutdown = Arc::clone(&shutdown);
    let thread = thread::spawn(move || run_server(listener, bound_addr, event_tx, thread_shutdown));

    Ok(AutomationHandle {
        shutdown,
        _thread: thread,
    })
}

fn run_server(
    listener: TcpListener,
    bound_addr: SocketAddr,
    event_tx: NotifySender,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                let client_tx = event_tx.clone();
                thread::spawn(move || {
                    let _ = stream.set_nonblocking(false);
                    if let Err(err) = handle_client(stream, client_tx) {
                        eprintln!("GitSpark automation client error on {bound_addr}: {err}");
                    }
                });
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(25));
            }
            Err(err) => {
                eprintln!("GitSpark automation listener error on {bound_addr}: {err}");
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

fn handle_client(stream: TcpStream, event_tx: NotifySender) -> io::Result<()> {
    let read_stream = stream.try_clone()?;
    let reader = BufReader::new(read_stream);
    let mut writer = stream;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<AutomationCommand>(&line) {
            Ok(command) => dispatch_command(command, &event_tx),
            Err(err) => AutomationResponse::failure(format!("invalid automation command: {err}")),
        };

        serde_json::to_writer(&mut writer, &response)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
    }

    Ok(())
}

fn dispatch_command(command: AutomationCommand, event_tx: &NotifySender) -> AutomationResponse {
    let (respond_to, response_rx) = mpsc::channel();
    event_tx.send(AppEvent::Automation(AutomationRequest {
        command,
        respond_to,
    }));

    match response_rx.recv_timeout(RESPONSE_TIMEOUT) {
        Ok(response) => response,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            AutomationResponse::failure("automation command timed out")
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            AutomationResponse::failure("automation command channel closed")
        }
    }
}

fn active_dialog_name(dialog: &ActiveDialog) -> &'static str {
    match dialog {
        ActiveDialog::None => "none",
        ActiveDialog::CreateBranch => "create_branch",
        ActiveDialog::DiscardChanges { .. } => "discard_changes",
        ActiveDialog::StashAndSwitch { .. } => "stash_and_switch",
        ActiveDialog::StashChanges => "stash_changes",
        ActiveDialog::RenameBranch { .. } => "rename_branch",
        ActiveDialog::DeleteBranch { .. } => "delete_branch",
        ActiveDialog::CreateTag { .. } => "create_tag",
        ActiveDialog::ChooseTagToDelete { .. } => "choose_tag_to_delete",
        ActiveDialog::DeleteTag { .. } => "delete_tag",
        ActiveDialog::ResetToCommit { .. } => "reset_to_commit",
        ActiveDialog::CreateRepository => "create_repository",
        ActiveDialog::CloneRepository => "clone_repository",
        ActiveDialog::RestoreStash => "restore_stash",
        ActiveDialog::DiscardStash => "discard_stash",
        ActiveDialog::PublishRepository => "publish_repository",
    }
}

fn network_action_name(action: NetworkAction) -> String {
    match action {
        NetworkAction::Fetch => "fetch",
        NetworkAction::Pull => "pull",
        NetworkAction::Push => "push",
        NetworkAction::PublishBranch => "publish_branch",
        NetworkAction::PublishRepository => "publish_repository",
    }
    .to_string()
}

fn git_operation_kind_name(kind: &GitOperationKind) -> &'static str {
    match kind {
        GitOperationKind::Merge => "merge",
        GitOperationKind::Rebase => "rebase",
    }
}

fn ai_provider_name(provider: &AiProvider) -> &'static str {
    match provider {
        AiProvider::OpenRouter => "openrouter",
        AiProvider::OpenAICompatible => "openai_compatible",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ping_command() {
        let command: AutomationCommand =
            serde_json::from_str(r#"{"command":"ping"}"#).expect("ping command parses");

        assert!(matches!(command, AutomationCommand::Ping));
    }

    #[test]
    fn parses_select_tab_command() {
        let command: AutomationCommand =
            serde_json::from_str(r#"{"command":"select_tab","tab":"history"}"#)
                .expect("select tab command parses");

        match command {
            AutomationCommand::SelectTab { tab } => {
                assert!(matches!(tab, AutomationSidebarTab::History));
            }
            _ => panic!("expected select_tab command"),
        }
    }

    #[test]
    fn parses_selector_click_command() {
        let command: AutomationCommand = serde_json::from_str(
            r#"{"command":"click","selector":{"by":"test_id","value":"change-src-main-rs"}}"#,
        )
        .expect("selector click command parses");

        match command {
            AutomationCommand::Click {
                selector: AutomationSelector::TestId { value },
            } => {
                assert_eq!(value, "change-src-main-rs");
            }
            _ => panic!("expected click command with test id selector"),
        }
    }

    #[test]
    fn parses_real_keyboard_commands() {
        let typed: AutomationCommand = serde_json::from_str(
            r#"{"command":"type_text","selector":{"by":"test_id","value":"input-commit-summary"},"text":"hello"}"#,
        )
        .expect("type text command parses");
        match typed {
            AutomationCommand::TypeText { text, .. } => assert_eq!(text, "hello"),
            _ => panic!("expected type_text command"),
        }

        let pressed: AutomationCommand = serde_json::from_str(
            r#"{"command":"press_keys","selector":{"by":"test_id","value":"input-commit-summary"},"keys":["cmd-enter","backspace"]}"#,
        )
        .expect("press keys command parses");
        match pressed {
            AutomationCommand::PressKeys { keys, .. } => {
                assert_eq!(keys, ["cmd-enter", "backspace"]);
            }
            _ => panic!("expected press_keys command"),
        }
    }

    #[test]
    fn parses_stash_commands() {
        let stash_all: AutomationCommand =
            serde_json::from_str(r#"{"command":"stash_all"}"#).expect("stash all command parses");
        assert!(matches!(stash_all, AutomationCommand::StashAll));

        let stash_pop: AutomationCommand =
            serde_json::from_str(r#"{"command":"stash_pop"}"#).expect("stash pop command parses");
        assert!(matches!(stash_pop, AutomationCommand::StashPop));
    }

    #[test]
    fn parses_publish_repository_network_action() {
        let command: AutomationCommand =
            serde_json::from_str(r#"{"command":"network_action","action":"publish_repository"}"#)
                .expect("publish repository network action parses");

        assert!(matches!(
            command,
            AutomationCommand::NetworkAction {
                action: AutomationNetworkAction::PublishRepository,
            }
        ));
    }

    #[test]
    fn parses_settings_field_command() {
        let model_command: AutomationCommand = serde_json::from_str(
            r#"{"command":"set_settings_field","field":"ai_model","text":"gpt-test"}"#,
        )
        .expect("settings field command parses");

        match model_command {
            AutomationCommand::SetSettingsField {
                field: AutomationSettingsField::AiModel,
                text,
            } => assert_eq!(text, "gpt-test"),
            _ => panic!("expected set_settings_field command"),
        }

        let endpoint_command: AutomationCommand = serde_json::from_str(
            r#"{"command":"set_settings_field","field":"ai_endpoint","text":"http://localhost/v1/chat/completions"}"#,
        )
        .expect("endpoint settings field command parses");

        match endpoint_command {
            AutomationCommand::SetSettingsField {
                field: AutomationSettingsField::AiEndpoint,
                text,
            } => assert_eq!(text, "http://localhost/v1/chat/completions"),
            _ => panic!("expected set_settings_field endpoint command"),
        }
    }

    #[test]
    fn parses_change_action_command() {
        let command: AutomationCommand = serde_json::from_str(
            r#"{"command":"change_action","path":"scratch.log","action":"ignore_extension"}"#,
        )
        .expect("change action command parses");

        match command {
            AutomationCommand::ChangeAction {
                path,
                action: AutomationChangeAction::IgnoreExtension,
            } => assert_eq!(path, "scratch.log"),
            _ => panic!("expected change_action command"),
        }

        let command: AutomationCommand = serde_json::from_str(
            r#"{"command":"change_action","path":"nested/ignored.tmp","action":"ignore_folder"}"#,
        )
        .expect("ignore folder change action command parses");

        match command {
            AutomationCommand::ChangeAction {
                path,
                action: AutomationChangeAction::IgnoreFolder,
            } => assert_eq!(path, "nested/ignored.tmp"),
            _ => panic!("expected ignore_folder change_action command"),
        }
    }

    #[test]
    fn parses_prompt_discard_change_action() {
        let command: AutomationCommand = serde_json::from_str(
            r#"{"command":"change_action","path":"README.md","action":"prompt_discard"}"#,
        )
        .expect("prompt discard command parses");

        assert!(matches!(
            command,
            AutomationCommand::ChangeAction {
                action: AutomationChangeAction::PromptDiscard,
                ..
            }
        ));
    }

    #[test]
    fn parses_native_open_change_actions() {
        let reveal: AutomationCommand = serde_json::from_str(
            r#"{"command":"change_action","path":"README.md","action":"reveal_in_finder"}"#,
        )
        .expect("reveal in finder command parses");
        assert!(matches!(
            reveal,
            AutomationCommand::ChangeAction {
                action: AutomationChangeAction::RevealInFinder,
                ..
            }
        ));

        let open_default: AutomationCommand = serde_json::from_str(
            r#"{"command":"change_action","path":"README.md","action":"open_with_default"}"#,
        )
        .expect("open with default command parses");
        assert!(matches!(
            open_default,
            AutomationCommand::ChangeAction {
                action: AutomationChangeAction::OpenWithDefault,
                ..
            }
        ));
    }

    #[test]
    fn parses_undo_last_commit_command() {
        let command: AutomationCommand = serde_json::from_str(r#"{"command":"undo_last_commit"}"#)
            .expect("undo last commit command parses");

        assert!(matches!(command, AutomationCommand::UndoLastCommit));
    }

    #[test]
    fn parses_history_action_command() {
        let command: AutomationCommand = serde_json::from_str(
            r#"{"command":"history_action","oid":"abc123","action":"copy_diff"}"#,
        )
        .expect("history action command parses");

        match command {
            AutomationCommand::HistoryAction {
                oid,
                action: AutomationHistoryAction::CopyDiff,
            } => assert_eq!(oid, "abc123"),
            _ => panic!("expected history_action command"),
        }
    }

    #[test]
    fn parses_history_repo_operation_commands() {
        let checkout: AutomationCommand = serde_json::from_str(
            r#"{"command":"history_action","oid":"abc123","action":"checkout_commit"}"#,
        )
        .expect("checkout commit command parses");
        assert!(matches!(
            checkout,
            AutomationCommand::HistoryAction {
                action: AutomationHistoryAction::CheckoutCommit,
                ..
            }
        ));

        let revert: AutomationCommand = serde_json::from_str(
            r#"{"command":"history_action","oid":"abc123","action":"revert_changes_in_commit"}"#,
        )
        .expect("revert commit command parses");
        assert!(matches!(
            revert,
            AutomationCommand::HistoryAction {
                action: AutomationHistoryAction::RevertChangesInCommit,
                ..
            }
        ));

        let cherry_pick: AutomationCommand = serde_json::from_str(
            r#"{"command":"history_action","oid":"abc123","action":"cherry_pick_commit"}"#,
        )
        .expect("cherry-pick commit command parses");
        assert!(matches!(
            cherry_pick,
            AutomationCommand::HistoryAction {
                action: AutomationHistoryAction::CherryPickCommit,
                ..
            }
        ));

        let delete_tag: AutomationCommand = serde_json::from_str(
            r#"{"command":"history_action","oid":"abc123","action":"delete_tag"}"#,
        )
        .expect("delete tag command parses");
        assert!(matches!(
            delete_tag,
            AutomationCommand::HistoryAction {
                action: AutomationHistoryAction::DeleteTag,
                ..
            }
        ));

        let view_on_github: AutomationCommand = serde_json::from_str(
            r#"{"command":"history_action","oid":"abc123","action":"view_on_github"}"#,
        )
        .expect("view on github command parses");
        assert!(matches!(
            view_on_github,
            AutomationCommand::HistoryAction {
                action: AutomationHistoryAction::ViewOnGithub,
                ..
            }
        ));
    }

    #[test]
    fn parses_branch_action_command() {
        let command: AutomationCommand = serde_json::from_str(
            r#"{"command":"branch_action","name":"delete/me","action":"delete"}"#,
        )
        .expect("branch action command parses");

        match command {
            AutomationCommand::BranchAction {
                name,
                action: AutomationBranchAction::Delete,
            } => assert_eq!(name, "delete/me"),
            _ => panic!("expected branch_action command"),
        }

        let view: AutomationCommand = serde_json::from_str(
            r#"{"command":"branch_action","name":"main","action":"view_on_github"}"#,
        )
        .expect("branch view on github command parses");
        assert!(matches!(
            view,
            AutomationCommand::BranchAction {
                action: AutomationBranchAction::ViewOnGithub,
                ..
            }
        ));
    }

    #[test]
    fn parses_branch_operation_commands() {
        let create: AutomationCommand =
            serde_json::from_str(r#"{"command":"create_branch","name":"e2e-created"}"#)
                .expect("create branch command parses");
        match create {
            AutomationCommand::CreateBranch { name } => assert_eq!(name, "e2e-created"),
            _ => panic!("expected create_branch command"),
        }

        let merge: AutomationCommand =
            serde_json::from_str(r#"{"command":"merge_branch","name":"merge/source"}"#)
                .expect("merge branch command parses");
        match merge {
            AutomationCommand::MergeBranch { name } => assert_eq!(name, "merge/source"),
            _ => panic!("expected merge_branch command"),
        }

        let rebase: AutomationCommand =
            serde_json::from_str(r#"{"command":"rebase_branch","name":"main"}"#)
                .expect("rebase branch command parses");
        match rebase {
            AutomationCommand::RebaseBranch { name } => assert_eq!(name, "main"),
            _ => panic!("expected rebase_branch command"),
        }

        let update: AutomationCommand =
            serde_json::from_str(r#"{"command":"update_from_default_branch"}"#)
                .expect("update from default branch command parses");
        assert!(matches!(update, AutomationCommand::UpdateFromDefaultBranch));

        let compare: AutomationCommand =
            serde_json::from_str(r#"{"command":"compare_branch","name":"main"}"#)
                .expect("compare branch command parses");
        match compare {
            AutomationCommand::CompareBranch { name } => assert_eq!(name, "main"),
            _ => panic!("expected compare_branch command"),
        }

        let compare_on_github: AutomationCommand =
            serde_json::from_str(r#"{"command":"compare_current_branch_on_github"}"#)
                .expect("compare current branch on github command parses");
        assert!(matches!(
            compare_on_github,
            AutomationCommand::CompareCurrentBranchOnGithub
        ));
    }

    #[test]
    fn derives_addr_from_env_value() {
        assert_eq!(automation_addr_from_value("1"), DEFAULT_ADDR);
        assert_eq!(automation_addr_from_value("9000"), "127.0.0.1:9000");
        assert_eq!(automation_addr_from_value("127.0.0.1:0"), "127.0.0.1:0");
    }

    #[test]
    fn creates_stable_test_slugs() {
        assert_eq!(stable_test_slug("src/main.rs"), "src-main-rs");
        assert_eq!(stable_test_slug("feature/login-ui"), "feature-login-ui");
        assert_eq!(stable_test_slug("..."), "item");
    }
}
