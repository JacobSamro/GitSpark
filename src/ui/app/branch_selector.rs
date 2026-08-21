use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use gpui_component::{Icon, IconName, h_flex, v_flex};

use crate::models::BranchInfo;
use crate::ui::app::GitSparkApp;
use crate::ui::ids::stable_id_slug;
use crate::ui::kit;
use crate::ui::theme;
use crate::ui::ui_state::{ActiveDialog, BranchSelectorMode};
use std::collections::HashMap;

#[derive(Clone)]
enum BranchListItem {
    SectionHeader(String),
    Branch(BranchInfo),
}

pub(super) fn render_branch_selector_overlay(
    app: &GitSparkApp,
    branch_filter_focused: bool,
    cx: &mut Context<GitSparkApp>,
) -> Div {
    let backdrop = div()
        .id("branch-selector-backdrop")
        .absolute()
        .top(theme::z(theme::TOOLBAR_HEIGHT))
        .left_0()
        .w_full()
        .bottom_0()
        .on_click(cx.listener(|app, _evt, _win, cx| {
            app.nav.show_branch_selector = false;
            app.nav.branch_selector_mode = BranchSelectorMode::Switch;
            app.repo.pending_cherry_pick_oid = None;
            cx.notify();
        }));

    let panel = render_branch_selector_panel(app, branch_filter_focused, cx)
        .id("branch-selector-panel")
        .on_click(|_evt, _win, cx| cx.stop_propagation())
        .absolute()
        .top(theme::z(theme::TOOLBAR_HEIGHT))
        // Anchored under the branch section, which the worktree section now
        // pushes to the right.
        .left(px(crate::ui::toolbar::branch_dropdown_left_offset()))
        .w(px(360.0))
        .h(px(486.0))
        .shadow_lg();

    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .child(backdrop)
        .child(panel)
}

fn render_branch_selector_panel(
    app: &GitSparkApp,
    branch_filter_focused: bool,
    cx: &mut Context<GitSparkApp>,
) -> Div {
    let snapshot = app.repo.snapshot.as_ref();
    let current_branch = snapshot
        .map(|s| s.repo.current_branch.clone())
        .unwrap_or_else(|| "main".to_string());
    let branches: Vec<BranchInfo> = snapshot.map(|s| s.branches.clone()).unwrap_or_default();

    let filter = app.filters.branch_filter_text.to_lowercase();
    let merge_mode = app.nav.branch_selector_mode == BranchSelectorMode::Merge;
    let rebase_mode = app.nav.branch_selector_mode == BranchSelectorMode::Rebase;
    let compare_mode = app.nav.branch_selector_mode == BranchSelectorMode::Compare;
    let target_mode = merge_mode || rebase_mode || compare_mode;
    let local_branches: Vec<&BranchInfo> = branches
        .iter()
        .filter(|b| !b.is_remote)
        .filter(|b| !target_mode || !b.is_current)
        .filter(|b| filter.is_empty() || b.name.to_lowercase().contains(&filter))
        .collect();

    let default_branch_name = app.default_branch_name();
    let branch_selector_target = current_branch.clone();
    let filter_bar = render_filter_bar(app, branch_filter_focused, target_mode, cx);
    // Branches checked out in a DIFFERENT worktree cannot be checked out
    // here; map them to the worktree that holds them so the rows can say so.
    let held_by: HashMap<String, String> = app
        .repo
        .worktrees
        .iter()
        .filter(|worktree| !worktree.is_current)
        .filter_map(|worktree| {
            worktree
                .branch
                .clone()
                .map(|branch| (branch, worktree.name.clone()))
        })
        .collect();
    let branch_list = render_branch_list(local_branches, default_branch_name, held_by, cx);
    let bottom_bar = render_bottom_bar(app, branch_selector_target);

    v_flex()
        .size_full()
        .bg(theme::panel_bg())
        .child(filter_bar)
        .child(
            div()
                .id("branch-selector-list-viewport")
                .flex_1()
                .min_h_0()
                .relative()
                .child(branch_list),
        )
        .child(bottom_bar)
}

fn render_filter_bar(
    app: &GitSparkApp,
    branch_filter_focused: bool,
    target_mode: bool,
    cx: &mut Context<GitSparkApp>,
) -> AnyElement {
    h_flex()
        .w_full()
        .flex_shrink_0()
        .px(px(10.0))
        .py(px(10.0))
        .gap(px(8.0))
        .items_center()
        .child(render_filter_input(app, branch_filter_focused, cx))
        .children(if target_mode {
            None
        } else {
            Some(
                h_flex()
                    .id("branch-new-btn")
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
                    .on_click(cx.listener(|app, _evt, window, cx| {
                        app.menu_new_branch(cx);
                        if matches!(app.nav.active_dialog, ActiveDialog::CreateBranch) {
                            window.focus(&app.new_branch_focus);
                        }
                    }))
                    .child(
                        div()
                            .text_size(theme::z(theme::FONT_SIZE))
                            .text_color(theme::text_main())
                            .child("New Branch"),
                    ),
            )
        })
        .into_any_element()
}

fn render_filter_input(
    app: &GitSparkApp,
    focused: bool,
    cx: &mut Context<GitSparkApp>,
) -> AnyElement {
    kit::filter_input(
        "branch-filter-input",
        &app.branch_filter_focus,
        &app.filters.branch_filter_text,
        app.branch_filter_cursor,
        focused,
        "Filter",
    )
    .key_context("text-field")
    .on_key_down(cx.listener(GitSparkApp::handle_branch_filter_key))
    .into_any_element()
}

fn render_branch_list(
    local_branches: Vec<&BranchInfo>,
    default_branch_name: String,
    held_by: HashMap<String, String>,
    cx: &mut Context<GitSparkApp>,
) -> AnyElement {
    let mut default_branches: Vec<BranchInfo> = Vec::new();
    let mut other_branches: Vec<BranchInfo> = Vec::new();
    for branch in local_branches {
        if branch.name == default_branch_name {
            default_branches.push(branch.clone());
        } else {
            other_branches.push(branch.clone());
        }
    }

    let mut items: Vec<BranchListItem> = Vec::new();
    if !default_branches.is_empty() {
        items.push(BranchListItem::SectionHeader("Default Branch".to_string()));
        items.extend(default_branches.into_iter().map(BranchListItem::Branch));
    }
    if !other_branches.is_empty() {
        items.push(BranchListItem::SectionHeader("Other Branches".to_string()));
        items.extend(other_branches.into_iter().map(BranchListItem::Branch));
    }

    if items.is_empty() {
        return div()
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .w_full()
            .child(
                v_flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .px(px(16.0))
                    .child(
                        div()
                            .text_size(theme::z(12.0))
                            .text_color(theme::text_muted())
                            .text_align(gpui::TextAlign::Center)
                            .child("Sorry, I can't find that branch"),
                    ),
            )
            .into_any_element();
    }

    let count = items.len();
    let view = cx.entity().clone();
    div()
        .id("branch-list-scroll")
        .absolute()
        .top_0()
        .bottom_0()
        .left_0()
        .w_full()
        .overflow_y_scrollbar()
        .child(
            uniform_list("branch-list", count, {
                move |range, _win, _cx| {
                    range
                        .map(|ix| match &items[ix] {
                            BranchListItem::SectionHeader(title) => {
                                render_section_header(ix, title)
                            }
                            BranchListItem::Branch(branch) => render_branch_row(
                                branch,
                                held_by.get(&branch.name).cloned(),
                                view.clone(),
                            ),
                        })
                        .collect()
                }
            })
            .flex_1()
            .with_sizing_behavior(ListSizingBehavior::Infer),
        )
        .into_any_element()
}

fn render_section_header(ix: usize, title: &str) -> AnyElement {
    div()
        .id(SharedString::from(format!("branch-section-{ix}")))
        .w_full()
        .px(px(10.0))
        .py(px(8.0))
        .child(
            div()
                .text_size(theme::z(theme::FONT_SIZE))
                .text_color(theme::text_main())
                .font_weight(FontWeight::BOLD)
                .child(title.to_string()),
        )
        .into_any_element()
}

/// `held_by` names the OTHER worktree that has this branch checked out.
///
/// Git refuses to check out a branch that is already checked out elsewhere
/// (see `refuses_to_check_out_a_branch_already_checked_out_elsewhere`), so
/// clicking such a row could only ever produce an error. The row says which
/// worktree holds it and stops being clickable, which turns a failure into an
/// explanation.
fn render_branch_row(
    branch: &BranchInfo,
    held_by: Option<String>,
    view: Entity<GitSparkApp>,
) -> AnyElement {
    let is_current = branch.is_current;
    let locked = held_by.is_some();
    let name = branch.name.clone();
    let ctx_name = branch.name.clone();
    let updated = branch.updated.clone();
    let branch_id = stable_id_slug(&branch.name);
    let vh = view.clone();

    let row = h_flex()
        .id(SharedString::from(format!("branch-{branch_id}")))
        .w_full()
        .h(px(36.0))
        .px(px(10.0))
        .items_center()
        .gap(px(8.0))
        .when(!locked, |el| {
            el.cursor_pointer().hover(|s| s.bg(theme::hover_bg()))
        })
        .bg(if is_current {
            theme::hover_bg()
        } else {
            gpui::transparent_black()
        })
        .child({
            let mut check_slot = div()
                .w(px(20.0))
                .flex_shrink_0()
                .items_center()
                .justify_center();
            if is_current {
                check_slot = check_slot.child(
                    Icon::new(IconName::Check)
                        .size(px(14.0))
                        .text_color(theme::text_main()),
                );
            }
            check_slot
        })
        .child(
            div().flex_1().overflow_x_hidden().child(
                div()
                    .text_size(theme::z(theme::FONT_SIZE))
                    .text_color(if locked {
                        theme::text_muted()
                    } else {
                        theme::text_main()
                    })
                    .whitespace_nowrap()
                    .child(branch.name.clone()),
            ),
        )
        .children(held_by.map(|worktree| {
            div()
                .flex_shrink_0()
                .px(theme::z(theme::SPACE_3))
                .rounded(theme::z(theme::RADIUS_PILL))
                .border_1()
                .border_color(theme::with_alpha(theme::warning(), 0.35))
                .text_size(theme::z(theme::FONT_SIZE_XS))
                .text_color(theme::warning())
                .child(format!("in {worktree}"))
        }))
        .children(updated.map(|updated| {
            div()
                .flex_shrink_0()
                .text_size(theme::z(12.0))
                .text_color(theme::text_muted())
                .child(updated)
        }))
        .when(!locked, |el| {
            el.on_click(move |_evt, _win, cx| {
                let name = name.clone();
                vh.update(cx, |app, cx| {
                    app.select_branch_from_selector(name, cx);
                });
            })
        });

    crate::ui::branch_context_menu::bind_branch_context_click(row, view, ctx_name)
        .into_any_element()
}

fn render_bottom_bar(app: &GitSparkApp, branch_selector_target: String) -> AnyElement {
    let branch_selector_footer = if app.repo.pending_cherry_pick_oid.is_some() {
        "Choose a branch to cherry-pick into"
    } else if app.nav.branch_selector_mode == BranchSelectorMode::Merge {
        "Choose a branch to merge into"
    } else if app.nav.branch_selector_mode == BranchSelectorMode::Rebase {
        "Choose a branch to rebase onto"
    } else if app.nav.branch_selector_mode == BranchSelectorMode::Compare {
        "Choose a branch to compare against"
    } else {
        "Choose a branch to switch to"
    };
    let show_branch_selector_target = app.repo.pending_cherry_pick_oid.is_some()
        || app.nav.branch_selector_mode == BranchSelectorMode::Merge
        || app.nav.branch_selector_mode == BranchSelectorMode::Rebase
        || app.nav.branch_selector_mode == BranchSelectorMode::Compare;

    h_flex()
        .id("branch-selector-merge-bar")
        .w_full()
        .h(px(52.0))
        .flex_shrink_0()
        .border_t_1()
        .border_color(theme::toolbar_button_border())
        .px(px(10.0))
        .bg(theme::overlay_bg())
        .items_center()
        .justify_center()
        .child(
            h_flex()
                .id("branch-selector-merge-button")
                .w_full()
                .h(px(32.0))
                .items_center()
                .justify_center()
                .gap(px(6.0))
                .rounded(theme::z(theme::CORNER_RADIUS))
                .border_1()
                .border_color(theme::surface_bg_alt())
                .bg(theme::panel_bg())
                .child(
                    div()
                        .text_size(theme::z(14.0))
                        .text_color(theme::text_main())
                        .child("⑂"),
                )
                .child(
                    div()
                        .text_size(theme::z(theme::FONT_SIZE))
                        .text_color(theme::text_muted())
                        .child(branch_selector_footer),
                )
                .when(show_branch_selector_target, |el| {
                    el.child(
                        div()
                            .text_size(theme::z(theme::FONT_SIZE))
                            .text_color(theme::text_main())
                            .font_weight(FontWeight::BOLD)
                            .child(branch_selector_target),
                    )
                }),
        )
        .into_any_element()
}
