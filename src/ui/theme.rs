//! GitHub Desktop Dark design tokens (see `design.md`).
//!
//! This is the single place raw colors live. Every token is a `fn`, never a
//! `const`, so a light theme stays a token swap and never a component fork
//! (design.md §13). A `rgb(0x…)` literal outside this file is a bug.
//!
//! Layout tokens come in three groups — [`SPACE_1`]..[`SPACE_8`] for spacing,
//! the `CORNER_RADIUS*` family for radii, and the frame dimensions further
//! down. All of them are *base* values: pass them through [`z`] so the user's
//! zoom factor applies.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use gpui::{FontFeatures, Hsla, Pixels, Styled, px};

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

#[allow(dead_code)]
pub fn accent_muted() -> Hsla {
    gpui::rgb(0x0969da).into()
}

pub fn checkbox_selected_bg() -> Hsla {
    gpui::rgb(0x58a6ff).into()
}

pub fn checkbox_selected_fg() -> Hsla {
    bg()
}

pub fn text_selection_bg() -> Hsla {
    gpui::rgb(0x264f78).into()
}

// Semantic
pub fn success() -> Hsla {
    gpui::rgb(0x3fb950).into()
}

pub fn warning() -> Hsla {
    gpui::rgb(0xd29922).into()
}

pub fn warning_bg() -> Hsla {
    gpui::rgb(0x2d2307).into()
}

pub fn danger() -> Hsla {
    gpui::rgb(0xf85149).into()
}

pub fn danger_hover() -> Hsla {
    gpui::rgb(0xff6961).into()
}

// ---------------------------------------------------------------------------
// Diff-specific colors
// ---------------------------------------------------------------------------

pub fn diff_add_bg() -> Hsla {
    gpui::rgb(0x0d3a1a).into()
}

pub fn diff_add_gutter_bg() -> Hsla {
    gpui::rgb(0x072d14).into()
}

pub fn diff_add_fg() -> Hsla {
    gpui::rgb(0xc9d1d9).into() // --diff-add-text-color: var(--diff-text-color)
}

pub fn diff_del_bg() -> Hsla {
    gpui::rgb(0x3d1f1a).into()
}

pub fn diff_del_gutter_bg() -> Hsla {
    gpui::rgb(0x2a120f).into()
}

pub fn diff_del_fg() -> Hsla {
    gpui::rgb(0xffd7d5).into() // --diff-delete-text-color: $red-100 (soft pink)
}

pub fn diff_hunk_bg() -> Hsla {
    gpui::rgb(0x010409).into()
}

pub fn diff_gutter_bg() -> Hsla {
    gpui::rgb(0x0a0e14).into()
}

pub fn diff_selected_bg() -> Hsla {
    gpui::rgb(0x0969da).into()
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

#[allow(dead_code)]
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
#[allow(dead_code)]
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
// Spacing scale (design.md §5.2)
//
// Eight steps, and SPACE_4 is the default gap. A layout that needs a value
// off this scale is usually the thing that's wrong, not the scale.
// ---------------------------------------------------------------------------

/// 2px — icon nudge, tag inner padding.
pub const SPACE_1: f32 = 2.0;
/// 4px — icon-to-label gap, close-button padding.
pub const SPACE_2: f32 = 4.0;
/// 6px — button vertical padding, tight row gaps.
pub const SPACE_3: f32 = 6.0;
/// 8px — the default gap: row gaps, button gaps, footer gaps.
pub const SPACE_4: f32 = 8.0;
/// 10px — toolbar section inner padding, list gaps.
pub const SPACE_5: f32 = 10.0;
/// 12px — button horizontal padding, dialog header vertical padding.
pub const SPACE_6: f32 = 12.0;
/// 16px — dialog body padding, section padding.
pub const SPACE_7: f32 = 16.0;
/// 24px — empty-state padding, welcome-screen breathing room.
#[allow(dead_code)]
pub const SPACE_8: f32 = 24.0;

// ---------------------------------------------------------------------------
// Geometry tokens
// ---------------------------------------------------------------------------

pub const TOOLBAR_HEIGHT: f32 = 50.0;
#[allow(dead_code)]
pub const TOOLBAR_INNER_HEIGHT: f32 = 50.0;
#[allow(dead_code)]
pub const TOOLBAR_ITEM_SPACING: f32 = 0.0;
pub const STATUS_BAR_HEIGHT: f32 = 26.0;
#[allow(dead_code)]
pub const SIDEBAR_WIDTH: f32 = 260.0;
#[allow(dead_code)]
pub const SIDEBAR_MIN_WIDTH: f32 = 220.0;
#[allow(dead_code)]
pub const ROW_HEIGHT: f32 = 32.0;
#[allow(dead_code)]
pub const ROW_HEIGHT_COMPACT: f32 = 28.0;
#[allow(dead_code)]
pub const CONTROL_HEIGHT: f32 = 34.0;
#[allow(dead_code)]
pub const TAB_HEIGHT: f32 = 34.0;
pub const CORNER_RADIUS: f32 = 6.0;
pub const CORNER_RADIUS_SM: f32 = 4.0;
/// Fully rounded — count pills, branch chips.
pub const RADIUS_PILL: f32 = 999.0;
#[allow(dead_code)]
pub const SECTION_PADDING: f32 = 12.0;
#[allow(dead_code)]
pub const ITEM_GAP: f32 = 8.0;
pub const DIFF_ROW_HEIGHT: f32 = 22.0;
pub const DIFF_HEADER_HEIGHT: f32 = 32.0;
pub const DIFF_LINE_NUM_WIDTH: f32 = 50.0;
#[allow(dead_code)]
pub const FILTER_BAR_HEIGHT: f32 = 32.0;

// ---------------------------------------------------------------------------
// Font sizes
// ---------------------------------------------------------------------------

// Six rungs. `FONT_SIZE_BODY` is the workhorse — most UI text is 12px, and
// `FONT_SIZE` (13) is reserved for the prominent line in a list row: file
// names, branch names, repo names, commit summaries.

/// 9px — status tags (A/M/D), badge counts. SEMIBOLD.
pub const FONT_SIZE_XS: f32 = 9.0;
/// 11px — metadata, timestamps, paths, hints.
pub const FONT_SIZE_SM: f32 = 11.0;
/// 12px — the default for UI text: labels, buttons, dialog copy.
pub const FONT_SIZE_BODY: f32 = 12.0;
/// 13px — the prominent line in a row: file, branch, repo, commit summary.
pub const FONT_SIZE: f32 = 13.0;
/// 14px — dialog titles, section headings. SEMIBOLD.
#[allow(dead_code)]
pub const FONT_SIZE_MD: f32 = 14.0;
/// 28px — empty-state and welcome-screen headlines.
#[allow(dead_code)]
pub const FONT_SIZE_LG: f32 = 28.0;

// ---------------------------------------------------------------------------
// Typography helpers
// ---------------------------------------------------------------------------

/// Text styling that gpui's `Styled` doesn't expose directly.
pub trait TextStyleExt: Styled + Sized {
    /// Tabular (fixed-width) numerals — OpenType `tnum`.
    ///
    /// San Francisco's default figures are proportional, so a count that
    /// updates in place (ahead/behind, changed-file counts, line numbers)
    /// shifts width as its digits change. Apply this wherever digits move
    /// under a stable label or line up in a column.
    fn tabular_nums(mut self) -> Self {
        self.text_style()
            .get_or_insert_with(Default::default)
            .font_features = Some(FontFeatures(Arc::new(vec![("tnum".to_string(), 1)])));
        self
    }
}

impl<T: Styled> TextStyleExt for T {}
