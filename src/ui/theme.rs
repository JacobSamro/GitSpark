use eframe::egui::{self, Color32, Stroke, TextStyle};

pub const BG: Color32 = Color32::from_rgb(13, 17, 23); // #0d1117 Main window
pub const PANEL_BG: Color32 = Color32::from_rgb(22, 27, 34); // #161b22 Sidebar/surface alt
pub const SURFACE_BG: Color32 = Color32::from_rgb(33, 38, 45); // #21262d Elevated surface
pub const SURFACE_BG_ALT: Color32 = Color32::from_rgb(48, 54, 61); // #30363d
pub const SURFACE_BG_MUTED: Color32 = Color32::from_rgb(1, 4, 9); // #010409 Deep bg
pub const BORDER: Color32 = Color32::from_rgb(48, 54, 61); // #30363d Borders
pub const TEXT_MAIN: Color32 = Color32::from_rgb(201, 209, 217); // #c9d1d9 Primary
pub const TEXT_MUTED: Color32 = Color32::from_rgb(139, 148, 158); // #8b949e Secondary/muted
pub const ACCENT: Color32 = Color32::from_rgb(31, 111, 235); // #1f6feb Blue accent
pub const ACCENT_MUTED: Color32 = Color32::from_rgb(9, 105, 218); // #0969da Selection bg
pub const SUCCESS: Color32 = Color32::from_rgb(63, 185, 80); // #3fb950 Added/new
pub const WARNING: Color32 = Color32::from_rgb(210, 153, 34); // #d29922 Modified
pub const DANGER: Color32 = Color32::from_rgb(248, 81, 73); // #f85149 Deleted
pub const DIFF_BG: Color32 = Color32::from_rgb(1, 4, 9); // #010409 Hunk header bg

// Diff Specific
pub const DIFF_ADD_BG: Color32 = Color32::from_rgb(13, 58, 26);
pub const DIFF_ADD_FG: Color32 = Color32::from_rgb(3, 201, 105);
pub const DIFF_DEL_BG: Color32 = Color32::from_rgb(61, 31, 26);
pub const DIFF_DEL_FG: Color32 = Color32::from_rgb(218, 54, 51);
pub const DIFF_HUNK_BG: Color32 = Color32::from_rgb(1, 4, 9);

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

    let proportional = egui::FontFamily::Proportional;
    let monospace = egui::FontFamily::Monospace;

    style.text_styles = [
        (
            TextStyle::Heading,
            egui::FontId::new(28.0, proportional.clone()),
        ),
        (
            TextStyle::Name("Heading2".into()),
            egui::FontId::new(14.0, proportional.clone()),
        ), // font-size-md
        (
            TextStyle::Body,
            egui::FontId::new(12.0, proportional.clone()),
        ), // font-size
        (
            TextStyle::Monospace,
            egui::FontId::new(11.0, monospace.clone()),
        ), // font-size-sm
        (
            TextStyle::Button,
            egui::FontId::new(12.0, proportional.clone()),
        ), // font-size
        (
            TextStyle::Small,
            egui::FontId::new(9.0, proportional.clone()),
        ), // font-size-xs
    ]
    .into();

    ctx.set_style(style);
}
