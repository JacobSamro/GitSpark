use eframe::egui::{self, Align, Align2, Color32, PopupCloseBehavior, RichText, Stroke, Vec2};
use egui_phosphor::regular as icons;

use crate::models::ChangeEntry;
use crate::ui::theme::{
    ACCENT_MUTED, BORDER, PANEL_BG, SURFACE_BG, TEXT_MAIN, TEXT_MUTED,
};
use crate::ui::ui_state::ChangeFilterOptions;

pub enum ChangesListAction {
    SelectChange(String),
    DiscardChange(String),
    IgnorePath(String),
    IgnoreExtension(String),
    CopyFullPath(String),
    CopyRelativePath(String),
    RevealInFinder(String),
    OpenInEditor(String),
    OpenWithDefault(String),
}

pub struct ChangesListProps<'a> {
    pub changes: &'a [ChangeEntry],
    pub selected_change: Option<&'a str>,
    pub filter_text: &'a mut String,
    pub change_filters: &'a mut ChangeFilterOptions,
}

pub fn render_changes_list(
    ui: &mut egui::Ui,
    props: &mut ChangesListProps<'_>,
) -> Option<ChangesListAction> {
    let mut action = None;

    render_changes_header(ui, props.changes.len());
    render_filter_bar(ui, props.changes, props.filter_text, props.change_filters);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;

            if props.changes.is_empty() {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("No changed files").color(TEXT_MUTED));
                });
                return;
            }

            for change in props.changes.iter() {
                let filter = props.filter_text.trim();
                if !filter.is_empty()
                    && !change
                        .path
                        .to_ascii_lowercase()
                        .contains(&filter.to_ascii_lowercase())
                {
                    continue;
                }

                if !matches_change_filters(change, *props.change_filters) {
                    continue;
                }

                if let Some(a) = render_change_row(
                    ui,
                    change,
                    props.selected_change == Some(change.path.as_str()),
                ) {
                    action = Some(a);
                }
            }
        });

    action
}

fn render_changes_header(ui: &mut egui::Ui, count: usize) {
    egui::Frame::default()
        .fill(SURFACE_BG)
        .stroke(Stroke::new(1.0, BORDER))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(format!("{count} changed files"))
                    .color(TEXT_MAIN)
                    .strong(),
            );
        });
    ui.add_space(8.0);
}

fn render_filter_bar(
    ui: &mut egui::Ui,
    changes: &[ChangeEntry],
    filter_text: &mut String,
    change_filters: &mut ChangeFilterOptions,
) {
    let popup_id = ui.make_persistent_id("changes_filter_options");
    let active_filter_count = change_filters.active_count();
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

                let edit = egui::TextEdit::singleline(filter_text)
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
                            render_filter_checkbox(
                                ui,
                                &mut change_filters.included_in_commit,
                                &format!("Included in commit ({})", changes.len()),
                            );
                            render_filter_checkbox(
                                ui,
                                &mut change_filters.excluded_from_commit,
                                "Excluded from commit (0)",
                            );
                            render_filter_checkbox(
                                ui,
                                &mut change_filters.new_files,
                                &format!(
                                    "New files ({})",
                                    count_changes_by_kind(changes, ChangeKind::New)
                                ),
                            );
                            render_filter_checkbox(
                                ui,
                                &mut change_filters.modified_files,
                                &format!(
                                    "Modified files ({})",
                                    count_changes_by_kind(changes, ChangeKind::Modified)
                                ),
                            );
                            render_filter_checkbox(
                                ui,
                                &mut change_filters.deleted_files,
                                &format!(
                                    "Deleted files ({})",
                                    count_changes_by_kind(changes, ChangeKind::Deleted)
                                ),
                            );
                        });
                },
            );
        });
    }
}

fn render_change_row(
    ui: &mut egui::Ui,
    change: &ChangeEntry,
    selected: bool,
) -> Option<ChangesListAction> {
    let mut action = None;

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
                let mut checked = true;
                ui.push_id(("change_row", &change.path), |ui| {
                    ui.checkbox(&mut checked, "");
                });

                let path_text = if change.path.len() > 40 {
                    format!(
                        "...{}",
                        &change.path[change.path.len().saturating_sub(37)..]
                    )
                } else {
                    change.path.clone()
                };

                ui.label(RichText::new(path_text).color(text_color));

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    let badge_color = status_color(&change.status);
                    let symbol = status_symbol(&change.status);

                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(16.0, 16.0), egui::Sense::hover());
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
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
        action = Some(ChangesListAction::SelectChange(change.path.clone()));
    }

    let path = change.path.clone();
    response.context_menu(|ui| {
        ui.set_min_width(280.0);

        if ui.button("Discard Changes...").clicked() {
            action = Some(ChangesListAction::DiscardChange(path.clone()));
            ui.close_menu();
        }
        ui.separator();
        if ui.button("Ignore File (Add to .gitignore)").clicked() {
            action = Some(ChangesListAction::IgnorePath(path.clone()));
            ui.close_menu();
        }

        let ext = std::path::Path::new(&path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        if !ext.is_empty() {
            if ui
                .button(format!("Ignore All .{ext} Files (Add to .gitignore)"))
                .clicked()
            {
                action = Some(ChangesListAction::IgnoreExtension(ext));
                ui.close_menu();
            }
        }

        ui.separator();
        if ui.button("Copy File Path").clicked() {
            action = Some(ChangesListAction::CopyFullPath(path.clone()));
            ui.close_menu();
        }
        if ui.button("Copy Relative File Path").clicked() {
            action = Some(ChangesListAction::CopyRelativePath(path.clone()));
            ui.close_menu();
        }
        ui.separator();
        if ui.button("Reveal in Finder").clicked() {
            action = Some(ChangesListAction::RevealInFinder(path.clone()));
            ui.close_menu();
        }
        if ui.button("Open in External Editor").clicked() {
            action = Some(ChangesListAction::OpenInEditor(path.clone()));
            ui.close_menu();
        }
        if ui.button("Open with Default Program").clicked() {
            action = Some(ChangesListAction::OpenWithDefault(path.clone()));
            ui.close_menu();
        }
    });

    action
}

fn render_filter_checkbox(ui: &mut egui::Ui, value: &mut bool, label: &str) {
    ui.horizontal(|ui| {
        let mut checkbox = *value;
        let response = ui
            .push_id(label, |ui| ui.checkbox(&mut checkbox, ""))
            .inner;
        if response.changed() {
            *value = checkbox;
        }
        ui.label(RichText::new(label).color(TEXT_MAIN).size(12.5));
    });
}

// --- Helper functions ---

use crate::ui::theme::{DANGER, SUCCESS, WARNING};

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

fn matches_change_filters(change: &ChangeEntry, filters: ChangeFilterOptions) -> bool {
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
    let any_type_filter = filters.new_files || filters.modified_files || filters.deleted_files;

    if !any_type_filter {
        return true;
    }

    (filters.new_files && kind == Some(ChangeKind::New))
        || (filters.modified_files && kind == Some(ChangeKind::Modified))
        || (filters.deleted_files && kind == Some(ChangeKind::Deleted))
}

fn count_changes_by_kind(changes: &[ChangeEntry], kind: ChangeKind) -> usize {
    changes
        .iter()
        .filter(|change| infer_change_kind(&change.status) == Some(kind))
        .count()
}
