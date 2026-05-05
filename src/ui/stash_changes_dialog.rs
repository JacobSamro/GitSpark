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

pub(crate) fn render_stash_changes_dialog(
    files: Arc<Vec<ChangeEntry>>,
    replaces_existing_stash: bool,
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
                            Icon::new(IconName::Inbox)
                                .size(px(16.0))
                                .text_color(theme::accent()),
                        )
                        .child(
                            div()
                                .text_size(theme::z(14.0))
                                .text_color(theme::text_main())
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Stash Changes"),
                        ),
                )
                .child(
                    div()
                        .id("stash-changes-close")
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
                        .child("Stash all current changes?"),
                )
                .child(
                    div()
                        .text_size(theme::z(12.0))
                        .text_color(theme::text_muted())
                        .child("These files will be saved in a new stash and removed from the working tree."),
                )
                .children(replaces_existing_stash.then(|| {
                    h_flex()
                        .id("stash-changes-replace-warning")
                        .w_full()
                        .gap(theme::z(8.0))
                        .items_start()
                        .p(theme::z(10.0))
                        .rounded(theme::z(theme::CORNER_RADIUS))
                        .border_1()
                        .border_color(theme::warning())
                        .bg(theme::warning_bg())
                        .child(
                            Icon::new(IconName::TriangleAlert)
                                .size(px(14.0))
                                .text_color(theme::warning()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_size(theme::z(12.0))
                                .text_color(theme::text_main())
                                .child("Stashing will replace the existing GitSpark stash for this branch."),
                        )
                }))
                .child(render_stash_file_list(
                    "stash-changes-file-list",
                    "stash-changes-files",
                    "stash-changes-file",
                    files,
                    "There are no local changes to stash.",
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
                        .id("stash-changes-cancel")
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
                        .id("stash-changes-confirm")
                        .px(theme::z(12.0))
                        .py(theme::z(6.0))
                        .rounded(theme::z(theme::CORNER_RADIUS))
                        .bg(theme::commit_button_bg())
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::commit_button_hover_bg()))
                        .child(
                            div()
                                .text_size(theme::z(12.0))
                                .text_color(theme::commit_button_text())
                                .child("Stash Changes"),
                        )
                        .on_click(cx.listener(|app, _evt, _win, cx| {
                            app.stash_changes(cx);
                        })),
                ),
        )
}
