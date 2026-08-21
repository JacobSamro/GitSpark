//! The update indicator in the title bar (design.md §8.12).
//!
//! Sits at the top right of the window chrome, the way Zed does it, and stays
//! out of the way: it renders nothing at all when there is no update. An
//! always-present "up to date" badge is noise in a strip the user looks at all
//! day, and it trains people to ignore the one message that matters.

use gpui::{
    AnyElement, App, ClickEvent, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window, div,
};
use gpui_component::h_flex;

use crate::update::UpdateState;

use super::theme;
use super::theme::z;

/// Which button this state gets. Only the two actionable states have one.
#[derive(Clone, Copy)]
enum Indicator {
    /// The finished update: the moment the whole indicator exists for.
    Primary,
    /// A failed check, which is a retry rather than a call to action.
    Secondary,
}

/// Render the indicator, or an empty element when idle.
///
/// `on_click` is only attached in the states the user can act on, so a
/// mid-download click cannot start a second install.
pub fn render(
    state: &UpdateState,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let Some(label) = state.indicator_label() else {
        return div().into_any_element();
    };

    // The states the user can act on are buttons; the ones they can only wait
    // through are text. Making a spinner look pressable would be a lie, and
    // leaving the finished update as quiet text buries the one moment the
    // indicator exists for.
    let variant = match state {
        UpdateState::ReadyToInstall { .. } => Some(Indicator::Primary),
        UpdateState::Failed { .. } => Some(Indicator::Secondary),
        _ => None,
    };

    // "Has a button" and "does something when clicked" must stay the same set.
    // A new actionable state that nobody gave a button to would look inert; a
    // button on an inert state would do nothing when pressed.
    debug_assert_eq!(
        variant.is_some(),
        state.is_actionable(),
        "actionable states and button states have drifted apart"
    );

    let Some(variant) = variant else {
        return h_flex()
            .id("update-indicator")
            .flex_shrink_0()
            .items_center()
            .px(z(theme::SPACE_3))
            .text_size(z(theme::FONT_SIZE_SM))
            .child(div().text_color(theme::text_muted()).child(label))
            .into_any_element();
    };

    // Hand-rolled rather than `kit::button` for two reasons the kit should not
    // absorb: it has to fit a 38px title bar, and its label is white in both
    // themes. The kit's Primary takes its text from `on_accent()`, which is
    // near-black in dark mode because the dark accent is a light blue — right
    // for the Commit button, wrong here.
    let (bg, hover_bg) = match variant {
        Indicator::Primary => (theme::accent(), theme::commit_button_hover_bg()),
        Indicator::Secondary => (theme::surface_bg(), theme::toolbar_hover_bg()),
    };

    h_flex()
        .id("update-indicator")
        .flex_shrink_0()
        .items_center()
        .gap(z(theme::SPACE_2))
        .px(z(theme::SPACE_4))
        .py(z(theme::SPACE_1))
        .rounded(z(theme::CORNER_RADIUS))
        .bg(bg)
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .on_click(on_click)
        .child(
            div()
                .text_size(z(theme::FONT_SIZE_SM))
                .text_color(gpui::white())
                .child(label),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use crate::update::UpdateState;
    use std::path::PathBuf;

    #[test]
    fn shows_nothing_when_idle() {
        // The indicator must be invisible until it has something to say.
        assert_eq!(UpdateState::Idle.indicator_label(), None);
    }

    #[test]
    fn names_the_version_when_ready_to_restart() {
        let state = UpdateState::ReadyToInstall {
            version: "0.6.0".into(),
            artifact: PathBuf::from("/tmp/x.dmg"),
        };
        let label = state.indicator_label().expect("has a label");
        assert!(label.contains("Restart"), "{label}");
        assert!(label.contains("0.6.0"), "version missing from: {label}");
    }

    #[test]
    fn only_ready_and_failed_are_clickable() {
        // Clicking mid-download should do nothing; there is no useful action.
        assert!(!UpdateState::Idle.is_actionable());
        assert!(!UpdateState::Checking.is_actionable());
        assert!(
            !UpdateState::Downloading {
                version: "0.6.0".into(),
                percent: 40
            }
            .is_actionable()
        );
        assert!(
            UpdateState::ReadyToInstall {
                version: "0.6.0".into(),
                artifact: PathBuf::new()
            }
            .is_actionable()
        );
        assert!(
            UpdateState::Failed {
                message: "boom".into()
            }
            .is_actionable()
        );
    }

    #[test]
    fn download_progress_is_visible_in_the_label() {
        let label = UpdateState::Downloading {
            version: "0.6.0".into(),
            percent: 42,
        }
        .indicator_label()
        .unwrap();
        assert!(label.contains("42%"), "{label}");
    }
}
