use gpui::{
    Context, Div, FontWeight, InteractiveElement, ParentElement, StatefulInteractiveElement,
    Styled, div, px,
};
use gpui_component::{Icon, IconName, h_flex, v_flex};

use crate::ui::app::GitSparkApp;
use crate::ui::theme;
use crate::ui::ui_state::ActiveDialog;

pub(crate) fn render_delete_branch_dialog(branch_name: &str, cx: &mut Context<GitSparkApp>) -> Div {
    let branch_name_for_click = branch_name.to_string();

    v_flex()
        .w(px(440.0))
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
                                .child("Delete Branch"),
                        ),
                )
                .child(
                    div()
                        .id("delete-branch-close")
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
                        .child(format!("Delete branch {branch_name}?")),
                )
                .child(
                    div()
                        .text_size(theme::z(12.0))
                        .text_color(theme::text_muted())
                        .child("This action cannot be undone."),
                ),
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
                        .id("delete-branch-cancel")
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
                        .id("delete-branch-confirm")
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
                                .child("Delete"),
                        )
                        .on_click(cx.listener(move |app, _evt, _win, cx| {
                            app.confirm_delete_branch(branch_name_for_click.clone(), cx);
                        })),
                ),
        )
}
