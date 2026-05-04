use gpui::{Context, Entity, InteractiveElement, MouseButton, Window, px};
use gpui_component::menu::{ContextMenu, ContextMenuExt, PopupMenu, PopupMenuItem};

use crate::ui::app::GitSparkApp;

#[derive(Clone, Debug)]
pub(crate) enum ChangesContextAction {
    DiscardChanges,
    IgnoreFile,
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
    _cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let ext = std::path::Path::new(&path)
        .extension()
        .map(|e| e.to_string_lossy().to_string());

    let mut m = menu
        .min_w(px(220.0))
        .max_w(px(280.0))
        .item(changes_menu_item(
            "Discard Changes...",
            true,
            view.clone(),
            path.clone(),
            ChangesContextAction::DiscardChanges,
        ))
        .separator()
        .item(changes_menu_item(
            "Ignore File (Add to .gitignore)",
            true,
            view.clone(),
            path.clone(),
            ChangesContextAction::IgnoreFile,
        ));

    if let Some(ext) = ext {
        m = m.item(changes_menu_item(
            &format!("Ignore All .{ext} Files"),
            true,
            view.clone(),
            path.clone(),
            ChangesContextAction::IgnoreExtension,
        ));
    }

    m.separator()
        .item(changes_menu_item(
            "Copy file path",
            true,
            view.clone(),
            path.clone(),
            ChangesContextAction::CopyFilePath,
        ))
        .item(changes_menu_item(
            "Copy relative file path",
            true,
            view.clone(),
            path.clone(),
            ChangesContextAction::CopyRelativePath,
        ))
        .separator()
        .item(changes_menu_item(
            reveal_label(),
            true,
            view.clone(),
            path.clone(),
            ChangesContextAction::RevealInFinder,
        ))
        .item(changes_menu_item(
            "Open in external editor",
            true,
            view.clone(),
            path.clone(),
            ChangesContextAction::OpenInExternalEditor,
        ))
        .item(changes_menu_item(
            "Open with default program",
            true,
            view.clone(),
            path.clone(),
            ChangesContextAction::OpenWithDefault,
        ))
        .separator()
        .item(changes_menu_item(
            "View on GitHub",
            true,
            view,
            path,
            ChangesContextAction::ViewOnGitHub,
        ))
}

fn changes_menu_item(
    label: &str,
    enabled: bool,
    view: Entity<GitSparkApp>,
    path: String,
    action: ChangesContextAction,
) -> PopupMenuItem {
    PopupMenuItem::new(label.to_string())
        .disabled(!enabled)
        .on_click(move |_event, _window, cx| {
            let path = path.clone();
            let action = action.clone();
            view.update(cx, |app, cx| {
                app.handle_changes_context_action(path, action, cx);
            });
        })
}

fn reveal_label() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "Show in Explorer"
    }

    #[cfg(target_os = "macos")]
    {
        "Show in Finder"
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        "Show in file manager"
    }
}

pub(crate) fn bind_changes_context_click(
    row: gpui::Stateful<gpui::Div>,
    view: Entity<GitSparkApp>,
    path: String,
) -> ContextMenu<gpui::Stateful<gpui::Div>> {
    row.on_mouse_down(MouseButton::Right, {
        let view = view.clone();
        let path = path.clone();
        move |_event, _window, cx| {
            let path = path.clone();
            view.update(cx, |app, cx| {
                app.selection.selected_change = Some(path);
                cx.notify();
            });
        }
    })
    .context_menu(move |menu, window, cx| {
        build_changes_context_menu(menu, view.clone(), path.clone(), window, cx)
    })
}
