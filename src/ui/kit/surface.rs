//! Elevation helpers (design.md §7).
//!
//! Four levels, and most surfaces are level 0 — a flat fill with a hairline
//! border and no shadow at all. The three helpers here cover the things that
//! genuinely float.
//!
//! Each helper applies the fill, the outline, *and* the shadow together,
//! because a 1px border on top of a drop shadow reads as an outline rather
//! than as elevation. Do not add `border_1()` after calling one.

use gpui::{BoxShadow, Hsla, Styled, hsla, point, px};

use crate::ui::theme;

/// Elevation levels for raised surfaces.
///
/// Implemented for every `Styled` element, so it chains:
/// `v_flex().w(z(440.0)).dialog()`.
pub trait Surface: Styled + Sized {
    /// `e1` — a resting card. Settings groups, the push suggestion card.
    /// Hairline border, barely-there shadow.
    fn card(self) -> Self {
        self.bg(theme::panel_bg())
            .rounded(theme::z(theme::CORNER_RADIUS))
            .border_1()
            .border_color(theme::border())
            .shadow(shadow(0.0, 1.0, 2.0, 0.20))
    }

    /// `e2` — an anchored overlay. Dropdowns, context menus, autocomplete.
    /// Sits close to its trigger, so the shadow is tight.
    fn overlay(self) -> Self {
        self.bg(theme::panel_bg())
            .rounded(theme::z(theme::CORNER_RADIUS))
            .border_1()
            .border_color(theme::border())
            .shadow(shadow(0.0, 6.0, 16.0, 0.40))
    }

    /// `e3` — a modal dialog. The only thing that floats free of the layout,
    /// so it casts the one real shadow in the app.
    fn dialog(self) -> Self {
        self.bg(theme::panel_bg())
            .rounded(theme::z(theme::CORNER_RADIUS))
            .border_1()
            .border_color(theme::border())
            .shadow(shadow(0.0, 12.0, 32.0, 0.55))
    }
}

impl<T: Styled> Surface for T {}

/// A single soft key shadow in pure black at `alpha`.
///
/// Offsets and blur are base values scaled by zoom; a shadow that stayed put
/// while the layout grew would read as a rendering bug.
fn shadow(x: f32, y: f32, blur: f32, alpha: f32) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: black(alpha),
        offset: point(theme::z(x), theme::z(y)),
        blur_radius: theme::z(blur),
        spread_radius: px(0.0),
    }]
}

fn black(alpha: f32) -> Hsla {
    hsla(0.0, 0.0, 0.0, alpha)
}
