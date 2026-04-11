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
                ..Default::default()
            },
            |_window, cx| cx.new(|cx| GitSparkApp::new(settings.clone(), cx)),
        )
        .unwrap();
    });
}
