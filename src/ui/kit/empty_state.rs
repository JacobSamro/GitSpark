//! Empty states and section headers (design.md §8.7, §8.8).

use gpui::{Div, FontWeight, ParentElement, SharedString, Styled, div};
use gpui_component::{Icon, IconName, v_flex};

use crate::ui::theme;
use crate::ui::theme::z;

/// Icon size in an empty state — large enough to anchor the block, small
/// enough not to become an illustration.
const EMPTY_ICON_SIZE: f32 = 28.0;

/// A centered "nothing here" block: headline, and optionally a line of
/// guidance and a leading icon.
///
/// Pass `None` for `hint` when the headline already says everything ("No
/// history"). A hint that restates the headline is worse than no hint.
pub fn empty_state(
    icon: Option<IconName>,
    headline: impl Into<SharedString>,
    hint: Option<SharedString>,
) -> Div {
    let mut el = v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .p(z(theme::SPACE_8))
        .gap(z(theme::SPACE_4));

    if let Some(name) = icon {
        el = el.child(
            Icon::new(name)
                .size(z(EMPTY_ICON_SIZE))
                .text_color(theme::text_muted()),
        );
    }

    el = el.child(
        div()
            .text_size(z(theme::FONT_SIZE))
            .text_color(theme::text_main())
            .child(headline.into()),
    );

    if let Some(hint) = hint {
        el = el.child(
            div()
                .text_size(z(theme::FONT_SIZE_SM))
                .text_color(theme::text_muted())
                .child(hint),
        );
    }

    el
}

/// A sidebar or settings group label.
///
/// Not uppercased — GitHub Desktop doesn't, and small caps at 11px costs more
/// legibility than the hierarchy is worth.
#[allow(dead_code)]
pub fn section_header(label: impl Into<SharedString>) -> Div {
    div()
        .px(z(theme::SPACE_5))
        .py(z(theme::SPACE_3))
        .text_size(z(theme::FONT_SIZE_SM))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::text_muted())
        .child(label.into())
}
