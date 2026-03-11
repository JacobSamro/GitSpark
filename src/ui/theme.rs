use eframe::egui::{self, Color32, Stroke, TextStyle};

pub const BG: Color32 = Color32::from_rgb(18, 22, 29);
pub const PANEL_BG: Color32 = Color32::from_rgb(24, 29, 38);
pub const SURFACE_BG: Color32 = Color32::from_rgb(31, 37, 47);
pub const SURFACE_BG_ALT: Color32 = Color32::from_rgb(34, 40, 51);
pub const SURFACE_BG_MUTED: Color32 = Color32::from_rgb(27, 32, 41);
pub const BORDER: Color32 = Color32::from_rgb(56, 63, 76);
pub const TEXT_MAIN: Color32 = Color32::from_rgb(221, 226, 232);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(146, 155, 168);
pub const ACCENT: Color32 = Color32::from_rgb(53, 105, 220);
pub const ACCENT_MUTED: Color32 = Color32::from_rgb(44, 77, 134);
pub const SUCCESS: Color32 = Color32::from_rgb(78, 168, 94);
pub const WARNING: Color32 = Color32::from_rgb(219, 180, 51);
pub const DANGER: Color32 = Color32::from_rgb(212, 83, 84);
pub const DIFF_BG: Color32 = Color32::from_rgb(17, 31, 20);

pub fn configure_visuals(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = BG;
    visuals.widgets.noninteractive.bg_fill = SURFACE_BG;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.bg_fill = SURFACE_BG;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.hovered.bg_fill = SURFACE_BG_ALT;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_MUTED);
    visuals.widgets.active.bg_fill = SURFACE_BG_ALT;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.selection.bg_fill = ACCENT_MUTED;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.override_text_color = Some(TEXT_MAIN);
    visuals.extreme_bg_color = SURFACE_BG_MUTED;
    visuals.faint_bg_color = SURFACE_BG_MUTED;
    visuals.code_bg_color = DIFF_BG;
    visuals.window_shadow = egui::epaint::Shadow::NONE;
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::Vec2::new(8.0, 6.0);
    style.spacing.button_padding = egui::Vec2::new(10.0, 6.0);
    style.spacing.indent = 14.0;
    style.visuals.window_corner_radius = 6.0.into();
    style.visuals.menu_corner_radius = 6.0.into();
    style
        .text_styles
        .insert(TextStyle::Heading, egui::FontId::proportional(18.0));
    style
        .text_styles
        .insert(TextStyle::Body, egui::FontId::proportional(13.5));
    style
        .text_styles
        .insert(TextStyle::Monospace, egui::FontId::monospace(13.0));
    ctx.set_style(style);
}
