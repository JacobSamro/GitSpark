//! The modal dialog shell (design.md §8.3).
//!
//! Fourteen files hand-rolled this same header/body/footer sandwich before
//! the kit existed — which is exactly why it exists. The four builders here
//! compose with `.child(..)` like any other element, so migrating a dialog is
//! a mechanical swap and no closure plumbing is needed: the close button and
//! footer buttons arrive already wired from the callsite.
//!
//! ```ignore
//! dialog_shell(DIALOG_WIDTH)
//!     .child(dialog_header(
//!         "Delete Branch",
//!         Some((IconName::TriangleAlert, theme::warning())),
//!         icon_button("delete-branch-close", IconName::Close)
//!             .on_click(cx.listener(..)),
//!     ))
//!     .child(dialog_body().child(..))
//!     .child(dialog_footer()
//!         .child(button("…-cancel", "Cancel", ButtonVariant::Secondary).on_click(..))
//!         .child(button("…-confirm", "Delete", ButtonVariant::Danger).on_click(..)))
//! ```

use gpui::{
    Div, FontWeight, Hsla, IntoElement, ParentElement, SharedString, Styled,
    div,
};
use gpui_component::{Icon, IconName, h_flex, v_flex};

use crate::ui::kit::surface::Surface;
use crate::ui::theme;
use crate::ui::theme::z;

// Dialog widths.
//
// These are the single source of truth: `app::dialogs` needs the width to
// center the dialog in the window, and the dialog itself needs it to size the
// shell. Those two numbers used to be typed out separately in two files,
// which is how a dialog ends up rendering 20px off-center.

/// A one-sentence confirmation. Fits a branch name without wrapping.
pub const DIALOG_WIDTH: f32 = 440.0;
/// A dialog carrying a file list or a short form — reset, stash, restore.
pub const DIALOG_WIDTH_WIDE: f32 = 500.0;
/// Settings, which is two panes side by side.
#[allow(dead_code)]
pub const DIALOG_WIDTH_SETTINGS: f32 = 720.0;

/// The floating container: fixed width, `panel_bg`, `e3` elevation.
///
/// Deliberately not `Stateful` — the shell itself has no interactive state,
/// and staying a plain `Div` keeps every `render_*_dialog` returning the one
/// type that `app::dialogs`' match arms require.
pub fn dialog_shell(width: f32) -> Div {
    v_flex().w(z(width)).dialog()
}

/// Title bar: optional status icon, title, and a trailing close button.
///
/// `close` is passed in already wired rather than taking a callback, so the
/// kit never needs to know about `GitSparkApp` or `Context`.
pub fn dialog_header(
    title: impl Into<SharedString>,
    icon: Option<(IconName, Hsla)>,
    close: impl IntoElement,
) -> Div {
    let mut title_row = h_flex().items_center().gap(z(theme::SPACE_4));

    if let Some((name, color)) = icon {
        title_row = title_row.child(Icon::new(name).size(z(16.0)).text_color(color));
    }

    h_flex()
        .w_full()
        .px(z(theme::SPACE_7))
        .py(z(theme::SPACE_6))
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(theme::border())
        .child(
            title_row.child(
                div()
                    .text_size(z(theme::FONT_SIZE_MD))
                    .text_color(theme::text_main())
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title.into()),
            ),
        )
        .child(close)
}

/// The content well. Callers add their own children.
pub fn dialog_body() -> Div {
    v_flex()
        .w_full()
        .p(z(theme::SPACE_7))
        .gap(z(theme::SPACE_5))
}

/// Right-aligned action bar.
///
/// Add Cancel first and the affirmative action last: on macOS the confirming
/// button is the rightmost one, and reversing that order is how people
/// discard work by muscle memory.
pub fn dialog_footer() -> Div {
    h_flex()
        .w_full()
        .px(z(theme::SPACE_7))
        .py(z(theme::SPACE_6))
        .justify_end()
        .gap(z(theme::SPACE_4))
        .border_t_1()
        .border_color(theme::border())
}

/// A line of body copy at the Body role.
#[allow(dead_code)]
pub fn dialog_text(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(z(theme::FONT_SIZE_BODY))
        .text_color(theme::text_main())
        .child(text.into())
}

/// A line of secondary explanation — consequences, caveats, "cannot be undone".
#[allow(dead_code)]
pub fn dialog_hint(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(z(theme::FONT_SIZE_BODY))
        .text_color(theme::text_muted())
        .child(text.into())
}
