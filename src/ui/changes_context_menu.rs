use gpui::prelude::FluentBuilder as _;
use gpui::{Context, Entity, Window};
use gpui_component::menu::{ContextMenu, ContextMenuExt, PopupMenu, PopupMenuItem};

use crate::ui::app::GitSparkApp;
use crate::ui::labels;

#[derive(Clone, Debug)]
pub(crate) enum ChangesContextAction {
    DiscardChanges,
    IgnoreFile,
    IgnoreFolder(String),
    IgnoreExtension,
    CopyFilePath,
    CopyRelativePath,
    RevealInFinder,
    OpenInExternalEditor,
    OpenWithDefault,
    ViewOnGitHub,
}

pub(crate) fn build_changes_context_menu(
    menu: PopupMenu,
    view: Entity<GitSparkApp>,
    path: String,
    _window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let ext = std::path::Path::new(&path)
        .extension()
        .map(|e| e.to_string_lossy().to_string());
    let folder = parent_folder_pattern(&path);
    let basename = std::path::Path::new(&path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let show_view_on_github = view.read(cx).repo_has_github_remote();
    let change_status = view
        .read(cx)
        .repo
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.changes.iter().find(|change| change.path == path))
        .map(|change| change.status.as_str())
        .unwrap_or_default();
    let ignore_enabled = basename != ".gitignore";
    let file_action_enabled = !is_deleted_change_status(change_status);

    let menu = menu
        .min_w(gpui::px(220.0))
        .max_w(gpui::px(280.0))
        .item(menu_item(
            labels::discard_changes_menu(),
            true,
            view.clone(),
            path.clone(),
            ChangesContextAction::DiscardChanges,
        ))
        .separator()
        .item(menu_item(
            labels::ignore_file_menu(),
            ignore_enabled,
            view.clone(),
            path.clone(),
            ChangesContextAction::IgnoreFile,
        ))
        .when_some(folder, |menu, folder| {
            menu.item(menu_item(
                labels::ignore_folder_menu(),
                ignore_enabled,
                view.clone(),
                path.clone(),
                ChangesContextAction::IgnoreFolder(folder),
            ))
        })
        .when_some(ext, |menu, ext| {
            menu.item(menu_item(
                labels::ignore_all_extension_menu(&ext),
                ignore_enabled,
                view.clone(),
                path.clone(),
                ChangesContextAction::IgnoreExtension,
            ))
        })
        .separator()
        .item(menu_item(
            labels::copy_file_path_menu(),
            true,
            view.clone(),
            path.clone(),
            ChangesContextAction::CopyFilePath,
        ))
        .item(menu_item(
            labels::copy_relative_file_path_menu(),
            true,
            view.clone(),
            path.clone(),
            ChangesContextAction::CopyRelativePath,
        ))
        .separator()
        .item(menu_item(
            labels::reveal_in_file_manager_menu(),
            file_action_enabled,
            view.clone(),
            path.clone(),
            ChangesContextAction::RevealInFinder,
        ))
        .item(menu_item(
            labels::open_in_external_editor_menu(),
            file_action_enabled,
            view.clone(),
            path.clone(),
            ChangesContextAction::OpenInExternalEditor,
        ))
        .item(menu_item(
            labels::open_with_default_program_menu(),
            file_action_enabled,
            view.clone(),
            path.clone(),
            ChangesContextAction::OpenWithDefault,
        ));

    menu.when(show_view_on_github, |menu| {
        menu.separator().item(menu_item(
            "View on GitHub",
            true,
            view,
            path,
            ChangesContextAction::ViewOnGitHub,
        ))
    })
}

fn menu_item(
    label: impl Into<String>,
    enabled: bool,
    view: Entity<GitSparkApp>,
    path: String,
    action: ChangesContextAction,
) -> PopupMenuItem {
    PopupMenuItem::new(label.into())
        .disabled(!enabled)
        .on_click(move |_event, _window, cx| {
            let path = path.clone();
            let action = action.clone();
            view.update(cx, |app, cx| {
                app.handle_changes_context_action(path, action, cx);
            });
        })
}

fn parent_folder_pattern(path: &str) -> Option<String> {
    std::path::Path::new(path).parent().and_then(|parent| {
        let folder = parent.to_string_lossy().replace('\\', "/");
        if folder.is_empty() {
            None
        } else {
            Some(format!("{folder}/"))
        }
    })
}

fn is_deleted_change_status(status: &str) -> bool {
    status.contains('D') && !status.contains('A') && !status.contains('?')
}

pub(crate) fn bind_changes_context_click(
    row: gpui::Stateful<gpui::Div>,
    view: Entity<GitSparkApp>,
    path: String,
) -> ContextMenu<gpui::Stateful<gpui::Div>> {
    row.context_menu(move |menu, window, cx| {
        {
            let path = path.clone();
            view.update(cx, |app, cx| {
                if app.selection.selected_change.as_deref() != Some(path.as_str()) {
                    app.selection.selected_diff_lines.clear();
                }
                app.selection.selected_change = Some(path.clone());
                app.refresh_file_diff(path);
                cx.notify();
            });
        }
        build_changes_context_menu(menu, view.clone(), path.clone(), window, cx)
    })
}
