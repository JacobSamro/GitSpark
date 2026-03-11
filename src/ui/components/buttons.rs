use crate::ui::app::SidebarTab;
use crate::ui::theme::{
    ACCENT, BORDER, PANEL_BG, SURFACE_BG, SURFACE_BG_MUTED, TEXT_MAIN, TEXT_MUTED,
};
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use egui_phosphor::regular as icons;

pub fn icon_button(ui: &mut egui::Ui, icon: &str, tooltip: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(icon).color(TEXT_MAIN).size(14.0))
            .fill(SURFACE_BG)
            .stroke(Stroke::new(1.0, BORDER))
            .corner_radius(4.0)
            .min_size(Vec2::new(28.0, 28.0)),
    )
    .on_hover_text(tooltip)
}

pub fn compact_action_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(label).size(12.5).color(TEXT_MAIN))
            .fill(SURFACE_BG_MUTED)
            .stroke(Stroke::new(1.0, BORDER))
            .corner_radius(4.0),
    )
}

pub fn tab_button(ui: &mut egui::Ui, value: &mut SidebarTab, tab: SidebarTab, label: &str) {
    let active = *value == tab;
    let response = ui.add_sized(
        [110.0, 30.0],
        egui::Button::new(
            RichText::new(label)
                .color(if active { TEXT_MAIN } else { TEXT_MUTED })
                .strong(),
        )
        .fill(if active { SURFACE_BG } else { PANEL_BG })
        .stroke(Stroke::new(0.0, Color32::TRANSPARENT))
        .corner_radius(0.0),
    );

    if active {
        let underline_rect = egui::Rect::from_min_max(
            response.rect.left_bottom() - Vec2::new(0.0, 2.0),
            response.rect.right_bottom() + Vec2::new(0.0, 1.0),
        );
        ui.painter().rect_filled(underline_rect, 0.0, ACCENT);
    }

    if response.clicked() {
        *value = tab;
    }
}
