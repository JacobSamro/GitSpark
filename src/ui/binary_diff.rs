use gpui::*;
use gpui_component::{h_flex, v_flex};

use crate::ui::GitSparkApp;
use crate::ui::theme::z;
use crate::ui::{labels, theme};

pub(crate) fn render_binary_diff_panel(file_path: &str, view: Option<&Entity<GitSparkApp>>) -> Div {
    let mut panel = v_flex()
        .items_center()
        .gap(z(10.0))
        .text_size(z(14.0))
        .text_color(theme::text_muted())
        .child("This binary file has changed.");

    if let Some(vh) = view {
        let reveal_view = vh.clone();
        let reveal_path = file_path.to_string();
        let open_view = vh.clone();
        let open_path = file_path.to_string();

        panel = panel.child(
            h_flex()
                .gap(z(8.0))
                .child(binary_action_button(
                    "diff-binary-reveal",
                    labels::reveal_in_file_manager_menu(),
                    move |_evt, _win, cx| {
                        let path = reveal_path.clone();
                        reveal_view.update(cx, |app, cx| {
                            app.reveal_in_finder(&path);
                            cx.notify();
                        });
                    },
                ))
                .child(binary_action_button(
                    "diff-binary-open-default",
                    "Open Anyway",
                    move |_evt, _win, cx| {
                        let path = open_path.clone();
                        open_view.update(cx, |app, cx| {
                            app.open_with_default_program(&path);
                            cx.notify();
                        });
                    },
                )),
        );
    }

    panel
}

fn binary_action_button(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    h_flex()
        .id(id)
        .h(z(28.0))
        .px(z(12.0))
        .items_center()
        .justify_center()
        .rounded(z(theme::CORNER_RADIUS_SM))
        .border_1()
        .border_color(theme::border())
        .bg(theme::surface_bg())
        .text_size(z(12.0))
        .text_color(theme::text_main())
        .cursor_pointer()
        .hover(|style| style.bg(theme::toolbar_hover_bg()))
        .child(label)
        .on_click(on_click)
}
