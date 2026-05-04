// Suppress the console window on Windows for GUI apps.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod assets;
mod git;
mod models;
mod storage;
mod ui;

use gpui::*;

use crate::storage::load_settings;
use crate::ui::GitSparkApp;

actions!(
    gitspark_menu,
    [
        MenuOpenRepository,
        MenuShowSettings,
        MenuShowChanges,
        MenuShowHistory,
        MenuShowRepositoryList,
        MenuShowBranchesList,
        MenuGoToSummary,
        MenuFetch,
        MenuPull,
        MenuPush,
        MenuPublishRepository,
        MenuOpenExternalEditor,
        MenuOpenInTerminal,
        MenuShowInFinder,
        MenuViewOnGitHub,
        MenuRepositorySettings,
        MenuNewBranch,
        MenuRenameBranch,
        MenuMergeBranch,
        MenuStashChanges,
        MenuZoomIn,
        MenuZoomOut,
        MenuZoomReset,
        MenuQuit
    ]
);

fn platform_titlebar_options() -> TitlebarOptions {
    #[cfg(target_os = "macos")]
    {
        TitlebarOptions {
            title: Some("GitSpark".into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(10.0), px(12.0))),
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        TitlebarOptions {
            title: Some("GitSpark".into()),
            appears_transparent: false,
            traffic_light_position: None,
        }
    }
}

fn configure_native_menus(cx: &mut App, view: Entity<GitSparkApp>) {
    cx.on_action(|_: &MenuQuit, cx| cx.quit());
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuOpenRepository, cx| {
            let _ = view.update(cx, |app, cx| app.menu_open_repository(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuShowSettings, cx| {
            let _ = view.update(cx, |app, cx| app.menu_show_settings(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuShowChanges, cx| {
            let _ = view.update(cx, |app, cx| app.menu_show_changes(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuShowHistory, cx| {
            let _ = view.update(cx, |app, cx| app.menu_show_history(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuShowRepositoryList, cx| {
            let _ = view.update(cx, |app, cx| app.menu_show_repository_list(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuShowBranchesList, cx| {
            let _ = view.update(cx, |app, cx| app.menu_show_branches_list(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuGoToSummary, cx| {
            let _ = view.update(cx, |app, cx| app.menu_go_to_summary(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuFetch, cx| {
            let _ = view.update(cx, |app, cx| app.menu_fetch(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuPull, cx| {
            let _ = view.update(cx, |app, cx| app.menu_pull(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuPush, cx| {
            let _ = view.update(cx, |app, cx| app.menu_push(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuPublishRepository, cx| {
            let _ = view.update(cx, |app, cx| app.menu_publish_repository(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuOpenExternalEditor, cx| {
            let _ = view.update(cx, |app, cx| app.menu_open_external_editor(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuOpenInTerminal, cx| {
            let _ = view.update(cx, |app, cx| app.menu_open_in_terminal(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuShowInFinder, cx| {
            let _ = view.update(cx, |app, cx| app.menu_show_in_finder(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuViewOnGitHub, cx| {
            let _ = view.update(cx, |app, cx| app.menu_view_on_github(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuRepositorySettings, cx| {
            let _ = view.update(cx, |app, cx| app.menu_repository_settings(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuNewBranch, cx| {
            let _ = view.update(cx, |app, cx| app.menu_new_branch(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuRenameBranch, cx| {
            let _ = view.update(cx, |app, cx| app.menu_rename_current_branch(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuMergeBranch, cx| {
            let _ = view.update(cx, |app, cx| app.menu_merge_branch(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuStashChanges, cx| {
            let _ = view.update(cx, |app, cx| app.menu_stash_changes(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuZoomIn, cx| {
            let _ = view.update(cx, |app, cx| app.menu_zoom_in(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuZoomOut, cx| {
            let _ = view.update(cx, |app, cx| app.menu_zoom_out(cx));
        });
    }
    cx.on_action(move |_: &MenuZoomReset, cx| {
        let _ = view.update(cx, |app, cx| app.menu_zoom_reset(cx));
    });
    cx.bind_keys([
        KeyBinding::new("cmd-o", MenuOpenRepository, None),
        KeyBinding::new("cmd-,", MenuShowSettings, None),
        KeyBinding::new("cmd-1", MenuShowChanges, None),
        KeyBinding::new("cmd-2", MenuShowHistory, None),
        KeyBinding::new("cmd-t", MenuShowRepositoryList, None),
        KeyBinding::new("cmd-b", MenuShowBranchesList, None),
        KeyBinding::new("cmd-g", MenuGoToSummary, None),
        KeyBinding::new("cmd-r", MenuFetch, None),
        KeyBinding::new("cmd-shift-p", MenuPush, None),
        KeyBinding::new("cmd-shift-a", MenuOpenExternalEditor, None),
        KeyBinding::new("ctrl-`", MenuOpenInTerminal, None),
        KeyBinding::new("cmd-shift-f", MenuShowInFinder, None),
        KeyBinding::new("cmd-shift-g", MenuViewOnGitHub, None),
        KeyBinding::new("cmd-shift-n", MenuNewBranch, None),
        KeyBinding::new("cmd-shift-r", MenuRenameBranch, None),
        KeyBinding::new("cmd-shift-s", MenuStashChanges, None),
        KeyBinding::new("cmd-+", MenuZoomIn, None),
        KeyBinding::new("cmd--", MenuZoomOut, None),
        KeyBinding::new("cmd-0", MenuZoomReset, None),
        KeyBinding::new("cmd-q", MenuQuit, None),
    ]);

    cx.set_menus(vec![
        Menu {
            name: "GitSpark".into(),
            items: vec![
                MenuItem::action("Settings...", MenuShowSettings),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Quit GitSpark", MenuQuit),
            ],
        },
        Menu {
            name: "File".into(),
            items: vec![MenuItem::action(
                "Add Local Repository...",
                MenuOpenRepository,
            )],
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::os_action("Undo", NoAction {}, OsAction::Undo),
                MenuItem::os_action("Redo", NoAction {}, OsAction::Redo),
                MenuItem::separator(),
                MenuItem::os_action("Cut", NoAction {}, OsAction::Cut),
                MenuItem::os_action("Copy", NoAction {}, OsAction::Copy),
                MenuItem::os_action("Paste", NoAction {}, OsAction::Paste),
                MenuItem::os_action("Select All", NoAction {}, OsAction::SelectAll),
            ],
        },
        Menu {
            name: "View".into(),
            items: vec![
                MenuItem::action("Show Changes", MenuShowChanges),
                MenuItem::action("Show History", MenuShowHistory),
                MenuItem::action("Show Repository List", MenuShowRepositoryList),
                MenuItem::action("Show Branches List", MenuShowBranchesList),
                MenuItem::separator(),
                MenuItem::action("Go to Summary", MenuGoToSummary),
                MenuItem::separator(),
                MenuItem::action("Zoom In", MenuZoomIn),
                MenuItem::action("Zoom Out", MenuZoomOut),
                MenuItem::action("Actual Size", MenuZoomReset),
            ],
        },
        Menu {
            name: "Repository".into(),
            items: vec![
                MenuItem::action("Fetch", MenuFetch),
                MenuItem::action("Pull", MenuPull),
                MenuItem::action("Push", MenuPush),
                MenuItem::action("Publish Repository", MenuPublishRepository),
                MenuItem::separator(),
                MenuItem::action("View on GitHub", MenuViewOnGitHub),
                MenuItem::action("Open in Terminal", MenuOpenInTerminal),
                MenuItem::action("Show in Finder", MenuShowInFinder),
                MenuItem::action("Open in External Editor", MenuOpenExternalEditor),
                MenuItem::separator(),
                MenuItem::action("Repository Settings...", MenuRepositorySettings),
            ],
        },
        Menu {
            name: "Branch".into(),
            items: vec![
                MenuItem::action("New Branch...", MenuNewBranch),
                MenuItem::action("Rename...", MenuRenameBranch),
                MenuItem::action("Merge into Current Branch...", MenuMergeBranch),
                MenuItem::separator(),
                MenuItem::action("Stash All Changes...", MenuStashChanges),
            ],
        },
    ]);
}

fn main() {
    let settings = match load_settings() {
        Ok(s) => s,
        Err(_) => models::AppSettings::default(),
    };

    let app = Application::new().with_assets(assets::CombinedAssets);
    app.run(move |cx| {
        gpui_component::init(cx);
        // Force dark theme on gpui-component to match our GitHub Dark theme
        gpui_component::Theme::change(gpui_component::ThemeMode::Dark, None, cx);
        let app_view = cx.new(|cx| GitSparkApp::new(settings.clone(), cx));
        configure_native_menus(cx, app_view.clone());

        // Use saved window size, or derive from primary display:
        //   60% of display width/height, capped to 16:9, min 960×600
        let (initial_width, initial_height) =
            if settings.window_size.width > 0.0 && settings.window_size.height > 0.0 {
                (settings.window_size.width, settings.window_size.height)
            } else if let Some(display) = cx.primary_display() {
                let dw = display.bounds().size.width;
                let dh = display.bounds().size.height;
                let win_h = dh * 0.6;
                let win_h = if win_h < px(600.0) { px(600.0) } else { win_h };
                let max_w = win_h * (16.0 / 9.0);
                let win_w_raw = dw * 0.6;
                let win_w = if win_w_raw < px(960.0) {
                    px(960.0)
                } else if win_w_raw > max_w {
                    max_w
                } else {
                    win_w_raw
                };
                // Pixels / Pixels -> f32
                (win_w / px(1.0), win_h / px(1.0))
            } else {
                (1280.0, 860.0)
            };

        let (window_bounds, restore_display_id) = if settings.window_size.has_position {
            // Find the saved display by ID so the window opens on the correct monitor
            let display_id = settings.window_size.display_id.and_then(|saved_id| {
                cx.displays().into_iter().find_map(|d| {
                    let id: u32 = d.id().into();
                    if id == saved_id { Some(d.id()) } else { None }
                })
            });
            (
                WindowBounds::Windowed(Bounds::new(
                    point(px(settings.window_size.x), px(settings.window_size.y)),
                    size(px(initial_width), px(initial_height)),
                )),
                display_id,
            )
        } else {
            (
                WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(initial_width), px(initial_height)),
                    cx,
                )),
                None,
            )
        };

        cx.open_window(
            WindowOptions {
                window_bounds: Some(window_bounds),
                display_id: restore_display_id,
                titlebar: Some(platform_titlebar_options()),
                window_min_size: Some(size(px(720.0), px(480.0))),
                ..Default::default()
            },
            move |_window, _cx| app_view.clone(),
        )
        .unwrap();
    });
}
