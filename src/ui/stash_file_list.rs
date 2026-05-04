use std::sync::Arc;

use gpui::*;
use gpui_component::h_flex;

use crate::models::ChangeEntry;
use crate::ui::theme;

pub fn render_stash_file_list(
    container_id: &'static str,
    list_id: &'static str,
    row_id_prefix: &'static str,
    files: Arc<Vec<ChangeEntry>>,
    empty_message: &'static str,
) -> AnyElement {
    let file_count = files.len();
    if file_count == 0 {
        return div()
            .id(container_id)
            .w_full()
            .p(theme::z(12.0))
            .rounded(theme::z(theme::CORNER_RADIUS))
            .border_1()
            .border_color(theme::border())
            .bg(theme::surface_bg_muted())
            .child(
                div()
                    .text_size(theme::z(12.0))
                    .text_color(theme::text_muted())
                    .child(empty_message),
            )
            .into_any_element();
    }

    div()
        .id(container_id)
        .h(px(128.0))
        .w_full()
        .rounded(theme::z(theme::CORNER_RADIUS))
        .border_1()
        .border_color(theme::border())
        .bg(theme::surface_bg_muted())
        .overflow_hidden()
        .child(
            uniform_list(list_id, file_count, {
                let files = files.clone();
                move |range, _window, _cx| {
                    range
                        .map(|ix| {
                            let file = &files[ix];
                            h_flex()
                                .id(SharedString::from(format!(
                                    "{row_id_prefix}-{}",
                                    stable_ui_id(&file.path)
                                )))
                                .h(px(28.0))
                                .w_full()
                                .px(theme::z(10.0))
                                .items_center()
                                .gap(theme::z(8.0))
                                .border_b_1()
                                .border_color(theme::border())
                                .child(
                                    div()
                                        .w(px(18.0))
                                        .text_size(theme::z(11.0))
                                        .text_color(theme::text_muted())
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(compact_change_status(&file.status)),
                                )
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex_1()
                                        .text_size(theme::z(12.0))
                                        .text_color(theme::text_main())
                                        .truncate()
                                        .child(file.path.clone()),
                                )
                                .into_any_element()
                        })
                        .collect()
                }
            })
            .h_full()
            .with_sizing_behavior(ListSizingBehavior::Infer),
        )
        .into_any_element()
}

fn compact_change_status(status: &str) -> &'static str {
    if status.contains('A') || status.contains('?') {
        "A"
    } else if status.contains('M') {
        "M"
    } else if status.contains('D') {
        "D"
    } else if status.contains('R') {
        "R"
    } else {
        "?"
    }
}

fn stable_ui_id(value: &str) -> String {
    let slug: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();

    if slug.is_empty() {
        "item".to_string()
    } else {
        slug
    }
}
