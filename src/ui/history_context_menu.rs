use gpui::{Context, Entity, Window, px};
use gpui_component::menu::{ContextMenu, ContextMenuExt, PopupMenu, PopupMenuItem};

use crate::ui::app::GitSparkApp;
use crate::ui::labels;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HistoryContextMenuAction {
    ResetToCommit,
    CheckoutCommit,
    ReorderCommit,
    RevertChangesInCommit,
    CreateBranchFromCommit,
    CreateTag,
    CherryPickCommit,
    CopySha,
    CopyDiff,
    CopyTag,
    ViewOnGitHub,
}

pub(crate) fn build_history_context_menu(
    menu: PopupMenu,
    view: Entity<GitSparkApp>,
    oid: String,
    _window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let has_github_remote = view.read(cx).repo_has_github_remote();

    menu.min_w(px(220.0))
        .max_w(px(260.0))
        .item(menu_item(
            labels::reset_to_commit_menu(),
            false,
            view.clone(),
            oid.clone(),
            HistoryContextMenuAction::ResetToCommit,
        ))
        .item(menu_item(
            labels::checkout_commit_menu(),
            true,
            view.clone(),
            oid.clone(),
            HistoryContextMenuAction::CheckoutCommit,
        ))
        .item(menu_item(
            labels::reorder_commit_menu(),
            false,
            view.clone(),
            oid.clone(),
            HistoryContextMenuAction::ReorderCommit,
        ))
        .item(menu_item(
            labels::revert_changes_in_commit_menu(),
            true,
            view.clone(),
            oid.clone(),
            HistoryContextMenuAction::RevertChangesInCommit,
        ))
        .separator()
        .item(menu_item(
            labels::create_branch_from_commit_menu(),
            true,
            view.clone(),
            oid.clone(),
            HistoryContextMenuAction::CreateBranchFromCommit,
        ))
        .item(menu_item(
            labels::create_tag_menu(),
            true,
            view.clone(),
            oid.clone(),
            HistoryContextMenuAction::CreateTag,
        ))
        .item(menu_item(
            labels::cherry_pick_commit_menu(),
            true,
            view.clone(),
            oid.clone(),
            HistoryContextMenuAction::CherryPickCommit,
        ))
        .separator()
        .item(menu_item(
            "Copy SHA",
            true,
            view.clone(),
            oid.clone(),
            HistoryContextMenuAction::CopySha,
        ))
        .item(menu_item(
            "Copy diff",
            true,
            view.clone(),
            oid.clone(),
            HistoryContextMenuAction::CopyDiff,
        ))
        .item(menu_item(
            "Copy Tag",
            false,
            view.clone(),
            oid.clone(),
            HistoryContextMenuAction::CopyTag,
        ))
        .item(menu_item(
            "View on GitHub",
            has_github_remote,
            view,
            oid,
            HistoryContextMenuAction::ViewOnGitHub,
        ))
}

fn menu_item(
    label: &'static str,
    enabled: bool,
    view: Entity<GitSparkApp>,
    oid: String,
    action: HistoryContextMenuAction,
) -> PopupMenuItem {
    PopupMenuItem::new(label)
        .disabled(!enabled)
        .on_click(move |_event, _window, cx| {
            let oid = oid.clone();
            view.update(cx, |app, cx| {
                app.handle_history_context_menu_action_for_oid(oid, action, cx);
            });
        })
}

pub(crate) fn bind_history_context_click(
    row: gpui::Stateful<gpui::Div>,
    view: Entity<GitSparkApp>,
    oid: String,
) -> ContextMenu<gpui::Stateful<gpui::Div>> {
    row.context_menu(move |menu, window, cx| {
        {
            let oid = oid.clone();
            view.update(cx, |app, cx| {
                app.select_commit(oid, cx);
            });
        }
        build_history_context_menu(menu, view.clone(), oid.clone(), window, cx)
    })
}
