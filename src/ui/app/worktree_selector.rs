//! The Current Worktree picker.
//!
//! A worktree is a second checkout of the same repository in its own
//! directory, so switching to one is *opening a different path*, not checking
//! out a branch — the row click goes to `open_repo_with_notify`, exactly like
//! the repository picker.
//!
//! Two constraints this surfaces rather than hides:
//!
//! - Git refuses to check out a branch that is already checked out in another
//!   worktree, so each row names the branch it holds. That branch is
//!   unavailable in the Branch picker while this worktree exists.
//! - The primary worktree cannot be removed and always sorts first; it is
//!   tagged so the distinction is visible before someone tries.

use gpui::*;
use gpui_component::{Icon, IconName, h_flex, v_flex};

use crate::models::WorktreeInfo;
use crate::ui::app::GitSparkApp;
use crate::ui::ids::stable_id_slug;
use crate::ui::kit;
use crate::ui::theme;

/// Matches the repository picker, so the two read as one control.
const ROW_HEIGHT: f32 = 40.0;
pub const PANEL_WIDTH: f32 = 340.0;

pub(super) fn render_worktree_selector_panel(
    app: &GitSparkApp,
    filter_focused: bool,
    cx: &mut Context<GitSparkApp>,
) -> Div {
    let filter = app.filters.worktree_filter_text.to_lowercase();
    let worktrees: Vec<WorktreeInfo> = app
        .repo
        .worktrees
        .iter()
        .filter(|worktree| {
            filter.is_empty()
                || worktree.name.to_lowercase().contains(&filter)
                || worktree
                    .branch
                    .as_deref()
                    .is_some_and(|branch| branch.to_lowercase().contains(&filter))
                || worktree
                    .path
                    .to_string_lossy()
                    .to_lowercase()
                    .contains(&filter)
        })
        .cloned()
        .collect();

    let filter_bar = kit::filter_bar().child(
        kit::filter_input(
            "worktree-filter-input",
            &app.worktree_filter_focus,
            &app.filters.worktree_filter_text,
            app.worktree_filter_cursor,
            filter_focused,
            "Filter",
        )
        .key_context("text-field")
        .on_key_down(cx.listener(GitSparkApp::handle_worktree_filter_key)),
    );

    let list: AnyElement = if worktrees.is_empty() {
        let message = if app.repo.worktrees.is_empty() {
            "No worktrees found."
        } else {
            "No worktrees match the filter."
        };
        div()
            .w_full()
            .px(px(12.0))
            .py(px(18.0))
            .text_size(theme::z(theme::FONT_SIZE))
            .text_color(theme::text_muted())
            .child(message)
            .into_any_element()
    } else {
        let view = cx.entity().downgrade();
        let rows = worktrees.clone();
        div()
            .id("worktree-list")
            .w_full()
            .max_h(px(300.0))
            .overflow_y_scroll()
            .child(v_flex().w_full().children(rows.into_iter().map(|worktree| {
                let id = stable_id_slug(&worktree.path.to_string_lossy());
                let path = worktree.path.clone();
                let view = view.clone();
                let is_current = worktree.is_current;

                let subtitle = match (&worktree.branch, worktree.is_detached) {
                    (Some(branch), _) => branch.clone(),
                    (None, true) => "detached HEAD".to_string(),
                    (None, false) => worktree.path.to_string_lossy().to_string(),
                };

                h_flex()
                    .id(SharedString::from(format!("worktree-{id}")))
                    .w_full()
                    .h(px(ROW_HEIGHT))
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
                    .child(
                        Icon::new(IconName::FolderClosed)
                            .size(px(16.0))
                            .text_color(theme::text_muted()),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .overflow_x_hidden()
                            .child(
                                div()
                                    .text_size(theme::z(theme::FONT_SIZE))
                                    .text_color(theme::text_main())
                                    .whitespace_nowrap()
                                    .child(worktree.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(theme::z(theme::FONT_SIZE_SM))
                                    .text_color(theme::text_muted())
                                    .whitespace_nowrap()
                                    .child(subtitle),
                            ),
                    )
                    // The primary worktree is the one that cannot be removed.
                    .children(worktree.is_main.then(|| {
                        div()
                            .flex_shrink_0()
                            .px(theme::z(theme::SPACE_3))
                            .rounded(theme::z(theme::RADIUS_PILL))
                            .border_1()
                            .border_color(theme::with_alpha(theme::success(), 0.35))
                            .text_size(theme::z(theme::FONT_SIZE_XS))
                            .text_color(theme::success())
                            .child("main")
                    }))
                    .child(if is_current {
                        div()
                            .flex_shrink_0()
                            .w(px(7.0))
                            .h(px(7.0))
                            .rounded(px(999.0))
                            .bg(theme::accent())
                            .into_any_element()
                    } else {
                        div().flex_shrink_0().w(px(7.0)).h(px(7.0)).into_any_element()
                    })
                    .on_click(move |_evt, _win, cx| {
                        if is_current {
                            return;
                        }
                        let path = path.clone();
                        view.update(cx, |app, cx| {
                            app.nav.show_worktree_selector = false;
                            app.open_repo_with_notify(path, cx);
                        })
                        .ok();
                    })
            })))
            .into_any_element()
    };

    let footer = h_flex()
        .w_full()
        .flex_shrink_0()
        .border_t_1()
        .border_color(theme::border())
        .child(
            footer_button("worktree-add", "Add worktree\u{2026}").on_click(cx.listener(
                |app, _evt, _win, cx| {
                    app.nav.show_worktree_selector = false;
                    app.add_worktree_dialog(cx);
                },
            )),
        )
        .child(
            footer_button("worktree-prune", "Prune").on_click(cx.listener(
                |app, _evt, _win, cx| {
                    app.prune_worktrees(cx);
                },
            )),
        );

    v_flex()
        .w(px(PANEL_WIDTH))
        .bg(theme::panel_bg())
        .rounded(theme::z(theme::CORNER_RADIUS))
        .border_1()
        .border_color(theme::border())
        .shadow_lg()
        .child(filter_bar)
        .child(
            div()
                .w_full()
                .px(px(10.0))
                .pb(px(6.0))
                .text_size(theme::z(theme::FONT_SIZE_SM))
                .text_color(theme::text_muted())
                .child("Worktrees"),
        )
        .child(list)
        .child(footer)
}

/// One footer action. Two of them split the panel width evenly, matching the
/// picker footers in the other selectors.
fn footer_button(id: &'static str, label: &'static str) -> Stateful<Div> {
    h_flex()
        .id(id)
        .flex_1()
        .h(px(34.0))
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover_bg()))
        .text_size(theme::z(theme::FONT_SIZE))
        .text_color(theme::text_muted())
        .child(label)
}
