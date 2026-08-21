use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RepoSummary {
    pub path: PathBuf,
    pub name: String,
    pub current_branch: String,
    pub head_oid: Option<String>,
    pub remote_name: Option<String>,
    #[serde(default)]
    pub has_github_remote: bool,
    pub ahead: usize,
    pub behind: usize,
    pub last_fetched: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChangeEntry {
    pub path: String,
    pub status: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DiffEntry {
    pub path: String,
    pub diff: String,
    pub is_binary: bool,
    #[serde(default)]
    pub is_image: bool,
    #[serde(default)]
    pub is_submodule: bool,
    #[serde(default)]
    pub submodule_old_oid: Option<String>,
    #[serde(default)]
    pub submodule_new_oid: Option<String>,
    /// The original diff text before any expansion (for collapse).
    #[serde(skip)]
    pub original_diff: Option<String>,
    /// The file contents (new/working tree version) for in-memory expansion.
    #[serde(skip)]
    pub file_contents: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
    pub is_remote: bool,
    pub updated: Option<String>,
}

/// One entry from `git worktree list`.
///
/// A worktree is a second checkout of the same repository in its own
/// directory, so switching to one is closer to opening a different folder
/// than to checking out a branch — see `GitClient::list_worktrees`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorktreeInfo {
    /// Absolute path to the working directory.
    pub path: PathBuf,
    /// Directory name, used as the display label.
    pub name: String,
    /// The branch checked out here, or `None` when detached.
    pub branch: Option<String>,
    /// The primary worktree — the one holding `.git` as a directory. There is
    /// exactly one, it cannot be removed, and it sorts first.
    pub is_main: bool,
    /// The worktree the app currently has open.
    pub is_current: bool,
    /// `git worktree lock` was used. Locked trees stay listed but cannot be
    /// pruned or removed without `--force`.
    pub is_locked: bool,
    /// The checkout is detached rather than on a branch.
    pub is_detached: bool,
}

#[derive(Clone, Debug, Default)]
pub struct BranchComparison {
    pub current_branch: String,
    pub target_branch: String,
    pub ahead: usize,
    pub behind: usize,
    pub commits: Vec<CommitInfo>,
    pub diffs: Vec<DiffEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitOperationKind {
    Merge,
    Rebase,
}

impl GitOperationKind {
    pub fn title(&self) -> &'static str {
        match self {
            Self::Merge => "Merge in progress",
            Self::Rebase => "Rebase in progress",
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Rebase => "rebase",
        }
    }
}

#[derive(Clone, Debug)]
pub struct GitOperationState {
    pub kind: GitOperationKind,
    pub current_branch: String,
    pub target_branch: Option<String>,
    pub conflicted_files: Vec<ChangeEntry>,
    pub can_continue: bool,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
pub struct CreateRepositoryOptions {
    pub name: String,
    pub description: String,
    pub branch_name: String,
    pub initialize_readme: bool,
    pub gitignore_template: String,
    pub license_template: String,
    pub initial_commit: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GitIdentity {
    pub user_name: String,
    pub user_email: String,
    pub pull_rebase: Option<bool>,
    pub default_branch: Option<String>,
}

pub const INVALID_GIT_AUTHOR_NAME_MESSAGE: &str =
    "Name is invalid, it consists only of disallowed characters.";

pub fn git_author_name_is_valid(name: &str) -> bool {
    name.is_empty()
        || !name.chars().all(|ch| {
            ch <= '\u{20}' || matches!(ch, '.' | ',' | ':' | ';' | '<' | '>' | '"' | '\\' | '\'')
        })
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CommitInfo {
    pub oid: String,
    pub short_oid: String,
    pub summary: String,
    pub body: String,
    pub author_name: String,
    pub author_email: String,
    pub date: String,
    pub is_head: bool,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct RepoSnapshot {
    /// Worktrees for this repository, listed on the worker thread with the
    /// rest of the snapshot. Listing them on demand meant a blocking
    /// `git worktree list` on the UI thread once per refresh.
    pub worktrees: Vec<WorktreeInfo>,
    pub repo: RepoSummary,
    pub changes: Vec<ChangeEntry>,
    pub diffs: Vec<DiffEntry>,
    pub branches: Vec<BranchInfo>,
    pub history: Vec<CommitInfo>,
    pub tags: Vec<String>,
    #[allow(dead_code)]
    pub stash_count: usize,
}

#[cfg(test)]
mod tests {
    use super::{AiProvider, endpoint_for_provider_change, git_author_name_is_valid};

    #[test]
    fn switching_away_from_openrouter_stops_pointing_at_openrouter() {
        // The reported bug: pick OpenRouter, then switch to OpenAI-compatible,
        // and requests kept going to OpenRouter's URL with an OpenAI key.
        let endpoint = endpoint_for_provider_change(
            AiProvider::OpenRouter.default_endpoint(),
            &AiProvider::OpenAICompatible,
        );
        assert_eq!(
            endpoint.as_deref(),
            Some(AiProvider::OpenAICompatible.default_endpoint())
        );
    }

    #[test]
    fn switching_to_openrouter_uses_its_endpoint() {
        let endpoint = endpoint_for_provider_change(
            AiProvider::OpenAICompatible.default_endpoint(),
            &AiProvider::OpenRouter,
        );
        assert_eq!(
            endpoint.as_deref(),
            Some(AiProvider::OpenRouter.default_endpoint())
        );
    }

    #[test]
    fn an_empty_endpoint_is_filled_in() {
        assert_eq!(
            endpoint_for_provider_change("   ", &AiProvider::OpenAICompatible).as_deref(),
            Some(AiProvider::OpenAICompatible.default_endpoint())
        );
    }

    #[test]
    fn a_custom_endpoint_is_preserved() {
        // Someone running a local llama.cpp or vLLM must not have their
        // endpoint reset because they toggled the provider.
        assert_eq!(
            endpoint_for_provider_change("http://localhost:8080/v1/chat/completions",
                &AiProvider::OpenAICompatible),
            None
        );
        assert_eq!(
            endpoint_for_provider_change("https://my-proxy.internal/v1/chat/completions",
                &AiProvider::OpenRouter),
            None
        );
    }

    #[test]
    fn a_default_endpoint_is_recognized_regardless_of_case_or_padding() {
        let padded = format!("  {}  ", AiProvider::OpenRouter.default_endpoint().to_uppercase());
        assert!(
            endpoint_for_provider_change(&padded, &AiProvider::OpenAICompatible).is_some(),
            "a default typed with different case should still be replaced"
        );
    }

    #[test]
    fn validates_git_author_name_like_git_ident() {
        for value in [".", ",", ":", ";", "<", ">", "\"", "\\", "'", " ", ".;:<>"] {
            assert!(!git_author_name_is_valid(value));
        }

        for codepoint in 0..=32 {
            let value = char::from_u32(codepoint).unwrap().to_string();
            assert!(!git_author_name_is_valid(&value));
        }

        assert!(git_author_name_is_valid(""));
        assert!(git_author_name_is_valid("this is great"));
        assert!(git_author_name_is_valid(";hi. there;\u{1f}"));
        assert!(git_author_name_is_valid("!"));
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AiSettings {
    pub provider: AiProvider,
    pub endpoint: String,
    pub model: String,
    pub api_key: String,
    pub system_prompt: String,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            provider: AiProvider::OpenAICompatible,
            endpoint: AiProvider::OpenAICompatible.default_endpoint().to_string(),
            model: "gpt-4.1-mini".to_string(),
            api_key: String::new(),
            system_prompt: "Write a concise conventional commit style message for the provided git diff. Return JSON with fields subject and body. Do not use markdown bold markers like ** and do not use fenced code blocks like ``` in either field. Plain sentences and normal '-' or '.' list items in the body are allowed.".to_string(),
        }
    }
}

/// The endpoint to use after switching provider, or `None` to keep the
/// current one.
///
/// Switching used to replace the endpoint only when it was empty, so going
/// OpenRouter -> OpenAI-compatible left the endpoint pointing at OpenRouter.
/// Requests then went to OpenRouter's URL carrying an OpenAI key, which fails
/// authentication — the whole provider looked broken.
///
/// A default belongs to the provider that supplied it, so any value matching
/// a known default is replaced. Anything else is a URL the user typed, and is
/// kept: someone running a local llama.cpp or vLLM does not want their
/// endpoint silently reset because they toggled the provider.
pub fn endpoint_for_provider_change(current: &str, next: &AiProvider) -> Option<String> {
    let current = current.trim();
    let is_a_default = AiProvider::all_default_endpoints()
        .iter()
        .any(|known| known.eq_ignore_ascii_case(current));

    (current.is_empty() || is_a_default).then(|| next.default_endpoint().to_string())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AiProvider {
    OpenRouter,
    OpenAICompatible,
}

impl AiProvider {
    #[allow(dead_code)]
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::OpenRouter => "OpenRouter",
            Self::OpenAICompatible => "OpenAI Compatible",
        }
    }

    pub fn default_endpoint(&self) -> &'static str {
        match self {
            Self::OpenRouter => "https://openrouter.ai/api/v1/chat/completions",
            Self::OpenAICompatible => "https://api.openai.com/v1/chat/completions",
        }
    }

    /// Every provider's default endpoint.
    ///
    /// Used to tell "the default we filled in" apart from "a URL the user
    /// typed", which is the distinction [`endpoint_for_provider_change`]
    /// needs.
    pub fn all_default_endpoints() -> [&'static str; 2] {
        [
            Self::OpenRouter.default_endpoint(),
            Self::OpenAICompatible.default_endpoint(),
        ]
    }

    pub fn api_key_hint(&self) -> &'static str {
        match self {
            Self::OpenRouter => "sk-or-v1-...",
            Self::OpenAICompatible => "sk-...",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct CommitSuggestion {
    pub subject: String,
    pub body: String,
    #[allow(dead_code)]
    pub raw: String,
}

#[derive(Clone, Debug, Default)]
pub struct RemoteModelOption {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WindowSize {
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    /// Whether x/y have been explicitly saved (0.0/0.0 is a valid position).
    #[serde(default)]
    pub has_position: bool,
    /// The display ID the window was on (for multi-monitor restore).
    #[serde(default)]
    pub display_id: Option<u32>,
}

impl Default for WindowSize {
    fn default() -> Self {
        Self {
            width: 0.0, // 0 = use 60% of screen
            height: 0.0,
            x: 0.0,
            y: 0.0,
            has_position: false,
            display_id: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSettings {
    pub recent_repos: Vec<PathBuf>,
    pub ai: AiSettings,
    #[serde(default)]
    pub window_size: WindowSize,
    /// Default branch name for new repos (persisted locally).
    #[serde(default)]
    pub default_branch: Option<String>,
    /// Appearance preference: "system", "light" or "dark". Resolved against
    /// the OS at startup by `ui::theme::resolve`.
    #[serde(default)]
    pub appearance: Option<String>,
    /// Repositories open as tabs, in strip order.
    ///
    /// Distinct from `recent_repos`, which is a history of everything ever
    /// opened; this is what was on screen when the app last closed, and the
    /// point of tabs is not having to reopen it.
    #[serde(default)]
    pub open_repos: Vec<PathBuf>,
    /// Which of `open_repos` was in front.
    #[serde(default)]
    pub active_repo: Option<usize>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            recent_repos: Vec::new(),
            ai: AiSettings::default(),
            window_size: WindowSize::default(),
            default_branch: None,
            appearance: None,
            open_repos: Vec::new(),
            active_repo: None,
        }
    }
}
