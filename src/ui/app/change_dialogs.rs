use super::helpers::*;
use super::*;

pub(super) fn render_discard_changes_dialog(
    paths: &[String],
    cx: &mut Context<GitSparkApp>,
) -> Div {
    let file_list = if paths.len() <= 10 {
        paths
            .iter()
            .map(|p| format!("  \u{2022} {p}"))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        let shown: Vec<_> = paths
            .iter()
            .take(10)
            .map(|p| format!("  \u{2022} {p}"))
            .collect();
        format!("{}\n  ...and {} more", shown.join("\n"), paths.len() - 10)
    };
    let path_count = paths.len();

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
                .gap(theme::z(8.0))
                .border_b_1()
                .border_color(theme::border())
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
                        .child("Confirm Discard Changes"),
                ),
        )
        .child(
            v_flex()
                .w_full()
                .p(theme::z(16.0))
                .gap(theme::z(8.0))
                .child(
                    div()
                        .text_size(theme::z(12.0))
                        .text_color(theme::text_main())
                        .child(format!(
                            "Are you sure you want to discard all changes to {path_count} file{}?",
                            if path_count == 1 { "" } else { "s" }
                        )),
                )
                .child(
                    div()
                        .text_size(theme::z(11.0))
                        .text_color(theme::text_muted())
                        .whitespace_nowrap()
                        .child(file_list),
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
                        .id("discard-cancel")
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
                        .id("discard-confirm")
                        .px(theme::z(12.0))
                        .py(theme::z(6.0))
                        .rounded(theme::z(theme::CORNER_RADIUS))
                        .bg(theme::danger())
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::danger_hover()))
                        .child(
                            div()
                                .text_size(theme::z(12.0))
                                .text_color(gpui::white())
                                .child("Discard Changes"),
                        )
                        .on_click(cx.listener(|app, _evt, _win, cx| {
                            if let ActiveDialog::DiscardChanges { paths } = &app.nav.active_dialog {
                                let paths = paths.clone();
                                for path in &paths {
                                    app.discard_change(path);
                                }
                            }
                            app.nav.active_dialog = ActiveDialog::None;
                            cx.notify();
                        })),
                ),
        )
}

pub(super) fn render_stash_and_switch_dialog(
    app: &GitSparkApp,
    target_branch: &str,
    cx: &mut Context<GitSparkApp>,
) -> Div {
    let target = target_branch.to_string();
    let bring_changes = app.repo.switch_branch_bring_changes;
    let files_to_stash = Arc::new(
        app.repo
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.changes.clone())
            .unwrap_or_default(),
    );
    let file_count = files_to_stash.len();
    let current_branch = app
        .repo
        .snapshot
        .as_ref()
        .map(|snapshot| snapshot.repo.current_branch.as_str())
        .unwrap_or("this branch");
    v_flex()
                    .w(px(576.0))
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
                            .border_b_1()
                            .border_color(theme::border())
                            .child(
                                div()
                                    .text_size(theme::z(14.0))
                                    .text_color(theme::text_main())
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Switch Branch"),
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
                                    .child("You have changes on this branch. What would you like to do with them?"),
                            )
                            .child(
                                v_flex()
                                    .w_full()
                                    .gap(theme::z(6.0))
                                    .child(
                                        h_flex()
                                            .w_full()
                                            .items_center()
                                            .justify_between()
                                            .child(
                                                div()
                                                    .text_size(theme::z(12.0))
                                                    .text_color(theme::text_main())
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child("Files affected"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(theme::z(11.0))
                                                    .text_color(theme::text_muted())
                                                    .child(pluralize_files(file_count)),
                                            ),
                                    )
                                    .child(render_stash_file_list(
                                        "branch-switch-file-list",
                                        "branch-switch-files",
                                        "branch-switch-file",
                                        files_to_stash.clone(),
                                        "No file list is available for these changes.",
                                    )),
                            )
                            .child(render_branch_switch_option(
                                "branch-switch-stash-option",
                                !bring_changes,
                                format!("Leave my changes on {current_branch}"),
                                "Your in-progress work will be stashed on this branch for you to return to later",
                                false,
                                cx,
                            ))
                            .child(render_branch_switch_option(
                                "branch-switch-bring-option",
                                bring_changes,
                                &format!("Bring my changes to {target}"),
                                "Your in-progress work will follow you to the new branch",
                                true,
                                cx,
                            )),
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
                                    .id("stash-cancel")
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
                                        app.repo.switch_branch_bring_changes = false;
                                        cx.notify();
                                    })),
                            )
                            .child({
                                let target = target.clone();
                                div()
                                    .id("stash-switch")
                                    .px(theme::z(12.0))
                                    .py(theme::z(6.0))
                                    .rounded(theme::z(theme::CORNER_RADIUS))
                                    .bg(theme::commit_button_bg())
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::commit_button_hover_bg()))
                                    .child(
                                        div()
                                            .text_size(theme::z(12.0))
                                            .text_color(theme::commit_button_text())
                                            .child("Switch Branch"),
                                    )
                                    .on_click(cx.listener(move |app, _evt, _win, cx| {
                                        if app.repo.switch_branch_bring_changes {
                                            app.switch_branch_with_changes(target.clone(), cx);
                                        } else {
                                            app.stash_and_switch_branch(target.clone(), cx);
                                        }
                                    }))
                            }),
                    )
}
