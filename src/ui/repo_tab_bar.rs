//! The repository tab strip (design.md §8.13).
//!
//! Sits between the title bar and the toolbar. That placement is the design:
//! worktree, branch and fetch state all belong to one repository, so the tabs
//! that choose the repository have to sit above the controls that read from
//! it. Switching a tab visibly changes everything beneath it.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Div, Entity, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, px,
};
use gpui_component::{Icon, IconName, h_flex};

use crate::ui::app::GitSparkApp;
use crate::ui::ids::stable_id_slug;
use crate::ui::theme;
use crate::ui::theme::z;

/// A tab never grows past this, so one long repository name cannot push the
/// rest of the strip off screen.
const TAB_MAX_WIDTH: f32 = 210.0;

/// Ties a tab to the close button inside it, so hovering anywhere on the tab
/// reveals the ×.
const TAB_GROUP: &str = "repo-tab";

/// The tabs and the `+`, with no surrounding chrome.
///
/// Separated from any background or border because on macOS this is embedded
/// directly in the window's title-bar row, beside the traffic lights, rather
/// than sitting in a strip of its own.
pub fn render_strip(app: &GitSparkApp, view: Entity<GitSparkApp>) -> Div {
    let active = app.active_tab;

    // The strip scrolls rather than shrinking tabs to illegibility or wrapping
    // to a second row — a second row would move everything below it every time
    // a repository is opened.
    let mut strip = h_flex()
        .id("repo-tab-strip")
        .flex_1()
        .min_w_0()
        .h_full()
        .overflow_x_scroll();

    for (index, tab) in app.tabs.iter().enumerate() {
        strip = strip.child(render_tab(
            index,
            &tab.label,
            &tab.path.to_string_lossy(),
            tab.changed_count,
            index == active,
            view.clone(),
        ));
    }

    h_flex()
        .flex_1()
        .min_w_0()
        .h_full()
        .child(strip)
        .child(render_add_button(view))
}

fn render_tab(
    index: usize,
    label: &str,
    path: &str,
    changed: usize,
    active: bool,
    view: Entity<GitSparkApp>,
) -> gpui::Stateful<Div> {
    let slug = stable_id_slug(path);
    let close_view = view.clone();

    let mut tab = h_flex()
        .id(SharedString::from(format!("repo-tab-{slug}")))
        .flex_shrink_0()
        .h_full()
        .max_w(z(TAB_MAX_WIDTH))
        .min_w_0()
        .relative()
        .items_center()
        .gap(z(theme::SPACE_3))
        .pl(z(theme::SPACE_5))
        .pr(z(theme::SPACE_4))
        .border_r_1()
        .border_color(theme::toolbar_button_border())
        .cursor_pointer()
        .group(TAB_GROUP)
        .text_size(z(theme::FONT_SIZE_BODY));

    if active {
        tab = tab
            .bg(theme::bg())
            .text_color(theme::text_main())
            // A 2px accent rail along the top edge — the same mark the
            // Changes / History tab bar already uses for "this one".
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(px(2.0))
                    .bg(theme::accent()),
            );
    } else {
        tab = tab
            .text_color(theme::text_muted())
            .hover(|s| s.bg(theme::hover_bg()).text_color(theme::text_main()));
    }

    tab.child(
        // Shrink-to-fit, never grow: the tab is as wide as its contents up to
        // TAB_MAX_WIDTH, at which point the label truncates. `flex_1` here
        // stretched every tab to the maximum and left the name floating away
        // from its badge.
        div()
            .flex_shrink()
            .min_w_0()
            .overflow_x_hidden()
            .whitespace_nowrap()
            .when(active, |el| el.font_weight(gpui::FontWeight::MEDIUM))
            .child(label.to_string()),
    )
    // A clean repository carries no badge at all; the count is only worth
    // space when there is something in it.
    .when(changed > 0, |el| {
        el.child(
            div()
                .flex_shrink_0()
                .px(z(theme::SPACE_2))
                .py(px(1.0))
                .rounded(z(theme::RADIUS_PILL))
                .bg(if active {
                    theme::selected_bg()
                } else {
                    theme::surface_bg()
                })
                .text_size(z(theme::FONT_SIZE_XS))
                .text_color(theme::text_muted())
                .child(changed.to_string()),
        )
    })
    .child(
        div()
            .id(SharedString::from(format!("repo-tab-close-{slug}")))
            .flex_shrink_0()
            .w(z(16.0))
            .h(z(16.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(z(theme::CORNER_RADIUS_SM))
            .text_color(theme::text_muted())
            .hover(|s| s.bg(theme::surface_bg_alt()).text_color(theme::text_main()))
            // Hidden until the tab is hovered, so a row of tabs is a row of
            // names rather than a row of close buttons. The active tab keeps
            // its own, since that is the one ⌘W acts on.
            .when(!active, |el| {
                el.invisible().group_hover(TAB_GROUP, |s| s.visible())
            })
            .child(Icon::new(IconName::Close).size(z(9.0)))
            .on_click(move |_evt, _win, cx| {
                close_view.update(cx, |app, cx| app.close_tab(index, cx));
            }),
    )
    .on_click(move |_evt, _win, cx| {
        view.update(cx, |app, cx| app.activate_tab(index, cx));
    })
}

fn render_add_button(view: Entity<GitSparkApp>) -> gpui::Stateful<Div> {
    h_flex()
        .id("repo-tab-add")
        .flex_shrink_0()
        .h_full()
        .w(z(34.0))
        .items_center()
        .justify_center()
        .border_l_1()
        .border_color(theme::toolbar_button_border())
        .cursor_pointer()
        .text_color(theme::text_muted())
        .hover(|s| s.bg(theme::hover_bg()).text_color(theme::text_main()))
        .child(Icon::new(IconName::Plus).size(z(13.0)))
        .on_click(move |_evt, window, cx| {
            view.update(cx, |app, cx| app.show_repo_list_for_new_tab(window, cx));
        })
}

/// The strip is only worth its height once there is something to switch
/// between — or something to add to.
pub fn should_render(app: &GitSparkApp) -> bool {
    !app.tabs.is_empty()
}

pub fn render_if_needed(app: &GitSparkApp, view: Entity<GitSparkApp>) -> AnyElement {
    if should_render(app) {
        render_strip(app, view).into_any_element()
    } else {
        // Still take the row's width so the update indicator stays right.
        div().flex_1().h_full().into_any_element()
    }
}
