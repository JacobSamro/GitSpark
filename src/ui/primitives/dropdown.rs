use eframe::egui::{self, Align, Align2, Color32, PopupCloseBehavior, RichText, Stroke, Vec2};
use egui_phosphor::regular as icons;

use crate::ui::theme::{
    ACCENT_MUTED, BORDER, PANEL_BG, SURFACE_BG, SURFACE_BG_MUTED, TEXT_MAIN, TEXT_MUTED,
    color_with_alpha,
};

pub fn styled_dropdown(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash,
    selected_text: &str,
    width: f32,
    popup_min_width: f32,
    add_popup_contents: impl FnOnce(&mut egui::Ui),
) {
    let popup_id = ui.make_persistent_id(id_salt);
    let height = 34.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::click());
    ui.painter().rect_filled(rect, 8.0, SURFACE_BG_MUTED);
    ui.painter().text(
        rect.left_center() + Vec2::new(12.0, 0.0),
        Align2::LEFT_CENTER,
        selected_text,
        egui::FontId::proportional(12.0),
        TEXT_MAIN,
    );
    ui.painter().text(
        rect.right_center() - Vec2::new(12.0, 0.0),
        Align2::RIGHT_CENTER,
        icons::CARET_DOWN,
        egui::FontId::proportional(12.0),
        TEXT_MUTED,
    );
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);

    if response.clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(popup_id));
    }

    ui.scope(|ui| {
        let visuals = &mut ui.style_mut().visuals;
        visuals.window_fill = SURFACE_BG_MUTED;
        visuals.window_stroke = Stroke::NONE;
        visuals.popup_shadow = egui::epaint::Shadow::NONE;

        egui::popup_below_widget(
            ui,
            popup_id,
            &response,
            PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                ui.set_min_width(width.max(popup_min_width));
                egui::Frame::default()
                    .fill(SURFACE_BG_MUTED)
                    .stroke(Stroke::new(1.0, color_with_alpha(BORDER, 210.0)))
                    .corner_radius(0.0)
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        add_popup_contents(ui);
                    });
            },
        );
    });
}

pub fn dropdown_row(ui: &mut egui::Ui, label: &str, is_selected: bool) -> egui::Response {
    let response = egui::Frame::default()
        .fill(if is_selected {
            color_with_alpha(ACCENT_MUTED, 46.0)
        } else {
            Color32::TRANSPARENT
        })
        .stroke(Stroke::NONE)
        .corner_radius(0.0)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(label)
                    .color(if is_selected {
                        Color32::WHITE
                    } else {
                        TEXT_MAIN
                    })
                    .size(12.0),
            );
        })
        .response
        .interact(egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);

    if response.hovered() && !is_selected {
        ui.painter().rect_filled(response.rect, 0.0, SURFACE_BG);
        ui.painter().text(
            response.rect.left_center() + Vec2::new(10.0, 0.0),
            Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(12.0),
            TEXT_MAIN,
        );
    }

    response
}

pub fn dropdown_trigger(
    ui: &mut egui::Ui,
    icon: &str,
    description: &str,
    title: &str,
    width: f32,
) -> egui::Response {
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
                
                // Text stack
                let text_width = width - 76.0;
                ui.allocate_ui_with_layout(
                     Vec2::new(text_width, 52.0),
                     egui::Layout::left_to_right(Align::Center),
                     |ui| {
                         ui.vertical(|ui| {
                             ui.add_space(10.0);
                             ui.label(
                                 RichText::new(description)
                                     .color(TEXT_MUTED)
                                     .size(10.0),
                             );
                             ui.label(
                                 RichText::new(title)
                                     .color(TEXT_MAIN)
                                     .size(13.0)
                                     .strong(),
                             );
                         });
                     },
                );
                
                ui.label(RichText::new(icons::CARET_DOWN).color(TEXT_MUTED).size(12.0));
            });
        })
        .response
        .interact(egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
        
    if response.hovered() {
        ui.painter().rect_filled(
            response.rect,
            0.0,
            color_with_alpha(SURFACE_BG_MUTED, 50.0),
        );
    }
    
    response
}

pub fn toolbar_dropdown(
    ui: &mut egui::Ui,
    id_source: &str,
    width: f32,
    trigger_response: &egui::Response,
    add_popup_contents: impl FnOnce(&mut egui::Ui),
) {
    let popup_id = ui.make_persistent_id(id_source);

    if trigger_response.clicked() {
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
            trigger_response,
            PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                ui.set_min_width(width.max(260.0));
                egui::Frame::default()
                    .fill(PANEL_BG)
                    .stroke(Stroke::new(1.0, BORDER))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        add_popup_contents(ui);
                    });
            },
        );
    });
}

pub fn settings_field_frame<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    egui::Frame::default()
        .fill(SURFACE_BG_MUTED)
        .stroke(Stroke::new(1.0, color_with_alpha(BORDER, 210.0)))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::same(1))
        .show(ui, add_contents)
}
