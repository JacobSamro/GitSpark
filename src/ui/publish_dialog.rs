use gpui::*;
use gpui_component::{Icon, IconName, h_flex, v_flex};

use crate::ui::app::{GitSparkApp, PublishField};
use crate::ui::theme;
use crate::ui::ui_state::ActiveDialog;

pub(crate) const PUBLISH_DIALOG_WIDTH: f32 = 560.0;
pub(crate) const PUBLISH_DIALOG_HEIGHT: f32 = 480.0;

pub(crate) fn render_publish_dialog(
    app: &GitSparkApp,
    window: &Window,
    cx: &mut Context<GitSparkApp>,
) -> Div {
    let publish_enabled =
        !app.network.publish_name.trim().is_empty() && app.network.active_action.is_none();

    v_flex()
        .w(px(PUBLISH_DIALOG_WIDTH))
        .bg(theme::panel_bg())
        .rounded(theme::z(theme::CORNER_RADIUS))
        .border_1()
        .border_color(theme::border())
        .shadow_lg()
        .overflow_hidden()
        .child(render_header(cx))
        .child(render_tabs())
        .child(
            v_flex()
                .w_full()
                .p(theme::z(20.0))
                .gap(theme::z(14.0))
                .child(render_publish_input(
                    app,
                    "publish-repo-name",
                    "Name",
                    PublishField::Name,
                    "Repository name",
                    window,
                    cx,
                ))
                .child(render_publish_input(
                    app,
                    "publish-repo-description",
                    "Description",
                    PublishField::Description,
                    "",
                    window,
                    cx,
                ))
                .child(render_private_checkbox(app, cx))
                .child(render_organization_dropdown()),
        )
        .child(render_footer(publish_enabled, cx))
}

fn render_header(cx: &mut Context<GitSparkApp>) -> impl IntoElement {
    h_flex()
        .w_full()
        .px(theme::z(20.0))
        .py(theme::z(14.0))
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(theme::border())
        .child(
            div()
                .text_size(theme::z(16.0))
                .text_color(theme::text_main())
                .font_weight(FontWeight::BOLD)
                .child("Publish Repository"),
        )
        .child(
            div()
                .id("publish-dialog-close")
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
        )
}

fn render_tabs() -> impl IntoElement {
    h_flex()
        .w_full()
        .h(theme::z(38.0))
        .border_b_1()
        .border_color(theme::border())
        .child(
            div()
                .id("publish-tab-github")
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .border_b_2()
                .border_color(theme::accent())
                .text_size(theme::z(13.0))
                .text_color(theme::text_main())
                .font_weight(FontWeight::SEMIBOLD)
                .child("GitHub.com"),
        )
        .child(
            div()
                .id("publish-tab-enterprise")
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(theme::z(13.0))
                .text_color(theme::text_muted())
                .child("GitHub Enterprise"),
        )
}

fn render_publish_input(
    app: &GitSparkApp,
    id: &'static str,
    label: &'static str,
    field: PublishField,
    placeholder: &'static str,
    window: &Window,
    cx: &mut Context<GitSparkApp>,
) -> Div {
    let (value, cursor, selection) = match field {
        PublishField::Name => (
            app.network.publish_name.as_str(),
            app.publish_name_cursor,
            app.publish_name_selection,
        ),
        PublishField::Description => (
            app.network.publish_description.as_str(),
            app.publish_description_cursor,
            app.publish_description_selection,
        ),
    };
    let focused = app.publish_focus.is_focused(window) && app.publish_active_field == Some(field);
    let text = crate::ui::text_field::render_text_content(
        value,
        cursor.min(value.len()),
        selection,
        focused,
        placeholder,
        false,
    );

    v_flex()
        .w_full()
        .gap(theme::z(6.0))
        .child(
            div()
                .text_size(theme::z(12.0))
                .text_color(theme::text_main())
                .child(label),
        )
        .child(
            div()
                .id(id)
                .track_focus(&app.publish_focus)
                .key_context("text-field")
                .on_key_down(cx.listener(GitSparkApp::handle_publish_key))
                .w_full()
                .h(theme::z(32.0))
                .px(theme::z(10.0))
                .flex()
                .items_center()
                .rounded(theme::z(theme::CORNER_RADIUS))
                .bg(theme::bg())
                .border_1()
                .border_color(if focused {
                    theme::accent()
                } else {
                    theme::border()
                })
                .cursor_text()
                .child(text)
                .on_click(cx.listener(move |app, _evt, window, cx| {
                    app.publish_active_field = Some(field);
                    match field {
                        PublishField::Name => {
                            app.publish_name_cursor = app.network.publish_name.len();
                            app.publish_name_selection = None;
                        }
                        PublishField::Description => {
                            app.publish_description_cursor = app.network.publish_description.len();
                            app.publish_description_selection = None;
                        }
                    }
                    window.focus(&app.publish_focus);
                    cx.notify();
                })),
        )
}

fn render_private_checkbox(app: &GitSparkApp, cx: &mut Context<GitSparkApp>) -> impl IntoElement {
    h_flex()
        .id("publish-repo-private")
        .w_full()
        .gap(theme::z(8.0))
        .items_center()
        .cursor_pointer()
        .child(
            div()
                .w(theme::z(14.0))
                .h(theme::z(14.0))
                .rounded(theme::z(3.0))
                .border_1()
                .border_color(theme::text_muted())
                .bg(if app.network.publish_private {
                    theme::accent()
                } else {
                    gpui::transparent_black()
                })
                .flex()
                .items_center()
                .justify_center()
                .children(if app.network.publish_private {
                    Some(
                        Icon::new(IconName::Check)
                            .size(theme::z(10.0))
                            .text_color(theme::commit_button_text()),
                    )
                } else {
                    None
                }),
        )
        .child(
            div()
                .text_size(theme::z(13.0))
                .text_color(theme::text_main())
                .child("Keep this code private"),
        )
        .on_click(cx.listener(|app, _evt, _win, cx| {
            app.network.publish_private = !app.network.publish_private;
            cx.notify();
        }))
}

fn render_organization_dropdown() -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(theme::z(6.0))
        .child(
            div()
                .text_size(theme::z(12.0))
                .text_color(theme::text_main())
                .child("Organization"),
        )
        .child(
            h_flex()
                .id("publish-repo-organization")
                .w_full()
                .h(theme::z(32.0))
                .px(theme::z(10.0))
                .items_center()
                .justify_between()
                .rounded(theme::z(theme::CORNER_RADIUS))
                .bg(theme::bg())
                .border_1()
                .border_color(theme::border())
                .child(
                    div()
                        .text_size(theme::z(13.0))
                        .text_color(theme::text_main())
                        .child("None"),
                )
                .child(
                    Icon::new(IconName::ChevronDown)
                        .size(theme::z(12.0))
                        .text_color(theme::text_muted()),
                ),
        )
}

fn render_footer(publish_enabled: bool, cx: &mut Context<GitSparkApp>) -> impl IntoElement {
    h_flex()
        .w_full()
        .px(theme::z(20.0))
        .py(theme::z(14.0))
        .justify_end()
        .gap(theme::z(8.0))
        .border_t_1()
        .border_color(theme::border())
        .child(
            div()
                .id("publish-cancel")
                .px(theme::z(18.0))
                .py(theme::z(7.0))
                .rounded(theme::z(theme::CORNER_RADIUS))
                .bg(theme::surface_bg())
                .border_1()
                .border_color(theme::surface_bg_alt())
                .cursor_pointer()
                .hover(|s| s.bg(theme::toolbar_hover_bg()))
                .child(
                    div()
                        .text_size(theme::z(13.0))
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
                .id("publish-confirm")
                .px(theme::z(18.0))
                .py(theme::z(7.0))
                .rounded(theme::z(theme::CORNER_RADIUS))
                .bg(if publish_enabled {
                    theme::commit_button_bg()
                } else {
                    theme::surface_bg_alt()
                })
                .cursor_pointer()
                .hover(move |s| {
                    if publish_enabled {
                        s.bg(theme::commit_button_hover_bg())
                    } else {
                        s
                    }
                })
                .child(
                    div()
                        .text_size(theme::z(13.0))
                        .text_color(if publish_enabled {
                            theme::commit_button_text()
                        } else {
                            theme::text_muted()
                        })
                        .child("Publish Repository"),
                )
                .on_click(cx.listener(|app, _evt, _win, cx| {
                    if !app.network.publish_name.trim().is_empty() {
                        app.publish_repository(cx);
                    }
                })),
        )
}
