use gpui::{Context, Entity, Window, px};
use gpui_component::menu::{ContextMenu, ContextMenuExt, PopupMenu, PopupMenuItem};

use crate::ui::app::GitSparkApp;
use crate::ui::changes_context_menu::ChangesContextAction;
use crate::ui::labels;

/// Right-click menu for a file row in the History tab's commit file list.
///
/// Reuses `ChangesContextAction` and `handle_changes_context_action` rather
/// than defining a parallel action type — every action offered here
/// (copy path, reveal, open in editor, open with default, view on GitHub)
/// already resolves purely from a repo-relative path and does not depend on
/// the file being an in-progress change.
pub(crate) fn build_commit_file_context_menu(
    menu: PopupMenu,
    view: Entity<GitSparkApp>,
    path: String,
    _window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let has_github_remote = view.read(cx).repo_has_github_remote();

    menu.min_w(px(220.0))
        .max_w(px(280.0))
        .item(menu_item(
            labels::copy_file_path_menu(),
            view.clone(),
            path.clone(),
            ChangesContextAction::CopyFilePath,
        ))
        .item(menu_item(
            labels::copy_relative_file_path_menu(),
            view.clone(),
            path.clone(),
            ChangesContextAction::CopyRelativePath,
        ))
        .separator()
        .item(menu_item(
            labels::reveal_in_file_manager_menu(),
            view.clone(),
            path.clone(),
            ChangesContextAction::RevealInFinder,
        ))
        .item(menu_item(
            labels::open_in_external_editor_menu(),
            view.clone(),
            path.clone(),
            ChangesContextAction::OpenInExternalEditor,
        ))
        .item(menu_item(
            labels::open_with_default_program_menu(),
            view.clone(),
            path.clone(),
            ChangesContextAction::OpenWithDefault,
        ))
        .separator()
        .item(
            menu_item(
                "View on GitHub",
                view,
                path,
                ChangesContextAction::ViewOnGitHub,
            )
            .disabled(!has_github_remote),
        )
}

fn menu_item(
    label: impl Into<String>,
    view: Entity<GitSparkApp>,
    path: String,
    action: ChangesContextAction,
) -> PopupMenuItem {
    PopupMenuItem::new(label.into()).on_click(move |_event, _window, cx| {
        let path = path.clone();
        let action = action.clone();
        view.update(cx, |app, cx| {
            app.handle_changes_context_action(path, action, cx);
        });
    })
}

pub(crate) fn bind_commit_file_context_click(
    row: gpui::Stateful<gpui::Div>,
    view: Entity<GitSparkApp>,
    path: String,
) -> ContextMenu<gpui::Stateful<gpui::Div>> {
    row.context_menu(move |menu, window, cx| {
        build_commit_file_context_menu(menu, view.clone(), path.clone(), window, cx)
    })
}
