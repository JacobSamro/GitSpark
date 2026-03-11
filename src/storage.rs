use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::models::AppSettings;

pub fn config_path() -> Result<PathBuf> {
    let base = dirs::config_dir().context("could not determine config dir")?;
    Ok(base.join("github-rusttop").join("settings.toml"))
}

pub fn load_settings() -> Result<AppSettings> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("failed to read settings from {}", path.display()))?;
    let settings: AppSettings = toml::from_str(&content)
        .with_context(|| format!("failed to parse settings from {}", path.display()))?;
    Ok(normalize_settings(settings))
}

pub fn save_settings(settings: &AppSettings) -> Result<()> {
    let path = config_path()?;
    let parent = path
        .parent()
        .context("config path did not have a parent directory")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create config dir {}", parent.display()))?;

    let normalized = normalize_settings(settings.clone());
    let content = toml::to_string_pretty(&normalized).context("failed to serialize settings")?;
    fs::write(&path, content)
        .with_context(|| format!("failed to write settings to {}", path.display()))?;
    Ok(())
}

pub fn push_recent_repo(settings: &mut AppSettings, path: impl Into<PathBuf>) {
    settings
        .recent_repos
        .insert(0, normalize_repo_path(path.into()));
    settings.recent_repos = dedupe_recent_repos(&settings.recent_repos);
}

pub fn dedupe_recent_repos(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut deduped = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for path in paths {
        let normalized = normalize_repo_path(path.clone());
        let key = path_key(&normalized);
        if seen.insert(key) {
            deduped.push(normalized);
        }
    }

    const MAX_RECENT_REPOS: usize = 12;
    deduped.truncate(MAX_RECENT_REPOS);
    deduped
}

fn normalize_settings(mut settings: AppSettings) -> AppSettings {
    settings.recent_repos = dedupe_recent_repos(&settings.recent_repos);
    settings.ai.endpoint = settings.ai.endpoint.trim().to_string();
    if settings.ai.endpoint.is_empty() {
        settings.ai.endpoint = AppSettings::default().ai.endpoint;
    }

    settings.ai.model = settings.ai.model.trim().to_string();
    if settings.ai.model.is_empty() {
        settings.ai.model = AppSettings::default().ai.model;
    }

    settings.ai.system_prompt = settings.ai.system_prompt.trim().to_string();
    if settings.ai.system_prompt.is_empty() {
        settings.ai.system_prompt = AppSettings::default().ai.system_prompt;
    }

    settings.ai.api_key = settings.ai.api_key.trim().to_string();
    settings.window_size.width = settings.window_size.width.clamp(720.0, 3840.0);
    settings.window_size.height = settings.window_size.height.clamp(520.0, 2160.0);
    settings
}

fn normalize_repo_path(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path).unwrap_or(path)
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}
