//! A small, transient confirmation banner for quick actions (clipboard
//! copies, etc.) where the status bar text alone is easy to miss.

use gpui::{Div, InteractiveElement, ParentElement, Styled, div, px};

use crate::ui::theme;

pub fn render_toast(message: &str) -> Div {
    div()
        .absolute()
        .bottom(px(48.0))
        .left_0()
        .right_0()
        .flex()
        .justify_center()
        .child(
            div()
                .id("toast")
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(12.0))
                .py(px(8.0))
                .bg(theme::panel_bg())
                .border_1()
                .border_color(theme::border())
                .rounded(theme::z(theme::CORNER_RADIUS))
                .shadow_lg()
                .text_size(px(12.0))
                .text_color(theme::text_main())
                .child("✓")
                .child(message.to_string()),
        )
}
