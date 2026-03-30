use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RepoSummary {
    pub path: PathBuf,
    pub name: String,
    pub current_branch: String,
    pub head_oid: Option<String>,
    pub remote_name: Option<String>,
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
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
    pub is_remote: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GitIdentity {
    pub user_name: String,
    pub user_email: String,
    pub pull_rebase: Option<bool>,
    pub default_branch: Option<String>,
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
    pub repo: RepoSummary,
    pub changes: Vec<ChangeEntry>,
    pub diffs: Vec<DiffEntry>,
    pub branches: Vec<BranchInfo>,
    pub history: Vec<CommitInfo>,
    pub stash_count: usize,
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AiProvider {
    OpenRouter,
    OpenAICompatible,
}

impl AiProvider {
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
}

impl Default for WindowSize {
    fn default() -> Self {
        Self {
            width: 0.0, // 0 = use 60% of screen
            height: 0.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSettings {
    pub recent_repos: Vec<PathBuf>,
    pub ai: AiSettings,
    #[serde(default)]
    pub window_size: WindowSize,
    #[serde(default)]
    pub update_channel: UpdateChannel,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            recent_repos: Vec::new(),
            ai: AiSettings::default(),
            window_size: WindowSize::default(),
            update_channel: UpdateChannel::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Auto-update types
// ---------------------------------------------------------------------------

/// Which update channel the user is subscribed to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateChannel {
    Release,
    Beta,
}

impl Default for UpdateChannel {
    fn default() -> Self {
        Self::Release
    }
}

impl UpdateChannel {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Release => "Release",
            Self::Beta => "Beta",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::Beta => "beta",
        }
    }

    /// Infer channel from a version tag string.
    /// Tags containing a pre-release suffix (e.g. `-beta.1`) map to `Beta`;
    /// everything else maps to `Release`.
    pub fn from_tag(tag: &str) -> Self {
        let version = tag.strip_prefix('v').unwrap_or(tag);
        if version.contains('-') {
            Self::Beta
        } else {
            Self::Release
        }
    }
}

/// Per-platform asset entry inside an update manifest.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateAsset {
    pub url: String,
    pub sha256: String,
}

/// The JSON manifest served from GitHub Pages at
/// `updates/{channel}/latest.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateManifest {
    pub channel: String,
    pub version: String,
    pub pub_date: String,
    pub notes_url: String,
    pub assets: HashMap<String, UpdateAsset>,
}

/// Current state of the update check lifecycle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateStatus {
    /// No check has been performed yet.
    Idle,
    /// Actively checking for a new version.
    Checking,
    /// A newer version is available.
    Available(String),
    /// The update payload is being downloaded.
    Downloading(String),
    /// The update has been downloaded and is ready to install on restart.
    ReadyToRestart(String),
    /// The check completed and the app is already up to date.
    UpToDate,
    /// The check or download failed.
    Error(String),
}
