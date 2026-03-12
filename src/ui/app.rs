use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::{env, process::Command};

use eframe::egui;
use rfd::FileDialog;

use crate::ai::AiClient;
use crate::git::GitClient;
use crate::models::{
    AiProvider, AppSettings, CommitSuggestion, DiffEntry, GitIdentity, RemoteModelOption,
    RepoSnapshot,
};
use crate::storage::{load_settings, push_recent_repo, save_settings};
use crate::ui::components::changes_list::ChangesListAction;
use crate::ui::components::diff_viewer::{self, DiffViewerProps};
use crate::ui::components::history_viewer::{self, HistoryViewerProps};
use crate::ui::components::menu_bar::{self, MenuAction};
use crate::ui::components::settings_window::{self, SettingsAction, SettingsProps};
use crate::ui::components::sidebar::{self, SidebarAction};
use crate::ui::components::status_bar;
use crate::ui::components::toolbar::{self, ToolbarAction, ToolbarProps};
use crate::ui::domain_state::{CommitState, NetworkAction, NetworkState, RepoState, SelectionState};
use crate::ui::theme::configure_visuals;
use crate::ui::ui_state::{
    FilterState, MainTab, MessageState, NavState, OpenRouterModelsState,
    SidebarTab,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum RepoRefreshReason {
    Manual,
    Focus,
    Watch,
}

enum AppEvent {
    RepoLoaded(Result<RepoSnapshot, String>),
    RepoRefreshed(PathBuf, Result<RepoSnapshot, String>, RepoRefreshReason),
    BranchSwitched(Result<RepoSnapshot, String>, String),
    BranchMerged(Result<RepoSnapshot, String>, String),
    CommitCreated(Result<RepoSnapshot, String>),
    NetworkActionCompleted(Result<RepoSnapshot, String>, String),
    AiCommitGenerated(Result<CommitSuggestion, String>),
    OpenRouterModelsLoaded(Result<Vec<RemoteModelOption>, String>),
    CommitDiffLoaded(String, Result<Vec<DiffEntry>, String>),
}

pub struct GitSparkApp {
    ctx: egui::Context,
    git: GitClient,
    settings: AppSettings,
    repo: RepoState,
    commit: CommitState,
    network: NetworkState,
    selection: SelectionState,
    nav: NavState,
    filters: FilterState,
    messages: MessageState,
    repo_watch_generation: Arc<AtomicU64>,
    watched_repo_path: Option<PathBuf>,
    last_window_focused: bool,
    event_tx: Sender<AppEvent>,
    event_rx: Receiver<AppEvent>,
}

impl GitSparkApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        cc.egui_ctx.set_fonts(fonts);

        configure_visuals(&cc.egui_ctx);

        let (settings, error_message) = match load_settings() {
            Ok(settings) => (settings, String::new()),
            Err(err) => (AppSettings::default(), err.to_string()),
        };

        let (event_tx, event_rx) = mpsc::channel();
        let mut app = Self {
            ctx: cc.egui_ctx.clone(),
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
            last_window_focused: true,
            event_tx,
            event_rx,
        };

        if let Some(last_repo) = settings.recent_repos.first() {
            app.open_repo(last_repo.clone());
        }

        app
    }

    fn open_repo_dialog(&mut self) {
        if let Some(path) = FileDialog::new().pick_folder() {
            self.open_repo(path);
        }
    }

    fn open_repo(&mut self, path: PathBuf) {
        self.messages.status_message = "Loading repository...".to_string();
        self.messages.error_message.clear();
        self.nav.show_repo_selector = false;
        self.stop_repo_watch();
        self.add_recent_repo(path.clone());
        let tx = self.event_tx.clone();
        let ctx = self.ctx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.open_repo(path).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::RepoLoaded(res));
            ctx.request_repaint();
        });
    }

    fn refresh_repo(&mut self) {
        self.request_repo_refresh(RepoRefreshReason::Manual);
    }

    fn request_repo_refresh(&mut self, reason: RepoRefreshReason) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        if reason == RepoRefreshReason::Manual {
            self.messages.status_message = "Refreshing repository...".to_string();
        }
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let ctx = self.ctx.clone();
        let git = GitClient::new();
        let event_path = path.clone();
        thread::spawn(move || {
            let res = git.refresh_repo(&path).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::RepoRefreshed(event_path, res, reason));
            ctx.request_repaint();
        });
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
        let ctx = self.ctx.clone();

        thread::spawn(move || {
            let git = GitClient::new();
            let mut last_fingerprint = git.read_watch_fingerprint(&path).ok();

            while generation.load(Ordering::SeqCst) == token {
                thread::sleep(Duration::from_millis(1200));

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
                ctx.request_repaint();
            }
        });
    }

    fn fetch_origin(&mut self) {
        self.run_network_action(NetworkAction::Fetch);
    }

    fn pull_origin(&mut self) {
        self.run_network_action(NetworkAction::Pull);
    }

    fn push_origin(&mut self) {
        self.run_network_action(NetworkAction::Push);
    }

    fn run_network_action(&mut self, action: NetworkAction) {
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
        let ctx = self.ctx.clone();
        let git = GitClient::new();
        let action_label_for_event = action_label.clone();

        thread::spawn(move || {
            let res = match action {
                NetworkAction::Fetch => git.fetch_origin(&path),
                NetworkAction::Pull => git.pull_origin(&path),
                NetworkAction::Push => git.push_origin(&path),
            }
            .map_err(|e| e.to_string());

            let _ = tx.send(AppEvent::NetworkActionCompleted(res, action_label_for_event));
            ctx.request_repaint();
        });
    }

    fn switch_branch(&mut self) {
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
        let ctx = self.ctx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.switch_branch(&path, &target).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::BranchSwitched(res, target));
            ctx.request_repaint();
        });
    }

    fn merge_branch(&mut self) {
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
        let ctx = self.ctx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.merge_branch(&path, &target).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::BranchMerged(res, target));
            ctx.request_repaint();
        });
    }

    fn commit_all(&mut self) {
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
        let ctx = self.ctx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.commit_all(&path, &message).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::CommitCreated(res));
            ctx.request_repaint();
        });
    }

    fn generate_ai_commit(&mut self) {
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
            self.messages.error_message = "No text diff available for AI commit generation.".to_string();
            return;
        }

        self.messages.status_message = "Generating AI commit suggestion...".to_string();
        self.messages.error_message.clear();
        self.commit.ai_in_flight = true;
        let tx = self.event_tx.clone();
        let ctx = self.ctx.clone();
        let ai = AiClient::new();
        let settings = self.settings.ai.clone();
        thread::spawn(move || {
            let res = ai
                .generate_commit_message(&settings, &diff)
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::AiCommitGenerated(res));
            ctx.request_repaint();
        });
    }

    fn ensure_openrouter_models(&mut self) {
        if self.settings.ai.provider != AiProvider::OpenRouter {
            return;
        }

        match self.filters.openrouter_models {
            OpenRouterModelsState::Idle | OpenRouterModelsState::Error(_) => {}
            OpenRouterModelsState::Loading | OpenRouterModelsState::Ready(_) => return,
        }

        self.filters.openrouter_models = OpenRouterModelsState::Loading;
        let tx = self.event_tx.clone();
        let ctx = self.ctx.clone();
        let ai = AiClient::new();
        thread::spawn(move || {
            let res = ai.fetch_openrouter_models().map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::OpenRouterModelsLoaded(res));
            ctx.request_repaint();
        });
    }

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

    fn add_recent_repo(&mut self, path: PathBuf) {
        push_recent_repo(&mut self.settings, path);
        self.persist_settings();
    }

    fn persist_settings(&mut self) {
        self.capture_window_size();
        if let Err(err) = save_settings(&self.settings) {
            self.messages.error_message = format!("Failed to save settings: {err}");
        }
    }

    fn capture_window_size(&mut self) {
        if let Some(inner_rect) = self.ctx.input(|input| input.viewport().inner_rect) {
            let size = inner_rect.size();
            if size.x.is_finite() && size.y.is_finite() && size.x > 0.0 && size.y > 0.0 {
                self.settings.window_size.width = size.x;
                self.settings.window_size.height = size.y;
            }
        }
    }

    fn adopt_snapshot(&mut self, snapshot: RepoSnapshot) {
        let previous_commit = self.selection.selected_commit.clone();
        let current_branch = snapshot.repo.current_branch.clone();
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

    fn repo_path(&self) -> Option<&Path> {
        self.repo
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.repo.path.as_path())
    }

    fn selected_diff(&self) -> Option<&DiffEntry> {
        let snapshot = self.repo.snapshot.as_ref()?;
        let selected_change = self.selection.selected_change.as_ref()?;
        snapshot
            .diffs
            .iter()
            .find(|diff| &diff.path == selected_change)
    }

    fn load_commit_diff(&mut self, oid: String) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            return;
        };

        let tx = self.event_tx.clone();
        let ctx = self.ctx.clone();
        let git = GitClient::new();

        thread::spawn(move || {
            let res = git.get_commit_diff(&path, &oid).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::CommitDiffLoaded(oid, res));
            ctx.request_repaint();
        });
    }

    fn select_commit(&mut self, oid: String) {
        let already_selected = self.selection.selected_commit.as_deref() == Some(oid.as_str());
        if already_selected && self.selection.commit_diffs.is_some() {
            return;
        }

        self.selection.selected_commit = Some(oid.clone());
        self.selection.selected_commit_file = None;
        self.selection.commit_diffs = None;
        self.load_commit_diff(oid);
    }

    fn handle_toolbar_action(&mut self, action: ToolbarAction) {
        match action {
            ToolbarAction::ToggleRepoSelector => {
                self.nav.show_repo_selector = !self.nav.show_repo_selector;
            }
            ToolbarAction::SwitchBranch(name) => {
                self.repo.branch_target = name;
                self.switch_branch();
            }
            ToolbarAction::RunNetworkAction(net_action) => {
                self.run_network_action(net_action);
            }
            ToolbarAction::FetchOrigin => self.fetch_origin(),
            ToolbarAction::PullOrigin => self.pull_origin(),
            ToolbarAction::PushOrigin => self.push_origin(),
            ToolbarAction::OpenRepoDialog => self.open_repo_dialog(),
            ToolbarAction::RefreshRepo => self.refresh_repo(),
            ToolbarAction::OpenRepo(path) => self.open_repo(path),
        }
    }

    fn handle_settings_action(&mut self, action: SettingsAction) {
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
                if self.settings.ai.provider == crate::models::AiProvider::OpenRouter {
                    self.ensure_openrouter_models();
                }
            }
            SettingsAction::SelectOpenRouterModel(model_id) => {
                self.settings.ai.model = model_id;
            }
            SettingsAction::RetryOpenRouterModels => {
                self.filters.openrouter_models = OpenRouterModelsState::Idle;
                self.ensure_openrouter_models();
            }
            SettingsAction::Close => {
                self.nav.show_settings = false;
            }
        }
    }

    fn handle_sidebar_action(&mut self, action: SidebarAction) {
        match action {
            SidebarAction::OpenRepoDialog => self.open_repo_dialog(),
            SidebarAction::OpenRepo(path) => self.open_repo(path),
            SidebarAction::HideRepoSelector => self.nav.show_repo_selector = false,
            SidebarAction::ChangesListAction(a) => self.handle_changes_list_action(a),
            SidebarAction::SelectCommit(oid) => self.select_commit(oid),
            SidebarAction::GenerateAiCommit => self.generate_ai_commit(),
            SidebarAction::ShowSettings => self.nav.show_settings = true,
            SidebarAction::CommitAll => self.commit_all(),
        }
    }

    fn handle_changes_list_action(&mut self, action: ChangesListAction) {
        match action {
            ChangesListAction::SelectChange(path) => {
                self.selection.selected_change = Some(path);
            }
            ChangesListAction::DiscardChange(path) => {
                self.discard_change(&path);
            }
            ChangesListAction::IgnorePath(path) => {
                self.ignore_path(&path);
            }
            ChangesListAction::IgnoreExtension(ext) => {
                self.ignore_extension(&ext);
            }
            ChangesListAction::CopyFullPath(path) => {
                if let Some(repo_path) = self.repo_path() {
                    let full_path = repo_path.join(&path);
                    self.ctx.copy_text(full_path.to_string_lossy().to_string());
                    self.messages.status_message = format!("Copied absolute path for '{path}'.");
                    self.messages.error_message.clear();
                }
            }
            ChangesListAction::CopyRelativePath(path) => {
                self.ctx.copy_text(path.clone());
                self.messages.status_message = format!("Copied relative path for '{path}'.");
                self.messages.error_message.clear();
            }
            ChangesListAction::RevealInFinder(path) => {
                self.reveal_in_finder(&path);
            }
            ChangesListAction::OpenInEditor(path) => {
                self.open_in_external_editor(&path);
            }
            ChangesListAction::OpenWithDefault(path) => {
                if let Some(repo_path) = self.repo_path() {
                    let full_path = repo_path.join(&path);
                    match open::that(&full_path) {
                        Ok(_) => {
                            self.messages.status_message =
                                format!("Opened '{path}' with the default program.");
                            self.messages.error_message.clear();
                        }
                        Err(err) => {
                            self.messages.error_message = format!(
                                "Failed to open '{path}' with default program: {err}"
                            );
                        }
                    }
                }
            }
        }
    }

    fn discard_change(&mut self, relative_path: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        match self.git.discard_change(&repo_path, relative_path) {
            Ok(snapshot) => {
                self.adopt_snapshot(snapshot);
                self.messages.status_message = format!("Discarded changes for '{}'.", relative_path);
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
                self.messages.error_message = format!("Failed to ignore '{}': {err}", relative_path);
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
                self.messages.error_message = format!("Failed to reveal '{}': {err}", relative_path);
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
                self.messages.status_message = format!("Opened '{}' in external editor.", relative_path);
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



}


impl eframe::App for GitSparkApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let is_window_focused = ctx.input(|input| input.viewport().focused.unwrap_or(input.focused));
        if is_window_focused && !self.last_window_focused && self.repo.snapshot.is_some() {
            self.request_repo_refresh(RepoRefreshReason::Focus);
        }
        self.last_window_focused = is_window_focused;

        while let Ok(event) = self.event_rx.try_recv() {
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
                    self.adopt_snapshot(snapshot);
                    self.commit.summary.clear();
                    self.commit.body.clear();
                    self.commit.ai_preview = None;
                    self.messages.status_message = "Commit created.".to_string();
                    self.messages.error_message.clear();
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
            }
        }

        if let Some(action) = menu_bar::render_menu_bar(ctx) {
            match action {
                MenuAction::OpenRepoDialog => self.open_repo_dialog(),
                MenuAction::ShowSettings => self.nav.show_settings = true,
                MenuAction::SetSidebarTab(tab) => self.nav.sidebar_tab = tab,
                MenuAction::Push => self.push_origin(),
                MenuAction::Pull => self.pull_origin(),
                MenuAction::Fetch => self.fetch_origin(),
                MenuAction::MergeBranch => self.merge_branch(),
                MenuAction::Exit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            }
        }
        // Toolbar
        {
            let snapshot = self.repo.snapshot.as_ref();
            let toolbar_props = ToolbarProps {
                repo_title: snapshot
                    .map(|s| s.repo.name.as_str())
                    .unwrap_or("Choose repository"),
                branch_title: snapshot
                    .map(|s| s.repo.current_branch.as_str())
                    .unwrap_or("No branch"),
                snapshot,
                active_network_action: self.network.active_action,
                recent_repos: &self.settings.recent_repos,
            };
            if let Some(action) = toolbar::render_toolbar(ctx, &toolbar_props) {
                self.handle_toolbar_action(action);
            }
        }

        status_bar::render_status_bar(
            ctx,
            &self.messages.status_message,
            &self.messages.error_message,
        );

        // Sidebar
        {
            let snapshot = self.repo.snapshot.as_ref();
            let changes: &[_] = snapshot
                .map(|s| s.changes.as_slice())
                .unwrap_or(&[]);
            let history: &[_] = snapshot
                .map(|s| s.history.as_slice())
                .unwrap_or(&[]);
            let current_repo_name = snapshot.map(|s| s.repo.name.as_str());
            let current_branch = snapshot.map(|s| s.repo.current_branch.as_str());
            let stash_count = snapshot.map(|s| s.stash_count).unwrap_or(0);
            let current_repo_path = snapshot.map(|s| &s.repo.path);
            let avatar_letter = self
                .repo
                .identity
                .user_name
                .chars()
                .next()
                .map(|c: char| c.to_uppercase().to_string())
                .unwrap_or_else(|| "?".to_string());

            let mut sidebar_props = sidebar::SidebarProps {
                sidebar_tab: &mut self.nav.sidebar_tab,
                show_repo_selector: self.nav.show_repo_selector,
                has_snapshot: self.repo.snapshot.is_some(),
                current_repo_name,
                current_branch,
                stash_count,
                changes,
                selected_change: self.selection.selected_change.as_deref(),
                filter_text: &mut self.filters.filter_text,
                change_filters: &mut self.filters.change_filters,
                history,
                selected_commit: self.selection.selected_commit.as_deref(),
                commit_summary: &mut self.commit.summary,
                commit_body: &mut self.commit.body,
                ai_in_flight: self.commit.ai_in_flight,
                ai_preview: self.commit.ai_preview.as_ref(),
                avatar_letter: &avatar_letter,
                recent_repos: &self.settings.recent_repos,
                current_repo_path,
                repo_filter_text: &mut self.filters.repo_filter_text,
            };

            if let Some(action) = sidebar::render_sidebar(ctx, &mut sidebar_props) {
                self.handle_sidebar_action(action);
            }
        }

        // Main workspace
        match self.nav.main_tab {
            MainTab::Workspace => {
                if self.nav.sidebar_tab == SidebarTab::History {
                    let selected_commit = self
                        .selection
                        .selected_commit
                        .as_deref()
                        .and_then(|oid| {
                            self.repo
                                .snapshot
                                .as_ref()
                                .and_then(|repo| repo.history.iter().find(|c| c.oid == oid))
                        })
                        .cloned();
                    let history_props = HistoryViewerProps {
                        selected_commit: selected_commit.as_ref(),
                        commit_diffs: self.selection.commit_diffs.as_deref(),
                        selected_commit_file: self.selection.selected_commit_file.as_deref(),
                    };
                    if let Some(path) = history_viewer::render_history_viewer(ctx, &history_props) {
                        self.selection.selected_commit_file = Some(path);
                    }
                } else {
                    let diff_props = DiffViewerProps {
                        selected_change: self.selection.selected_change.as_deref(),
                        selected_diff: self.selected_diff(),
                    };
                    diff_viewer::render_diff_viewer(ctx, &diff_props);
                }
            }
        }

        // Settings window
        if self.nav.show_settings {
            if self.settings.ai.provider == crate::models::AiProvider::OpenRouter {
                self.ensure_openrouter_models();
            }
            let repo_path_display = self
                .repo
                .snapshot
                .as_ref()
                .map(|s| s.repo.path.display().to_string());
            let mut settings_props = SettingsProps {
                open: self.nav.show_settings,
                settings_section: &mut self.nav.settings_section,
                status_message: &self.messages.status_message,
                identity: &mut self.repo.identity,
                has_repo: self.repo.snapshot.is_some(),
                repo_path_display,
                ai_settings: &mut self.settings,
                openrouter_models: &self.filters.openrouter_models,
                openrouter_model_filter: &mut self.filters.openrouter_model_filter,
            };
            let (still_open, action) =
                settings_window::render_settings_window(ctx, &mut settings_props);
            self.nav.show_settings = still_open;
            if let Some(action) = action {
                self.handle_settings_action(action);
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.stop_repo_watch();
        self.persist_settings();
    }
}

fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}


