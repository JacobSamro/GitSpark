use std::sync::Arc;

use gpui::{Context, Div, ParentElement, StatefulInteractiveElement, Styled, div};
use gpui_component::IconName;

use crate::models::ChangeEntry;
use crate::ui::app::GitSparkApp;
use crate::ui::kit::dialog::{
    DIALOG_WIDTH_WIDE, dialog_body, dialog_footer, dialog_header, dialog_shell,
};
use crate::ui::kit::{ButtonVariant, button, button_state, icon_button};
use crate::ui::stash_file_list::render_stash_file_list;
use crate::ui::theme;
use crate::ui::theme::z;
use crate::ui::ui_state::ActiveDialog;

pub(crate) fn render_discard_stash_dialog(
    files: Arc<Vec<ChangeEntry>>,
    cx: &mut Context<GitSparkApp>,
) -> Div {
    let can_discard = !files.is_empty();

    dialog_shell(DIALOG_WIDTH_WIDE)
        .child(dialog_header(
            "Discard Stash",
            Some((IconName::TriangleAlert, theme::warning())),
            icon_button("discard-stash-close", IconName::Close).on_click(cx.listener(
                |app, _evt, _win, cx| {
                    app.nav.active_dialog = ActiveDialog::None;
                    cx.notify();
                },
            )),
        ))
        .child(
            dialog_body()
                .child(
                    div()
                        .text_size(z(theme::FONT_SIZE_BODY))
                        .text_color(theme::text_main())
                        .child("Discard this branch stash?"),
                )
                .child(
                    div()
                        .text_size(z(theme::FONT_SIZE_BODY))
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
            dialog_footer()
                .child(
                    button("discard-stash-cancel", "Cancel", ButtonVariant::Secondary).on_click(
                        cx.listener(|app, _evt, _win, cx| {
                            app.nav.active_dialog = ActiveDialog::None;
                            cx.notify();
                        }),
                    ),
                )
                .child(
                    button_state(
                        "discard-stash-confirm",
                        "Discard Stash",
                        ButtonVariant::Danger,
                        can_discard,
                    )
                    .on_click(cx.listener(|app, _evt, _win, cx| {
                        if app.repo.stash_files.is_empty() {
                            app.messages.error_message =
                                "Load the stashed file list before discarding the stash."
                                    .to_string();
                            cx.notify();
                            return;
                        }
                        app.discard_stash(cx);
                    })),
                ),
        )
}
