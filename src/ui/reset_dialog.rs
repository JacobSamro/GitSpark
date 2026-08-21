use gpui::{Context, Div, ParentElement, StatefulInteractiveElement, Styled, div};
use gpui_component::IconName;

use crate::ui::app::GitSparkApp;
use crate::ui::kit::dialog::{
    DIALOG_WIDTH_WIDE, dialog_body, dialog_footer, dialog_header, dialog_shell,
};
use crate::ui::kit::{ButtonVariant, button, icon_button};
use crate::ui::theme;
use crate::ui::theme::z;
use crate::ui::ui_state::ActiveDialog;

pub(crate) fn render_reset_to_commit_dialog(
    target_oid: &str,
    cx: &mut Context<GitSparkApp>,
) -> Div {
    let target_oid_for_click = target_oid.to_string();
    let short_oid = short_commit_label(target_oid);

    dialog_shell(DIALOG_WIDTH_WIDE)
        .child(dialog_header(
            "Reset to Commit",
            Some((IconName::TriangleAlert, theme::warning())),
            icon_button("reset-to-commit-close", IconName::Close).on_click(cx.listener(
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
                        .child(
                            "You have changes in progress. Resetting to a previous commit might result in some of these changes being lost. Do you want to continue anyway?",
                        ),
                )
                .child(
                    div()
                        .text_size(z(theme::FONT_SIZE_SM))
                        .text_color(theme::text_muted())
                        .child(format!("Target commit: {short_oid}")),
                ),
        )
        .child(
            dialog_footer()
                .child(
                    button("reset-to-commit-cancel", "Cancel", ButtonVariant::Secondary).on_click(
                        cx.listener(|app, _evt, _win, cx| {
                            app.nav.active_dialog = ActiveDialog::None;
                            cx.notify();
                        }),
                    ),
                )
                .child(
                    button("reset-to-commit-confirm", "Continue", ButtonVariant::Danger).on_click(
                        cx.listener(move |app, _evt, _win, cx| {
                            app.reset_to_commit(target_oid_for_click.clone(), cx);
                        }),
                    ),
                ),
        )
}

fn short_commit_label(oid: &str) -> &str {
    oid.get(..7).unwrap_or(oid)
}
