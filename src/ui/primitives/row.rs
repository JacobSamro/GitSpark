use eframe::egui::{self, Align, Color32, RichText, Stroke, Vec2};

use crate::ui::theme::{
    ACCENT_MUTED, BORDER, TEXT_MAIN, TEXT_MUTED, color_with_alpha,
};

pub fn selectable_row(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    selected: bool,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    ui.push_id(id, |ui| {
        let response = egui::Frame::default()
            .fill(if selected {
                ACCENT_MUTED
            } else {
                Color32::TRANSPARENT
            })
            .inner_margin(egui::Margin::symmetric(10, 5))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                add_contents(ui);
            })
            .response
            .interact(egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);

        if response.hovered() && !selected {
            ui.painter().rect_filled(
                response.rect,
                0.0,
                color_with_alpha(ACCENT_MUTED, 30.0),
            );
        }

        ui.painter().hline(
            response.rect.x_range(),
            response.rect.bottom(),
            Stroke::new(1.0, BORDER),
        );

        response
    })
    .inner
}

pub fn file_row(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    path: &str,
    status_icon: &str,
    status_color: Color32,
    selected: bool,
) -> egui::Response {
    selectable_row(ui, id, selected, |ui| {
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), 16.0),
            egui::Layout::left_to_right(Align::Center),
            |ui| {
                ui.label(RichText::new(status_icon).color(status_color));
                ui.add_space(4.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(path).color(if selected {
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
}

pub fn commit_row(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    summary: &str,
    author: &str,
    date: &str,
    is_head: bool,
    selected: bool,
) -> egui::Response {
    selectable_row(ui, id, selected, |ui| {
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), 28.0),
            egui::Layout::top_down(Align::Min),
            |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(summary)
                                .color(if selected {
                                    Color32::WHITE
                                } else {
                                    TEXT_MAIN
                                })
                                .strong(),
                        )
                        .truncate(),
                    );
                    if is_head {
                        ui.label(
                            RichText::new(" HEAD")
                                .small()
                                .color(ACCENT_MUTED),
                        );
                    }
                });
                ui.label(
                    RichText::new(format!("{author} - {date}"))
                        .small()
                        .color(TEXT_MUTED),
                );
            },
        );
    })
}

pub fn settings_nav_row(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    subtitle: &str,
    active: bool,
) -> egui::Response {
    let fill = if active {
        color_with_alpha(ACCENT_MUTED, 56.0)
    } else {
        Color32::TRANSPARENT
    };
    let response = egui::Frame::default()
        .fill(fill)
        .stroke(Stroke::NONE)
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(10, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(icon)
                        .color(if active { Color32::WHITE } else { TEXT_MUTED })
                        .size(14.0),
                );
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(title)
                            .color(if active { Color32::WHITE } else { TEXT_MAIN })
                            .size(13.0)
                            .strong(),
                    );
                    ui.label(
                        RichText::new(subtitle)
                            .color(if active {
                                Color32::from_gray(215)
                            } else {
                                TEXT_MUTED
                            })
                            .size(10.0),
                    );
                });
            });
        })
        .response
        .interact(egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);

    ui.add_space(4.0);
    response
}
