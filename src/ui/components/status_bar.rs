use eframe::egui::{self, RichText};

use crate::ui::theme::{DANGER, PANEL_BG, STATUS_BAR_HEIGHT, TEXT_MUTED};

pub fn render_status_bar(ctx: &egui::Context, status_message: &str, error_message: &str) {
    egui::TopBottomPanel::bottom("status_bar")
        .exact_height(STATUS_BAR_HEIGHT)
        .show_separator_line(false)
        .frame(
            egui::Frame::default()
                .fill(PANEL_BG)
                .inner_margin(egui::Margin::same(6)),
        )
        .show(ctx, |ui| {
            let text = if !error_message.is_empty() {
                RichText::new(error_message).color(DANGER)
            } else {
                RichText::new(status_message).color(TEXT_MUTED)
            };
            ui.label(text);
        });
}
