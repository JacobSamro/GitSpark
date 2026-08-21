//! The filterable picker (design.md §8.10).
//!
//! Repository, branch and — once it exists — worktree selection are the same
//! control: an overlay panel with a filter field at the top and a virtualized
//! list below. The filter field is the part worth extracting, because
//! `gpui_component::Input` is unusable here (design.md §10.1) and every
//! consumer therefore hand-rolls the same native `FocusHandle` +
//! `on_key_down` field, complete with its own manually painted caret.
//!
//! Builders return the element with focus already tracked; the callsite
//! attaches its own key handler, so the kit never needs to know about
//! `GitSparkApp`:
//!
//! ```ignore
//! picker::filter_input("branch-filter-input", &app.branch_filter_focus,
//!                      &app.filters.branch_filter_text,
//!                      app.branch_filter_cursor, focused, "Filter")
//!     .key_context("text-field")
//!     .on_key_down(cx.listener(GitSparkApp::handle_branch_filter_key))
//! ```

use gpui::{
    Div, InteractiveElement, IntoElement, ParentElement, SharedString, Stateful, Styled, div, px,
};
use gpui_component::{Icon, IconName, h_flex};

use crate::ui::theme;

/// Height of the filter field. Fixed rather than zoom-scaled so the painted
/// caret keeps lining up with the text at every zoom level.
const FIELD_HEIGHT: f32 = 28.0;
const CARET_HEIGHT: f32 = 14.0;

/// Split `text` at `cursor`, snapping to the nearest char boundary at or
/// below it.
///
/// The two hand-rolled copies of this field sliced with `&text[..cursor]`
/// after clamping to `len()`, which is not enough: a byte index inside a
/// multi-byte character is in range but not a boundary, and slicing there
/// panics. Typing an accented character or an emoji into either filter would
/// take the app down.
fn split_at_cursor(text: &str, cursor: usize) -> (&str, &str) {
    let mut index = cursor.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    text.split_at(index)
}

/// The filter field: search glyph, text with a painted caret, focus ring.
///
/// Returns with `track_focus` already applied. Chain `.key_context(..)` and
/// `.on_key_down(..)` at the callsite.
pub fn filter_input(
    id: &'static str,
    focus: &gpui::FocusHandle,
    text: &str,
    cursor: usize,
    focused: bool,
    placeholder: impl Into<SharedString>,
) -> Stateful<Div> {
    let content = if text.is_empty() && !focused {
        div()
            .text_size(theme::z(theme::FONT_SIZE))
            .text_color(theme::text_muted())
            .child(placeholder.into())
            .into_any_element()
    } else {
        let (before, after) = split_at_cursor(text, cursor);
        h_flex()
            .items_center()
            .overflow_x_hidden()
            .text_size(theme::z(theme::FONT_SIZE))
            .child(
                div()
                    .text_color(theme::text_main())
                    .whitespace_nowrap()
                    .child(before.to_string()),
            )
            .child(if focused {
                // Hand-painted caret: there is no Input to blink one for us.
                div()
                    .w(px(1.0))
                    .h(px(CARET_HEIGHT))
                    .bg(theme::text_main())
                    .flex_shrink_0()
                    .into_any_element()
            } else {
                div().into_any_element()
            })
            .child(
                div()
                    .text_color(theme::text_main())
                    .whitespace_nowrap()
                    .child(after.to_string()),
            )
            .into_any_element()
    };

    h_flex()
        .id(id)
        .track_focus(focus)
        .flex_1()
        .h(px(FIELD_HEIGHT))
        .px(px(8.0))
        .items_center()
        .gap(px(6.0))
        .rounded(theme::z(theme::CORNER_RADIUS))
        .border_1()
        .border_color(if focused {
            theme::accent()
        } else {
            theme::surface_bg_alt()
        })
        .bg(theme::bg())
        .cursor_text()
        .child(
            Icon::new(IconName::Search)
                .size(px(14.0))
                .text_color(theme::text_muted()),
        )
        .child(content)
}

/// The padded row a filter field sits in. Callers add trailing actions.
pub fn filter_bar() -> Div {
    h_flex()
        .w_full()
        .flex_shrink_0()
        .px(px(10.0))
        .py(px(10.0))
        .gap(px(8.0))
        .items_center()
}

#[cfg(test)]
mod tests {
    use super::split_at_cursor;

    #[test]
    fn splits_ascii_at_the_cursor() {
        assert_eq!(split_at_cursor("branch", 3), ("bra", "nch"));
    }

    #[test]
    fn clamps_a_cursor_past_the_end() {
        assert_eq!(split_at_cursor("dev", 99), ("dev", ""));
    }

    #[test]
    fn snaps_back_to_a_char_boundary() {
        // "é" is two bytes; a cursor at 1 is inside it. Slicing there panics,
        // which is what both hand-rolled filters used to do.
        let (before, after) = split_at_cursor("é", 1);
        assert_eq!((before, after), ("", "é"));
    }

    #[test]
    fn handles_multibyte_text_at_every_byte_index() {
        let text = "feat/ünïcode-🌿";
        for cursor in 0..=text.len() + 2 {
            let (before, after) = split_at_cursor(text, cursor);
            assert_eq!(format!("{before}{after}"), text);
        }
    }
}
