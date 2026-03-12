use eframe::egui::{self, Color32, Stroke, TextStyle};

// --- Colors ---
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

// --- Geometry tokens ---
pub const TOOLBAR_HEIGHT: f32 = 68.0;
pub const TOOLBAR_INNER_HEIGHT: f32 = 52.0;
pub const TOOLBAR_PADDING: egui::Vec2 = egui::Vec2::new(12.0, 5.0);
pub const TOOLBAR_ITEM_SPACING: f32 = 8.0;
pub const STATUS_BAR_HEIGHT: f32 = 26.0;
pub const SIDEBAR_DEFAULT_WIDTH: f32 = 260.0;
pub const SIDEBAR_MIN_WIDTH: f32 = 220.0;
pub const ROW_HEIGHT: f32 = 32.0;
pub const ROW_HEIGHT_COMPACT: f32 = 28.0;
pub const CONTROL_HEIGHT: f32 = 34.0;
pub const TAB_HEIGHT: f32 = 34.0;
pub const CORNER_RADIUS: f32 = 6.0;
pub const CORNER_RADIUS_SM: f32 = 4.0;
pub const SECTION_PADDING: f32 = 12.0;
pub const ITEM_GAP: f32 = 8.0;
pub const DIFF_ROW_HEIGHT: f32 = 20.0;

// --- Utility functions ---

pub fn color_with_alpha(color: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_premultiplied(
        color.r(),
        color.g(),
        color.b(),
        alpha.clamp(0.0, 255.0) as u8,
    )
}

pub fn blend_color(from: Color32, to: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |a: u8, b: u8| -> u8 { (a as f32 + (b as f32 - a as f32) * t).round() as u8 };
    Color32::from_rgba_premultiplied(
        mix(from.r(), to.r()),
        mix(from.g(), to.g()),
        mix(from.b(), to.b()),
        mix(from.a(), to.a()),
    )
}

// --- Frame presets ---

pub fn panel_frame() -> egui::Frame {
    egui::Frame::default()
        .fill(PANEL_BG)
        .inner_margin(egui::Margin::same(0))
}

pub fn surface_frame() -> egui::Frame {
    egui::Frame::default()
        .fill(SURFACE_BG)
        .stroke(Stroke::new(1.0, BORDER))
}

pub fn card_frame() -> egui::Frame {
    egui::Frame::default()
        .fill(SURFACE_BG_MUTED)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CORNER_RADIUS)
        .inner_margin(egui::Margin::same(SECTION_PADDING as i8))
}

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
    visuals.popup_shadow = egui::epaint::Shadow::NONE;
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::Vec2::new(8.0, 6.0);
    style.spacing.button_padding = egui::Vec2::new(10.0, 6.0);
    style.spacing.indent = 14.0;
    style.visuals.window_corner_radius = CORNER_RADIUS.into();
    style.visuals.menu_corner_radius = CORNER_RADIUS.into();

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
