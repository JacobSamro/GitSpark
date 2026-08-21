//! The update indicator in the title bar (design.md §8.12).
//!
//! Sits at the top right of the window chrome, the way Zed does it, and stays
//! out of the way: it renders nothing at all when there is no update. An
//! always-present "up to date" badge is noise in a strip the user looks at all
//! day, and it trains people to ignore the one message that matters.

use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement,
    Styled, div, px,
};
use gpui_component::h_flex;

use crate::update::UpdateState;

use super::theme;
use super::theme::z;

/// Render the indicator, or an empty element when idle.
pub fn render(state: &UpdateState) -> AnyElement {
    let Some(label) = state.indicator_label() else {
        return div().into_any_element();
    };

    // Ready-to-install is the only state worth colouring: it is the one the
    // user has to act on. Progress and failure stay muted so a stalled or
    // failed update does not shout louder than a finished one.
    let (fg, dot) = match state {
        UpdateState::ReadyToInstall { .. } => (theme::accent(), Some(theme::accent())),
        UpdateState::Failed { .. } => (theme::text_muted(), Some(theme::danger())),
        _ => (theme::text_muted(), None),
    };

    let mut row = h_flex()
        .id("update-indicator")
        .flex_shrink_0()
        .items_center()
        .gap(z(theme::SPACE_2))
        .px(z(theme::SPACE_3))
        .py(z(theme::SPACE_1))
        .rounded(z(theme::RADIUS_PILL))
        .text_size(z(theme::FONT_SIZE_SM));

    if state.is_actionable() {
        row = row.cursor_pointer().hover(|s| s.bg(theme::hover_bg()));
    }

    row.children(dot.map(|colour| {
        div()
            .w(px(6.0))
            .h(px(6.0))
            .flex_shrink_0()
            .rounded(px(999.0))
            .bg(colour)
    }))
    .child(div().text_color(fg).child(label))
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
