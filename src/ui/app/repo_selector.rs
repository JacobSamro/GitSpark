use gpui::*;
use gpui_component::{Icon, IconName, h_flex, v_flex};

use crate::ui::app::GitSparkApp;
use crate::ui::ids::stable_id_slug;
use crate::ui::kit;
use crate::ui::theme;

pub(super) fn render_repo_selector_panel(
    app: &GitSparkApp,
    repo_filter_focused: bool,
    cx: &mut Context<GitSparkApp>,
) -> Div {
    let recent_repos = app.settings.recent_repos.clone();
    let current_repo = app
        .repo
        .snapshot
        .as_ref()
        .map(|s| s.repo.name.clone())
        .unwrap_or_default();

    // --- Header: current repo + caret up ---
    let _header = h_flex()
        .id("repo-selector-header")
        .w_full()
        .h(theme::z(theme::TOOLBAR_HEIGHT))
        .flex_shrink_0()
        .bg(theme::toolbar_bg())
        .border_b_1()
        .border_color(theme::toolbar_button_border())
        .px(px(10.0))
        .gap(px(10.0))
        .items_center()
        .cursor_pointer()
        .on_click(cx.listener(|app, _evt, _win, cx| {
            app.nav.show_repo_selector = false;
            cx.notify();
        }))
        // Repo icon
        .child(
            div().flex_shrink_0().child(
                gpui_component::Icon::new(gpui_component::IconName::FolderOpen)
                    .size(px(16.0))
                    .text_color(theme::text_main()),
            ),
        )
        // Text stack
        .child(
            v_flex()
                .flex_1()
                .gap(px(2.0))
                .overflow_hidden()
                .child(
                    div()
                        .text_size(theme::z(theme::FONT_SIZE_SM))
                        .text_color(theme::text_muted())
                        .child("Current Repository"),
                )
                .child(
                    div()
                        .text_size(theme::z(theme::FONT_SIZE))
                        .text_color(theme::text_main())
                        .font_weight(FontWeight::SEMIBOLD)
                        .overflow_x_hidden()
                        .whitespace_nowrap()
                        .child(current_repo),
                ),
        )
        // Caret up
        .child(
            div().flex_shrink_0().child(
                gpui_component::Icon::new(gpui_component::IconName::ChevronUp)
                    .size(px(10.0))
                    .text_color(theme::text_muted()),
            ),
        );

    // --- Filter bar ---
    let filter_bar = kit::filter_bar()
        .child(
            kit::filter_input(
                "repo-filter-input",
                &app.repo_filter_focus,
                &app.filters.repo_filter_text,
                app.repo_filter_cursor,
                repo_filter_focused,
                "Filter",
            )
            .key_context("text-field")
            .on_key_down(cx.listener(GitSparkApp::handle_repo_filter_key)),
        )
        // Add button
        .child(
            h_flex()
                .id("repo-add-btn")
                .flex_shrink_0()
                .h(px(28.0))
                .px(px(12.0))
                .items_center()
                .justify_center()
                .rounded(theme::z(theme::CORNER_RADIUS))
                .bg(theme::overlay_bg())
                .border_1()
                .border_color(theme::surface_bg_alt())
                .cursor_pointer()
                .hover(|s| s.bg(theme::toolbar_hover_bg()))
                .on_click(cx.listener(|app, _evt, _win, cx| {
                    app.open_repo_dialog(cx);
                }))
                .child(
                    h_flex()
                        .items_center()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(theme::z(theme::FONT_SIZE))
                                .text_color(theme::text_main())
                                .child("Add"),
                        )
                        .child(
                            Icon::new(IconName::ChevronDown)
                                .size(px(8.0))
                                .text_color(theme::text_muted()),
                        ),
                ),
        );

    // --- Repo list ---
    // Filter repos by search text
    let repo_filter = app.filters.repo_filter_text.to_lowercase();
    let repos_snapshot: Vec<_> = recent_repos
        .iter()
        .filter(|p| {
            repo_filter.is_empty()
                || p.file_name()
                    .map(|n| n.to_string_lossy().to_lowercase().contains(&repo_filter))
                    .unwrap_or(false)
        })
        .cloned()
        .collect();
    let repo_list = if repos_snapshot.is_empty() {
        let empty_message = if repo_filter.is_empty() {
            "No recent repositories"
        } else {
            "Sorry, I can't find that repository"
        };
        div().flex_1().child(
            v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .px(px(14.0))
                .child(
                    div()
                        .text_size(theme::z(12.0))
                        .text_color(theme::text_muted())
                        .text_align(gpui::TextAlign::Center)
                        .child(empty_message),
                ),
        )
    } else {
        let count = repos_snapshot.len();
        div().flex_1().child(
            uniform_list("repo-list", count, {
                let repos = repos_snapshot.clone();
                let current = app
                    .repo
                    .snapshot
                    .as_ref()
                    .map(|s| s.repo.path.clone())
                    .unwrap_or_default();
                let view = cx.entity().clone();
                move |range, _win, _cx| {
                    range
                        .map(|ix| {
                            let repo_path = &repos[ix];
                            let display_name = repo_path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| repo_path.to_string_lossy().to_string());
                            let is_current =
                                repo_path.to_string_lossy() == current.to_string_lossy();
                            let repo_id = stable_id_slug(&repo_path.to_string_lossy());
                            let path_clone = repo_path.clone();
                            let vh = view.clone();

                            h_flex()
                                .id(SharedString::from(format!("repo-{repo_id}")))
                                .w_full()
                                .h(px(40.0))
                                .px(px(10.0))
                                .items_center()
                                .gap(px(8.0))
                                .cursor_pointer()
                                .hover(|s| s.bg(theme::hover_bg()))
                                .bg(if is_current {
                                    theme::surface_bg_alt()
                                } else {
                                    gpui::transparent_black()
                                })
                                // Repo icon
                                .child(
                                    Icon::new(IconName::FolderClosed)
                                        .size(px(16.0))
                                        .text_color(theme::text_muted()),
                                )
                                // Repo name
                                .child(
                                    div().flex_1().overflow_x_hidden().child(
                                        div()
                                            .text_size(theme::z(theme::FONT_SIZE))
                                            .text_color(theme::text_main())
                                            .whitespace_nowrap()
                                            .child(display_name),
                                    ),
                                )
                                .child(if is_current {
                                    div()
                                        .id(SharedString::from(format!(
                                            "repo-current-indicator-{}",
                                            repo_path.to_string_lossy()
                                        )))
                                        .w(px(7.0))
                                        .h(px(7.0))
                                        .rounded(px(999.0))
                                        .bg(theme::accent())
                                        .into_any_element()
                                } else {
                                    div().w(px(7.0)).h(px(7.0)).into_any_element()
                                })
                                .on_click(move |_evt, _win, cx| {
                                    let p = path_clone.clone();
                                    vh.update(cx, |app, cx| {
                                        app.open_repo_with_notify(p, cx);
                                    });
                                })
                                .into_any_element()
                        })
                        .collect()
                }
            })
            .flex_1()
            .with_sizing_behavior(ListSizingBehavior::Infer),
        )
    };

    // --- Section header: "Recent" ---
    let section_header = div().w_full().px(px(10.0)).py(px(8.0)).child(
        div()
            .text_size(theme::z(theme::FONT_SIZE))
            .text_color(theme::text_main())
            .font_weight(FontWeight::BOLD)
            .child("Recent"),
    );

    // --- Fill the sidebar panel ---
    v_flex()
        .size_full()
        .bg(theme::panel_bg())
        .border_r_1()
        .border_color(theme::border())
        .child(filter_bar)
        .child(section_header)
        .child(repo_list)
}
