use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{h_flex, v_flex};

use crate::ui::app::GitSparkApp;
use crate::ui::ids::stable_id_slug;
use crate::ui::labels;
use crate::ui::theme;
use crate::ui::ui_state::ChangeContextMenuState;

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

#[derive(Clone)]
struct ChangesContextMenuItem {
    suffix: &'static str,
    label: String,
    enabled: bool,
    action: ChangesContextAction,
}

enum ChangesContextMenuEntry {
    Item(ChangesContextMenuItem),
    Separator,
}

fn changes_context_menu_entries(app: &GitSparkApp, path: &str) -> Vec<ChangesContextMenuEntry> {
    let ext = std::path::Path::new(&path)
        .extension()
        .map(|e| e.to_string_lossy().to_string());
    let folder = parent_folder_pattern(path);
    let basename = std::path::Path::new(&path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let show_view_on_github = app.repo_has_github_remote();
    let change_status = app
        .repo
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.changes.iter().find(|change| change.path == path))
        .map(|change| change.status.as_str())
        .unwrap_or_default();
    let ignore_enabled = basename != ".gitignore";
    let file_action_enabled = !is_deleted_change_status(change_status);

    let mut entries = vec![
        ChangesContextMenuEntry::Item(ChangesContextMenuItem {
            suffix: "discard",
            label: labels::discard_changes_menu().to_string(),
            enabled: true,
            action: ChangesContextAction::DiscardChanges,
        }),
        ChangesContextMenuEntry::Separator,
        ChangesContextMenuEntry::Item(ChangesContextMenuItem {
            suffix: "ignore-path",
            label: labels::ignore_file_menu().to_string(),
            enabled: ignore_enabled,
            action: ChangesContextAction::IgnoreFile,
        }),
    ];

    if let Some(folder) = folder {
        entries.push(ChangesContextMenuEntry::Item(ChangesContextMenuItem {
            suffix: "ignore-folder",
            label: labels::ignore_folder_menu().to_string(),
            enabled: ignore_enabled,
            action: ChangesContextAction::IgnoreFolder(folder),
        }));
    }

    if let Some(ext) = ext {
        entries.push(ChangesContextMenuEntry::Item(ChangesContextMenuItem {
            suffix: "ignore-extension",
            label: labels::ignore_all_extension_menu(&ext),
            enabled: ignore_enabled,
            action: ChangesContextAction::IgnoreExtension,
        }));
    }

    entries.extend([
        ChangesContextMenuEntry::Separator,
        ChangesContextMenuEntry::Item(ChangesContextMenuItem {
            suffix: "copy-full-path",
            label: labels::copy_file_path_menu().to_string(),
            enabled: true,
            action: ChangesContextAction::CopyFilePath,
        }),
        ChangesContextMenuEntry::Item(ChangesContextMenuItem {
            suffix: "copy-relative-path",
            label: labels::copy_relative_file_path_menu().to_string(),
            enabled: true,
            action: ChangesContextAction::CopyRelativePath,
        }),
        ChangesContextMenuEntry::Separator,
        ChangesContextMenuEntry::Item(ChangesContextMenuItem {
            suffix: "reveal-in-finder",
            label: labels::reveal_in_file_manager_menu().to_string(),
            enabled: file_action_enabled,
            action: ChangesContextAction::RevealInFinder,
        }),
        ChangesContextMenuEntry::Item(ChangesContextMenuItem {
            suffix: "open-in-editor",
            label: labels::open_in_external_editor_menu().to_string(),
            enabled: file_action_enabled,
            action: ChangesContextAction::OpenInExternalEditor,
        }),
        ChangesContextMenuEntry::Item(ChangesContextMenuItem {
            suffix: "open-with-default",
            label: labels::open_with_default_program_menu().to_string(),
            enabled: file_action_enabled,
            action: ChangesContextAction::OpenWithDefault,
        }),
    ]);

    if show_view_on_github {
        entries.extend([
            ChangesContextMenuEntry::Separator,
            ChangesContextMenuEntry::Item(ChangesContextMenuItem {
                suffix: "view-on-github",
                label: "View on GitHub".to_string(),
                enabled: true,
                action: ChangesContextAction::ViewOnGitHub,
            }),
        ]);
    }

    entries
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
    row: Stateful<Div>,
    view: Entity<GitSparkApp>,
    path: String,
) -> Stateful<Div> {
    row.on_mouse_down(MouseButton::Right, move |event, _window, cx| {
        let path = path.clone();
        let position = event.position;
        view.update(cx, |app, cx| {
            if app.selection.selected_change.as_deref() != Some(path.as_str()) {
                app.selection.selected_diff_lines.clear();
            }
            app.selection.selected_change = Some(path.clone());
            app.refresh_file_diff(path.clone());
            app.nav.change_context_menu = Some(ChangeContextMenuState {
                path,
                x: position.x / px(1.0),
                y: position.y / px(1.0),
            });
            cx.notify();
        });
    })
}

pub(crate) fn render_changes_context_menu_overlay(
    app: &GitSparkApp,
    window: &Window,
    cx: &mut Context<GitSparkApp>,
) -> Option<Stateful<Div>> {
    let state = app.nav.change_context_menu.clone()?;
    let entries = changes_context_menu_entries(app, &state.path);
    if entries.is_empty() {
        return None;
    }

    let view = cx.entity().clone();
    let window_size = window.window_bounds().get_bounds().size;
    let max_left = ((window_size.width / px(1.0)) - 292.0).max(8.0);
    let max_top = ((window_size.height / px(1.0)) - 360.0).max(8.0);
    let left = state.x.clamp(8.0, max_left);
    let top = state.y.clamp(8.0, max_top);
    let path = state.path.clone();
    let slug = stable_id_slug(&path);

    let backdrop_view = view.clone();
    let backdrop = div()
        .id("change-context-menu-backdrop")
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
            backdrop_view.update(cx, |app, cx| {
                app.nav.change_context_menu = None;
                cx.notify();
            });
        });

    let menu = v_flex()
        .id("change-context-menu")
        .absolute()
        .left(px(left))
        .top(px(top))
        .w(px(280.0))
        .p(px(4.0))
        .gap(px(2.0))
        .bg(theme::panel_bg())
        .border_1()
        .border_color(theme::border())
        .rounded(theme::z(theme::CORNER_RADIUS))
        .shadow_lg()
        .children(entries.into_iter().map(|entry| {
            match entry {
                ChangesContextMenuEntry::Separator => div()
                    .h(px(1.0))
                    .mx(px(6.0))
                    .my(px(3.0))
                    .bg(theme::border())
                    .into_any_element(),
                ChangesContextMenuEntry::Item(item) => {
                    let item_view = view.clone();
                    let item_path = path.clone();
                    let action = item.action.clone();
                    let id =
                        SharedString::from(format!("change-context-menu-{slug}-{}", item.suffix));
                    let item_row = h_flex()
                        .id(id)
                        .w_full()
                        .h(px(28.0))
                        .px(px(9.0))
                        .items_center()
                        .rounded(px(4.0))
                        .text_size(px(12.0))
                        .text_color(if item.enabled {
                            theme::text_main()
                        } else {
                            theme::text_muted()
                        })
                        .when(item.enabled, |row| {
                            row.cursor_pointer()
                                .hover(|style| style.bg(theme::hover_bg()))
                                .on_click(move |_event, _window, cx| {
                                    let path = item_path.clone();
                                    let action = action.clone();
                                    item_view.update(cx, |app, cx| {
                                        app.nav.change_context_menu = None;
                                        app.handle_changes_context_action(path, action, cx);
                                        cx.notify();
                                    });
                                })
                        })
                        .child(item.label);
                    item_row.into_any_element()
                }
            }
        }));

    Some(
        div()
            .id("change-context-menu-overlay")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(backdrop)
            .child(menu),
    )
}
