// Suppress the console window on Windows for GUI apps.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod assets;
mod git;
mod gitoxide;
mod models;
mod storage;
mod ui;
mod update;

use gpui::*;

use crate::storage::load_settings;
use crate::ui::GitSparkApp;

const WINDOW_MIN_WIDTH: f32 = 960.0;
const WINDOW_MIN_HEIGHT: f32 = 600.0;
const DEFAULT_WINDOW_WIDTH: f32 = 1280.0;
const DEFAULT_WINDOW_HEIGHT: f32 = 860.0;

actions!(
    gitspark_menu,
    [
        MenuOpenRepository,
        MenuNewRepository,
        MenuCloneRepository,
        MenuShowSettings,
        MenuShowChanges,
        MenuShowHistory,
        MenuShowRepositoryList,
        MenuShowBranchesList,
        MenuGoToSummary,
        MenuShowStashedChanges,
        MenuReload,
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
        MenuDeleteBranch,
        MenuUpdateFromDefaultBranch,
        MenuCompareBranch,
        MenuMergeBranch,
        MenuRebaseBranch,
        MenuCompareOnGitHub,
        MenuViewBranchOnGitHub,
        MenuDiscardAllChanges,
        MenuStashChanges,
        MenuNewRepositoryTab,
        MenuCloseRepositoryTab,
        MenuNextRepositoryTab,
        MenuPreviousRepositoryTab,
        MenuZoomIn,
        MenuZoomOut,
        MenuZoomReset,
        MenuDisabled,
        MenuQuit
    ]
);

#[derive(Clone, Copy, Default)]
pub(crate) struct MenuAvailability {
    pub has_repository: bool,
    pub fetch: bool,
    pub pull: bool,
    pub push: bool,
    pub publish_repository: bool,
    pub view_repository_on_github: bool,
    pub create_branch: bool,
    pub modify_current_branch: bool,
    pub compare_on_github: bool,
    pub change_worktree: bool,
}

fn platform_titlebar_options() -> TitlebarOptions {
    #[cfg(target_os = "macos")]
    {
        TitlebarOptions {
            title: Some("GitSpark".into()),
            appears_transparent: true,
            // Centred in the strip rather than a hand-tuned constant: the
            // two used to be set independently, so shrinking the title bar
            // left the lights sitting above centre.
            traffic_light_position: Some(point(
                px(10.0),
                px((ui::theme::TITLEBAR_HEIGHT - ui::theme::TRAFFIC_LIGHT_DIAMETER) / 2.0),
            )),
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
        cx.on_action(move |_: &MenuNewRepository, cx| {
            let _ = view.update(cx, |app, cx| app.menu_new_repository(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuCloneRepository, cx| {
            let _ = view.update(cx, |app, cx| app.menu_clone_repository(cx));
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
        cx.on_action(move |_: &MenuShowStashedChanges, cx| {
            let _ = view.update(cx, |app, cx| app.menu_show_stashed_changes(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuReload, cx| {
            let _ = view.update(cx, |app, cx| app.menu_reload(cx));
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
        cx.on_action(move |_: &MenuDeleteBranch, cx| {
            let _ = view.update(cx, |app, cx| app.menu_delete_current_branch(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuUpdateFromDefaultBranch, cx| {
            let _ = view.update(cx, |app, cx| app.menu_update_from_default_branch(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuCompareBranch, cx| {
            let _ = view.update(cx, |app, cx| app.menu_compare_branch(cx));
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
        cx.on_action(move |_: &MenuRebaseBranch, cx| {
            let _ = view.update(cx, |app, cx| app.menu_rebase_branch(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuCompareOnGitHub, cx| {
            let _ = view.update(cx, |app, cx| app.menu_compare_current_branch_on_github(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuViewBranchOnGitHub, cx| {
            let _ = view.update(cx, |app, cx| app.menu_view_current_branch_on_github(cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuDiscardAllChanges, cx| {
            let _ = view.update(cx, |app, cx| app.menu_discard_all_changes(cx));
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
        cx.on_action(move |_: &MenuNewRepositoryTab, cx| {
            let _ = view.update(cx, |app, cx| {
                app.nav.show_repo_selector = true;
                cx.notify();
            });
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuCloseRepositoryTab, cx| {
            let _ = view.update(cx, |app, cx| {
                let active = app.active_tab;
                app.close_tab(active, cx);
            });
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuNextRepositoryTab, cx| {
            let _ = view.update(cx, |app, cx| app.cycle_tab(1, cx));
        });
    }
    {
        let view = view.clone();
        cx.on_action(move |_: &MenuPreviousRepositoryTab, cx| {
            let _ = view.update(cx, |app, cx| app.cycle_tab(-1, cx));
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
        KeyBinding::new("cmd-t", MenuNewRepositoryTab, None),
        KeyBinding::new("cmd-b", MenuShowBranchesList, None),
        KeyBinding::new("cmd-g", MenuGoToSummary, None),
        KeyBinding::new("ctrl-h", MenuShowStashedChanges, None),
        // "cmd-" resolves to the platform key (Cmd on macOS, Super/Win
        // elsewhere) not Ctrl, so both are bound explicitly rather than
        // assuming one covers the other.
        KeyBinding::new("cmd-r", MenuReload, None),
        KeyBinding::new("ctrl-r", MenuReload, None),
        KeyBinding::new("cmd-p", MenuPush, None),
        KeyBinding::new("cmd-n", MenuNewRepository, None),
        KeyBinding::new("cmd-o", MenuOpenRepository, None),
        KeyBinding::new("cmd-shift-o", MenuCloneRepository, None),
        KeyBinding::new("cmd-shift-p", MenuPull, None),
        KeyBinding::new("cmd-shift-t", MenuFetch, None),
        KeyBinding::new("cmd-shift-a", MenuOpenExternalEditor, None),
        KeyBinding::new("ctrl-`", MenuOpenInTerminal, None),
        KeyBinding::new("cmd-shift-f", MenuShowInFinder, None),
        KeyBinding::new("cmd-shift-g", MenuViewOnGitHub, None),
        KeyBinding::new("cmd-shift-n", MenuNewBranch, None),
        KeyBinding::new("cmd-shift-r", MenuRenameBranch, None),
        KeyBinding::new("cmd-shift-d", MenuDeleteBranch, None),
        KeyBinding::new("cmd-shift-u", MenuUpdateFromDefaultBranch, None),
        KeyBinding::new("cmd-shift-b", MenuCompareBranch, None),
        KeyBinding::new("cmd-shift-m", MenuMergeBranch, None),
        KeyBinding::new("cmd-shift-e", MenuRebaseBranch, None),
        KeyBinding::new("cmd-shift-c", MenuCompareOnGitHub, None),
        KeyBinding::new("cmd-alt-b", MenuViewBranchOnGitHub, None),
        KeyBinding::new("cmd-shift-backspace", MenuDiscardAllChanges, None),
        KeyBinding::new("cmd-shift-s", MenuStashChanges, None),
        KeyBinding::new("cmd-=", MenuZoomIn, None),
        KeyBinding::new("cmd-+", MenuZoomIn, None),
        KeyBinding::new("cmd--", MenuZoomOut, None),
        KeyBinding::new("cmd-0", MenuZoomReset, None),
        KeyBinding::new("cmd-w", MenuCloseRepositoryTab, None),
        // NOT cmd-1..9: those already move between Changes and History, and
        // a sidebar tab is switched far more often than a repository.
        KeyBinding::new("ctrl-tab", MenuNextRepositoryTab, None),
        KeyBinding::new("cmd-shift-]", MenuNextRepositoryTab, None),
        KeyBinding::new("ctrl-shift-tab", MenuPreviousRepositoryTab, None),
        KeyBinding::new("cmd-shift-[", MenuPreviousRepositoryTab, None),
        KeyBinding::new("cmd-q", MenuQuit, None),
    ]);

    install_native_menus(cx, MenuAvailability::default());
}

fn menu_action(name: &'static str, enabled: bool, action: impl Action) -> MenuItem {
    if enabled {
        MenuItem::action(name, action)
    } else {
        MenuItem::action(name, MenuDisabled)
    }
}

pub(crate) fn install_native_menus(cx: &mut App, availability: MenuAvailability) {
    cx.set_menus(vec![
        Menu {
            name: "GitSpark".into(),
            items: vec![
                MenuItem::action("Settings…", MenuShowSettings),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Quit GitSpark", MenuQuit),
            ],
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("New Repository…", MenuNewRepository),
                MenuItem::separator(),
                MenuItem::action("Add Local Repository…", MenuOpenRepository),
                MenuItem::action("Clone Repository…", MenuCloneRepository),
            ],
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
                menu_action(
                    "Show Branches List",
                    availability.has_repository,
                    MenuShowBranchesList,
                ),
                MenuItem::separator(),
                MenuItem::action("Go to Summary", MenuGoToSummary),
                menu_action(
                    "Show Stashed Changes",
                    availability.has_repository,
                    MenuShowStashedChanges,
                ),
                MenuItem::separator(),
                MenuItem::action("Reset Zoom", MenuZoomReset),
                MenuItem::action("Zoom In", MenuZoomIn),
                MenuItem::action("Zoom Out", MenuZoomOut),
            ],
        },
        Menu {
            name: "Repository".into(),
            items: vec![
                menu_action("Reload", availability.has_repository, MenuReload),
                MenuItem::separator(),
                menu_action("Push", availability.push, MenuPush),
                menu_action("Pull", availability.pull, MenuPull),
                menu_action("Fetch", availability.fetch, MenuFetch),
                menu_action(
                    "Publish Repository",
                    availability.publish_repository,
                    MenuPublishRepository,
                ),
                MenuItem::separator(),
                menu_action(
                    "View on GitHub",
                    availability.view_repository_on_github,
                    MenuViewOnGitHub,
                ),
                menu_action(
                    "Open in Terminal",
                    availability.has_repository,
                    MenuOpenInTerminal,
                ),
                menu_action(
                    "Show in Finder",
                    availability.has_repository,
                    MenuShowInFinder,
                ),
                menu_action(
                    "Open in External Editor",
                    availability.has_repository,
                    MenuOpenExternalEditor,
                ),
                MenuItem::separator(),
                menu_action(
                    "Repository Settings…",
                    availability.has_repository,
                    MenuRepositorySettings,
                ),
            ],
        },
        Menu {
            name: "Branch".into(),
            items: vec![
                menu_action("New Branch…", availability.create_branch, MenuNewBranch),
                menu_action(
                    "Rename…",
                    availability.modify_current_branch,
                    MenuRenameBranch,
                ),
                menu_action(
                    "Delete…",
                    availability.modify_current_branch,
                    MenuDeleteBranch,
                ),
                MenuItem::separator(),
                menu_action(
                    "Update from Default Branch",
                    availability.modify_current_branch,
                    MenuUpdateFromDefaultBranch,
                ),
                menu_action(
                    "Compare to Branch",
                    availability.modify_current_branch,
                    MenuCompareBranch,
                ),
                menu_action(
                    "Merge into Current Branch…",
                    availability.modify_current_branch,
                    MenuMergeBranch,
                ),
                menu_action(
                    "Rebase Current Branch…",
                    availability.modify_current_branch,
                    MenuRebaseBranch,
                ),
                MenuItem::separator(),
                menu_action(
                    "Compare on GitHub",
                    availability.compare_on_github,
                    MenuCompareOnGitHub,
                ),
                menu_action(
                    "View Branch on GitHub",
                    availability.modify_current_branch && availability.view_repository_on_github,
                    MenuViewBranchOnGitHub,
                ),
                MenuItem::separator(),
                menu_action(
                    "Discard All Changes…",
                    availability.change_worktree,
                    MenuDiscardAllChanges,
                ),
                menu_action(
                    "Stash All Changes…",
                    availability.change_worktree,
                    MenuStashChanges,
                ),
            ],
        },
    ]);
}

fn main() {
    let settings = match load_settings() {
        Ok(s) => s,
        Err(_) => models::AppSettings::default(),
    };

    // Load the appearance preference before anything draws, so the first
    // frame is already in the right palette rather than flashing and
    // correcting. Resolution against the OS happens once the app exists.
    // Defaults to Dark, not System, deliberately: the app shipped dark-only,
    // so following the OS here would silently flip every existing user on a
    // light Mac. System is one click away in Settings ▸ Appearance.
    ui::theme::set_appearance(ui::theme::Appearance::from_str(
        settings.appearance.as_deref().unwrap_or("dark"),
    ));

    let app = Application::new().with_assets(assets::CombinedAssets);
    app.run(move |cx| {
        gpui_component::init(cx);

        // Resolve System against the OS, then keep gpui-component's own theme
        // in step — without this its stock components follow the system
        // appearance while ours follow the preference.
        let dark = ui::theme::resolve(matches!(
            cx.window_appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        ));
        gpui_component::Theme::change(
            if dark {
                gpui_component::ThemeMode::Dark
            } else {
                gpui_component::ThemeMode::Light
            },
            None,
            cx,
        );
        let app_view = cx.new(|cx| GitSparkApp::new(settings.clone(), cx));
        configure_native_menus(cx, app_view.clone());

        // Use saved window size, or derive from primary display:
        //   60% of display width/height, capped to 16:9, min 960×600
        let (initial_width, initial_height) =
            if settings.window_size.width > 0.0 && settings.window_size.height > 0.0 {
                (
                    settings.window_size.width.max(WINDOW_MIN_WIDTH),
                    settings.window_size.height.max(WINDOW_MIN_HEIGHT),
                )
            } else if let Some(display) = cx.primary_display() {
                let dw = display.bounds().size.width;
                let dh = display.bounds().size.height;
                let win_h = dh * 0.6;
                let win_h = if win_h < px(WINDOW_MIN_HEIGHT) {
                    px(WINDOW_MIN_HEIGHT)
                } else {
                    win_h
                };
                let max_w = win_h * (16.0 / 9.0);
                let win_w_raw = dw * 0.6;
                let win_w = if win_w_raw < px(WINDOW_MIN_WIDTH) {
                    px(WINDOW_MIN_WIDTH)
                } else if win_w_raw > max_w {
                    max_w
                } else {
                    win_w_raw
                };
                // Pixels / Pixels -> f32
                (win_w / px(1.0), win_h / px(1.0))
            } else {
                (DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT)
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
                window_min_size: Some(size(px(WINDOW_MIN_WIDTH), px(WINDOW_MIN_HEIGHT))),
                ..Default::default()
            },
            move |_window, _cx| app_view.clone(),
        )
        .unwrap();
    });
}
