//! Buttons and icon buttons (design.md §8.1, §8.2).
//!
//! `gpui_component::Button` only renders label-only buttons without `Root`
//! (design.md §10), and it carries none of our variant language, so the kit
//! draws its own. Builders return a `Stateful<Div>`; the callsite attaches
//! `.on_click(cx.listener(..))`.

use gpui::{
    Div, ElementId, InteractiveElement, ParentElement, SharedString, Stateful, Styled, div,
};
use gpui_component::{Icon, IconName, h_flex};

use crate::ui::theme;
use crate::ui::theme::z;

// A disabled button drops to a neutral gray rather than fading its own
// variant color. A translucent red still reads as "destructive"; gray reads
// as "unavailable", which is the thing we actually need to communicate. One
// rule for all four variants.

/// The four button roles. There is exactly one `Primary` per dialog.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ButtonVariant {
    /// The affirmative action. GitHub's call-to-action blue.
    Primary,
    /// Everything that isn't affirmative or destructive, including Cancel.
    Secondary,
    /// Destructive: delete, discard, force-push. Red means data loss.
    Danger,
    /// Tertiary action inside a body — no fill until hovered.
    Ghost,
}

impl ButtonVariant {
    fn bg(self) -> gpui::Hsla {
        match self {
            Self::Primary => theme::commit_button_bg(),
            Self::Secondary => theme::surface_bg(),
            Self::Danger => theme::danger(),
            Self::Ghost => gpui::transparent_black().into(),
        }
    }

    fn hover_bg(self) -> gpui::Hsla {
        match self {
            Self::Primary => theme::commit_button_hover_bg(),
            Self::Secondary => theme::toolbar_hover_bg(),
            Self::Danger => theme::danger_hover(),
            Self::Ghost => theme::hover_bg(),
        }
    }

    fn text(self) -> gpui::Hsla {
        match self {
            Self::Primary | Self::Danger => theme::commit_button_text(),
            Self::Secondary => theme::text_main(),
            Self::Ghost => theme::text_muted(),
        }
    }

    /// Only `Secondary` carries a border — the filled variants read as raised
    /// on color alone, and an outline on top of a fill looks like a mistake.
    fn border(self) -> Option<gpui::Hsla> {
        match self {
            Self::Secondary => Some(theme::surface_bg_alt()),
            _ => None,
        }
    }
}

/// An enabled button. See [`button_state`] when the button can be disabled.
pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    variant: ButtonVariant,
) -> Stateful<Div> {
    button_state(id, label, variant, true)
}

/// A button that may be disabled.
///
/// A disabled button keeps its exact footprint — swapping it for a smaller or
/// borderless element would shift the footer under the cursor — but goes
/// neutral gray and drops hover and the pointer cursor, the two signals that
/// say "this will do something".
pub fn button_state(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    variant: ButtonVariant,
    enabled: bool,
) -> Stateful<Div> {
    let (bg, text, border) = if enabled {
        (variant.bg(), variant.text(), variant.border())
    } else {
        (
            theme::surface_bg(),
            theme::text_muted(),
            Some(theme::surface_bg_alt()),
        )
    };

    let mut el = h_flex()
        .id(id)
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .px(z(theme::SPACE_6))
        .py(z(theme::SPACE_3))
        .rounded(z(theme::CORNER_RADIUS))
        .bg(bg)
        .child(
            div()
                .text_size(z(theme::FONT_SIZE_BODY))
                .text_color(text)
                .child(label.into()),
        );

    if let Some(border) = border {
        el = el.border_1().border_color(border);
    }

    if enabled {
        el = el.cursor_pointer().hover(move |s| s.bg(variant.hover_bg()));
    }

    el
}

/// A square icon-only button: dialog close, row affordances, toolbar extras.
///
/// Exists because `gpui_component::Button::icon()` renders an empty box
/// without `Root` (design.md §10.2).
pub fn icon_button(id: impl Into<ElementId>, icon: IconName) -> Stateful<Div> {
    div()
        .id(id)
        .flex_shrink_0()
        .cursor_pointer()
        .p(z(theme::SPACE_2))
        .rounded(z(theme::CORNER_RADIUS_SM))
        .hover(|s| s.bg(theme::hover_bg()))
        .child(
            Icon::new(icon)
                .size(z(14.0))
                .text_color(theme::text_muted()),
        )
}
