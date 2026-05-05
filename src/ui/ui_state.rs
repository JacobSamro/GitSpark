use crate::models::RemoteModelOption;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SidebarTab {
    Changes,
    History,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    Git,
    Ai,
    Appearance,
    Integrations,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MainTab {
    Workspace,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BranchSelectorMode {
    Switch,
    Merge,
}

#[derive(Clone)]
pub enum OpenRouterModelsState {
    Idle,
    Loading,
    Ready(Vec<RemoteModelOption>),
    Error(String),
}

/// Which dialog is currently showing (at most one).
#[derive(Clone, PartialEq)]
pub enum ActiveDialog {
    None,
    CreateBranch,
    DiscardChanges {
        paths: Vec<String>,
    },
    #[allow(dead_code)]
    StashAndSwitch {
        target_branch: String,
    },
    RenameBranch {
        old_name: String,
    },
    DeleteBranch {
        branch_name: String,
    },
    CreateTag {
        target_oid: String,
    },
    ResetToCommit {
        target_oid: String,
    },
    StashChanges,
    RestoreStash,
    DiscardStash,
    PublishRepository,
}

impl Default for ActiveDialog {
    fn default() -> Self {
        Self::None
    }
}

pub struct NavState {
    #[allow(dead_code)]
    pub main_tab: MainTab,
    pub sidebar_tab: SidebarTab,
    pub show_settings: bool,
    pub settings_scope: SettingsScope,
    pub show_repo_selector: bool,
    pub show_branch_selector: bool,
    pub show_network_dropdown: bool,
    pub change_context_menu: Option<ChangeContextMenuState>,
    pub settings_section: SettingsSection,
    pub branch_selector_mode: BranchSelectorMode,
    pub active_dialog: ActiveDialog,
    /// Undo commit: Some((summary, timestamp)) after a successful commit
    pub undo_commit: Option<(String, std::time::Instant)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChangeContextMenuState {
    pub path: String,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsScope {
    Global,
    Repository,
}

impl Default for NavState {
    fn default() -> Self {
        Self {
            main_tab: MainTab::Workspace,
            sidebar_tab: SidebarTab::Changes,
            show_settings: false,
            settings_scope: SettingsScope::Global,
            show_repo_selector: false,
            show_branch_selector: false,
            show_network_dropdown: false,
            change_context_menu: None,
            settings_section: SettingsSection::Git,
            branch_selector_mode: BranchSelectorMode::Switch,
            active_dialog: ActiveDialog::None,
            undo_commit: None,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct ChangeFilterOptions {
    #[allow(dead_code)]
    pub included_in_commit: bool,
    #[allow(dead_code)]
    pub excluded_from_commit: bool,
    #[allow(dead_code)]
    pub new_files: bool,
    #[allow(dead_code)]
    pub modified_files: bool,
    #[allow(dead_code)]
    pub deleted_files: bool,
}

impl ChangeFilterOptions {
    #[allow(dead_code)]
    pub fn active_count(self) -> usize {
        [
            self.included_in_commit,
            self.excluded_from_commit,
            self.new_files,
            self.modified_files,
            self.deleted_files,
        ]
        .into_iter()
        .filter(|active| *active)
        .count()
    }
}

pub struct FilterState {
    #[allow(dead_code)]
    pub filter_text: String,
    #[allow(dead_code)]
    pub change_filters: ChangeFilterOptions,
    pub repo_filter_text: String,
    pub branch_filter_text: String,
    pub openrouter_model_filter: String,
    pub openrouter_models: OpenRouterModelsState,
}

impl Default for FilterState {
    fn default() -> Self {
        Self {
            filter_text: String::new(),
            change_filters: ChangeFilterOptions::default(),
            repo_filter_text: String::new(),
            branch_filter_text: String::new(),
            openrouter_model_filter: String::new(),
            openrouter_models: OpenRouterModelsState::Idle,
        }
    }
}

pub struct MessageState {
    pub status_message: String,
    pub error_message: String,
}

impl MessageState {
    pub fn new(status: &str, error: String) -> Self {
        Self {
            status_message: status.to_string(),
            error_message: error,
        }
    }
}

impl Default for MessageState {
    fn default() -> Self {
        Self {
            status_message: "Open a repository to get started.".to_string(),
            error_message: String::new(),
        }
    }
}
