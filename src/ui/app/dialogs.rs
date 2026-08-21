use super::*;

pub(super) fn render_active_dialog(
    app: &GitSparkApp,
    window: &Window,
    cx: &mut Context<GitSparkApp>,
) -> Div {
    // Backdrop
    let backdrop = div()
        .id("dialog-backdrop")
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .bg(gpui::hsla(0.0, 0.0, 0.0, 0.5))
        .on_click(cx.listener(|app, _evt, _win, cx| {
            app.nav.active_dialog = ActiveDialog::None;
            cx.notify();
        }));

    let (dialog_width, dialog_height) = match &app.nav.active_dialog {
        ActiveDialog::CreateBranch => (400.0, 230.0),
        ActiveDialog::RenameBranch { .. } => (400.0, 230.0),
        ActiveDialog::DeleteBranch { .. } => (kit::dialog::DIALOG_WIDTH, 220.0),
        ActiveDialog::CreateTag { .. } => (400.0, 230.0),
        ActiveDialog::ChooseTagToDelete { .. } => (420.0, 320.0),
        ActiveDialog::DeleteTag { .. } => (420.0, 220.0),
        ActiveDialog::ResetToCommit { .. } => (kit::dialog::DIALOG_WIDTH_WIDE, 240.0),
        ActiveDialog::CreateRepository => (560.0, 540.0),
        ActiveDialog::CloneRepository => (560.0, 390.0),
        ActiveDialog::DiscardChanges { .. } => (420.0, 230.0),
        ActiveDialog::StashAndSwitch { .. } => (576.0, 360.0),
        ActiveDialog::StashChanges => (500.0, 360.0),
        ActiveDialog::RestoreStash => (500.0, 360.0),
        ActiveDialog::DiscardStash => (kit::dialog::DIALOG_WIDTH_WIDE, 400.0),
        ActiveDialog::PublishRepository => (
            crate::ui::publish_dialog::PUBLISH_DIALOG_WIDTH,
            crate::ui::publish_dialog::PUBLISH_DIALOG_HEIGHT,
        ),
        ActiveDialog::None => (0.0, 0.0),
    };
    let bounds = window.bounds();
    let window_width = bounds.size.width / px(1.0);
    let window_height = bounds.size.height / px(1.0);
    let dialog_left = ((window_width - dialog_width) / 2.0).max(16.0);
    let dialog_top = ((window_height - dialog_height) / 2.0).max(16.0);

    let dialog_content = match &app.nav.active_dialog {
        ActiveDialog::CreateBranch => branch_dialogs::render_create_branch_dialog(app, cx),
        ActiveDialog::RenameBranch { old_name } => {
            branch_dialogs::render_rename_branch_dialog(app, old_name, cx)
        }
        ActiveDialog::DeleteBranch { branch_name } => {
            crate::ui::delete_branch_dialog::render_delete_branch_dialog(branch_name, cx)
        }
        ActiveDialog::StashChanges => {
            let files = app
                .repo
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.changes.clone())
                .unwrap_or_default();
            crate::ui::stash_changes_dialog::render_stash_changes_dialog(
                Arc::new(files),
                app.repo.has_stash,
                cx,
            )
        }
        ActiveDialog::DiscardStash => crate::ui::discard_stash_dialog::render_discard_stash_dialog(
            Arc::new(app.repo.stash_files.clone()),
            cx,
        ),
        ActiveDialog::CreateTag { target_oid } => {
            tag_dialogs::render_create_tag_dialog(app, target_oid, cx)
        }
        ActiveDialog::DeleteTag { tag_name } => tag_dialogs::render_delete_tag_dialog(tag_name, cx),
        ActiveDialog::ChooseTagToDelete { target_oid } => {
            tag_dialogs::render_choose_tag_to_delete_dialog(app, target_oid, cx)
        }
        ActiveDialog::ResetToCommit { target_oid } => {
            crate::ui::reset_dialog::render_reset_to_commit_dialog(target_oid, cx)
        }
        ActiveDialog::CreateRepository => {
            crate::ui::repository_dialog::render_create_repository_dialog(app, window, cx)
        }
        ActiveDialog::CloneRepository => {
            crate::ui::repository_dialog::render_clone_repository_dialog(app, window, cx)
        }
        ActiveDialog::DiscardChanges { paths } => {
            change_dialogs::render_discard_changes_dialog(paths, cx)
        }
        ActiveDialog::StashAndSwitch { target_branch } => {
            change_dialogs::render_stash_and_switch_dialog(app, target_branch, cx)
        }
        ActiveDialog::RestoreStash => crate::ui::restore_stash_dialog::render_restore_stash_dialog(
            Arc::new(app.repo.stash_files.clone()),
            cx,
        ),
        ActiveDialog::PublishRepository => {
            crate::ui::publish_dialog::render_publish_dialog(app, window, cx)
        }
        _ => div(),
    };

    // Center the dialog
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .child(backdrop)
        .child(
            div()
                .id("dialog-container")
                .on_click(|_evt, _win, cx| cx.stop_propagation())
                .absolute()
                .left(px(dialog_left))
                .top(px(dialog_top))
                .child(dialog_content),
        )
}

pub(super) fn render_network_dropdown_overlay(
    app: &GitSparkApp,
    cx: &mut Context<GitSparkApp>,
) -> Div {
    let backdrop = div()
        .id("network-dropdown-backdrop")
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .on_click(cx.listener(|app, _evt, _win, cx| {
            app.nav.show_network_dropdown = false;
            cx.stop_propagation();
            cx.notify();
        }));

    let panel = render_network_dropdown(app, cx)
        .id("network-dropdown-panel")
        .on_click(|_evt, _win, cx| cx.stop_propagation());

    // Position using h_flex: spacer pushes panel to align under the network section
    let positioned = h_flex()
        .absolute()
        .top(theme::z(theme::TOOLBAR_HEIGHT))
        .left_0()
        .w_full()
        .child(
            div()
                .flex_none()
                .w(px(crate::ui::toolbar::network_dropdown_left_offset())),
        )
        .child(panel);

    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .child(backdrop)
        .child(positioned)
}

fn render_network_dropdown(app: &GitSparkApp, cx: &mut Context<GitSparkApp>) -> Div {
    let snapshot = app.repo.snapshot.as_ref();
    let remote_name = snapshot
        .and_then(|s| s.repo.remote_name.as_deref())
        .unwrap_or("origin");

    let fetch_title = format!("Fetch {remote_name}");
    let fetch_desc = format!("Fetch the latest changes from {remote_name}");

    v_flex()
        .w(px(crate::ui::toolbar::NETWORK_DROPDOWN_WIDTH))
        .bg(theme::panel_bg())
        .border_1()
        .border_color(theme::toolbar_button_border())
        .rounded_b(theme::z(theme::CORNER_RADIUS))
        .shadow_lg()
        .child(
            h_flex()
                .id("net-fetch")
                .w_full()
                .p(px(12.0))
                .gap(px(10.0))
                .items_center()
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover_bg()))
                .child(
                    gpui::svg()
                        .path("icons/rotate-cw.svg")
                        .size(px(20.0))
                        .text_color(theme::text_main())
                        .flex_shrink_0(),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(theme::z(14.0))
                                .text_color(theme::text_main())
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(fetch_title),
                        )
                        .child(
                            div()
                                .text_size(theme::z(12.0))
                                .text_color(theme::text_muted())
                                .child(fetch_desc),
                        ),
                )
                .on_click(cx.listener(|app, _evt, _win, cx| {
                    cx.stop_propagation();
                    app.nav.show_network_dropdown = false;
                    app.handle_toolbar_action(
                        ToolbarAction::RunNetworkAction(NetworkAction::Fetch),
                        cx,
                    );
                })),
        )
}
