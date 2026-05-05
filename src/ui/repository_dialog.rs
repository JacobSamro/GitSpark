use std::path::PathBuf;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{Icon, IconName, h_flex, v_flex};
use rfd::FileDialog;

use crate::ui::app::{GitSparkApp, RepositoryField};
use crate::ui::text_field::render_text_content;
use crate::ui::theme;
use crate::ui::ui_state::ActiveDialog;

const CREATE_DIALOG_WIDTH: f32 = 560.0;
const CLONE_DIALOG_WIDTH: f32 = 560.0;

pub(crate) fn render_create_repository_dialog(
    app: &GitSparkApp,
    window: &Window,
    cx: &mut Context<GitSparkApp>,
) -> Div {
    let validation = app.create_repository_validation_message();
    let can_create = validation.is_none();

    div().child(
        v_flex()
            .id("create-repository-dialog")
            .w(px(CREATE_DIALOG_WIDTH))
            .bg(theme::panel_bg())
            .rounded(theme::z(theme::CORNER_RADIUS))
            .border_1()
            .border_color(theme::border())
            .shadow_lg()
            .overflow_hidden()
            .child(render_header(
                "Create a New Repository",
                "create-repository-close",
                cx,
            ))
            .child(
                v_flex()
                    .w_full()
                    .p(theme::z(20.0))
                    .gap(theme::z(14.0))
                    .child(render_repository_input(
                        app,
                        "create-repository-name-input",
                        "Name",
                        RepositoryField::CreateName,
                        "Repository name",
                        window,
                        cx,
                    ))
                    .child(render_repository_input(
                        app,
                        "create-repository-description-input",
                        "Description",
                        RepositoryField::CreateDescription,
                        "Optional description",
                        window,
                        cx,
                    ))
                    .child(render_path_input(
                        app,
                        "create-repository-path-input",
                        "Local path",
                        RepositoryField::CreatePath,
                        "Choose a folder",
                        "create-repository-browse",
                        window,
                        cx,
                    ))
                    .child(render_validation_row(
                        "create-repository-validation-message",
                        validation.as_deref(),
                    )),
            )
            .child(render_footer(
                "create-repository-cancel",
                "create-repository-confirm",
                "Create Repository",
                can_create,
                |app, cx| app.create_repository(cx),
                cx,
            )),
    )
}

pub(crate) fn render_clone_repository_dialog(
    app: &GitSparkApp,
    window: &Window,
    cx: &mut Context<GitSparkApp>,
) -> Div {
    let validation = app.clone_repository_validation_message();
    let can_clone = validation.is_none();

    div().child(
        v_flex()
            .id("clone-repository-dialog")
            .w(px(CLONE_DIALOG_WIDTH))
            .bg(theme::panel_bg())
            .rounded(theme::z(theme::CORNER_RADIUS))
            .border_1()
            .border_color(theme::border())
            .shadow_lg()
            .overflow_hidden()
            .child(render_header(
                "Clone a Repository",
                "clone-repository-close",
                cx,
            ))
            .child(
                v_flex()
                    .w_full()
                    .p(theme::z(20.0))
                    .gap(theme::z(14.0))
                    .child(render_repository_input(
                        app,
                        "clone-repository-url-input",
                        "Repository URL",
                        RepositoryField::CloneUrl,
                        "https://github.com/owner/repository.git",
                        window,
                        cx,
                    ))
                    .child(render_path_input(
                        app,
                        "clone-repository-path-input",
                        "Local path",
                        RepositoryField::ClonePath,
                        "Choose an empty destination folder",
                        "clone-repository-browse",
                        window,
                        cx,
                    ))
                    .child(render_validation_row(
                        "clone-repository-validation-message",
                        validation.as_deref(),
                    )),
            )
            .child(render_footer(
                "clone-repository-cancel",
                "clone-repository-confirm",
                "Clone",
                can_clone,
                |app, cx| app.clone_repository(cx),
                cx,
            )),
    )
}

fn render_header(
    title: &'static str,
    close_id: &'static str,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
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
                .child(title),
        )
        .child(
            div()
                .id(close_id)
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

fn render_repository_input(
    app: &GitSparkApp,
    id: &'static str,
    label: &'static str,
    field: RepositoryField,
    placeholder: &'static str,
    window: &Window,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(theme::z(6.0))
        .child(render_label(label))
        .child(render_text_box(app, id, field, placeholder, window, cx))
}

fn render_path_input(
    app: &GitSparkApp,
    id: &'static str,
    label: &'static str,
    field: RepositoryField,
    placeholder: &'static str,
    browse_id: &'static str,
    window: &Window,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(theme::z(6.0))
        .child(render_label(label))
        .child(
            h_flex()
                .w_full()
                .gap(theme::z(8.0))
                .child(render_text_box(app, id, field, placeholder, window, cx).flex_1())
                .child(render_browse_button(browse_id, field, cx)),
        )
}

fn render_label(label: &'static str) -> impl IntoElement {
    div()
        .text_size(theme::z(12.0))
        .text_color(theme::text_muted())
        .child(label)
}

fn render_validation_row(id: &'static str, message: Option<&str>) -> impl IntoElement {
    div()
        .id(id)
        .h(theme::z(16.0))
        .text_size(theme::z(11.0))
        .text_color(theme::danger())
        .child(message.unwrap_or("").to_string())
}

fn render_text_box(
    app: &GitSparkApp,
    id: &'static str,
    field: RepositoryField,
    placeholder: &'static str,
    window: &Window,
    cx: &mut Context<GitSparkApp>,
) -> Stateful<Div> {
    let value = app.repository_field_value(field);
    let focused = app.repository_field_focused(field, window);
    let border = if focused {
        theme::accent()
    } else {
        theme::surface_bg_alt()
    };

    div()
        .id(id)
        .track_focus(&app.repository_focus)
        .key_context("text-field")
        .on_key_down(cx.listener(GitSparkApp::handle_repository_key))
        .w_full()
        .h(theme::z(30.0))
        .flex()
        .items_center()
        .bg(theme::bg())
        .border_1()
        .border_color(border)
        .px(theme::z(8.0))
        .rounded(theme::z(theme::CORNER_RADIUS))
        .cursor_text()
        .child(render_text_content(
            value,
            app.repository_field_cursor(field),
            app.repository_field_selection(field),
            focused,
            placeholder,
            false,
        ))
        .on_click(cx.listener(move |app, _evt, window, cx| {
            app.activate_repository_field(field, window, cx);
        }))
}

fn render_browse_button(
    id: &'static str,
    field: RepositoryField,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
    div()
        .id(id)
        .px(theme::z(12.0))
        .h(theme::z(30.0))
        .flex()
        .items_center()
        .justify_center()
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
                .child("Choose..."),
        )
        .on_click(cx.listener(move |app, _evt, _win, cx| {
            let Some(path) = FileDialog::new().pick_folder() else {
                return;
            };
            let path = normalize_dialog_path(path);
            match field {
                RepositoryField::CreatePath => {
                    app.repo.create_repo_path = path;
                    app.repository_create_path_cursor = app.repo.create_repo_path.len();
                    app.repository_create_path_selection = None;
                    app.repository_active_field = Some(RepositoryField::CreatePath);
                }
                RepositoryField::ClonePath => {
                    app.repo.clone_repo_path = path;
                    app.repository_clone_path_cursor = app.repo.clone_repo_path.len();
                    app.repository_clone_path_selection = None;
                    app.repository_active_field = Some(RepositoryField::ClonePath);
                }
                _ => {}
            }
            cx.notify();
        }))
}

fn render_footer(
    cancel_id: &'static str,
    confirm_id: &'static str,
    confirm_label: &'static str,
    enabled: bool,
    confirm: fn(&mut GitSparkApp, &mut Context<GitSparkApp>),
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
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
                .id(cancel_id)
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
                .id(confirm_id)
                .px(theme::z(12.0))
                .py(theme::z(6.0))
                .rounded(theme::z(theme::CORNER_RADIUS))
                .bg(if enabled {
                    theme::commit_button_bg()
                } else {
                    theme::surface_bg()
                })
                .border_1()
                .border_color(if enabled {
                    theme::commit_button_bg()
                } else {
                    theme::surface_bg_alt()
                })
                .when(enabled, |el| {
                    el.cursor_pointer()
                        .hover(|s| s.bg(theme::commit_button_hover_bg()))
                })
                .child(
                    div()
                        .text_size(theme::z(12.0))
                        .text_color(if enabled {
                            theme::commit_button_text()
                        } else {
                            theme::text_muted()
                        })
                        .child(confirm_label),
                )
                .on_click(cx.listener(move |app, _evt, _win, cx| {
                    if enabled {
                        confirm(app, cx);
                    } else {
                        cx.notify();
                    }
                })),
        )
}

fn normalize_dialog_path(path: PathBuf) -> String {
    path.to_string_lossy().to_string()
}
