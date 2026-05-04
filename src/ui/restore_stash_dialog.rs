use std::sync::Arc;

use gpui::{
    Context, Div, FontWeight, InteractiveElement, ParentElement, StatefulInteractiveElement,
    Styled, div, prelude::FluentBuilder, px,
};
use gpui_component::{Icon, IconName, h_flex, v_flex};

use crate::models::ChangeEntry;
use crate::ui::app::GitSparkApp;
use crate::ui::stash_file_list::render_stash_file_list;
use crate::ui::theme;
use crate::ui::ui_state::ActiveDialog;

pub(crate) fn render_restore_stash_dialog(
    files: Arc<Vec<ChangeEntry>>,
    cx: &mut Context<GitSparkApp>,
) -> Div {
    let file_count = files.len();
    let can_restore = file_count > 0;

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
                                .child("Restore Stashed Changes"),
                        ),
                )
                .child(
                    div()
                        .id("restore-stash-close")
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
                        .child(format!(
                            "Restore the latest stash with {}?",
                            pluralize_files(file_count)
                        )),
                )
                .child(
                    div()
                        .text_size(theme::z(12.0))
                        .text_color(theme::text_muted())
                        .child("This can modify files in the selected repository and may fail if the current changes conflict."),
                )
                .child(render_stash_file_list(
                    "restore-stash-file-list",
                    "restore-stash-files",
                    "restore-stash-file",
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
                        .id("restore-stash-cancel")
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
                        .id("restore-stash-discard")
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
                                .child("Discard Stash"),
                        )
                        .on_click(cx.listener(|app, _evt, _win, cx| {
                            app.show_discard_stash_dialog(cx);
                        })),
                )
                .child(
                    div()
                        .id("restore-stash-confirm")
                        .px(theme::z(12.0))
                        .py(theme::z(6.0))
                        .rounded(theme::z(theme::CORNER_RADIUS))
                        .bg(if can_restore {
                            theme::commit_button_bg()
                        } else {
                            theme::surface_bg()
                        })
                        .border_1()
                        .border_color(if can_restore {
                            theme::commit_button_bg()
                        } else {
                            theme::surface_bg_alt()
                        })
                        .when(can_restore, |el| {
                            el.cursor_pointer()
                                .hover(|s| s.bg(theme::commit_button_hover_bg()))
                        })
                        .child(
                            div()
                                .text_size(theme::z(12.0))
                                .text_color(if can_restore {
                                    theme::commit_button_text()
                                } else {
                                    theme::text_muted()
                                })
                                .child("Restore Stash"),
                        )
                        .on_click(cx.listener(|app, _evt, _win, cx| {
                            if app.repo.stash_files.is_empty() {
                                app.messages.error_message =
                                    "Load the stashed file list before restoring the stash."
                                        .to_string();
                                cx.notify();
                                return;
                            }
                            app.nav.active_dialog = ActiveDialog::None;
                            app.restore_stash(cx);
                        })),
                ),
        )
}

fn pluralize_files(count: usize) -> String {
    match count {
        1 => "1 file".to_string(),
        count => format!("{count} files"),
    }
}
