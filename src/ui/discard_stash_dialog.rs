use std::sync::Arc;

use gpui::{
    Context, Div, FontWeight, InteractiveElement, ParentElement, StatefulInteractiveElement,
    Styled, div, px,
};
use gpui_component::{Icon, IconName, h_flex, v_flex};

use crate::models::ChangeEntry;
use crate::ui::app::GitSparkApp;
use crate::ui::stash_file_list::render_stash_file_list;
use crate::ui::theme;
use crate::ui::ui_state::ActiveDialog;

pub(crate) fn render_discard_stash_dialog(
    files: Arc<Vec<ChangeEntry>>,
    cx: &mut Context<GitSparkApp>,
) -> Div {
    v_flex()
        .w(px(500.0))
        .bg(theme::panel_bg())
        .rounded(theme::z(theme::CORNER_RADIUS))
        .border_1()
        .border_color(theme::border())
        .shadow_lg()
        .child(
            h_flex()
                .w_full()
                .px(theme::z(16.0))
                .py(theme::z(12.0))
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(theme::border())
                .child(
                    h_flex()
                        .items_center()
                        .gap(theme::z(8.0))
                        .child(
                            Icon::new(IconName::TriangleAlert)
                                .size(px(16.0))
                                .text_color(theme::warning()),
                        )
                        .child(
                            div()
                                .text_size(theme::z(14.0))
                                .text_color(theme::text_main())
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Discard Stash"),
                        ),
                )
                .child(
                    div()
                        .id("discard-stash-close")
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::hover_bg()))
                        .rounded(px(4.0))
                        .p(px(4.0))
                        .child(
                            Icon::new(IconName::Close)
                                .size(px(14.0))
                                .text_color(theme::text_muted()),
                        )
                        .on_click(cx.listener(|app, _evt, _win, cx| {
                            app.nav.active_dialog = ActiveDialog::None;
                            cx.notify();
                        })),
                ),
        )
        .child(
            v_flex()
                .w_full()
                .p(theme::z(16.0))
                .gap(theme::z(10.0))
                .child(
                    div()
                        .text_size(theme::z(12.0))
                        .text_color(theme::text_main())
                        .child("Discard the latest stash?"),
                )
                .child(
                    div()
                        .text_size(theme::z(12.0))
                        .text_color(theme::text_muted())
                        .child("This permanently removes the stashed changes listed below."),
                )
                .child(render_stash_file_list(
                    "discard-stash-file-list",
                    "discard-stash-files",
                    "discard-stash-file",
                    files,
                    "No file list is available for this stash.",
                )),
        )
        .child(
            h_flex()
                .w_full()
                .px(theme::z(16.0))
                .py(theme::z(12.0))
                .justify_end()
                .gap(theme::z(8.0))
                .border_t_1()
                .border_color(theme::border())
                .child(
                    div()
                        .id("discard-stash-cancel")
                        .px(theme::z(12.0))
                        .py(theme::z(6.0))
                        .rounded(theme::z(theme::CORNER_RADIUS))
                        .bg(theme::surface_bg())
                        .border_1()
                        .border_color(theme::surface_bg_alt())
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::toolbar_hover_bg()))
                        .child(
                            div()
                                .text_size(theme::z(12.0))
                                .text_color(theme::text_main())
                                .child("Cancel"),
                        )
                        .on_click(cx.listener(|app, _evt, _win, cx| {
                            app.nav.active_dialog = ActiveDialog::None;
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .id("discard-stash-confirm")
                        .px(theme::z(12.0))
                        .py(theme::z(6.0))
                        .rounded(theme::z(theme::CORNER_RADIUS))
                        .bg(theme::danger())
                        .cursor_pointer()
                        .hover(|s| s.bg(gpui::Hsla::from(gpui::rgb(0xff6961))))
                        .child(
                            div()
                                .text_size(theme::z(12.0))
                                .text_color(gpui::white())
                                .child("Discard Stash"),
                        )
                        .on_click(cx.listener(|app, _evt, _win, cx| {
                            app.discard_stash(cx);
                        })),
                ),
        )
}
