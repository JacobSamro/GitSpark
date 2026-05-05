use super::helpers::*;
use super::*;

pub(super) fn render_create_tag_dialog(
    app: &GitSparkApp,
    target_oid: &str,
    cx: &mut Context<GitSparkApp>,
) -> Div {
    let tag_name = &app.repo.new_branch_name;
    let target_oid_for_click = target_oid.to_string();
    let short_oid = short_commit_label(target_oid);
    let validation_message = app.create_tag_validation_message();
    let show_validation_message = !tag_name.trim().is_empty();
    let can_create = validation_message.is_none();

    v_flex()
        .w(px(400.0))
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
                    div()
                        .text_size(theme::z(14.0))
                        .text_color(theme::text_main())
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Create a Tag"),
                )
                .child(
                    div()
                        .id("create-tag-close")
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
                .gap(theme::z(12.0))
                .child(
                    v_flex()
                        .gap(theme::z(4.0))
                        .child(
                            div()
                                .text_size(theme::z(12.0))
                                .text_color(theme::text_muted())
                                .child("Name"),
                        )
                        .child(
                            div()
                                .id("create-tag-name-input")
                                .track_focus(&app.new_branch_focus)
                                .key_context("text-field")
                                .on_key_down(cx.listener(GitSparkApp::handle_new_branch_key))
                                .w_full()
                                .h(theme::z(28.0))
                                .px(theme::z(8.0))
                                .flex()
                                .items_center()
                                .rounded(theme::z(theme::CORNER_RADIUS))
                                .bg(theme::bg())
                                .border_1()
                                .border_color(theme::accent())
                                .cursor_text()
                                .child(
                                    div()
                                        .text_size(theme::z(12.0))
                                        .text_color(if tag_name.is_empty() {
                                            theme::text_muted()
                                        } else {
                                            theme::text_main()
                                        })
                                        .child(if tag_name.is_empty() {
                                            "v1.0.0".to_string()
                                        } else {
                                            tag_name.clone()
                                        }),
                                )
                                .on_click(cx.listener(|app, _evt, window, cx| {
                                    window.focus(&app.new_branch_focus);
                                    app.new_branch_cursor = app.repo.new_branch_name.len();
                                    app.new_branch_selection = None;
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .text_size(theme::z(11.0))
                        .text_color(theme::text_muted())
                        .child(format!("Target commit: {short_oid}")),
                )
                .children(
                    validation_message
                        .as_ref()
                        .filter(|_| show_validation_message)
                        .map(|message| {
                            div()
                                .id("create-tag-validation-message")
                                .text_size(theme::z(11.0))
                                .text_color(theme::danger())
                                .child(message.clone())
                        }),
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
                        .id("create-tag-cancel")
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
                        .id("create-tag-confirm")
                        .px(theme::z(12.0))
                        .py(theme::z(6.0))
                        .rounded(theme::z(theme::CORNER_RADIUS))
                        .bg(if can_create {
                            theme::commit_button_bg()
                        } else {
                            theme::surface_bg()
                        })
                        .border_1()
                        .border_color(if can_create {
                            theme::commit_button_bg()
                        } else {
                            theme::surface_bg_alt()
                        })
                        .when(can_create, |el| {
                            el.cursor_pointer()
                                .hover(|s| s.bg(theme::commit_button_hover_bg()))
                        })
                        .child(
                            div()
                                .text_size(theme::z(12.0))
                                .text_color(if can_create {
                                    theme::commit_button_text()
                                } else {
                                    theme::text_muted()
                                })
                                .child("Create Tag"),
                        )
                        .on_click(cx.listener(move |app, _evt, _win, cx| {
                            if app.create_tag_validation_message().is_some() {
                                cx.notify();
                                return;
                            }
                            app.create_tag(target_oid_for_click.clone(), cx);
                        })),
                ),
        )
}

pub(super) fn render_delete_tag_dialog(tag_name: &str, cx: &mut Context<GitSparkApp>) -> Div {
    let tag_name_for_click = tag_name.to_string();
    v_flex()
        .w(px(420.0))
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
                    div()
                        .text_size(theme::z(14.0))
                        .text_color(theme::text_main())
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Delete Tag"),
                )
                .child(
                    div()
                        .id("delete-tag-close")
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
                        .id("delete-tag-confirmation")
                        .text_size(theme::z(12.0))
                        .line_height(theme::z(18.0))
                        .text_color(theme::text_main())
                        .child(format!(
                            "Are you sure you want to delete the tag '{tag_name}'?"
                        )),
                )
                .child(
                    div()
                        .text_size(theme::z(11.0))
                        .text_color(theme::text_muted())
                        .child("This removes the local tag from this repository."),
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
                        .id("delete-tag-cancel")
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
                        .id("delete-tag-confirm")
                        .px(theme::z(12.0))
                        .py(theme::z(6.0))
                        .rounded(theme::z(theme::CORNER_RADIUS))
                        .bg(theme::danger())
                        .border_1()
                        .border_color(theme::danger())
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::danger_hover()))
                        .child(
                            div()
                                .text_size(theme::z(12.0))
                                .text_color(theme::commit_button_text())
                                .child("Delete"),
                        )
                        .on_click(cx.listener(move |app, _evt, _win, cx| {
                            app.delete_tag(tag_name_for_click.clone(), cx);
                        })),
                ),
        )
}

pub(super) fn render_choose_tag_to_delete_dialog(
    app: &GitSparkApp,
    target_oid: &str,
    cx: &mut Context<GitSparkApp>,
) -> Div {
    let tags = app.commit_tags_for_oid(target_oid);
    let short_oid = short_commit_label(target_oid).to_string();
    let view = cx.entity().clone();
    let tag_count = tags.len();

    v_flex()
        .w(px(420.0))
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
                    div()
                        .text_size(theme::z(14.0))
                        .text_color(theme::text_main())
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Delete Tag"),
                )
                .child(
                    div()
                        .id("choose-delete-tag-close")
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
                        .id("choose-delete-tag-description")
                        .text_size(theme::z(12.0))
                        .line_height(theme::z(18.0))
                        .text_color(theme::text_main())
                        .child(format!(
                            "Choose which tag to delete from commit {short_oid}."
                        )),
                )
                .child(
                    div()
                        .id("choose-delete-tag-list-scroll")
                        .w_full()
                        .h(px(160.0))
                        .overflow_y_scrollbar()
                        .rounded(theme::z(theme::CORNER_RADIUS))
                        .border_1()
                        .border_color(theme::border())
                        .bg(theme::bg())
                        .child(
                            uniform_list("choose-delete-tag-list", tag_count, {
                                let tags = tags.clone();
                                move |range, _win, _cx| {
                                    range
                                        .map(|ix| {
                                            let tag_name = tags[ix].clone();
                                            let tag_id = stable_id_slug(&tag_name);
                                            let row_view = view.clone();

                                            h_flex()
                                                .id(SharedString::from(format!(
                                                    "choose-delete-tag-{tag_id}"
                                                )))
                                                .w_full()
                                                .h(px(36.0))
                                                .px(theme::z(12.0))
                                                .items_center()
                                                .cursor_pointer()
                                                .hover(|s| s.bg(theme::hover_bg()))
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .min_w_0()
                                                        .text_size(theme::z(12.0))
                                                        .text_color(theme::text_main())
                                                        .whitespace_nowrap()
                                                        .child(tag_name.clone()),
                                                )
                                                .on_click(move |_evt, _win, cx| {
                                                    let tag_name = tag_name.clone();
                                                    row_view.update(cx, |app, cx| {
                                                        app.nav.active_dialog =
                                                            ActiveDialog::DeleteTag { tag_name };
                                                        cx.notify();
                                                    });
                                                })
                                                .into_any_element()
                                        })
                                        .collect()
                                }
                            })
                            .flex_1()
                            .with_sizing_behavior(ListSizingBehavior::Infer),
                        ),
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
                        .id("choose-delete-tag-cancel")
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
                ),
        )
}
