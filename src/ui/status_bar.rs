use gpui::*;
use gpui_component::{Icon, IconName, h_flex};

use crate::ui::theme;
use crate::ui::theme::z;

pub fn render_status_bar(
    status_message: &str,
    error_message: &str,
    branch_name: Option<&str>,
    change_count: usize,
) -> Div {
    let has_error = !error_message.is_empty();

    let (text, color) = if has_error {
        (error_message.to_string(), theme::danger())
    } else {
        (status_message.to_string(), theme::text_muted())
    };

    let mut bar = h_flex()
        .w_full()
        .h(z(theme::STATUS_BAR_HEIGHT))
        .flex_shrink_0()
        .bg(theme::panel_bg())
        .border_t_1()
        .border_color(theme::border())
        .px(z(12.0))
        .items_center()
        .gap(z(12.0))
        .child(div().text_size(z(11.0)).text_color(color).child(text))
        .child(div().flex_1()); // spacer

    // Right side: branch name + change count
    if let Some(branch) = branch_name {
        bar = bar.child(
            h_flex()
                .items_center()
                .gap(z(4.0))
                .child(
                    Icon::new(IconName::GitHub)
                        .size(z(12.0))
                        .text_color(theme::text_muted()),
                )
                .child(
                    div()
                        .text_size(z(11.0))
                        .text_color(theme::text_muted())
                        .child(branch.to_string()),
                ),
        );
    }

    if change_count > 0 {
        bar = bar.child(
            div()
                .text_size(z(11.0))
                .text_color(theme::text_muted())
                .child(format!("{change_count} changed")),
        );
    }

    bar
}
