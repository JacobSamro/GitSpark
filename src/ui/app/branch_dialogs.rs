use super::helpers::*;
use super::*;

pub(super) fn render_create_branch_dialog(app: &GitSparkApp, cx: &mut Context<GitSparkApp>) -> Div {
    let branch_name = &app.repo.new_branch_name;
    let validation_message = app.create_branch_validation_message();
    let show_validation_message = !branch_name.trim().is_empty();
    let can_create = app.can_create_branch_from_dialog();
    let current = app
        .repo
        .snapshot
        .as_ref()
        .map(|s| s.repo.current_branch.as_str())
        .unwrap_or("main");
    let starting_point = app
        .repo
        .new_branch_start_point
        .as_deref()
        .map(|oid| format!("Based on commit: {}", short_commit_label(oid)))
        .unwrap_or_else(|| format!("Based on current branch: {current}"));

    v_flex()
        .w(px(400.0))
        .bg(theme::panel_bg())
        .rounded(theme::z(theme::CORNER_RADIUS))
        .border_1()
        .border_color(theme::border())
        .shadow_lg()
        // Header
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
                        .child("Create a Branch"),
                )
                .child(
                    div()
                        .id("dialog-close")
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
        // Body
        .child(
            v_flex()
                .w_full()
                .p(theme::z(16.0))
                .gap(theme::z(12.0))
                // Name field
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
                                .id("new-branch-name-input")
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
                                        .text_color(if branch_name.is_empty() {
                                            theme::text_muted()
                                        } else {
                                            theme::text_main()
                                        })
                                        .child(if branch_name.is_empty() {
                                            "branch-name".to_string()
                                        } else {
                                            branch_name.clone()
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
                // Starting point
                .child(
                    div()
                        .text_size(theme::z(11.0))
                        .text_color(theme::text_muted())
                        .child(starting_point),
                )
                .children(
                    validation_message
                        .as_ref()
                        .filter(|_| show_validation_message)
                        .map(|message| {
                            div()
                                .id("create-branch-validation-message")
                                .text_size(theme::z(11.0))
                                .text_color(branch_validation_message_color(message))
                                .child(message.clone())
                        }),
                ),
        )
        // Footer
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
                        .id("dialog-cancel")
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
                        .id("dialog-create-branch")
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
                                .child("Create Branch"),
                        )
                        .on_click(cx.listener(|app, _evt, _win, cx| {
                            if !app.can_create_branch_from_dialog() {
                                cx.notify();
                                return;
                            }
                            app.create_branch(cx);
                        })),
                ),
        )
}

pub(super) fn render_rename_branch_dialog(
    app: &GitSparkApp,
    old_name: &str,
    cx: &mut Context<GitSparkApp>,
) -> Div {
    let branch_name = &app.repo.new_branch_name;
    let old_name_for_click = old_name.to_string();
    let validation_message = app.rename_branch_validation_message(old_name);
    let show_validation_message = !branch_name.trim().is_empty() && branch_name.trim() != old_name;
    let can_rename = app.can_rename_branch_from_dialog(old_name);

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
                        .child("Rename Branch"),
                )
                .child(
                    div()
                        .id("rename-branch-close")
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
                                .id("rename-branch-name-input")
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
                                        .text_color(if branch_name.is_empty() {
                                            theme::text_muted()
                                        } else {
                                            theme::text_main()
                                        })
                                        .child(if branch_name.is_empty() {
                                            "branch-name".to_string()
                                        } else {
                                            branch_name.clone()
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
                        .child(format!("Current branch name: {old_name}")),
                ),
        )
        .children(
            validation_message
                .as_ref()
                .filter(|_| show_validation_message)
                .map(|message| {
                    div()
                        .id("rename-branch-validation-message")
                        .px(theme::z(16.0))
                        .pb(theme::z(12.0))
                        .text_size(theme::z(11.0))
                        .text_color(branch_validation_message_color(message))
                        .child(message.clone())
                }),
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
                        .id("rename-branch-cancel")
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
                        .id("rename-branch-confirm")
                        .px(theme::z(12.0))
                        .py(theme::z(6.0))
                        .rounded(theme::z(theme::CORNER_RADIUS))
                        .bg(if can_rename {
                            theme::commit_button_bg()
                        } else {
                            theme::surface_bg()
                        })
                        .border_1()
                        .border_color(if can_rename {
                            theme::commit_button_bg()
                        } else {
                            theme::surface_bg_alt()
                        })
                        .when(can_rename, |el| {
                            el.cursor_pointer()
                                .hover(|s| s.bg(theme::commit_button_hover_bg()))
                        })
                        .child(
                            div()
                                .text_size(theme::z(12.0))
                                .text_color(if can_rename {
                                    theme::commit_button_text()
                                } else {
                                    theme::text_muted()
                                })
                                .child("Rename Branch"),
                        )
                        .on_click(cx.listener(move |app, _evt, _win, cx| {
                            if !app.can_rename_branch_from_dialog(&old_name_for_click) {
                                cx.notify();
                                return;
                            }
                            app.rename_branch(old_name_for_click.clone(), cx);
                        })),
                ),
        )
}
