//! Zed One Dark (deepened) / One Light design tokens (see `design.md`).
//!
//! This is the single place raw colors live. Every token is a `fn`, never a
//! `const`, so the appearance switch is a token swap and never a component
//! fork (design.md §13). A `rgb(0x…)` literal outside this file is a bug —
//! it will not follow the light arm, and that is exactly how a light mode
//! ends up half-applied.
//!
//! Layout tokens come in three groups — [`SPACE_1`]..[`SPACE_8`] for spacing,
//! the `CORNER_RADIUS*` family for radii, and the frame dimensions further
//! down. All of them are *base* values: pass them through [`z`] so the user's
//! zoom factor applies.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

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
// Appearance — the resolved light/dark flag every color token reads
//
// design.md §13: light is a token swap, never a component fork. That is only
// true if tokens are functions, so they can re-resolve when this flag flips.
// A `rgb(0x…)` literal outside this file breaks it.
// ---------------------------------------------------------------------------

/// The user's appearance preference. `System` follows the OS.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub fn as_str(self) -> &'static str {
        match self {
            Appearance::System => "system",
            Appearance::Light => "light",
            Appearance::Dark => "dark",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "light" => Appearance::Light,
            "dark" => Appearance::Dark,
            _ => Appearance::System,
        }
    }

    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            Appearance::System => "System",
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
        }
    }
}

// 0 = system, 1 = light, 2 = dark.
static PREF: AtomicU8 = AtomicU8::new(0);
/// The RESOLVED appearance every token below reads. Defaults to dark so the
/// first frame — drawn before any window exists to ask — is not a white flash.
static DARK: AtomicBool = AtomicBool::new(true);

pub fn appearance() -> Appearance {
    match PREF.load(Ordering::Relaxed) {
        1 => Appearance::Light,
        2 => Appearance::Dark,
        _ => Appearance::System,
    }
}

pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

/// Record the preference without resolving it. Call [`resolve`] afterwards.
pub fn set_appearance(pref: Appearance) {
    PREF.store(
        match pref {
            Appearance::System => 0,
            Appearance::Light => 1,
            Appearance::Dark => 2,
        },
        Ordering::Relaxed,
    );
}

/// Resolve the preference against the OS appearance and return the result.
///
/// `system_is_dark` comes from the window (or the app, before a window
/// exists); an explicit preference ignores it entirely.
pub fn resolve(system_is_dark: bool) -> bool {
    let dark = match appearance() {
        Appearance::System => system_is_dark,
        Appearance::Light => false,
        Appearance::Dark => true,
    };
    DARK.store(dark, Ordering::Relaxed);
    dark
}

/// Pick between the dark and light value of a token.
///
/// Every color below goes through this, which is what makes the appearance
/// switch a swap rather than a fork.
#[inline]
fn pick(dark: u32, light: u32) -> Hsla {
    gpui::rgb(if is_dark() { dark } else { light }).into()
}

// ---------------------------------------------------------------------------
// Colors — Zed One Dark (deepened) / Zed One Light
//
// Surfaces are a darkened derivation: stock One Dark bottoms out at #282c33,
// a fairly light "dark", so the ramp here sits about two stops below it. The
// HUES — created / modified / deleted / player and the syntax set — are Zed's
// literal values in both arms.
//
// Note the inversion in `bg` vs `panel_bg`: in dark the diff surface is the
// DARKEST thing on screen and the chrome sits above it; in light it is the
// BRIGHTEST. Depth means "furthest from the chrome", not "darker than it".
// ---------------------------------------------------------------------------

// Core surfaces
pub fn bg() -> Hsla {
    pick(0x0e1013, 0xffffff) // buffer: the diff, the reading surface
}

pub fn panel_bg() -> Hsla {
    pick(0x15171b, 0xf2f2f3) // chrome: sidebar, dialogs, status bar
}

pub fn surface_bg() -> Hsla {
    pick(0x1a1d22, 0xeaeaeb) // raised control: inputs, buttons, hunk headers
}

pub fn surface_bg_alt() -> Hsla {
    pick(0x262a31, 0xdfdfe0) // raised border / pressed
}

pub fn surface_bg_muted() -> Hsla {
    pick(0x0a0c0f, 0xf7f7f8) // recessed
}

// Borders
pub fn border() -> Hsla {
    pick(0x2a2f37, 0xc9c9ca)
}

// Text
pub fn text_main() -> Hsla {
    pick(0xd3d7de, 0x383a41)
}

pub fn text_muted() -> Hsla {
    pick(0x868d99, 0x6b6d76)
}

// Accent — Zed's player[0]. The light value is NOT the dark one: #74ade8
// measures about 1.9:1 on white, so light darkens to a One Light blue.
pub fn accent() -> Hsla {
    pick(0x74ade8, 0x4257c9)
}

#[allow(dead_code)]
pub fn accent_muted() -> Hsla {
    pick(0x5b93cc, 0x35489f)
}

pub fn checkbox_selected_bg() -> Hsla {
    accent()
}

/// The check glyph, knocked out of the accent fill. Resolving to [`bg`] is
/// correct in both arms: near-black on light blue, white on deep blue.
pub fn checkbox_selected_fg() -> Hsla {
    bg()
}

pub fn text_selection_bg() -> Hsla {
    pick(0x2f4c6b, 0xd8deef)
}

// Status — Zed's created / modified / deleted, re-picked for white in light.
pub fn success() -> Hsla {
    pick(0xa1c181, 0x3f8a3a)
}

pub fn warning() -> Hsla {
    pick(0xdec184, 0xb07a08)
}

pub fn warning_bg() -> Hsla {
    pick(0x2a2415, 0xfbf3df)
}

pub fn danger() -> Hsla {
    pick(0xd07277, 0xc0392e)
}

pub fn danger_hover() -> Hsla {
    pick(0xdc8a8e, 0xa82f26)
}

// ---------------------------------------------------------------------------
// Diff-specific colors
//
// Row tints are the hue at ~13% over the buffer in dark, ~10% in light, then
// flattened to an opaque value. A 10% green over #282c33 and over #0e1013 are
// not the same signal — the deeper ground eats it, hence 13.
// ---------------------------------------------------------------------------

pub fn diff_add_bg() -> Hsla {
    pick(0x212721, 0xecf3eb)
}

pub fn diff_add_gutter_bg() -> Hsla {
    pick(0x1b2419, 0xe3efe1)
}

pub fn diff_add_fg() -> Hsla {
    text_main() // the background carries the signal, not the text
}

pub fn diff_del_bg() -> Hsla {
    pick(0x271d20, 0xf9ebea)
}

pub fn diff_del_gutter_bg() -> Hsla {
    pick(0x2e1e20, 0xf5dfdd)
}

pub fn diff_del_fg() -> Hsla {
    text_main()
}

pub fn diff_hunk_bg() -> Hsla {
    surface_bg()
}

pub fn diff_gutter_bg() -> Hsla {
    pick(0x121519, 0xfafafb)
}

/// Intra-line highlight for the changed characters inside a modified line.
///
/// A step stronger than the row tint, so the word-level diff reads through it.
pub fn diff_add_highlight_bg() -> Hsla {
    pick(0x2f5c3c, 0xc7e6c9)
}

pub fn diff_del_highlight_bg() -> Hsla {
    pick(0x6a3236, 0xf6c9c5)
}

/// Line-selection fill for partial commits.
///
/// Deliberately quiet. This fills the WHOLE gutter on every selected line,
/// and every line starts selected, so a saturated value turns the gutter into
/// the loudest thing in the diff — which is backwards, since selection is the
/// default state and the code is the content. One step above the gutter with
/// a blue cast is enough to read as "included".
pub fn diff_selected_bg() -> Hsla {
    pick(0x1c2a39, 0xdbe4f4)
}

// ---------------------------------------------------------------------------
// Interactive colors
// ---------------------------------------------------------------------------

pub fn hover_bg() -> Hsla {
    pick(0x22262d, 0xe6e6e8) // element.hover
}

pub fn list_hover_bg() -> Hsla {
    pick(0x262a31, 0xe0e0e3) // rows sit on panel_bg, so one step further
}

pub fn commit_button_bg() -> Hsla {
    accent()
}

pub fn commit_button_hover_bg() -> Hsla {
    pick(0x8cbcec, 0x3a4eb8)
}

/// Text and glyphs knocked out of an `accent()` fill.
///
/// Theme-dependent, and this is the trap: the dark arm's accent (`#74ade8`)
/// is LIGHT, so white text on it is barely legible — it needs near-black. The
/// light arm's accent is dark and needs white. A literal `gpui::white()` on a
/// selected row is therefore a bug in dark mode, not just in light.
pub fn on_accent() -> Hsla {
    pick(0x0e1013, 0xffffff)
}

/// The commit CTA's label. Same thing as [`on_accent`]; kept as its own name
/// because that is what the callsites read as.
pub fn commit_button_text() -> Hsla {
    on_accent()
}

/// Fill for the title-bar update button.
///
/// The one accent surface that does NOT follow [`accent`], for the reason
/// spelled out on [`on_accent`]: the dark arm's accent is a light blue, and
/// this button's label is white. Rather than knock the label to near-black in
/// dark mode — which would make the same button read as two different
/// controls between themes — the fill holds the deep blue in both arms, where
/// white sits legibly. It stays inside the accent family, so it still reads
/// as the app's call-to-action colour.
pub fn update_button_bg() -> Hsla {
    pick(0x4257c9, 0x4257c9)
}

pub fn update_button_hover_bg() -> Hsla {
    pick(0x3a4eb8, 0x3a4eb8)
}

pub fn line_num_color() -> Hsla {
    pick(0x59606b, 0x9a9ca3)
}

// ---------------------------------------------------------------------------
// Toolbar-specific colors
// ---------------------------------------------------------------------------

pub fn toolbar_bg() -> Hsla {
    panel_bg()
}

pub fn toolbar_button_border() -> Hsla {
    pick(0x1e2128, 0xdfdfe0) // border.variant
}

pub fn toolbar_hover_bg() -> Hsla {
    hover_bg()
}

pub fn toolbar_badge_bg() -> Hsla {
    pick(0x2b3039, 0xdfdfe0)
}

// Push suggestion card
pub fn push_card_bg() -> Hsla {
    pick(0x17232e, 0xe8edfa)
}

pub fn push_card_border() -> Hsla {
    accent()
}

pub fn push_card_text() -> Hsla {
    pick(0x9fc4e4, 0x3a4b8f)
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
