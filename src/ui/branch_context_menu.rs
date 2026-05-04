use gpui::{Context, Entity, Window, px};
use gpui_component::menu::{ContextMenu, ContextMenuExt, PopupMenu, PopupMenuItem};

use crate::ui::app::GitSparkApp;
use crate::ui::labels;

#[derive(Clone, Debug)]
pub(crate) enum BranchContextAction {
    Rename,
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
    let remote_available = view
        .read(cx)
        .repo
        .snapshot
        .as_ref()
        .and_then(|s| s.repo.remote_name.as_deref())
        .is_some();

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
            labels::rename_branch_menu(),
            false, // not implemented yet
            view.clone(),
            branch_name.clone(),
            BranchContextAction::Rename,
        ))
        .item(branch_menu_item(
            labels::delete_branch_menu(),
            !is_current, // can't delete the current branch
            view.clone(),
            branch_name.clone(),
            BranchContextAction::Delete,
        ))
        .separator()
        .item(branch_menu_item(
            "View on GitHub",
            remote_available,
            view,
            branch_name,
            BranchContextAction::ViewOnGitHub,
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
