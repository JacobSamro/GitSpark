use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::{env, process::Command};

use eframe::egui::{self, Align, Align2, Color32, PopupCloseBehavior, RichText, Stroke, Vec2};
use egui_phosphor::regular as icons;
use rfd::FileDialog;

use crate::ai::AiClient;
use crate::git::GitClient;
use crate::models::{AppSettings, CommitSuggestion, DiffEntry, GitIdentity, RepoSnapshot};
use crate::storage::{load_settings, push_recent_repo, save_settings};
use crate::ui::components::buttons::{compact_action_button, tab_button};
use crate::ui::components::diff::render_diff_text;
use crate::ui::theme::{
    ACCENT_MUTED, BG, BORDER, DANGER, DIFF_BG, PANEL_BG, SUCCESS, SURFACE_BG, SURFACE_BG_MUTED,
    TEXT_MAIN, TEXT_MUTED, WARNING, configure_visuals,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum MainTab {
    Workspace,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SidebarTab {
    Changes,
    History,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NetworkAction {
    Fetch,
    Pull,
    Push,
}

#[derive(Clone, Copy, Default)]
struct ChangeFilterOptions {
    included_in_commit: bool,
    excluded_from_commit: bool,
    new_files: bool,
    modified_files: bool,
    deleted_files: bool,
}

impl ChangeFilterOptions {
    fn active_count(self) -> usize {
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
    CommitDiffLoaded(String, Result<Vec<DiffEntry>, String>),
}

pub struct GitSparkApp {
    ctx: egui::Context,
    git: GitClient,
    settings: AppSettings,
    show_settings: bool,
    show_repo_selector: bool,
    current_repo: Option<RepoSnapshot>,
    repo_identity: GitIdentity,
    repo_filter_text: String,
    selected_recent_repo: Option<usize>,
    selected_change: Option<String>,
    selected_commit: Option<String>,
    commit_diffs: Option<Vec<DiffEntry>>,
    selected_commit_file: Option<String>,
    branch_target: String,
    merge_target: String,
    commit_summary: String,
    commit_body: String,
    ai_preview: Option<CommitSuggestion>,
    status_message: String,
    error_message: String,
    main_tab: MainTab,
    sidebar_tab: SidebarTab,
    filter_text: String,
    change_filters: ChangeFilterOptions,
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
            show_settings: false,
            show_repo_selector: false,
            current_repo: None,
            repo_identity: GitIdentity::default(),
            repo_filter_text: String::new(),
            selected_recent_repo: None,
            selected_change: None,
            selected_commit: None,
            commit_diffs: None,
            selected_commit_file: None,
            branch_target: String::new(),
            merge_target: String::new(),
            commit_summary: String::new(),
            commit_body: String::new(),
            ai_preview: None,
            status_message: "Open a repository to get started.".to_string(),
            error_message,
            main_tab: MainTab::Workspace,
            sidebar_tab: SidebarTab::Changes,
            filter_text: String::new(),
            change_filters: ChangeFilterOptions::default(),
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
        self.status_message = "Loading repository...".to_string();
        self.error_message.clear();
        self.show_repo_selector = false;
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
            self.error_message = "No repository selected.".to_string();
            return;
        };

        if reason == RepoRefreshReason::Manual {
            self.status_message = "Refreshing repository...".to_string();
        }
        self.error_message.clear();
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
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.error_message = "No repository selected.".to_string();
            return;
        };

        let remote_name = self
            .current_repo
            .as_ref()
            .and_then(|snapshot| snapshot.repo.remote_name.clone())
            .unwrap_or_else(|| "origin".to_string());
        let action_label = action.title(&remote_name);

        self.status_message = format!("{action_label}...");
        self.error_message.clear();

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
            self.error_message = "No repository selected.".to_string();
            return;
        };

        let target = self.branch_target.trim().to_string();
        if target.is_empty() {
            self.error_message = "Choose a branch first.".to_string();
            return;
        }

        self.status_message = format!("Switching to '{}'...", target);
        self.error_message.clear();
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
            self.error_message = "No repository selected.".to_string();
            return;
        };

        let target = self.merge_target.trim().to_string();
        if target.is_empty() {
            self.error_message = "Choose a branch to merge.".to_string();
            return;
        }

        self.status_message = format!("Merging '{}'...", target);
        self.error_message.clear();
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
            self.error_message = "No repository selected.".to_string();
            return;
        };

        if self.commit_summary.trim().is_empty() {
            self.error_message = "Commit summary cannot be empty.".to_string();
            return;
        }

        let message = if self.commit_body.trim().is_empty() {
            self.commit_summary.trim().to_string()
        } else {
            format!(
                "{}\n\n{}",
                self.commit_summary.trim(),
                self.commit_body.trim()
            )
        };

        self.status_message = "Creating commit...".to_string();
        self.error_message.clear();
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
        let Some(snapshot) = &self.current_repo else {
            self.error_message =
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
            self.error_message = "No text diff available for AI commit generation.".to_string();
            return;
        }

        self.status_message = "Generating AI commit suggestion...".to_string();
        self.error_message.clear();
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

    fn save_git_config(&mut self) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.error_message = "No repository selected.".to_string();
            return;
        };

        match self.git.write_identity(&path, &self.repo_identity) {
            Ok(()) => {
                self.status_message = "Git config saved.".to_string();
                self.error_message.clear();
            }
            Err(err) => {
                self.error_message = format!("Failed to save git config: {err}");
            }
        }
    }

    fn load_identity(&mut self, path: &Path) {
        match self.git.read_identity(path) {
            Ok(identity) => {
                self.repo_identity = identity;
            }
            Err(err) => {
                self.repo_identity = GitIdentity::default();
                self.error_message = format!("Could not load git config: {err}");
            }
        }
    }

    fn add_recent_repo(&mut self, path: PathBuf) {
        push_recent_repo(&mut self.settings, path);
        self.selected_recent_repo = Some(0);
        self.persist_settings();
    }

    fn persist_settings(&mut self) {
        self.capture_window_size();
        if let Err(err) = save_settings(&self.settings) {
            self.error_message = format!("Failed to save settings: {err}");
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
        let previous_commit = self.selected_commit.clone();
        let current_branch = snapshot.repo.current_branch.clone();
        self.selected_change = snapshot.changes.first().map(|change| change.path.clone());
        self.branch_target = current_branch;
        self.merge_target = snapshot
            .branches
            .iter()
            .find(|branch| !branch.is_current && !branch.is_remote)
            .map(|branch| branch.name.clone())
            .unwrap_or_default();
        self.load_identity(&snapshot.repo.path);
        self.ensure_repo_watch(&snapshot.repo.path);
        self.current_repo = Some(snapshot);

        let next_selected_commit = self.current_repo.as_ref().and_then(|repo| {
            previous_commit
                .filter(|oid| repo.history.iter().any(|commit| commit.oid == *oid))
                .or_else(|| repo.history.first().map(|commit| commit.oid.clone()))
        });

        self.selected_commit = next_selected_commit.clone();
        self.selected_commit_file = None;
        self.commit_diffs = None;

        if let Some(oid) = next_selected_commit {
            self.load_commit_diff(oid);
        }
    }

    fn repo_path(&self) -> Option<&Path> {
        self.current_repo
            .as_ref()
            .map(|snapshot| snapshot.repo.path.as_path())
    }

    fn selected_diff(&self) -> Option<&DiffEntry> {
        let snapshot = self.current_repo.as_ref()?;
        let selected_change = self.selected_change.as_ref()?;
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
        let already_selected = self.selected_commit.as_deref() == Some(oid.as_str());
        if already_selected && self.commit_diffs.is_some() {
            return;
        }

        self.selected_commit = Some(oid.clone());
        self.selected_commit_file = None;
        self.commit_diffs = None;
        self.load_commit_diff(oid);
    }

    fn render_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_bar")
            .exact_height(68.0)
            .frame(
                egui::Frame::default()
                    .fill(SURFACE_BG)
                    .inner_margin(egui::Margin::symmetric(12, 5))
                    .stroke(Stroke::new(1.0, BORDER)),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.set_min_height(52.0);
                let repo_title = self
                    .current_repo
                    .as_ref()
                    .map(|snapshot| snapshot.repo.name.clone())
                    .unwrap_or_else(|| "Choose repository".to_string());
                let branch_title = self
                    .current_repo
                    .as_ref()
                    .map(|snapshot| snapshot.repo.current_branch.clone())
                    .unwrap_or_else(|| "No branch".to_string());
                ui.with_layout(egui::Layout::left_to_right(Align::Min), |ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(238.0, 52.0),
                        egui::Layout::top_down(Align::Min),
                        |ui| {
                            self.render_repository_toolbar_trigger(
                                ui,
                                icons::FOLDER_NOTCH_OPEN,
                                "Current Repository",
                                &repo_title,
                                238.0,
                            );
                        },
                    );
                    let first_sep_x = ui.cursor().left() - 4.0;
                    ui.painter().vline(
                        first_sep_x,
                        ui.max_rect().y_range(),
                        Stroke::new(1.0, BORDER),
                    );

                    ui.allocate_ui_with_layout(
                        Vec2::new(214.0, 52.0),
                        egui::Layout::top_down(Align::Min),
                        |ui| {
                            self.render_dropdown_toolbar_block(
                                ui,
                                "toolbar_branch",
                                icons::GIT_BRANCH,
                                "Current Branch",
                                &branch_title,
                                214.0,
                                |app, ui| app.render_branch_toolbar_menu(ui),
                            );
                        },
                    );
                    let second_sep_x = ui.cursor().left() - 4.0;
                    ui.painter().vline(
                        second_sep_x,
                        ui.max_rect().y_range(),
                        Stroke::new(1.0, BORDER),
                    );

                    ui.allocate_ui_with_layout(
                        Vec2::new(224.0, 52.0),
                        egui::Layout::top_down(Align::Min),
                        |ui| {
                            self.render_network_toolbar_block(ui);
                        },
                    );
                });
            });
    }

    fn render_dropdown_toolbar_block<F>(
        &mut self,
        ui: &mut egui::Ui,
        id_source: &str,
        icon: &str,
        description: &str,
        title: &str,
        width: f32,
        add_popup_contents: F,
    ) where
        F: FnOnce(&mut Self, &mut egui::Ui),
    {
        let popup_id = ui.make_persistent_id(id_source);
        let response = egui::Frame::default()
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::NONE)
            .corner_radius(0.0)
            .inner_margin(egui::Margin::same(0))
            .show(ui, |ui| {
                ui.set_min_size(Vec2::new(width, 52.0));
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.add_sized(
                        [18.0, 52.0],
                        egui::Label::new(RichText::new(icon).size(15.0).color(TEXT_MUTED)),
                    );
                    ui.add_space(12.0);
                    render_toolbar_text_stack(
                        ui,
                        description,
                        title,
                        width - 76.0,
                        Some(icons::CARET_DOWN),
                    );
                });
            })
            .response
            .interact(egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);

        if response.clicked() {
            ui.memory_mut(|mem| mem.toggle_popup(popup_id));
        }

        ui.scope(|ui| {
            let visuals = &mut ui.style_mut().visuals;
            visuals.window_fill = PANEL_BG;
            visuals.window_stroke = Stroke::NONE;
            visuals.popup_shadow = egui::epaint::Shadow::NONE;

            egui::popup_below_widget(
                ui,
                popup_id,
                &response,
                PopupCloseBehavior::CloseOnClickOutside,
                |ui| {
                    ui.set_min_width(width.max(260.0));
                    egui::Frame::default()
                        .fill(PANEL_BG)
                        .stroke(Stroke::NONE)
                        .corner_radius(6.0)
                        .inner_margin(egui::Margin::same(10))
                        .show(ui, |ui| {
                            add_popup_contents(self, ui);
                        });
                },
            );
        });
    }

    fn render_repository_toolbar_trigger(
        &mut self,
        ui: &mut egui::Ui,
        icon: &str,
        description: &str,
        title: &str,
        width: f32,
    ) {
        let response = egui::Frame::default()
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::NONE)
            .corner_radius(0.0)
            .inner_margin(egui::Margin::same(0))
            .show(ui, |ui| {
                ui.set_min_size(Vec2::new(width, 52.0));
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.add_sized(
                        [18.0, 52.0],
                        egui::Label::new(RichText::new(icon).size(15.0).color(TEXT_MUTED)),
                    );
                    ui.add_space(12.0);
                    render_toolbar_text_stack(
                        ui,
                        description,
                        title,
                        width - 76.0,
                        Some(icons::CARET_DOWN),
                    );
                });
            })
            .response
            .interact(egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);

        if response.clicked() {
            self.show_repo_selector = !self.show_repo_selector;
        }
    }

    fn render_repository_toolbar_menu(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Current Repository").small().color(TEXT_MUTED));
        ui.add_space(6.0);

        if let Some(snapshot) = &self.current_repo {
            ui.label(RichText::new(&snapshot.repo.name).strong().color(TEXT_MAIN));
            ui.label(
                RichText::new(snapshot.repo.path.display().to_string())
                    .small()
                    .color(TEXT_MUTED),
            );
            ui.separator();
        }

        if self.settings.recent_repos.is_empty() {
            ui.label(RichText::new("No recent repositories").color(TEXT_MUTED));
        } else {
            for path in self.settings.recent_repos.clone() {
                let is_current = self
                    .current_repo
                    .as_ref()
                    .map(|snapshot| snapshot.repo.path == path)
                    .unwrap_or(false);
                let repo_name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Repository");
                let icon = if is_current {
                    icons::CHECK
                } else {
                    icons::FOLDER_NOTCH_OPEN
                };
                let button = egui::Button::new(
                    RichText::new(format!("{icon}  {repo_name}")).color(TEXT_MAIN),
                )
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::NONE)
                .min_size(Vec2::new(ui.available_width(), 24.0));

                if ui.add(button).clicked() {
                    self.open_repo(path);
                    ui.close_menu();
                }
            }
        }

        ui.separator();
        if ui.button("Choose Repository...").clicked() {
            self.open_repo_dialog();
            ui.close_menu();
        }
        if ui.button("Refresh").clicked() {
            self.refresh_repo();
            ui.close_menu();
        }
    }

    fn render_repository_sidebar_overlay(&mut self, ui: &mut egui::Ui) {
        egui::Frame::default()
            .fill(PANEL_BG)
            .inner_margin(egui::Margin::same(0))
            .show(ui, |ui| {
                egui::TopBottomPanel::top("repo_selector_header")
                    .resizable(false)
                    .frame(
                        egui::Frame::default()
                            .fill(PANEL_BG)
                            .inner_margin(egui::Margin::symmetric(12, 10)),
                    )
                    .show_inside(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(icons::FOLDER_NOTCH_OPEN).color(TEXT_MUTED));
                            ui.add_space(6.0);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 1.0;
                                ui.label(
                                    RichText::new("Current Repository")
                                        .small()
                                        .color(TEXT_MUTED),
                                );
                                ui.label(
                                    RichText::new(
                                        self.current_repo
                                            .as_ref()
                                            .map(|snapshot| snapshot.repo.name.as_str())
                                            .unwrap_or("Choose repository"),
                                    )
                                    .strong()
                                    .color(TEXT_MAIN),
                                );
                            });
                            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(icons::CARET_UP).color(TEXT_MUTED),
                                        )
                                        .fill(Color32::TRANSPARENT)
                                        .stroke(Stroke::NONE)
                                        .min_size(Vec2::new(20.0, 20.0)),
                                    )
                                    .clicked()
                                {
                                    self.show_repo_selector = false;
                                }
                            });
                        });
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            let filter = egui::TextEdit::singleline(&mut self.repo_filter_text)
                                .hint_text("Filter")
                                .desired_width(ui.available_width() - 112.0)
                                .margin(egui::Margin::symmetric(8, 6));
                            ui.add_sized([ui.available_width() - 96.0, 32.0], filter);

                            let add_button = egui::Button::new(
                                RichText::new("Add  ▾").color(TEXT_MAIN).strong(),
                            )
                            .fill(SURFACE_BG)
                            .stroke(Stroke::new(1.0, BORDER))
                            .corner_radius(8.0);
                            if ui.add_sized([88.0, 32.0], add_button).clicked() {
                                self.open_repo_dialog();
                            }
                        });
                    });

                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::default()
                            .fill(PANEL_BG)
                            .inner_margin(egui::Margin::symmetric(12, 0)),
                    )
                    .show_inside(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                if self.settings.recent_repos.is_empty() {
                                    ui.add_space(12.0);
                                    ui.label(
                                        RichText::new("No recent repositories")
                                            .color(TEXT_MUTED),
                                    );
                                    return;
                                }

                                for path in self.settings.recent_repos.clone() {
                                    let is_current = self
                                        .current_repo
                                        .as_ref()
                                        .map(|snapshot| snapshot.repo.path == path)
                                        .unwrap_or(false);
                                    let repo_name = path
                                        .file_name()
                                        .and_then(|name| name.to_str())
                                        .unwrap_or("Repository");
                                    if !self.repo_filter_text.trim().is_empty()
                                        && !repo_name
                                            .to_ascii_lowercase()
                                            .contains(&self.repo_filter_text.to_ascii_lowercase())
                                        && !path
                                            .display()
                                            .to_string()
                                            .to_ascii_lowercase()
                                            .contains(&self.repo_filter_text.to_ascii_lowercase())
                                    {
                                        continue;
                                    }

                                    let response = egui::Frame::default()
                                        .fill(if is_current {
                                            SURFACE_BG
                                        } else {
                                            Color32::TRANSPARENT
                                        })
                                        .inner_margin(egui::Margin::symmetric(2, 8))
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    RichText::new(if is_current {
                                                        icons::CHECK
                                                    } else {
                                                        icons::FOLDER_NOTCH_OPEN
                                                    })
                                                    .color(TEXT_MUTED),
                                                );
                                                ui.add_space(8.0);
                                                ui.add_sized(
                                                    [ui.available_width(), 20.0],
                                                    egui::Label::new(
                                                        RichText::new(repo_name)
                                                            .color(TEXT_MAIN)
                                                            .strong(),
                                                    )
                                                    .truncate(),
                                                );
                                            });
                                        })
                                        .response
                                        .interact(egui::Sense::click())
                                        .on_hover_cursor(egui::CursorIcon::PointingHand);

                                    ui.painter().hline(
                                        response.rect.x_range(),
                                        response.rect.bottom(),
                                        Stroke::new(1.0, BORDER),
                                    );

                                    if response.clicked() {
                                        self.open_repo(path);
                                    }
                                }
                            });
                    });
            });
    }

    fn render_branch_toolbar_menu(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Current Branch").small().color(TEXT_MUTED));
        ui.add_space(6.0);

        let branches = self
            .current_repo
            .as_ref()
            .map(|snapshot| snapshot.branches.clone())
            .unwrap_or_default();

        if branches.is_empty() {
            ui.label(RichText::new("No branches available").color(TEXT_MUTED));
            return;
        }

        for branch in branches.iter().filter(|branch| !branch.is_remote) {
            let label = if branch.is_current {
                format!("✓ {}", branch.name)
            } else {
                branch.name.clone()
            };

            if ui
                .add(
                    egui::Button::new(RichText::new(label).color(TEXT_MAIN))
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::NONE)
                        .min_size(Vec2::new(ui.available_width(), 24.0)),
                )
                .clicked()
            {
                if !branch.is_current {
                    self.branch_target = branch.name.clone();
                    self.switch_branch();
                }
                ui.close_menu();
            }
        }

        let remote_branches = branches
            .into_iter()
            .filter(|branch| branch.is_remote)
            .collect::<Vec<_>>();
        if !remote_branches.is_empty() {
            ui.separator();
            ui.label(RichText::new("Remote Branches").small().color(TEXT_MUTED));
            for branch in remote_branches {
                if ui
                    .add(
                        egui::Button::new(RichText::new(branch.name.clone()).color(TEXT_MUTED))
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::NONE)
                            .min_size(Vec2::new(ui.available_width(), 24.0)),
                    )
                    .clicked()
                {
                    self.branch_target = branch.name;
                    self.switch_branch();
                    ui.close_menu();
                }
            }
        }
    }

    fn render_network_toolbar_block(&mut self, ui: &mut egui::Ui) {
        let Some(snapshot) = self.current_repo.clone() else {
            self.render_disabled_network_toolbar_block(ui);
            return;
        };

        let popup_id = ui.make_persistent_id("toolbar_network");
        let remote_name = snapshot
            .repo
            .remote_name
            .clone()
            .unwrap_or_else(|| "origin".to_string());
        let primary_action = NetworkAction::from_snapshot(&snapshot);
        let title = primary_action.title(&remote_name);
        let description = snapshot
            .repo
            .last_fetched
            .as_ref()
            .map(|value| format!("Last fetched {value}"))
            .unwrap_or_else(|| "Never fetched".to_string());
        let block = egui::Frame::default()
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::NONE)
            .corner_radius(0.0)
            .inner_margin(egui::Margin::same(0))
            .show(ui, |ui| {
                ui.set_min_size(Vec2::new(224.0, 52.0));
                ui.horizontal(|ui| {
                    let main = ui
                        .allocate_ui_with_layout(
                            Vec2::new(185.0, 52.0),
                            egui::Layout::left_to_right(Align::Center),
                            |ui| {
                                ui.add_space(12.0);
                                ui.add_sized(
                                    [18.0, 52.0],
                                    egui::Label::new(
                                        RichText::new(primary_action.icon())
                                            .size(15.0)
                                            .color(TEXT_MUTED),
                                    ),
                                );
                                ui.add_space(12.0);
                                render_network_text_stack(
                                    ui,
                                    &description,
                                    &title,
                                    snapshot.repo.ahead,
                                    snapshot.repo.behind,
                                    143.0,
                                );
                            },
                        )
                        .response
                        .interact(egui::Sense::click())
                        .on_hover_cursor(egui::CursorIcon::PointingHand);

                    let divider_x = ui.min_rect().left() + 185.0;
                    ui.painter().vline(
                        divider_x,
                        ui.max_rect().y_range(),
                        Stroke::new(1.0, BORDER),
                    );
                    let arrow = ui
                        .add_sized(
                            [39.0, 52.0],
                            egui::Button::new(
                                RichText::new(icons::CARET_DOWN)
                                    .size(11.0)
                                    .color(TEXT_MUTED),
                            )
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::NONE),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand);

                    (main, arrow)
                })
                .inner
            });
        let (main_response, arrow_response) = block.inner;

        if main_response.clicked() {
            self.run_network_action(primary_action);
        }

        if arrow_response.clicked() {
            ui.memory_mut(|mem| mem.toggle_popup(popup_id));
        }

        ui.scope(|ui| {
            let visuals = &mut ui.style_mut().visuals;
            visuals.window_fill = PANEL_BG;
            visuals.window_stroke = Stroke::NONE;
            visuals.popup_shadow = egui::epaint::Shadow::NONE;

            egui::popup_below_widget(
                ui,
                popup_id,
                &block.response,
                PopupCloseBehavior::CloseOnClickOutside,
                |ui| {
                    ui.set_min_width(246.0);
                    self.render_network_toolbar_menu(ui, &snapshot, &remote_name, primary_action);
                },
            );
        });
    }

    fn render_disabled_network_toolbar_block(&self, ui: &mut egui::Ui) {
        egui::Frame::default()
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::NONE)
            .corner_radius(0.0)
            .inner_margin(egui::Margin::same(0))
            .show(ui, |ui| {
                ui.set_min_size(Vec2::new(224.0, 52.0));
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.add_sized(
                        [18.0, 52.0],
                        egui::Label::new(
                            RichText::new(icons::ARROW_CLOCKWISE)
                                .size(15.0)
                                .color(TEXT_MUTED),
                        ),
                    );
                    ui.add_space(12.0);
                    render_toolbar_text_stack(
                        ui,
                        "Open a repository first",
                        "Fetch origin",
                        143.0,
                        Some(icons::CARET_DOWN),
                    );
                });
            });
    }

    fn render_network_toolbar_menu(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &RepoSnapshot,
        remote_name: &str,
        primary_action: NetworkAction,
    ) {
        ui.label(RichText::new("Remote").small().color(TEXT_MUTED));
        ui.add_space(6.0);

        if ui.button(primary_action.title(remote_name)).clicked() {
            self.run_network_action(primary_action);
            ui.close_menu();
        }

        if primary_action != NetworkAction::Fetch && ui.button(format!("Fetch {remote_name}")).clicked() {
            self.fetch_origin();
            ui.close_menu();
        }

        if snapshot.repo.behind > 0
            && primary_action != NetworkAction::Pull
            && ui.button(format!("Pull {remote_name}")).clicked()
        {
            self.pull_origin();
            ui.close_menu();
        }

        if snapshot.repo.ahead > 0
            && primary_action != NetworkAction::Push
            && ui.button(format!("Push {remote_name}")).clicked()
        {
            self.push_origin();
            ui.close_menu();
        }

        ui.separator();
        ui.label(
            RichText::new(format!("{}  {}↑ {}↓", remote_name, snapshot.repo.ahead, snapshot.repo.behind))
                .small()
                .color(TEXT_MUTED),
        );
    }

    fn render_status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(26.0)
            .frame(
                egui::Frame::default()
                    .fill(PANEL_BG)
                    .inner_margin(egui::Margin::same(6)),
            )
            .show(ctx, |ui| {
                let text = if !self.error_message.is_empty() {
                    RichText::new(&self.error_message).color(DANGER)
                } else {
                    RichText::new(&self.status_message).color(TEXT_MUTED)
                };
                ui.label(text);
            });
    }

    fn render_sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("sidebar")
            .resizable(true)
            .default_width(260.0)
            .min_width(220.0)
            .frame(
                egui::Frame::default()
                    .fill(PANEL_BG)
                    .inner_margin(egui::Margin::same(0)),
            )
            .show(ctx, |ui| {
                if self.show_repo_selector {
                    self.render_repository_sidebar_overlay(ui);
                    return;
                }

                self.render_sidebar_tabs(ui);

                if self.current_repo.is_some() {
                    match self.sidebar_tab {
                        SidebarTab::Changes => {
                            // Commit area at the bottom
                            egui::TopBottomPanel::bottom("commit_area_panel")
                                .resizable(false)
                                .min_height(170.0)
                                .frame(
                                    egui::Frame::default()
                                        .fill(PANEL_BG)
                                        .inner_margin(egui::Margin::symmetric(0, 4))
                                        .stroke(Stroke::new(1.0, BORDER)),
                                )
                                .show_inside(ui, |ui| {
                                    self.render_commit_sidebar(ui);
                                });

                            // Changes list in the remaining space
                            egui::CentralPanel::default()
                                .frame(
                                    egui::Frame::default()
                                        .fill(PANEL_BG)
                                        .inner_margin(egui::Margin::same(0)),
                                )
                                .show_inside(ui, |ui| {
                                    ui.spacing_mut().item_spacing.y = 0.0;
                                    self.render_filter_bar(ui);
                                    self.render_changes_header(ui);

                                    let changes = self
                                        .current_repo
                                        .as_ref()
                                        .map(|s| s.changes.clone())
                                        .unwrap_or_default();
                                    let filtered_changes: Vec<_> = changes
                                        .iter()
                                        .filter(|change| {
                                            matches_filter(&self.filter_text, &change.path)
                                                && matches_change_filters(
                                                    change,
                                                    self.change_filters,
                                                )
                                        })
                                        .collect();

                                    egui::ScrollArea::vertical()
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| {
                                            for (index, change) in
                                                filtered_changes.iter().enumerate()
                                            {
                                                self.render_change_row(ui, change, index);
                                            }

                                            if changes.is_empty() {
                                                ui.add_space(20.0);
                                                ui.vertical_centered(|ui| {
                                                    ui.label(RichText::new("No changes").color(TEXT_MUTED));
                                                });
                                            } else if filtered_changes.is_empty() {
                                                ui.add_space(20.0);
                                                ui.vertical_centered(|ui| {
                                                    ui.label(
                                                        RichText::new("No matching changed files")
                                                            .color(TEXT_MUTED),
                                                    );
                                                });
                                            }
                                        });
                                });
                        }
                        SidebarTab::History => {
                             // Render history list
                             self.render_history_sidebar(ui);
                        }
                    }
                } else {
                    egui::CentralPanel::default().show_inside(ui, |ui| {
                        egui::Frame::default()
                            .fill(SURFACE_BG_MUTED)
                            .inner_margin(egui::Margin::same(12))
                            .stroke(Stroke::new(1.0, BORDER))
                            .show(ui, |ui| {
                                ui.label(RichText::new("No repository loaded").color(TEXT_MAIN).strong());
                                ui.label(RichText::new("Use the + button in the header or the recent repository picker to load a repo.").color(TEXT_MUTED));

                                ui.add_space(10.0);
                                self.render_recent_repos_picker(ui);
                            });
                    });
                }
            });
    }

    fn render_sidebar_tabs(&mut self, ui: &mut egui::Ui) {
        egui::Frame::default()
            .fill(SURFACE_BG_MUTED)
            .stroke(Stroke::new(1.0, BORDER))
            .inner_margin(egui::Margin::same(0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_height(36.0);
                ui.spacing_mut().item_spacing = Vec2::ZERO;

                let tab_width = ui.available_width() / 2.0;
                ui.horizontal(|ui| {
                    tab_button(
                        ui,
                        &mut self.sidebar_tab,
                        SidebarTab::Changes,
                        "Changes",
                        tab_width,
                    );
                    tab_button(
                        ui,
                        &mut self.sidebar_tab,
                        SidebarTab::History,
                        "History",
                        tab_width,
                    );
                });

                let divider_x = ui.min_rect().center().x;
                ui.painter().vline(
                    divider_x,
                    ui.min_rect().y_range(),
                    Stroke::new(1.0, BORDER),
                );
            });
    }

    fn render_history_sidebar(&mut self, ui: &mut egui::Ui) {
        let history = self
            .current_repo
            .as_ref()
            .map(|repo| repo.history.clone())
            .unwrap_or_default();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;

                if history.is_empty() {
                    ui.add_space(20.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("No history").color(TEXT_MUTED));
                    });
                    return;
                }

                for commit in &history {
                    self.render_history_row(ui, commit);
                }
            });
    }

    fn render_history_row(&mut self, ui: &mut egui::Ui, commit: &crate::models::CommitInfo) {
        let is_selected = self.selected_commit.as_deref() == Some(commit.oid.as_str());
        let bg_color = if is_selected {
            ACCENT_MUTED
        } else {
            Color32::TRANSPARENT
        };
        let summary_color = if is_selected {
            Color32::WHITE
        } else {
            TEXT_MAIN
        };
        let meta_color = if is_selected {
            Color32::from_gray(225)
        } else {
            TEXT_MUTED
        };
        let summary = if commit.summary.trim().is_empty() {
            "Empty commit message"
        } else {
            commit.summary.trim()
        };

        let mut meta_parts = Vec::new();
        if commit.is_head {
            meta_parts.push("HEAD".to_string());
        }
        meta_parts.push(commit.short_oid.clone());
        meta_parts.push(commit.author_name.clone());
        meta_parts.push(commit.date.clone());
        let meta_text = meta_parts.join(" • ");

        let response = egui::Frame::default()
            .fill(bg_color)
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_min_height(40.0);
                ui.set_width(ui.available_width());
                let row_rect = ui.max_rect();
                let painter = ui.painter();
                let text_left = row_rect.left();
                let text_top = row_rect.top();

                painter.text(
                    egui::pos2(text_left, text_top),
                    Align2::LEFT_TOP,
                    truncate_single_line(summary, 54),
                    egui::FontId::proportional(12.5),
                    summary_color,
                );
                painter.text(
                    egui::pos2(text_left, text_top + 21.0),
                    Align2::LEFT_TOP,
                    truncate_single_line(&meta_text, 72),
                    egui::FontId::proportional(11.0),
                    meta_color,
                );
            })
            .response
            .interact(egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);

        ui.painter().hline(
            response.rect.x_range(),
            response.rect.bottom(),
            Stroke::new(1.0, BORDER),
        );

        if response.clicked() {
            self.select_commit(commit.oid.clone());
        }
    }

    fn render_filter_bar(&mut self, ui: &mut egui::Ui) {
        let changes = self
            .current_repo
            .as_ref()
            .map(|snapshot| snapshot.changes.clone())
            .unwrap_or_default();
        let popup_id = ui.make_persistent_id("changes_filter_options");
        let active_filter_count = self.change_filters.active_count();
        let mut button_response = None;

        egui::Frame::default()
            .fill(SURFACE_BG)
            .stroke(Stroke::NONE)
            .corner_radius(0.0)
            .inner_margin(egui::Margin::same(0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_height(32.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;

                    let response = ui
                        .add(
                            egui::Button::new(
                                RichText::new(format!(
                                    "{}  {}",
                                    icons::FUNNEL_SIMPLE,
                                    icons::CARET_DOWN
                                ))
                                .color(TEXT_MAIN)
                                .size(14.0),
                            )
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::NONE)
                            .corner_radius(0.0)
                            .min_size(Vec2::new(52.0, 32.0)),
                        )
                        .on_hover_text("Filter options");
                    button_response = Some(response);

                    let divider_x = ui.min_rect().left() + 52.0;
                    ui.painter().vline(
                        divider_x,
                        ui.max_rect().y_range(),
                        Stroke::new(1.0, BORDER),
                    );

                    let edit = egui::TextEdit::singleline(&mut self.filter_text)
                        .hint_text("Filter files")
                        .background_color(Color32::TRANSPARENT)
                        .desired_width(f32::INFINITY)
                        .margin(egui::Margin::symmetric(10, 6));

                    ui.add_sized([ui.available_width(), 32.0], edit);
                });
            });

        if let Some(button_response) = button_response {
            if button_response.clicked() {
                ui.memory_mut(|mem| mem.toggle_popup(popup_id));
            }

            if active_filter_count > 0 {
                let badge_center = egui::pos2(
                    button_response.rect.right() - 12.0,
                    button_response.rect.top() + 8.0,
                );
                ui.painter().circle_filled(badge_center, 4.0, ACCENT_MUTED);
            }

            ui.scope(|ui| {
                let visuals = &mut ui.style_mut().visuals;
                visuals.window_fill = PANEL_BG;
                visuals.window_stroke = Stroke::new(1.0, BORDER);
                visuals.popup_shadow = egui::epaint::Shadow::NONE;

                egui::popup_below_widget(
                    ui,
                    popup_id,
                    &button_response,
                    PopupCloseBehavior::CloseOnClickOutside,
                    |ui| {
                        ui.set_min_width(290.0);
                        egui::Frame::default()
                            .fill(PANEL_BG)
                            .stroke(Stroke::new(1.0, BORDER))
                            .corner_radius(12.0)
                            .inner_margin(egui::Margin::same(16))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("Filter Options")
                                            .size(18.0)
                                            .strong()
                                            .color(TEXT_MAIN),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(Align::Center),
                                        |ui| {
                                            if ui
                                                .add(
                                                    egui::Button::new(
                                                        RichText::new(icons::X)
                                                            .size(16.0)
                                                            .color(TEXT_MAIN),
                                                    )
                                                    .fill(SURFACE_BG)
                                                    .stroke(Stroke::new(1.0, ACCENT_MUTED))
                                                    .corner_radius(8.0)
                                                    .min_size(Vec2::new(36.0, 36.0)),
                                                )
                                                .clicked()
                                            {
                                                ui.memory_mut(|mem| mem.close_popup());
                                            }
                                        },
                                    );
                                });

                                ui.add_space(12.0);
                                render_filter_option_checkbox(
                                    ui,
                                    &mut self.change_filters.included_in_commit,
                                    &format!("Included in commit ({})", changes.len()),
                                );
                                render_filter_option_checkbox(
                                    ui,
                                    &mut self.change_filters.excluded_from_commit,
                                    "Excluded from commit (0)",
                                );
                                render_filter_option_checkbox(
                                    ui,
                                    &mut self.change_filters.new_files,
                                    &format!(
                                        "New files ({})",
                                        count_changes_by_kind(&changes, ChangeKind::New)
                                    ),
                                );
                                render_filter_option_checkbox(
                                    ui,
                                    &mut self.change_filters.modified_files,
                                    &format!(
                                        "Modified files ({})",
                                        count_changes_by_kind(&changes, ChangeKind::Modified)
                                    ),
                                );
                                render_filter_option_checkbox(
                                    ui,
                                    &mut self.change_filters.deleted_files,
                                    &format!(
                                        "Deleted files ({})",
                                        count_changes_by_kind(&changes, ChangeKind::Deleted)
                                    ),
                                );
                            });
                    },
                );
            });
        }
    }

    fn render_changes_header(&mut self, ui: &mut egui::Ui) {
        egui::Frame::default()
            .fill(SURFACE_BG)
            .stroke(Stroke::new(1.0, BORDER))
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let count = self
                    .current_repo
                    .as_ref()
                    .map(|snapshot| snapshot.changes.len())
                    .unwrap_or(0);
                ui.label(
                    RichText::new(format!("{count} changed files"))
                        .color(TEXT_MAIN)
                        .strong(),
                );
            });
        ui.add_space(8.0);
    }

    fn render_change_row(
        &mut self,
        ui: &mut egui::Ui,
        change: &crate::models::ChangeEntry,
        _index: usize,
    ) {
        let selected = self.selected_change.as_deref() == Some(change.path.as_str());

        let (bg_fill, text_color) = if selected {
            (ACCENT_MUTED, Color32::WHITE)
        } else {
            (Color32::TRANSPARENT, TEXT_MAIN)
        };

        let response = egui::Frame::default()
            .fill(bg_fill)
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(24.0);
                ui.horizontal(|ui| {
                    // Checkbox (visual only for now)
                    let mut checked = true;
                    ui.checkbox(&mut checked, "");

                    // Path
                    let path_text = if change.path.len() > 40 {
                        format!(
                            "...{}",
                            &change.path[change.path.len().saturating_sub(37)..]
                        )
                    } else {
                        change.path.clone()
                    };

                    ui.label(RichText::new(path_text).color(text_color));

                    // Status Icon (Right aligned)
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        let badge_color = status_color(&change.status);
                        let symbol = status_symbol(&change.status);

                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(16.0, 16.0), egui::Sense::hover());
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            symbol,
                            egui::FontId::proportional(12.0),
                            badge_color,
                        );
                    });
                });
            })
            .response
            .interact(egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);

        if response.clicked() {
            self.selected_change = Some(change.path.clone());
        }

        response.context_menu(|ui| {
            ui.set_min_width(280.0);

            if ui.button("Discard Changes...").clicked() {
                self.discard_change(&change.path);
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Ignore File (Add to .gitignore)").clicked() {
                self.ignore_path(&change.path);
                ui.close_menu();
            }

            let ext = std::path::Path::new(&change.path)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("");

            if !ext.is_empty() {
                if ui
                    .button(format!("Ignore All .{} Files (Add to .gitignore)", ext))
                    .clicked()
                {
                    self.ignore_extension(ext);
                    ui.close_menu();
                }
            }

            ui.separator();
            if ui.button("Copy File Path").clicked() {
                if let Some(repo_path) = self.repo_path() {
                    let full_path = repo_path.join(&change.path);
                    ui.ctx().copy_text(full_path.to_string_lossy().to_string());
                    self.status_message = format!("Copied absolute path for '{}'.", change.path);
                    self.error_message.clear();
                }
                ui.close_menu();
            }
            if ui.button("Copy Relative File Path").clicked() {
                ui.ctx().copy_text(change.path.clone());
                self.status_message = format!("Copied relative path for '{}'.", change.path);
                self.error_message.clear();
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Reveal in Finder").clicked() {
                self.reveal_in_finder(&change.path);
                ui.close_menu();
            }
            if ui.button("Open in External Editor").clicked() {
                self.open_in_external_editor(&change.path);
                ui.close_menu();
            }
            if ui.button("Open with Default Program").clicked() {
                if let Some(repo_path) = self.repo_path() {
                    let full_path = repo_path.join(&change.path);
                    match open::that(&full_path) {
                        Ok(_) => {
                            self.status_message =
                                format!("Opened '{}' with the default program.", change.path);
                            self.error_message.clear();
                        }
                        Err(err) => {
                            self.error_message = format!(
                                "Failed to open '{}' with default program: {err}",
                                change.path
                            );
                        }
                    }
                }
                ui.close_menu();
            }
        });
    }

    fn discard_change(&mut self, relative_path: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.error_message = "No repository selected.".to_string();
            return;
        };

        match self.git.discard_change(&repo_path, relative_path) {
            Ok(snapshot) => {
                self.adopt_snapshot(snapshot);
                self.status_message = format!("Discarded changes for '{}'.", relative_path);
                self.error_message.clear();
            }
            Err(err) => {
                self.error_message =
                    format!("Failed to discard changes for '{}': {err}", relative_path);
            }
        }
    }

    fn ignore_path(&mut self, relative_path: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.error_message = "No repository selected.".to_string();
            return;
        };

        let pattern = relative_path.replace('\\', "/");
        match self.git.append_gitignore_pattern(&repo_path, &pattern) {
            Ok(snapshot) => {
                self.adopt_snapshot(snapshot);
                self.status_message = format!("Added '{}' to .gitignore.", relative_path);
                self.error_message.clear();
            }
            Err(err) => {
                self.error_message = format!("Failed to ignore '{}': {err}", relative_path);
            }
        }
    }

    fn ignore_extension(&mut self, ext: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.error_message = "No repository selected.".to_string();
            return;
        };

        let pattern = format!("*.{ext}");
        match self.git.append_gitignore_pattern(&repo_path, &pattern) {
            Ok(snapshot) => {
                self.adopt_snapshot(snapshot);
                self.status_message = format!("Added '{}' to .gitignore.", pattern);
                self.error_message.clear();
            }
            Err(err) => {
                self.error_message = format!("Failed to ignore '{}': {err}", pattern);
            }
        }
    }

    fn reveal_in_finder(&mut self, relative_path: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.error_message = "No repository selected.".to_string();
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
                self.status_message = format!("Revealed '{}' in Finder.", relative_path);
                self.error_message.clear();
            }
            Err(err) => {
                self.error_message = format!("Failed to reveal '{}': {err}", relative_path);
            }
        }
    }

    fn open_in_external_editor(&mut self, relative_path: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.error_message = "No repository selected.".to_string();
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
                self.status_message = format!("Opened '{}' in external editor.", relative_path);
                self.error_message.clear();
            }
            Err(err) => {
                self.error_message = format!(
                    "Failed to open '{}' in external editor: {err}",
                    relative_path
                );
            }
        }
    }

    fn render_stash_row(&mut self, ui: &mut egui::Ui) {
        let stash_count = self
            .current_repo
            .as_ref()
            .map(|repo| repo.stash_count)
            .unwrap_or(0);

        if stash_count == 0 {
            return;
        }

        ui.add_space(8.0);
        let label = if stash_count == 1 {
            "▸ Stashed Changes".to_string()
        } else {
            format!("▸ Stashed Changes ({stash_count})")
        };

        let response = ui.add(
            egui::Button::new(RichText::new(label).color(TEXT_MUTED))
                .fill(SURFACE_BG)
                .stroke(Stroke::new(1.0, BORDER))
                .corner_radius(5.0)
                .min_size(Vec2::new(ui.available_width(), 24.0)),
        );

        if response.clicked() {
            // TODO: Open stash view
        }
    }

    fn render_commit_sidebar(&mut self, ui: &mut egui::Ui) {
        egui::Frame::default()
            .fill(PANEL_BG)
            .inner_margin(egui::Margin::symmetric(8, 8))
            .show(ui, |ui| {
                egui::Frame::default().fill(PANEL_BG).show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_width(ui.available_width());

                        ui.horizontal(|ui| {
                            let (avatar_rect, _) =
                                ui.allocate_exact_size(Vec2::new(24.0, 24.0), egui::Sense::hover());
                            ui.painter().circle_filled(
                                avatar_rect.center(),
                                11.5,
                                Color32::from_rgb(201, 178, 158),
                            );
                            ui.painter().text(
                                avatar_rect.center(),
                                Align2::CENTER_CENTER,
                                "J",
                                egui::FontId::proportional(12.0),
                                Color32::from_rgb(70, 56, 47),
                            );

                            let summary = egui::TextEdit::singleline(&mut self.commit_summary)
                                .desired_width(f32::INFINITY)
                                .hint_text("Summary (required)")
                                .background_color(SURFACE_BG)
                                .margin(egui::Margin::symmetric(6, 4));
                            ui.add_sized([ui.available_width(), 24.0], summary);
                        });

                        ui.add_space(8.0);

                        egui::Frame::default()
                            .fill(SURFACE_BG)
                            .stroke(Stroke::new(1.0, BORDER))
                            .corner_radius(5.0)
                            .inner_margin(egui::Margin::same(0))
                            .show(ui, |ui| {
                                ui.add_sized(
                                    [ui.available_width(), 108.0],
                                    egui::TextEdit::multiline(&mut self.commit_body)
                                        .desired_width(f32::INFINITY)
                                        .hint_text("Description")
                                        .background_color(SURFACE_BG)
                                        .margin(egui::Margin::symmetric(8, 8)),
                                );

                                let separator_y = ui.cursor().top();
                                ui.painter().hline(
                                    ui.min_rect().x_range(),
                                    separator_y,
                                    Stroke::new(1.0, BORDER),
                                );

                                egui::Frame::default()
                                    .fill(SURFACE_BG)
                                    .inner_margin(egui::Margin::symmetric(10, 3))
                                    .show(ui, |ui| {
                                        ui.set_height(22.0);
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = 8.0;

                                            let toolbar_icon =
                                                |ui: &mut egui::Ui, icon: &str, tip: &str| {
                                                    ui.add(
                                                        egui::Button::new(
                                                            RichText::new(icon)
                                                                .size(15.0)
                                                                .color(TEXT_MUTED),
                                                        )
                                                        .fill(Color32::TRANSPARENT)
                                                        .stroke(Stroke::NONE)
                                                        .min_size(Vec2::new(18.0, 18.0)),
                                                    )
                                                    .on_hover_cursor(
                                                        egui::CursorIcon::PointingHand,
                                                    )
                                                    .on_hover_text(tip)
                                                };

                                            if toolbar_icon(ui, icons::SPARKLE, "Generate with AI")
                                                .clicked()
                                            {
                                                self.generate_ai_commit();
                                            }

                                            if toolbar_icon(ui, icons::GEAR, "Commit settings")
                                                .clicked()
                                            {
                                                self.show_settings = true;
                                            }
                                        });
                                    });
                            });

                        if let Some(preview) = &self.ai_preview {
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new(format!("AI: {}", preview.subject))
                                    .small()
                                    .color(TEXT_MUTED),
                            );
                        }

                        self.render_stash_row(ui);
                        ui.add_space(8.0);
                        let branch_label = self
                            .current_repo
                            .as_ref()
                            .map(|snapshot| snapshot.repo.current_branch.clone())
                            .unwrap_or_else(|| "branch".to_string());
                        let commit_button = egui::Button::new(
                            RichText::new(format!("Commit to {branch_label}"))
                                .color(Color32::from_rgb(223, 230, 240))
                                .strong(),
                        )
                        .fill(Color32::from_rgb(58, 96, 194))
                        .stroke(Stroke::NONE)
                        .corner_radius(5.0);
                        if ui
                            .add_sized([ui.available_width(), 24.0], commit_button)
                            .clicked()
                        {
                            self.commit_all();
                        }

                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new("Committed just now")
                                        .size(12.0)
                                        .color(TEXT_MUTED),
                                );
                                ui.label(
                                    RichText::new(truncate_commit_footer(&self.commit_summary))
                                        .size(12.0)
                                        .color(TEXT_MAIN),
                                );
                            });
                            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                let undo = egui::Button::new(
                                    RichText::new("Undo").size(12.0).color(TEXT_MAIN),
                                )
                                .fill(SURFACE_BG)
                                .stroke(Stroke::new(1.0, BORDER))
                                .corner_radius(5.0)
                                .min_size(Vec2::new(42.0, 22.0));
                                let _ = ui.add(undo);
                            });
                        });
                    });
                });
            });
    }

    fn render_workspace(&mut self, ctx: &egui::Context) {
        if self.sidebar_tab == SidebarTab::History {
            self.render_history_workspace(ctx);
            return;
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(BG))
            .show(ctx, |ui| {
                if self.selected_change.is_none() {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("No file selected")
                                .color(TEXT_MUTED)
                                .size(14.0),
                        );
                    });
                    return;
                }

                self.render_diff_header(ui);

                egui::Frame::default()
                    .fill(DIFF_BG)
                    .stroke(Stroke::new(1.0, BORDER))
                    .inner_margin(egui::Margin::same(0))
                    .show(ui, |ui| match self.selected_diff() {
                        Some(diff) if diff.is_binary => {
                            ui.centered_and_justified(|ui| {
                                ui.label(
                                    RichText::new("Binary file changed.")
                                        .color(TEXT_MUTED)
                                        .size(14.0),
                                );
                            });
                        }
                        Some(diff) if diff.diff.trim().is_empty() => {
                            ui.centered_and_justified(|ui| {
                                ui.label(
                                    RichText::new("No diff text available.")
                                        .color(TEXT_MUTED)
                                        .size(14.0),
                                );
                            });
                        }
                        Some(diff) => {
                            render_diff_text(ui, &diff.diff, &diff.path);
                        }
                        None => {
                            ui.centered_and_justified(|ui| {
                                ui.label(
                                    RichText::new("No diff available for this file.")
                                        .color(TEXT_MUTED)
                                        .size(14.0),
                                );
                            });
                        }
                    });
            });
    }

    fn render_history_workspace(&mut self, ctx: &egui::Context) {
        let selected_commit = self
            .selected_commit
            .as_deref()
            .and_then(|oid| {
                self.current_repo
                    .as_ref()
                    .and_then(|repo| repo.history.iter().find(|commit| commit.oid == oid))
            })
            .cloned();

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(BG))
            .show(ctx, |ui| {
                let Some(commit) = selected_commit.as_ref() else {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("Select a commit to view details").color(TEXT_MUTED),
                        );
                    });
                    return;
                };

                egui::TopBottomPanel::top("commit_info")
                    .resizable(false)
                    .frame(egui::Frame::default().fill(SURFACE_BG).inner_margin(12.0))
                    .show_inside(ui, |ui| {
                        ui.add_sized(
                            [ui.available_width(), 24.0],
                            egui::Label::new(
                                RichText::new(&commit.summary)
                                    .color(TEXT_MAIN)
                                    .size(18.0)
                                    .strong(),
                            )
                            .truncate(),
                        );

                        if !commit.body.is_empty() {
                            ui.add_space(4.0);
                            ui.add_sized(
                                [ui.available_width(), 18.0],
                                egui::Label::new(RichText::new(&commit.body).color(TEXT_MUTED))
                                    .truncate(),
                            );
                        }

                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.add_sized(
                                [ui.available_width() - 90.0, 16.0],
                                egui::Label::new(
                                    RichText::new(format!(
                                        "{} committed {}",
                                        commit.author_name, commit.date
                                    ))
                                    .color(TEXT_MUTED),
                                )
                                .truncate(),
                            );
                            ui.label(RichText::new(&commit.short_oid).monospace().color(TEXT_MUTED));
                        });
                    });

                egui::CentralPanel::default()
                    .frame(egui::Frame::default().fill(PANEL_BG).inner_margin(0.0))
                    .show_inside(ui, |ui| {
                        let Some(diffs) = self.commit_diffs.as_ref() else {
                            ui.centered_and_justified(|ui| {
                                ui.spinner();
                            });
                            return;
                        };

                        let existing_selected_path = self.selected_commit_file.clone();
                        let mut next_selected_path = None::<String>;

                        egui::SidePanel::left("commit_file_list")
                            .resizable(true)
                            .min_width(180.0)
                            .default_width(220.0)
                            .frame(
                                egui::Frame::default()
                                    .fill(PANEL_BG)
                                    .stroke(Stroke::new(1.0, BORDER))
                                    .inner_margin(0.0),
                            )
                            .show_inside(ui, |ui| {
                                egui::TopBottomPanel::top("commit_file_list_header")
                                    .resizable(false)
                                    .frame(
                                        egui::Frame::default()
                                            .fill(SURFACE_BG)
                                            .inner_margin(egui::Margin::symmetric(12, 10)),
                                    )
                                    .show_inside(ui, |ui| {
                                        ui.label(
                                            RichText::new(format!("{} changed files", diffs.len()))
                                                .strong()
                                                .color(TEXT_MAIN),
                                        );
                                    });

                                egui::CentralPanel::default().show_inside(ui, |ui| {
                                    egui::ScrollArea::vertical()
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| {
                                            ui.spacing_mut().item_spacing = Vec2::ZERO;

                                            for diff in diffs {
                                                let is_selected = existing_selected_path
                                                    .as_deref()
                                                    == Some(diff.path.as_str());

                                                let response = egui::Frame::default()
                                                    .fill(if is_selected {
                                                        ACCENT_MUTED
                                                    } else {
                                                        Color32::TRANSPARENT
                                                    })
                                                    .inner_margin(egui::Margin::symmetric(10, 8))
                                                    .show(ui, |ui| {
                                                        ui.set_min_height(24.0);
                                                        ui.set_width(ui.available_width());
                                                        ui.add_sized(
                                                            [ui.available_width(), 16.0],
                                                            egui::Label::new(
                                                                RichText::new(&diff.path).color(
                                                                    if is_selected {
                                                                        Color32::WHITE
                                                                    } else {
                                                                        TEXT_MAIN
                                                                    },
                                                                ),
                                                            )
                                                            .truncate(),
                                                        );
                                                    })
                                                    .response
                                                    .interact(egui::Sense::click())
                                                    .on_hover_cursor(
                                                        egui::CursorIcon::PointingHand,
                                                    );

                                                ui.painter().hline(
                                                    response.rect.x_range(),
                                                    response.rect.bottom(),
                                                    Stroke::new(1.0, BORDER),
                                                );

                                                if response.clicked() {
                                                    next_selected_path = Some(diff.path.clone());
                                                }
                                            }
                                        });
                                });
                            });

                        let active_selected_path = next_selected_path
                            .clone()
                            .or(existing_selected_path.clone());

                        if let Some(path) = next_selected_path {
                            self.selected_commit_file = Some(path);
                        }

                        egui::CentralPanel::default()
                            .frame(egui::Frame::default().fill(DIFF_BG).inner_margin(0.0))
                            .show_inside(ui, |ui| {
                                if let Some(selected_path) = active_selected_path.as_deref() {
                                    Self::render_diff_title(ui, selected_path);

                                    egui::CentralPanel::default().show_inside(ui, |ui| {
                                        if let Some(diff) =
                                            diffs.iter().find(|d| d.path == selected_path)
                                        {
                                            if diff.is_binary {
                                                ui.centered_and_justified(|ui| {
                                                    ui.label(
                                                        RichText::new("Binary file changed.")
                                                            .color(TEXT_MUTED),
                                                    );
                                                });
                                            } else if diff.diff.trim().is_empty() {
                                                ui.centered_and_justified(|ui| {
                                                    ui.label(
                                                        RichText::new("No diff text available.")
                                                            .color(TEXT_MUTED),
                                                    );
                                                });
                                            } else {
                                                egui::ScrollArea::both()
                                                    .auto_shrink([false, false])
                                                    .show(ui, |ui| {
                                                        ui.style_mut().spacing.item_spacing =
                                                            Vec2::ZERO;
                                                        render_diff_text(
                                                            ui,
                                                            &diff.diff,
                                                            selected_path,
                                                        );
                                                    });
                                            }
                                        }
                                    });
                                } else {
                                    ui.centered_and_justified(|ui| {
                                        ui.label(
                                            RichText::new("Select a file to view diff")
                                                .color(TEXT_MUTED),
                                        );
                                    });
                                }
                            });
                    });
            });
    }

    fn render_diff_header(&mut self, ui: &mut egui::Ui) {
        let path = self
            .selected_change
            .as_deref()
            .unwrap_or("Select a file from the left panel");
        Self::render_diff_title(ui, path);
    }

    fn render_diff_title(ui: &mut egui::Ui, path: &str) {
        egui::Frame::default()
            .fill(SURFACE_BG)
            .stroke(Stroke::new(1.0, BORDER))
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(path).color(TEXT_MAIN).size(14.0).strong());
                });
            });
    }

    fn render_settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_settings;
        egui::Window::new(RichText::new("Settings").strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_min_width(400.0);
                egui::Frame::default()
                    .fill(SURFACE_BG)
                    .inner_margin(egui::Margin::same(16))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.heading(RichText::new("Application Settings").color(TEXT_MAIN));
                        });
                        ui.add_space(20.0);

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);

                        ui.label(RichText::new("Git").color(TEXT_MUTED).small());
                        ui.label("User Name");
                        ui.text_edit_singleline(&mut self.repo_identity.user_name);
                        ui.label("User Email");
                        ui.text_edit_singleline(&mut self.repo_identity.user_email);
                        ui.label("Default Branch");
                        let default_branch = self
                            .repo_identity
                            .default_branch
                            .get_or_insert_with(String::new);
                        ui.text_edit_singleline(default_branch);
                        let mut pull_rebase = self.repo_identity.pull_rebase.unwrap_or(false);
                        ui.checkbox(&mut pull_rebase, "Use pull.rebase");
                        self.repo_identity.pull_rebase = Some(pull_rebase);
                        if compact_action_button(ui, "Save Git Config").clicked() {
                            self.save_git_config();
                        }

                        ui.add_space(14.0);
                        ui.separator();
                        ui.add_space(8.0);

                        ui.label(RichText::new("AI").color(TEXT_MUTED).small());
                        ui.label("Recent Repositories");
                        self.render_recent_repos_picker(ui);
                        ui.add_space(8.0);
                        ui.label("Endpoint");
                        ui.text_edit_singleline(&mut self.settings.ai.endpoint);
                        ui.label("Model");
                        ui.text_edit_singleline(&mut self.settings.ai.model);
                        ui.label("API Key");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.settings.ai.api_key)
                                .password(true),
                        );
                        ui.label("System Prompt");
                        ui.add(
                            egui::TextEdit::multiline(&mut self.settings.ai.system_prompt)
                                .desired_width(f32::INFINITY)
                                .desired_rows(5),
                        );

                        if compact_action_button(ui, "Save Preferences").clicked() {
                            self.persist_settings();
                            if self.error_message.is_empty() {
                                self.status_message = "App settings saved.".to_string();
                            }
                        }
                    });
            });
        self.show_settings = open;
    }

    fn render_recent_repos_picker(&mut self, ui: &mut egui::Ui) {
        if self.settings.recent_repos.is_empty() {
            ui.label(RichText::new("No recent repositories yet.").color(TEXT_MUTED));
            return;
        }

        let selected_text = self
            .selected_recent_repo
            .and_then(|index| self.settings.recent_repos.get(index))
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "Choose recent repo".to_string());

        egui::ComboBox::from_id_salt("recent_repos_picker")
            .selected_text(selected_text)
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for (index, path) in self.settings.recent_repos.iter().enumerate() {
                    ui.selectable_value(
                        &mut self.selected_recent_repo,
                        Some(index),
                        path.display().to_string(),
                    );
                }
            });
    }
}

impl eframe::App for GitSparkApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let is_window_focused = ctx.input(|input| input.viewport().focused.unwrap_or(input.focused));
        if is_window_focused && !self.last_window_focused && self.current_repo.is_some() {
            self.request_repo_refresh(RepoRefreshReason::Focus);
        }
        self.last_window_focused = is_window_focused;

        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                AppEvent::RepoLoaded(Ok(snapshot)) => {
                    self.adopt_snapshot(snapshot);
                    self.status_message = "Repository loaded.".to_string();
                    self.error_message.clear();
                }
                AppEvent::RepoLoaded(Err(err)) => {
                    self.error_message = format!("Failed to open repository: {err}");
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
                        self.status_message = "Repository refreshed.".to_string();
                    }
                    self.error_message.clear();
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
                        self.error_message = format!("Refresh failed: {err}");
                    } else {
                        self.error_message = err;
                    }
                }
                AppEvent::BranchSwitched(Ok(snapshot), branch) => {
                    self.adopt_snapshot(snapshot);
                    self.status_message = format!("Switched to branch '{branch}'.");
                    self.error_message.clear();
                }
                AppEvent::BranchSwitched(Err(err), _) => {
                    self.error_message = format!("Branch switch failed: {err}");
                }
                AppEvent::BranchMerged(Ok(snapshot), branch) => {
                    self.adopt_snapshot(snapshot);
                    self.status_message = format!("Merged '{branch}'.");
                    self.error_message.clear();
                }
                AppEvent::BranchMerged(Err(err), _) => {
                    self.error_message = format!("Merge failed: {err}");
                }
                AppEvent::CommitCreated(Ok(snapshot)) => {
                    self.adopt_snapshot(snapshot);
                    self.commit_summary.clear();
                    self.commit_body.clear();
                    self.ai_preview = None;
                    self.status_message = "Commit created.".to_string();
                    self.error_message.clear();
                }
                AppEvent::CommitCreated(Err(err)) => {
                    self.error_message = format!("Commit failed: {err}");
                }
                AppEvent::NetworkActionCompleted(Ok(snapshot), action_label) => {
                    self.adopt_snapshot(snapshot);
                    self.status_message = format!("{action_label} complete.");
                    self.error_message.clear();
                }
                AppEvent::NetworkActionCompleted(Err(err), action_label) => {
                    self.error_message = format!("{action_label} failed: {err}");
                }
                AppEvent::AiCommitGenerated(Ok(suggestion)) => {
                    self.commit_summary = suggestion.subject.clone();
                    self.commit_body = suggestion.body.clone();
                    self.ai_preview = Some(suggestion);
                    self.status_message = "Generated commit suggestion.".to_string();
                    self.error_message.clear();
                }
                AppEvent::AiCommitGenerated(Err(err)) => {
                    self.error_message = format!("AI generation failed: {err}");
                }
                AppEvent::CommitDiffLoaded(oid, Ok(diffs)) => {
                    if self.selected_commit.as_deref() == Some(oid.as_str()) {
                        if let Some(first) = diffs.first() {
                            self.selected_commit_file = Some(first.path.clone());
                        }
                        self.commit_diffs = Some(diffs);
                    }
                }
                AppEvent::CommitDiffLoaded(_, Err(err)) => {
                    self.error_message = format!("Failed to load commit details: {err}");
                }
            }
        }

        self.render_menu_bar(ctx);
        self.render_top_bar(ctx);
        self.render_status_bar(ctx);
        self.render_sidebar(ctx);

        match self.main_tab {
            MainTab::Workspace => self.render_workspace(ctx),
        }

        if self.show_settings {
            self.render_settings_window(ctx);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.stop_repo_watch();
        self.persist_settings();
    }
}

impl GitSparkApp {
    fn render_menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu_bar")
            .exact_height(28.0)
            .frame(
                egui::Frame::default()
                    .fill(SURFACE_BG_MUTED)
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .stroke(Stroke::new(0.0, Color32::TRANSPARENT)),
            )
            .show(ctx, |ui| {
                let previous_override = ui.visuals().override_text_color;
                ui.visuals_mut().override_text_color = Some(Color32::from_rgb(235, 240, 246));
                egui::menu::bar(ui, |ui| {
                    ui.menu_button(RichText::new("File").color(Color32::WHITE), |ui| {
                        if ui.button("New Repository...").clicked() {
                            ui.close_menu();
                        }
                        if ui.button("Add Local Repository...").clicked() {
                            self.open_repo_dialog();
                            ui.close_menu();
                        }
                        if ui.button("Clone Repository...").clicked() {
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Options...").clicked() {
                            self.show_settings = true;
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Exit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });

                    ui.menu_button(RichText::new("Edit").color(Color32::WHITE), |ui| {
                        let _ = ui.button("Undo");
                        let _ = ui.button("Redo");
                        ui.separator();
                        let _ = ui.button("Cut");
                        let _ = ui.button("Copy");
                        let _ = ui.button("Paste");
                        let _ = ui.button("Select All");
                    });

                    ui.menu_button(RichText::new("View").color(Color32::WHITE), |ui| {
                        if ui.button("Changes").clicked() {
                            self.sidebar_tab = SidebarTab::Changes;
                            ui.close_menu();
                        }
                        if ui.button("History").clicked() {
                            self.sidebar_tab = SidebarTab::History;
                            ui.close_menu();
                        }
                        ui.separator();
                        let _ = ui.button("Repository List");
                        ui.separator();
                        let _ = ui.button("Toggle Full Screen");
                    });

                    ui.menu_button(RichText::new("Repository").color(Color32::WHITE), |ui| {
                        if ui.button("Push").clicked() {
                            self.push_origin();
                            ui.close_menu();
                        }
                        if ui.button("Pull").clicked() {
                            self.pull_origin();
                            ui.close_menu();
                        }
                        if ui.button("Fetch").clicked() {
                            self.fetch_origin();
                            ui.close_menu();
                        }
                        if ui.button("Remove...").clicked() {
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("View on GitHub").clicked() {
                            ui.close_menu();
                        }
                        if ui.button("Open in Terminal").clicked() {
                            ui.close_menu();
                        }
                        if ui.button("Show in Finder").clicked() {
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Repository Settings...").clicked() {
                            ui.close_menu();
                        }
                    });

                    ui.menu_button(RichText::new("Branch").color(Color32::WHITE), |ui| {
                        let _ = ui.button("New Branch...");
                        let _ = ui.button("Rename Branch...");
                        let _ = ui.button("Delete Branch...");
                        ui.separator();
                        let _ = ui.button("Update from Default Branch");
                        let _ = ui.button("Compare to Branch");
                        let _ = ui.button("Merge into Current Branch...");
                    });

                    ui.menu_button(RichText::new("Help").color(Color32::WHITE), |ui| {
                        let _ = ui.button("Report Issue...");
                        let _ = ui.button("Contact Support...");
                        ui.separator();
                        let _ = ui.button("Show Logs...");
                        ui.separator();
                        let _ = ui.button("About RustTop");
                    });
                });
                ui.visuals_mut().override_text_color = previous_override;
            });
    }
}

impl NetworkAction {
    fn from_snapshot(snapshot: &RepoSnapshot) -> Self {
        if snapshot.repo.behind > 0 {
            Self::Pull
        } else if snapshot.repo.ahead > 0 {
            Self::Push
        } else {
            Self::Fetch
        }
    }

    fn title(self, remote_name: &str) -> String {
        match self {
            Self::Fetch => format!("Fetch {remote_name}"),
            Self::Pull => format!("Pull {remote_name}"),
            Self::Push => format!("Push {remote_name}"),
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Fetch => icons::ARROW_CLOCKWISE,
            Self::Pull => icons::ARROW_DOWN,
            Self::Push => icons::ARROW_UP,
        }
    }
}

fn matches_filter(filter: &str, path: &str) -> bool {
    let filter = filter.trim();
    filter.is_empty()
        || path
            .to_ascii_lowercase()
            .contains(&filter.to_ascii_lowercase())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChangeKind {
    New,
    Modified,
    Deleted,
}

fn infer_change_kind(status: &str) -> Option<ChangeKind> {
    if status.contains('?') || status.contains('A') {
        Some(ChangeKind::New)
    } else if status.contains('M') {
        Some(ChangeKind::Modified)
    } else if status.contains('D') {
        Some(ChangeKind::Deleted)
    } else {
        None
    }
}

fn matches_change_filters(change: &crate::models::ChangeEntry, filters: ChangeFilterOptions) -> bool {
    if filters.active_count() == 0 {
        return true;
    }

    if filters.excluded_from_commit {
        return false;
    }

    if filters.included_in_commit
        && !filters.new_files
        && !filters.modified_files
        && !filters.deleted_files
    {
        return true;
    }

    let kind = infer_change_kind(&change.status);

    (!filters.new_files || kind == Some(ChangeKind::New))
        && (!filters.modified_files || kind == Some(ChangeKind::Modified))
        && (!filters.deleted_files || kind == Some(ChangeKind::Deleted))
}

fn count_changes_by_kind(changes: &[crate::models::ChangeEntry], kind: ChangeKind) -> usize {
    changes
        .iter()
        .filter(|change| infer_change_kind(&change.status) == Some(kind))
        .count()
}

fn render_filter_option_checkbox(ui: &mut egui::Ui, value: &mut bool, label: &str) {
    ui.horizontal(|ui| {
        let mut checkbox = *value;
        let response = ui.checkbox(&mut checkbox, "");
        if response.changed() {
            *value = checkbox;
        }
        ui.label(RichText::new(label).color(TEXT_MAIN).size(12.5));
    });
}

fn status_color(status: &str) -> Color32 {
    if status.contains('?') || status.contains('A') {
        SUCCESS
    } else if status.contains('M') {
        WARNING
    } else if status.contains('D') || status.contains('U') {
        DANGER
    } else {
        TEXT_MUTED
    }
}

fn status_symbol(status: &str) -> &'static str {
    if status.contains('?') || status.contains('A') {
        icons::PLUS
    } else if status.contains('M') {
        icons::DOT_OUTLINE
    } else if status.contains('D') {
        icons::MINUS
    } else if status.contains('U') {
        icons::WARNING
    } else {
        icons::QUESTION
    }
}

fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn render_ahead_behind(ui: &mut egui::Ui, ahead: usize, behind: usize) {
    if ahead == 0 && behind == 0 {
        return;
    }

    ui.add_space(6.0);
    if ahead > 0 {
        ui.label(
            RichText::new(format!("{ahead}{}", icons::ARROW_UP))
                .size(11.0)
                .color(TEXT_MUTED),
        );
    }
    if behind > 0 {
        ui.label(
            RichText::new(format!("{behind}{}", icons::ARROW_DOWN))
                .size(11.0)
                .color(TEXT_MUTED),
        );
    }
}

fn render_toolbar_text_stack(
    ui: &mut egui::Ui,
    description: &str,
    title: &str,
    width: f32,
    trailing_icon: Option<&str>,
) {
    let chevron_width = if trailing_icon.is_some() { 18.0 } else { 0.0 };
    let text_width = (width - chevron_width).max(0.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 52.0), egui::Sense::hover());
    let painter = ui.painter();
    let text_left = rect.left();
    let text_top = rect.top() + 9.0;

    painter.text(
        egui::pos2(text_left, text_top),
        Align2::LEFT_TOP,
        truncate_single_line(description, 30),
        egui::FontId::proportional(10.0),
        TEXT_MUTED,
    );
    painter.text(
        egui::pos2(text_left, text_top + 13.0),
        Align2::LEFT_TOP,
        truncate_single_line(title, 24),
        egui::FontId::proportional(12.5),
        TEXT_MAIN,
    );

    if let Some(icon) = trailing_icon {
        painter.text(
            egui::pos2(rect.left() + text_width + 4.0, rect.top() + 20.0),
            Align2::LEFT_TOP,
            icon,
            egui::FontId::proportional(11.0),
            TEXT_MUTED,
        );
    }
}

fn render_network_text_stack(
    ui: &mut egui::Ui,
    description: &str,
    title: &str,
    ahead: usize,
    behind: usize,
    width: f32,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 52.0), egui::Sense::hover());
    let painter = ui.painter();
    let text_left = rect.left();
    let text_top = rect.top() + 9.0;

    painter.text(
        egui::pos2(text_left, text_top),
        Align2::LEFT_TOP,
        truncate_single_line(description, 26),
        egui::FontId::proportional(10.0),
        TEXT_MUTED,
    );
    painter.text(
        egui::pos2(text_left, text_top + 13.0),
        Align2::LEFT_TOP,
        truncate_single_line(title, 18),
        egui::FontId::proportional(12.5),
        TEXT_MAIN,
    );

    let mut indicator_x = text_left + 92.0;
    if ahead > 0 {
        painter.text(
            egui::pos2(indicator_x, text_top + 13.0),
            Align2::LEFT_TOP,
            format!("{ahead}{}", icons::ARROW_UP),
            egui::FontId::proportional(11.0),
            TEXT_MUTED,
        );
        indicator_x += 22.0;
    }
    if behind > 0 {
        painter.text(
            egui::pos2(indicator_x, text_top + 13.0),
            Align2::LEFT_TOP,
            format!("{behind}{}", icons::ARROW_DOWN),
            egui::FontId::proportional(11.0),
            TEXT_MUTED,
        );
    }
}

fn truncate_single_line(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    let mut chars = trimmed.chars();
    let shortened: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{shortened}...")
    } else {
        shortened
    }
}

fn truncate_commit_footer(summary: &str) -> String {
    let trimmed = summary.trim();
    if trimmed.is_empty() {
        return "No commit message yet".to_string();
    }

    let max = 34;
    let mut chars = trimmed.chars();
    let shortened: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{shortened}...")
    } else {
        shortened
    }
}
