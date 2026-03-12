use std::path::PathBuf;

use eframe::egui::{self, Align, Color32, RichText, Stroke, Vec2};
use egui_phosphor::regular as icons;

use crate::ui::components::changes_list::{self, ChangesListAction, ChangesListProps};
use crate::ui::components::commit::{self, CommitPanelAction, CommitPanelProps};
use crate::ui::components::history_list::{self, HistoryListProps};
use crate::ui::primitives::button::tab_button;
use crate::ui::theme::{
    ACCENT_MUTED, BORDER, PANEL_BG, SURFACE_BG, SURFACE_BG_MUTED, TEXT_MAIN, TEXT_MUTED,
};
use crate::ui::ui_state::{ChangeFilterOptions, SidebarTab};

use crate::models::{ChangeEntry, CommitInfo, CommitSuggestion};

pub enum SidebarAction {
    // Repo selector actions
    OpenRepoDialog,
    OpenRepo(PathBuf),
    HideRepoSelector,

    // Changes list actions
    ChangesListAction(ChangesListAction),

    // History list actions
    SelectCommit(String),

    // Commit panel actions
    GenerateAiCommit,
    ShowSettings,
    CommitAll,
}

pub struct SidebarProps<'a> {
    // Navigation
    pub sidebar_tab: &'a mut SidebarTab,
    pub show_repo_selector: bool,

    // Repo data
    pub has_snapshot: bool,
    pub current_repo_name: Option<&'a str>,
    pub current_branch: Option<&'a str>,
    pub stash_count: usize,

    // Changes
    pub changes: &'a [ChangeEntry],
    pub selected_change: Option<&'a str>,
    pub filter_text: &'a mut String,
    pub change_filters: &'a mut ChangeFilterOptions,

    // History
    pub history: &'a [CommitInfo],
    pub selected_commit: Option<&'a str>,

    // Commit
    pub commit_summary: &'a mut String,
    pub commit_body: &'a mut String,
    pub ai_in_flight: bool,
    pub ai_preview: Option<&'a CommitSuggestion>,
    pub avatar_letter: &'a str,

    // Repo selector
    pub recent_repos: &'a [PathBuf],
    pub current_repo_path: Option<&'a PathBuf>,
    pub repo_filter_text: &'a mut String,
}

pub fn render_sidebar(
    ctx: &egui::Context,
    props: &mut SidebarProps<'_>,
) -> Option<SidebarAction> {
    let mut action = None;

    egui::SidePanel::left("sidebar")
        .resizable(true)
        .default_width(260.0)
        .min_width(220.0)
        .show_separator_line(false)
        .frame(
            egui::Frame::default()
                .fill(PANEL_BG)
                .inner_margin(egui::Margin::same(0)),
        )
        .show(ctx, |ui| {
            if props.show_repo_selector {
                action = render_repository_overlay(ui, props);
                return;
            }

            render_sidebar_tabs(ui, props.sidebar_tab);

            if props.has_snapshot {
                match *props.sidebar_tab {
                    SidebarTab::Changes => {
                        // Commit area at the bottom
                        egui::TopBottomPanel::bottom("commit_area_panel")
                            .resizable(false)
                            .min_height(170.0)
                            .show_separator_line(false)
                            .frame(
                                egui::Frame::default()
                                    .fill(PANEL_BG)
                                    .inner_margin(egui::Margin::symmetric(0, 4))
                                    .stroke(Stroke::new(1.0, BORDER)),
                            )
                            .show_inside(ui, |ui| {
                                let branch_label = props
                                    .current_branch
                                    .unwrap_or("branch");
                                let mut commit_props = CommitPanelProps {
                                    summary: props.commit_summary,
                                    body: props.commit_body,
                                    ai_in_flight: props.ai_in_flight,
                                    ai_preview: props.ai_preview,
                                    branch_label,
                                    stash_count: props.stash_count,
                                    avatar_letter: props.avatar_letter,
                                };
                                let output = commit::render_commit_panel(ui, &mut commit_props);
                                if let Some(commit_action) = output.action {
                                    action = Some(match commit_action {
                                        CommitPanelAction::GenerateAiCommit => {
                                            SidebarAction::GenerateAiCommit
                                        }
                                        CommitPanelAction::ShowSettings => {
                                            SidebarAction::ShowSettings
                                        }
                                        CommitPanelAction::CommitAll => SidebarAction::CommitAll,
                                    });
                                }
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
                                let mut changes_props = ChangesListProps {
                                    changes: props.changes,
                                    selected_change: props.selected_change,
                                    filter_text: props.filter_text,
                                    change_filters: props.change_filters,
                                };
                                if let Some(changes_action) =
                                    changes_list::render_changes_list(ui, &mut changes_props)
                                {
                                    action =
                                        Some(SidebarAction::ChangesListAction(changes_action));
                                }
                            });
                    }
                    SidebarTab::History => {
                        let history_props = HistoryListProps {
                            history: props.history,
                            selected_commit: props.selected_commit,
                        };
                        if let Some(oid) = history_list::render_history_list(ui, &history_props) {
                            action = Some(SidebarAction::SelectCommit(oid));
                        }
                    }
                }
            } else {
                render_no_repo_message(ui, &mut action);
            }
        });

    action
}

fn render_sidebar_tabs(ui: &mut egui::Ui, sidebar_tab: &mut SidebarTab) {
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
                    sidebar_tab,
                    SidebarTab::Changes,
                    "Changes",
                    tab_width,
                );
                tab_button(
                    ui,
                    sidebar_tab,
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

fn render_no_repo_message(ui: &mut egui::Ui, action: &mut Option<SidebarAction>) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
        egui::Frame::default()
            .fill(SURFACE_BG_MUTED)
            .inner_margin(egui::Margin::same(12))
            .stroke(Stroke::new(1.0, BORDER))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("No repository loaded")
                        .color(TEXT_MAIN)
                        .strong(),
                );
                ui.label(
                    RichText::new(
                        "Use the + button in the header or open a repository to get started.",
                    )
                    .color(TEXT_MUTED),
                );

                ui.add_space(10.0);
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("Open Repository").color(Color32::WHITE),
                        )
                        .fill(ACCENT_MUTED)
                        .stroke(Stroke::NONE)
                        .corner_radius(6.0)
                        .min_size(Vec2::new(140.0, 32.0)),
                    )
                    .clicked()
                {
                    *action = Some(SidebarAction::OpenRepoDialog);
                }
            });
    });
}

fn render_repository_overlay(
    ui: &mut egui::Ui,
    props: &mut SidebarProps<'_>,
) -> Option<SidebarAction> {
    let mut action = None;

    egui::Frame::default()
        .fill(PANEL_BG)
        .inner_margin(egui::Margin::same(0))
        .show(ui, |ui| {
            // Header
            egui::TopBottomPanel::top("repo_selector_header")
                .resizable(false)
                .show_separator_line(false)
                .frame(
                    egui::Frame::default()
                        .fill(PANEL_BG)
                        .inner_margin(egui::Margin::symmetric(12, 10)),
                )
                .show_inside(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(icons::FOLDER_NOTCH_OPEN).color(TEXT_MUTED),
                        );
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
                                    props.current_repo_name.unwrap_or("Choose repository"),
                                )
                                .strong()
                                .color(TEXT_MAIN),
                            );
                        });
                        ui.with_layout(
                            egui::Layout::right_to_left(Align::Center),
                            |ui| {
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(icons::CARET_UP)
                                                .color(TEXT_MUTED),
                                        )
                                        .fill(Color32::TRANSPARENT)
                                        .stroke(Stroke::NONE)
                                        .min_size(Vec2::new(20.0, 20.0)),
                                    )
                                    .clicked()
                                {
                                    action = Some(SidebarAction::HideRepoSelector);
                                }
                            },
                        );
                    });
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        let filter =
                            egui::TextEdit::singleline(props.repo_filter_text)
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
                            action = Some(SidebarAction::OpenRepoDialog);
                        }
                    });
                });

            // Repo list
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
                            if props.recent_repos.is_empty() {
                                ui.add_space(12.0);
                                ui.label(
                                    RichText::new("No recent repositories")
                                        .color(TEXT_MUTED),
                                );
                                return;
                            }

                            for path in props.recent_repos.iter() {
                                let is_current = props
                                    .current_repo_path
                                    .map(|p| p == path)
                                    .unwrap_or(false);
                                let repo_name = path
                                    .file_name()
                                    .and_then(|name| name.to_str())
                                    .unwrap_or("Repository");
                                let filter = props.repo_filter_text.trim();
                                if !filter.is_empty()
                                    && !repo_name
                                        .to_ascii_lowercase()
                                        .contains(&filter.to_ascii_lowercase())
                                    && !path
                                        .display()
                                        .to_string()
                                        .to_ascii_lowercase()
                                        .contains(&filter.to_ascii_lowercase())
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
                                    action =
                                        Some(SidebarAction::OpenRepo(path.clone()));
                                }
                            }
                        });
                });
        });

    action
}
