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

pub(crate) fn render_about_dialog(cx: &mut Context<GitSparkApp>) -> Div {
    dialog_shell(DIALOG_WIDTH)
        .child(dialog_header(
            "About GitSpark",
            None,
            icon_button("about-close", IconName::Close).on_click(cx.listener(
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
                        .text_size(z(18.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme::text_main())
                        .child("GitSpark"),
                )
                .child(
                    div()
                        .text_size(z(theme::FONT_SIZE_BODY))
                        .text_color(theme::text_muted())
                        .child(format!("Version {}", env!("CARGO_PKG_VERSION"))),
                ),
        )
        .child(
            dialog_footer()
                .child(
                    button(
                        "about-changelog",
                        "View Changelog",
                        ButtonVariant::Secondary,
                    )
                    .on_click(cx.listener(|app, _evt, _win, cx| {
                        app.open_changelog(cx);
                    })),
                )
                .child(
                    button("about-close-btn", "Close", ButtonVariant::Primary).on_click(
                        cx.listener(|app, _evt, _win, cx| {
                            app.nav.active_dialog = ActiveDialog::None;
                            cx.notify();
                        }),
                    ),
                ),
        )
}
