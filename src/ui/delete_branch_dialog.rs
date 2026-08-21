use gpui::{Context, Div, ParentElement, StatefulInteractiveElement, Styled, div};
use gpui_component::IconName;

use crate::ui::app::GitSparkApp;
use crate::ui::kit::dialog::{
    DIALOG_WIDTH, dialog_body, dialog_footer, dialog_header, dialog_shell,
};
use crate::ui::kit::{ButtonVariant, button, icon_button};
use crate::ui::theme;
use crate::ui::theme::z;
use crate::ui::ui_state::ActiveDialog;

pub(crate) fn render_delete_branch_dialog(branch_name: &str, cx: &mut Context<GitSparkApp>) -> Div {
    let branch_name_for_click = branch_name.to_string();

    dialog_shell(DIALOG_WIDTH)
        .child(dialog_header(
            "Delete Branch",
            Some((IconName::TriangleAlert, theme::warning())),
            icon_button("delete-branch-close", IconName::Close).on_click(cx.listener(
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
                        .child(format!("Delete branch {branch_name}?")),
                )
                .child(
                    div()
                        .text_size(z(theme::FONT_SIZE_BODY))
                        .text_color(theme::text_muted())
                        .child("This action cannot be undone."),
                ),
        )
        .child(
            dialog_footer()
                .child(
                    button("delete-branch-cancel", "Cancel", ButtonVariant::Secondary).on_click(
                        cx.listener(|app, _evt, _win, cx| {
                            app.nav.active_dialog = ActiveDialog::None;
                            cx.notify();
                        }),
                    ),
                )
                .child(
                    button("delete-branch-confirm", "Delete", ButtonVariant::Danger).on_click(
                        cx.listener(move |app, _evt, _win, cx| {
                            app.confirm_delete_branch(branch_name_for_click.clone(), cx);
                        }),
                    ),
                ),
        )
}
