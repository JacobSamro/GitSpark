use gpui::{Context, Entity, Window, px};
use gpui_component::menu::{ContextMenu, ContextMenuExt, PopupMenu, PopupMenuItem};

use crate::ui::app::GitSparkApp;
use crate::ui::labels;

#[derive(Clone, Debug)]
pub(crate) enum BranchContextAction {
    Rename,
    CopyName,
    Delete,
    ViewOnGitHub,
}

pub(crate) fn build_branch_context_menu(
    menu: PopupMenu,
    view: Entity<GitSparkApp>,
    branch_name: String,
    _window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let has_github_remote = view.read(cx).repo_has_github_remote();

    let is_current = view
        .read(cx)
        .repo
        .snapshot
        .as_ref()
        .map(|s| s.repo.current_branch == branch_name)
        .unwrap_or(false);

    menu.min_w(px(180.0))
        .max_w(px(240.0))
        .item(branch_menu_item(
            labels::rename_branch_context_menu(),
            true,
            view.clone(),
            branch_name.clone(),
            BranchContextAction::Rename,
        ))
        .item(branch_menu_item(
            labels::copy_branch_name_menu(),
            true,
            view.clone(),
            branch_name.clone(),
            BranchContextAction::CopyName,
        ))
        .separator()
        .item(branch_menu_item(
            labels::view_branch_on_github_menu(),
            has_github_remote,
            view.clone(),
            branch_name.clone(),
            BranchContextAction::ViewOnGitHub,
        ))
        .separator()
        .item(branch_menu_item(
            labels::delete_branch_context_menu(),
            !is_current,
            view,
            branch_name,
            BranchContextAction::Delete,
        ))
}

fn branch_menu_item(
    label: &str,
    enabled: bool,
    view: Entity<GitSparkApp>,
    branch_name: String,
    action: BranchContextAction,
) -> PopupMenuItem {
    PopupMenuItem::new(label.to_string())
        .disabled(!enabled)
        .on_click(move |_event, _window, cx| {
            let name = branch_name.clone();
            let action = action.clone();
            view.update(cx, |app, cx| {
                app.handle_branch_context_action(name, action, cx);
            });
        })
}

pub(crate) fn bind_branch_context_click(
    row: gpui::Stateful<gpui::Div>,
    view: Entity<GitSparkApp>,
    branch_name: String,
) -> ContextMenu<gpui::Stateful<gpui::Div>> {
    row.context_menu({
        let view = view.clone();
        let name = branch_name.clone();
        move |menu, window, cx| {
            build_branch_context_menu(menu, view.clone(), name.clone(), window, cx)
        }
    })
}
