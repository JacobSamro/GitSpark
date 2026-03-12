use eframe::egui::{self, Align, Color32, RichText, Stroke, Vec2};
use egui_phosphor::regular as icons;

use crate::models::{CommitInfo, DiffEntry};
use crate::ui::components::diff::render_diff_text_readonly;
use crate::ui::theme::{
    ACCENT_MUTED, BG, BORDER, DIFF_BG, PANEL_BG, SURFACE_BG, TEXT_MAIN, TEXT_MUTED,
};

pub struct HistoryViewerProps<'a> {
    pub selected_commit: Option<&'a CommitInfo>,
    pub commit_diffs: Option<&'a [DiffEntry]>,
    pub selected_commit_file: Option<&'a str>,
}

/// Returns the path of a newly selected commit file, if any.
pub fn render_history_viewer(
    ctx: &egui::Context,
    props: &HistoryViewerProps<'_>,
) -> Option<String> {
    let mut selected_file_action = None;

    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(BG))
        .show(ctx, |ui| {
            let Some(commit) = props.selected_commit else {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        RichText::new("Select a commit to view details").color(TEXT_MUTED),
                    );
                });
                return;
            };

            // Commit info header
            egui::TopBottomPanel::top("commit_info")
                .resizable(false)
                .show_separator_line(false)
                .frame(egui::Frame::default().fill(SURFACE_BG).inner_margin(12.0))
                .show_inside(ui, |ui| {
                    ui.with_layout(egui::Layout::left_to_right(Align::Min).with_main_wrap(true), |ui| {
                        ui.set_width(ui.available_width());
                        ui.add(
                            egui::Label::new(
                                RichText::new(&commit.summary)
                                    .color(TEXT_MAIN)
                                    .size(18.0)
                                    .strong(),
                            )
                            .truncate(),
                        );
                    });

                    if !commit.body.is_empty() {
                        ui.add_space(4.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(&commit.body).color(TEXT_MUTED),
                            )
                            .truncate(),
                        );
                    }

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "{} committed {}",
                                commit.author_name, commit.date
                            ))
                            .color(TEXT_MUTED),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(&commit.short_oid)
                                .monospace()
                                .color(TEXT_MUTED),
                        );
                        let copy_btn = ui.add(
                            egui::Button::new(
                                RichText::new(icons::COPY).size(13.0).color(TEXT_MUTED),
                            )
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::NONE)
                            .min_size(Vec2::new(20.0, 20.0)),
                        ).on_hover_text("Copy commit hash");
                        if copy_btn.clicked() {
                            ui.ctx().copy_text(commit.oid.clone());
                        }
                    });
                });

            // Diff content
            egui::CentralPanel::default()
                .frame(egui::Frame::default().fill(PANEL_BG).inner_margin(0.0))
                .show_inside(ui, |ui| {
                    let Some(diffs) = props.commit_diffs else {
                        ui.centered_and_justified(|ui| {
                            ui.spinner();
                        });
                        return;
                    };

                    let existing_selected_path = props.selected_commit_file;
                    let mut next_selected_path = None::<String>;

                    // File list sidebar
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
                        .show_separator_line(false)
                        .show_inside(ui, |ui| {
                            egui::TopBottomPanel::top("commit_file_list_header")
                                .resizable(false)
                                .show_separator_line(false)
                                .frame(
                                    egui::Frame::default()
                                        .fill(SURFACE_BG)
                                        .inner_margin(egui::Margin::symmetric(12, 10)),
                                )
                                .show_inside(ui, |ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "{} changed files",
                                            diffs.len()
                                        ))
                                        .strong()
                                        .color(TEXT_MAIN),
                                    );
                                });

                            egui::CentralPanel::default()
                                .frame(
                                    egui::Frame::default()
                                        .fill(PANEL_BG)
                                        .inner_margin(0.0),
                                )
                                .show_inside(ui, |ui| {
                                    egui::ScrollArea::vertical()
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| {
                                            ui.spacing_mut().item_spacing = Vec2::ZERO;

                                            for diff in diffs {
                                                let is_selected = existing_selected_path
                                                    == Some(diff.path.as_str());

                                                let response = egui::Frame::default()
                                                    .fill(if is_selected {
                                                        ACCENT_MUTED
                                                    } else {
                                                        Color32::TRANSPARENT
                                                    })
                                                    .inner_margin(egui::Margin::symmetric(10, 5))
                                                    .show(ui, |ui| {
                                                        ui.set_width(ui.available_width());
                                                        ui.allocate_ui_with_layout(
                                                            Vec2::new(
                                                                ui.available_width(),
                                                                16.0,
                                                            ),
                                                            egui::Layout::left_to_right(
                                                                Align::Center,
                                                            ),
                                                            |ui| {
                                                                ui.add(
                                                                    egui::Label::new(
                                                                        RichText::new(
                                                                            truncate_single_line(
                                                                                &diff.path, 44,
                                                                            ),
                                                                        )
                                                                        .color(if is_selected {
                                                                            Color32::WHITE
                                                                        } else {
                                                                            TEXT_MAIN
                                                                        }),
                                                                    )
                                                                    .truncate(),
                                                                );
                                                            },
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
                                                    next_selected_path =
                                                        Some(diff.path.clone());
                                                }
                                            }
                                        });
                                });
                        });

                    let active_selected_path = next_selected_path
                        .clone()
                        .or_else(|| existing_selected_path.map(String::from));

                    if next_selected_path.is_some() {
                        selected_file_action = next_selected_path;
                    }

                    // Diff display
                    egui::CentralPanel::default()
                        .frame(
                            egui::Frame::default().fill(DIFF_BG).inner_margin(0.0),
                        )
                        .show_inside(ui, |ui| {
                            if let Some(selected_path) = active_selected_path.as_deref() {
                                render_diff_title(ui, selected_path);

                                egui::CentralPanel::default()
                                    .frame(
                                        egui::Frame::default()
                                            .fill(DIFF_BG)
                                            .inner_margin(0.0),
                                    )
                                    .show_inside(ui, |ui| {
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
                                                        RichText::new(
                                                            "No diff text available.",
                                                        )
                                                        .color(TEXT_MUTED),
                                                    );
                                                });
                                            } else {
                                                egui::ScrollArea::both()
                                                    .auto_shrink([false, false])
                                                    .show(ui, |ui| {
                                                        ui.style_mut().spacing.item_spacing =
                                                            Vec2::ZERO;
                                                        render_diff_text_readonly(
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

    selected_file_action
}

fn render_diff_title(ui: &mut egui::Ui, path: &str) {
    egui::Frame::default()
        .fill(SURFACE_BG)
        .stroke(Stroke::new(1.0, BORDER))
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(
                egui::Label::new(RichText::new(path).color(TEXT_MAIN).size(14.0).strong())
                    .truncate(),
            );
        });
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
