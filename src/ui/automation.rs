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

use gpui::Context;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::git::GitClient;
use crate::models::AiProvider;
use crate::ui::app::SettingsAction;
use crate::ui::app::{AppEvent, GitSparkApp, NotifySender, SidebarAction, ToolbarAction};
use crate::ui::branch_context_menu::BranchContextAction;
use crate::ui::changes_context_menu::ChangesContextAction;
use crate::ui::domain_state::NetworkAction;
use crate::ui::history_context_menu::HistoryContextMenuAction;
use crate::ui::settings_modal::SettingsField;
use crate::ui::ui_state::SettingsSection;
use crate::ui::ui_state::{ActiveDialog, SidebarTab};

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
    NetworkAction {
        action: AutomationNetworkAction,
    },
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
}

impl From<AutomationNetworkAction> for NetworkAction {
    fn from(action: AutomationNetworkAction) -> Self {
        match action {
            AutomationNetworkAction::Fetch => Self::Fetch,
            AutomationNetworkAction::Pull => Self::Pull,
            AutomationNetworkAction::Push => Self::Push,
        }
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AutomationSettingsSection {
    Git,
    Ai,
    Appearance,
    Integrations,
}

impl From<AutomationSettingsSection> for SettingsSection {
    fn from(section: AutomationSettingsSection) -> Self {
        match section {
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
    settings_section: AutomationSettingsSection,
    git_user_name: String,
    git_user_email: String,
    git_default_branch: Option<String>,
    git_pull_rebase: Option<bool>,
    ai_provider: String,
    ai_model: String,
    ai_endpoint: String,
    ai_system_prompt: String,
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
    SetBranchFilter,
    SetCommitBody,
    SetCommitSummary,
    SetRepoFilter,
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
    ConfirmResetToCommit,
    ConfirmStashAndSwitch,
    ShowRestoreStash,
    RestoreStash,
    SaveGitSettings,
    SaveAiSettings,
    ChangeAiProvider(AiProvider),
    GenerateAiCommit,
    UndoLastCommit,
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
                self.perform_stash_action(true, cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::StashPop => {
                self.perform_stash_action(false, cx);
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::ShowSettings { show } => {
                self.nav.show_settings = show;
                cx.notify();
                AutomationResponse::success(self.automation_snapshot())
            }
            AutomationCommand::ShowRepoSelector { show } => {
                self.nav.show_repo_selector = show;
                if show {
                    self.nav.show_branch_selector = false;
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
            AutomationCommand::NetworkAction { action } => {
                self.handle_toolbar_action(ToolbarAction::RunNetworkAction(action.into()), cx);
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
                }),
            test_tree: self.automation_test_tree(),
            sidebar_tab: self.nav.sidebar_tab.into(),
            selected_change: self.selection.selected_change.clone(),
            selected_commit: self.selection.selected_commit.clone(),
            selected_commit_file: self.selection.selected_commit_file.clone(),
            show_settings: self.nav.show_settings,
            show_repo_selector: self.nav.show_repo_selector,
            show_branch_selector: self.nav.show_branch_selector,
            show_network_dropdown: self.nav.show_network_dropdown,
            active_dialog: active_dialog_name(&self.nav.active_dialog).to_string(),
            network_action: self.network.active_action.map(network_action_name),
            ai_in_flight: self.commit.ai_in_flight,
            commit_summary: self.commit.summary.clone(),
            commit_body: self.commit.body.clone(),
            repo_filter_text: self.filters.repo_filter_text.clone(),
            branch_filter_text: self.filters.branch_filter_text.clone(),
            status_message: self.messages.status_message.clone(),
            error_message: self.messages.error_message.clone(),
            settings_section: self.nav.settings_section.into(),
            git_user_name: self.repo.global_identity.user_name.clone(),
            git_user_email: self.repo.global_identity.user_email.clone(),
            git_default_branch: self.repo.global_identity.default_branch.clone(),
            git_pull_rebase: self.repo.identity.pull_rebase,
            ai_provider: ai_provider_name(&self.settings.ai.provider).to_string(),
            ai_model: self.settings.ai.model.clone(),
            ai_endpoint: self.settings.ai.endpoint.clone(),
            ai_system_prompt: self.settings.ai.system_prompt.clone(),
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
            ),
            automation_node(
                "commit-body",
                AutomationRole::Textbox,
                Some("input-commit-body"),
                Some(self.commit.body.as_str()),
                Some(AutomationNodeAction::SetCommitBody),
            ),
            automation_node(
                "commit-all",
                AutomationRole::Button,
                Some("button-commit-all"),
                Some("Commit"),
                Some(AutomationNodeAction::CommitAll),
            )
            .enabled(self.can_commit()),
            automation_node(
                "undo-last-commit",
                AutomationRole::Button,
                Some("button-undo-last-commit"),
                Some("Undo"),
                Some(AutomationNodeAction::UndoLastCommit),
            )
            .visible(self.nav.undo_commit.is_some()),
            automation_node(
                "settings-toggle",
                AutomationRole::Button,
                Some("button-settings"),
                Some("Settings"),
                Some(AutomationNodeAction::ShowSettings(!self.nav.show_settings)),
            ),
            automation_node(
                "generate-ai-commit",
                AutomationRole::Button,
                Some("button-generate-ai-commit"),
                Some("Generate AI commit"),
                Some(AutomationNodeAction::GenerateAiCommit),
            )
            .enabled(!self.commit.ai_in_flight),
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
            ),
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
                "status-message",
                AutomationRole::Status,
                Some("status-message"),
                Some(self.messages.status_message.as_str()),
                None,
            ),
        ];

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

        if self.nav.show_branch_selector {
            children.push(automation_node(
                "branch-new",
                AutomationRole::Button,
                Some("button-branch-new"),
                Some("New Branch"),
                Some(AutomationNodeAction::StartCreateBranch),
            ));
        }

        if matches!(self.nav.active_dialog, ActiveDialog::CreateBranch) {
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
                ),
            ]);
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
                    "restore-stash-cancel",
                    AutomationRole::Button,
                    Some("restore-stash-cancel"),
                    Some("Cancel"),
                    Some(AutomationNodeAction::CancelDialog),
                ),
                automation_node(
                    "restore-stash-confirm",
                    AutomationRole::Button,
                    Some("restore-stash-confirm"),
                    Some("Restore Stash"),
                    Some(AutomationNodeAction::RestoreStash),
                ),
            ]);
        }

        if matches!(self.nav.active_dialog, ActiveDialog::RenameBranch { .. }) {
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
                ),
            ]);
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
                ),
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
                children.extend(change_action_nodes(change.path.as_str(), has_github_remote));
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
                    snapshot
                        .history
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

            for commit in &snapshot.history {
                children.extend(history_action_nodes(
                    commit.short_oid.as_str(),
                    commit.oid.as_str(),
                    &commit.tags,
                    self.can_reset_to_commit(&commit.oid),
                    has_github_remote,
                ));
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

        children.extend([
            automation_node(
                "network-fetch",
                AutomationRole::Button,
                Some("button-network-fetch"),
                Some("Fetch"),
                Some(AutomationNodeAction::Network(NetworkAction::Fetch)),
            ),
            automation_node(
                "network-pull",
                AutomationRole::Button,
                Some("button-network-pull"),
                Some("Pull"),
                Some(AutomationNodeAction::Network(NetworkAction::Pull)),
            ),
            automation_node(
                "network-push",
                AutomationRole::Button,
                Some("button-network-push"),
                Some("Push"),
                Some(AutomationNodeAction::Network(NetworkAction::Push)),
            ),
        ]);

        automation_node(
            "gitspark-root",
            AutomationRole::App,
            Some("gitspark-root"),
            Some("GitSpark"),
            None,
        )
        .children(children)
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
            AutomationNodeAction::SelectTab(tab) => {
                self.nav.sidebar_tab = tab;
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
            AutomationNodeAction::SetRepoFilter => {
                let Some(text) = fill_text else {
                    return AutomationResponse::failure("fill text is required");
                };
                self.filters.repo_filter_text = text;
                cx.notify();
            }
            AutomationNodeAction::SetSettingsField(field) => {
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
                    self.repo.pending_cherry_pick_oid = None;
                    self.nav.show_network_dropdown = false;
                }
                cx.notify();
            }
            AutomationNodeAction::ShowSettings(show) => {
                self.nav.show_settings = show;
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
            AutomationNodeAction::ShowRestoreStash => {
                self.show_restore_stash_dialog(cx);
            }
            AutomationNodeAction::RestoreStash => {
                self.nav.active_dialog = ActiveDialog::None;
                self.restore_stash(cx);
            }
            AutomationNodeAction::SaveGitSettings => {
                self.handle_settings_action(SettingsAction::SaveGitConfig, cx);
            }
            AutomationNodeAction::SaveAiSettings => {
                self.handle_settings_action(SettingsAction::SaveAiSettings, cx);
            }
            AutomationNodeAction::ChangeAiProvider(provider) => {
                self.handle_settings_action(SettingsAction::ChangeProvider(provider), cx);
            }
            AutomationNodeAction::GenerateAiCommit => {
                self.handle_sidebar_action(SidebarAction::GenerateAiCommit, cx);
            }
            AutomationNodeAction::UndoLastCommit => {
                self.undo_last_commit(cx);
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
        self.nav.settings_section = section;
        let field = crate::ui::settings_modal::default_settings_field(section);
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
            SettingsField::GitUserName => {
                self.repo.global_identity.user_name = text;
                self.settings_modal.git_user_name_selection = None;
            }
            SettingsField::GitUserEmail => {
                self.repo.global_identity.user_email = text;
                self.settings_modal.git_user_email_selection = None;
            }
            SettingsField::GitDefaultBranch => {
                self.repo.global_identity.default_branch = if text.trim().is_empty() {
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

    fn perform_stash_action(&mut self, stash_all: bool, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            cx.notify();
            return;
        };

        self.messages.status_message = if stash_all {
            "Stashing changes...".to_string()
        } else {
            "Restoring stash...".to_string()
        };
        self.messages.error_message.clear();

        let tx = self.event_tx.clone();
        thread::spawn(move || {
            let git = GitClient::new();
            let res = if stash_all {
                git.stash_all(&path)
            } else {
                git.stash_pop(&path)
            }
            .map_err(|e| e.to_string());
            let label = if stash_all {
                "Stashed changes"
            } else {
                "Restored stash"
            };
            let _ = tx.send(AppEvent::NetworkActionCompleted(res, label.to_string()));
        });

        cx.notify();
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

fn settings_automation_nodes(app: &GitSparkApp) -> Vec<AutomationNode> {
    let sections = [
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
    ];

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
        SettingsSection::Git => {
            nodes.extend([
                settings_field_node(
                    "settings-git-user-name",
                    "User Name",
                    SettingsField::GitUserName,
                    app.repo.global_identity.user_name.as_str(),
                ),
                settings_field_node(
                    "settings-git-user-email",
                    "User Email",
                    SettingsField::GitUserEmail,
                    app.repo.global_identity.user_email.as_str(),
                ),
                settings_field_node(
                    "settings-git-default-branch",
                    "Default Branch",
                    SettingsField::GitDefaultBranch,
                    app.repo
                        .global_identity
                        .default_branch
                        .as_deref()
                        .unwrap_or(""),
                ),
                automation_node(
                    "settings-save-git",
                    AutomationRole::Button,
                    Some("settings-save-git"),
                    Some("Save Git Config"),
                    Some(AutomationNodeAction::SaveGitSettings),
                )
                .enabled(app.repo.snapshot.is_some()),
            ]);
        }
        SettingsSection::Ai => {
            nodes.extend([
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
                settings_field_node(
                    "settings-ai-model",
                    "Model",
                    SettingsField::AiModel,
                    app.settings.ai.model.as_str(),
                ),
                if app.settings.ai.provider == AiProvider::OpenRouter {
                    automation_node(
                        "settings-ai-endpoint",
                        AutomationRole::Status,
                        Some("settings-ai-endpoint"),
                        Some(app.settings.ai.endpoint.as_str()),
                        None,
                    )
                } else {
                    settings_field_node(
                        "settings-ai-endpoint",
                        "Endpoint",
                        SettingsField::AiEndpoint,
                        app.settings.ai.endpoint.as_str(),
                    )
                },
                settings_field_node(
                    "settings-ai-api-key",
                    "API Key",
                    SettingsField::AiApiKey,
                    app.settings.ai.api_key.as_str(),
                ),
                settings_field_node(
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
        SettingsSection::Appearance | SettingsSection::Integrations => {}
    }

    nodes
}

fn settings_field_node(
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
    .children(vec![automation_node(
        format!("{test_id}-label"),
        AutomationRole::Status,
        None::<String>,
        Some(label),
        None,
    )])
}

fn change_action_nodes(path: &str, has_github_remote: bool) -> Vec<AutomationNode> {
    let slug = stable_test_slug(path);
    let extension = std::path::Path::new(path)
        .extension()
        .map(|ext| ext.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut actions = vec![
        (
            "discard",
            crate::ui::labels::discard_changes_menu().to_string(),
            AutomationChangeAction::Discard,
        ),
        (
            "prompt-discard",
            "Prompt discard change".to_string(),
            AutomationChangeAction::PromptDiscard,
        ),
        (
            "ignore-path",
            crate::ui::labels::ignore_file_menu().to_string(),
            AutomationChangeAction::IgnorePath,
        ),
        (
            "ignore-extension",
            if extension.is_empty() {
                "Ignore extension".to_string()
            } else {
                crate::ui::labels::ignore_all_extension_menu(&extension)
            },
            AutomationChangeAction::IgnoreExtension,
        ),
        (
            "copy-full-path",
            crate::ui::labels::copy_file_path_menu().to_string(),
            AutomationChangeAction::CopyFullPath,
        ),
        (
            "copy-relative-path",
            crate::ui::labels::copy_relative_file_path_menu().to_string(),
            AutomationChangeAction::CopyRelativePath,
        ),
        (
            "reveal-in-finder",
            crate::ui::labels::reveal_in_file_manager_menu().to_string(),
            AutomationChangeAction::RevealInFinder,
        ),
        (
            "open-in-editor",
            crate::ui::labels::open_in_external_editor_menu().to_string(),
            AutomationChangeAction::OpenInEditor,
        ),
        (
            "open-with-default",
            crate::ui::labels::open_with_default_program_menu().to_string(),
            AutomationChangeAction::OpenWithDefault,
        ),
    ];

    if has_github_remote {
        actions.push((
            "view-on-github",
            "View on GitHub".to_string(),
            AutomationChangeAction::ViewOnGithub,
        ));
    }

    actions
        .into_iter()
        .map(|(suffix, label, action)| {
            automation_node(
                format!("change-{slug}-{suffix}"),
                AutomationRole::Button,
                Some(format!("change-{slug}-{suffix}")),
                Some(label.as_str()),
                Some(AutomationNodeAction::ChangeFile(path.to_string(), action)),
            )
        })
        .collect()
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
            crate::ui::labels::reset_to_commit_menu(),
            AutomationHistoryAction::ResetToCommit,
        ),
        (
            "checkout",
            crate::ui::labels::checkout_commit_menu(),
            AutomationHistoryAction::CheckoutCommit,
        ),
        (
            "revert",
            crate::ui::labels::revert_changes_in_commit_menu(),
            AutomationHistoryAction::RevertChangesInCommit,
        ),
        (
            "cherry-pick",
            crate::ui::labels::cherry_pick_commit_menu(),
            AutomationHistoryAction::CherryPickCommit,
        ),
        (
            "create-branch",
            crate::ui::labels::create_branch_from_commit_menu(),
            AutomationHistoryAction::CreateBranchFromCommit,
        ),
        (
            "create-tag",
            crate::ui::labels::create_tag_menu(),
            AutomationHistoryAction::CreateTag,
        ),
        ("copy-sha", "Copy SHA", AutomationHistoryAction::CopySha),
        ("copy-diff", "Copy diff", AutomationHistoryAction::CopyDiff),
    ];

    let copy_tag_label = if tags.len() > 1 {
        "Copy Tags"
    } else {
        "Copy Tag"
    };
    actions.push(("copy-tag", copy_tag_label, AutomationHistoryAction::CopyTag));

    if has_github_remote {
        actions.push((
            "view-on-github",
            "View on GitHub",
            AutomationHistoryAction::ViewOnGithub,
        ));
    }

    actions
        .into_iter()
        .map(|(suffix, label, action)| {
            let enabled = match action {
                AutomationHistoryAction::CopyTag => !tags.is_empty(),
                AutomationHistoryAction::ResetToCommit => can_reset_to_commit,
                _ => true,
            };
            automation_node(
                format!("commit-{slug}-{suffix}"),
                AutomationRole::Button,
                Some(format!("commit-{slug}-{suffix}")),
                Some(label),
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
    let mut slug = String::new();
    let mut previous_dash = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash && !slug.is_empty() {
            slug.push('-');
            previous_dash = true;
        }
    }

    while slug.ends_with('-') {
        slug.pop();
    }

    if slug.is_empty() {
        "item".to_string()
    } else {
        slug
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
        ActiveDialog::RenameBranch { .. } => "rename_branch",
        ActiveDialog::DeleteBranch { .. } => "delete_branch",
        ActiveDialog::CreateTag { .. } => "create_tag",
        ActiveDialog::ResetToCommit { .. } => "reset_to_commit",
        ActiveDialog::RestoreStash => "restore_stash",
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
    fn parses_stash_commands() {
        let stash_all: AutomationCommand =
            serde_json::from_str(r#"{"command":"stash_all"}"#).expect("stash all command parses");
        assert!(matches!(stash_all, AutomationCommand::StashAll));

        let stash_pop: AutomationCommand =
            serde_json::from_str(r#"{"command":"stash_pop"}"#).expect("stash pop command parses");
        assert!(matches!(stash_pop, AutomationCommand::StashPop));
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
