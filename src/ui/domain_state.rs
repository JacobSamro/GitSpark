use std::collections::HashSet;

use crate::ui::diff_line_selection::DiffLineSelection;

use crate::models::{
    BranchComparison, ChangeEntry, CommitSuggestion, DiffEntry, GitIdentity, GitOperationState,
    RepoSnapshot, WorktreeInfo,
};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum NetworkAction {
    Fetch,
    Pull,
    Push,
    #[allow(dead_code)]
    PublishBranch,
    PublishRepository,
}

impl NetworkAction {
    pub fn from_snapshot(snapshot: &RepoSnapshot) -> Self {
        if snapshot.repo.remote_name.is_none() {
            return Self::PublishRepository;
        }
        if snapshot.repo.behind > 0 {
            Self::Pull
        } else if snapshot.repo.ahead > 0 {
            Self::Push
        } else {
            Self::Fetch
        }
    }

    pub fn title(self, remote_name: &str) -> String {
        match self {
            Self::Fetch => format!("Fetch {remote_name}"),
            Self::Pull => format!("Pull {remote_name}"),
            Self::Push => format!("Push {remote_name}"),
            Self::PublishBranch => "Publish branch".to_string(),
            Self::PublishRepository => "Publish repository".to_string(),
        }
    }

    pub fn pending_title(self, remote_name: &str) -> String {
        match self {
            Self::Fetch => format!("Fetching {remote_name}\u{2026}"),
            Self::Pull => format!("Pulling {remote_name}\u{2026}"),
            Self::Push => format!("Pushing {remote_name}\u{2026}"),
            Self::PublishBranch => "Publishing branch\u{2026}".to_string(),
            Self::PublishRepository => "Publishing repository\u{2026}".to_string(),
        }
    }

    #[allow(dead_code)]
    pub fn is_available(self) -> bool {
        !matches!(self, Self::PublishRepository)
    }
}

pub struct RepoState {
    pub snapshot: Option<RepoSnapshot>,
    /// Worktrees for the open repository.
    ///
    /// Loaded lazily when the picker opens rather than on every refresh: it
    /// is a separate `git worktree list` shell-out, and the toolbar can name
    /// the current worktree from the repo path alone.
    pub worktrees: Vec<WorktreeInfo>,
    pub identity: GitIdentity,
    pub local_identity: GitIdentity,
    pub global_identity: GitIdentity,
    pub use_local_identity: bool,
    pub remote_name: Option<String>,
    pub remote_url: String,
    pub ignored_files_text: String,
    pub branch_target: String,
    pub merge_target: String,
    pub new_branch_name: String,
    pub new_branch_start_point: Option<String>,
    pub create_repo_name: String,
    pub create_repo_description: String,
    pub create_repo_path: String,
    pub create_repo_branch_name: String,
    pub create_repo_initialize_readme: bool,
    pub create_repo_gitignore_template: String,
    pub create_repo_license_template: String,
    pub create_repo_initial_commit: bool,
    pub clone_repo_url: String,
    pub clone_repo_path: String,
    pub clone_repo_name: String,
    pub pending_cherry_pick_oid: Option<String>,
    pub switch_branch_bring_changes: bool,
    pub has_stash: bool,
    pub stash_files: Vec<ChangeEntry>,
    pub comparison: Option<BranchComparison>,
    pub operation: Option<GitOperationState>,
}

impl Default for RepoState {
    fn default() -> Self {
        Self {
            snapshot: None,
            worktrees: Vec::new(),
            identity: GitIdentity::default(),
            local_identity: GitIdentity::default(),
            global_identity: GitIdentity::default(),
            use_local_identity: false,
            remote_name: None,
            remote_url: String::new(),
            ignored_files_text: String::new(),
            branch_target: String::new(),
            merge_target: String::new(),
            new_branch_name: String::new(),
            new_branch_start_point: None,
            create_repo_name: String::new(),
            create_repo_description: String::new(),
            create_repo_path: String::new(),
            create_repo_branch_name: "main".to_string(),
            create_repo_initialize_readme: true,
            create_repo_gitignore_template: String::new(),
            create_repo_license_template: String::new(),
            create_repo_initial_commit: true,
            clone_repo_url: String::new(),
            clone_repo_path: String::new(),
            clone_repo_name: String::new(),
            pending_cherry_pick_oid: None,
            switch_branch_bring_changes: false,
            has_stash: false,
            stash_files: Vec::new(),
            comparison: None,
            operation: None,
        }
    }
}

pub struct CommitState {
    pub summary: String,
    pub body: String,
    pub ai_preview: Option<CommitSuggestion>,
    pub ai_in_flight: bool,
    /// Files included in next commit (paths). Empty = all included.
    pub included_files: HashSet<String>,
    /// Whether all files are included (tri-state: true=all, false=none, depends on included_files)
    pub include_all: bool,
}

impl Default for CommitState {
    fn default() -> Self {
        Self {
            summary: String::new(),
            body: String::new(),
            ai_preview: None,
            ai_in_flight: false,
            included_files: HashSet::new(),
            include_all: true,
        }
    }
}

pub struct NetworkState {
    pub active_action: Option<NetworkAction>,
    pub publish_name: String,
    pub publish_description: String,
    pub publish_private: bool,
}

impl Default for NetworkState {
    fn default() -> Self {
        Self {
            active_action: None,
            publish_name: String::new(),
            publish_description: String::new(),
            publish_private: true,
        }
    }
}

pub struct SelectionState {
    pub selected_change: Option<String>,
    pub selected_commit: Option<String>,
    pub selected_commit_file: Option<String>,
    pub commit_diffs: Option<Vec<DiffEntry>>,
    pub selected_diff_lines: HashSet<DiffLineSelection>,
}

impl Default for SelectionState {
    fn default() -> Self {
        Self {
            selected_change: None,
            selected_commit: None,
            selected_commit_file: None,
            commit_diffs: None,
            selected_diff_lines: HashSet::new(),
        }
    }
}
