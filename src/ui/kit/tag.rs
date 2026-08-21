//! Status tags and inline count pills (design.md §8.5, §8.6).

use gpui::{Div, FontWeight, Hsla, ParentElement, SharedString, Styled, div};
use gpui_component::h_flex;

use crate::ui::theme;
use crate::ui::theme::z;
use crate::ui::theme::TextStyleExt;

/// Side of the square A/M/D marker in a changes row.
const TAG_SIZE: f32 = 16.0;

/// The single-letter file-status square: `A`dded, `M`odified, `D`eleted,
/// `R`enamed, `U`nmerged.
///
/// Takes the raw git status letter so callers can pass a porcelain code
/// straight through; anything unrecognized falls back to muted, which keeps
/// an unexpected status visible instead of blanking the column.
pub fn status_tag(status: &str) -> Div {
    let (glyph, color) = match status {
        "A" | "?" => ("A", theme::success()),
        "M" => ("M", theme::warning()),
        "D" => ("D", theme::danger()),
        "R" => ("R", theme::accent()),
        "U" => ("U", theme::danger()),
        other => (other, theme::text_muted()),
    };

    h_flex()
        .flex_shrink_0()
        .size(z(TAG_SIZE))
        .items_center()
        .justify_center()
        .rounded(z(theme::CORNER_RADIUS_SM))
        .bg(theme::with_alpha(color, 0.15))
        .text_size(z(theme::FONT_SIZE_XS))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child(SharedString::from(glyph.to_string()))
}

/// An inline count pill — tab-bar counters, toolbar ahead/behind.
///
/// This is deliberately not `gpui_component::Badge`, which renders as an
/// absolutely positioned notification dot rather than an inline counter
/// (design.md §10.4). Digits are tabular so the pill doesn't twitch as the
/// count changes.
pub fn pill(count: usize) -> Div {
    pill_colored(count, theme::toolbar_badge_bg(), theme::text_main())
}

/// A count pill in explicit colors — ahead/behind, conflict counts.
#[allow(dead_code)]
pub fn pill_colored(count: usize, bg: Hsla, fg: Hsla) -> Div {
    div()
        .flex_shrink_0()
        .px(z(theme::SPACE_3))
        .py(z(theme::SPACE_1))
        .rounded(z(theme::RADIUS_PILL))
        .bg(bg)
        .text_size(z(theme::FONT_SIZE_SM))
        .text_color(fg)
        .tabular_nums()
        .child(count.to_string())
}
