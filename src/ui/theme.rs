use std::sync::atomic::{AtomicU32, Ordering};

use gpui::{Hsla, Pixels, px};

// ---------------------------------------------------------------------------
// Zoom — global scale factor for layout
// ---------------------------------------------------------------------------

static ZOOM_FACTOR_BITS: AtomicU32 = AtomicU32::new(0); // initialized to 1.0 below

/// Set the global zoom factor (called from app on zoom change).
pub fn set_zoom(factor: f32) {
    ZOOM_FACTOR_BITS.store(factor.to_bits(), Ordering::Relaxed);
}

/// Get the current zoom factor.
pub fn zoom() -> f32 {
    let bits = ZOOM_FACTOR_BITS.load(Ordering::Relaxed);
    if bits == 0 { 1.0 } else { f32::from_bits(bits) }
}

/// Scale a pixel value by the current zoom factor.
pub fn z(val: f32) -> Pixels {
    px(val * zoom())
}

// ---------------------------------------------------------------------------
// Colors — GitHub Dark theme
// ---------------------------------------------------------------------------

// Core backgrounds — Primer dark primitives (neutral scale)
pub fn bg() -> Hsla {
    gpui::rgb(0x0d1117).into() // neutral-1
}

pub fn panel_bg() -> Hsla {
    gpui::rgb(0x151b23).into() // neutral-2
}

pub fn surface_bg() -> Hsla {
    gpui::rgb(0x212830).into() // neutral-3
}

pub fn surface_bg_alt() -> Hsla {
    gpui::rgb(0x2a313c).into() // neutral-5
}

pub fn surface_bg_muted() -> Hsla {
    gpui::rgb(0x010409).into() // black
}

// Borders
pub fn border() -> Hsla {
    gpui::rgb(0x3d444d).into() // neutral-7
}

// Text — Primer dark
pub fn text_main() -> Hsla {
    gpui::rgb(0xd1d7e0).into() // neutral-11
}

pub fn text_muted() -> Hsla {
    gpui::rgb(0x9198a1).into() // neutral-9
}

// Accent
pub fn accent() -> Hsla {
    gpui::rgb(0x1f6feb).into()
}

pub fn accent_muted() -> Hsla {
    gpui::rgb(0x0969da).into()
}

// Semantic
pub fn success() -> Hsla {
    gpui::rgb(0x3fb950).into()
}

pub fn warning() -> Hsla {
    gpui::rgb(0xd29922).into()
}

pub fn danger() -> Hsla {
    gpui::rgb(0xf85149).into()
}

// ---------------------------------------------------------------------------
// Diff-specific colors
// ---------------------------------------------------------------------------

pub fn diff_add_bg() -> Hsla {
    gpui::rgb(0x0d3a1a).into()
}

pub fn diff_add_fg() -> Hsla {
    gpui::rgb(0xc9d1d9).into() // --diff-add-text-color: var(--diff-text-color)
}

pub fn diff_del_bg() -> Hsla {
    gpui::rgb(0x3d1f1a).into()
}

pub fn diff_del_fg() -> Hsla {
    gpui::rgb(0xffd7d5).into() // --diff-delete-text-color: $red-100 (soft pink)
}

pub fn diff_hunk_bg() -> Hsla {
    gpui::rgb(0x010409).into()
}

// ---------------------------------------------------------------------------
// Interactive colors
// ---------------------------------------------------------------------------

pub fn hover_bg() -> Hsla {
    gpui::rgb(0x151b23).into() // neutral-2
}

pub fn list_hover_bg() -> Hsla {
    gpui::rgb(0x1c2128).into() // slightly lighter than hover_bg for list rows
}

pub fn commit_button_bg() -> Hsla {
    gpui::rgb(0x0969da).into() // GitHub $blue
}

pub fn commit_button_hover_bg() -> Hsla {
    gpui::rgb(0x0b7bef).into() // lighten($blue, 5%)
}

pub fn commit_button_text() -> Hsla {
    gpui::rgb(0xffffff).into() // pure white
}

pub fn line_num_color() -> Hsla {
    gpui::rgb(0x656c76).into() // neutral-8 — --diff-line-number-color
}

// ---------------------------------------------------------------------------
// Toolbar-specific colors
// ---------------------------------------------------------------------------

pub fn toolbar_bg() -> Hsla {
    gpui::rgb(0x0a0e14).into() // darken($gray-900, 3%)
}

pub fn toolbar_button_border() -> Hsla {
    gpui::rgb(0x141414).into() // --box-border-color in dark
}

pub fn toolbar_hover_bg() -> Hsla {
    gpui::rgb(0x262c36).into() // neutral-4 — --toolbar-button-hover-background-color
}

pub fn toolbar_badge_bg() -> Hsla {
    gpui::rgb(0x3d444d).into() // neutral-7
}

// Push suggestion card (blue highlight)
pub fn push_card_bg() -> Hsla {
    gpui::rgb(0x032f62).into() // deep navy
}

pub fn push_card_border() -> Hsla {
    gpui::rgb(0x1f6feb).into() // accent blue border
}

pub fn push_card_text() -> Hsla {
    gpui::rgb(0x8db1d8).into() // muted blue-white
}

pub fn text_field_focus_shadow() -> Hsla {
    with_alpha(accent(), 0.25) // --text-field-focus-shadow-color
}

// ---------------------------------------------------------------------------
// Color utilities
// ---------------------------------------------------------------------------

/// Return a copy of `color` with its alpha channel replaced.
/// `alpha` is clamped to 0.0..=1.0.
pub fn with_alpha(color: Hsla, alpha: f32) -> Hsla {
    Hsla {
        a: alpha.clamp(0.0, 1.0),
        ..color
    }
}

/// Linearly interpolate between two colors. `t` is clamped to 0.0..=1.0.
pub fn blend(from: Hsla, to: Hsla, t: f32) -> Hsla {
    let t = t.clamp(0.0, 1.0);
    Hsla {
        h: from.h + (to.h - from.h) * t,
        s: from.s + (to.s - from.s) * t,
        l: from.l + (to.l - from.l) * t,
        a: from.a + (to.a - from.a) * t,
    }
}

// ---------------------------------------------------------------------------
// Geometry tokens
// ---------------------------------------------------------------------------

pub const TOOLBAR_HEIGHT: f32 = 50.0;
pub const TOOLBAR_INNER_HEIGHT: f32 = 50.0;
pub const TOOLBAR_ITEM_SPACING: f32 = 0.0;
pub const STATUS_BAR_HEIGHT: f32 = 26.0;
pub const SIDEBAR_WIDTH: f32 = 260.0;
pub const SIDEBAR_MIN_WIDTH: f32 = 220.0;
pub const ROW_HEIGHT: f32 = 32.0;
pub const ROW_HEIGHT_COMPACT: f32 = 28.0;
pub const CONTROL_HEIGHT: f32 = 34.0;
pub const TAB_HEIGHT: f32 = 34.0;
pub const CORNER_RADIUS: f32 = 6.0;
pub const CORNER_RADIUS_SM: f32 = 4.0;
pub const SECTION_PADDING: f32 = 12.0;
pub const ITEM_GAP: f32 = 8.0;
pub const DIFF_ROW_HEIGHT: f32 = 22.0;
pub const DIFF_HEADER_HEIGHT: f32 = 32.0;
pub const DIFF_LINE_NUM_WIDTH: f32 = 50.0;
pub const FILTER_BAR_HEIGHT: f32 = 32.0;

// ---------------------------------------------------------------------------
// Font sizes
// ---------------------------------------------------------------------------

pub const FONT_SIZE_XS: f32 = 9.0;
pub const FONT_SIZE_SM: f32 = 11.0;
pub const FONT_SIZE: f32 = 12.0;
pub const FONT_SIZE_MD: f32 = 14.0;
pub const FONT_SIZE_LG: f32 = 28.0;
