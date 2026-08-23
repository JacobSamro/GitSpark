//! A small, transient confirmation banner for quick actions (clipboard
//! copies, etc.) where the status bar text alone is easy to miss.

use gpui::{Div, InteractiveElement, ParentElement, Styled, div};

use crate::ui::theme;

pub fn render_toast(message: &str) -> Div {
    div()
        .absolute()
        .bottom(theme::z(48.0))
        .left_0()
        .right_0()
        .flex()
        .justify_center()
        .child(
            div()
                .id("toast")
                .flex()
                .items_center()
                .gap(theme::z(6.0))
                .px(theme::z(12.0))
                .py(theme::z(8.0))
                .bg(theme::panel_bg())
                .border_1()
                .border_color(theme::border())
                .rounded(theme::z(theme::CORNER_RADIUS))
                .shadow_lg()
                .text_size(theme::z(theme::FONT_SIZE_BODY))
                .text_color(theme::text_main())
                .child("✓")
                .child(message.to_string()),
        )
}
