use gpui::*;
use gpui_component::scroll::ScrollableElement;
use gpui_component::{Icon, IconName, h_flex, v_flex};

use crate::models::{ChangeEntry, CommitInfo};
use crate::ui::app::GitSparkApp;
use crate::ui::changes_context_menu;
use crate::ui::history_context_menu;
use crate::ui::ids::stable_id_slug;
use crate::ui::theme;
use crate::ui::theme::z;
use crate::ui::ui_state::SidebarTab;

// ---------------------------------------------------------------------------
// Row heights (fixed, for uniform_list)
// ---------------------------------------------------------------------------

const CHANGE_ROW_HEIGHT: f32 = 29.0;
// GitHub Desktop's `RowHeight` from app/src/ui/history/commit-list.tsx.
const HISTORY_ROW_HEIGHT: f32 = 50.0;

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

#[allow(dead_code)]
fn accent_selection_bg() -> Hsla {
    theme::with_alpha(theme::accent_muted(), 0.2)
}

// ---------------------------------------------------------------------------
// Status helpers
// ---------------------------------------------------------------------------

/// Render a file status icon matching GitHub Desktop's style.
/// Modified = square with dot (orange), Added = plus-square (green), Deleted = minus-square (red).
///
/// The colour no longer changes with selection. It did when a selected row was
/// an accent fill and the status hue was unreadable on it; on the selection
/// surface the hue is legible, and status is worth more than sameness — the
/// whole point of the icon is which kind of change this is.
fn render_status_icon(status: &str, _selected: bool) -> Div {
    let (icon_path, color): (&str, Hsla) = match status {
        "M" => ("icons/dot-square.svg", theme::warning()),
        "A" => ("icons/dot-square.svg", theme::success()), // reuse dot-square for now
        "D" => ("icons/dot-square.svg", theme::danger()),
        _ => ("icons/dot-square.svg", theme::text_muted()),
    };

    div()
        .flex_shrink_0()
        .child(gpui::svg().path(icon_path).size(z(16.0)).text_color(color))
}

fn status_label(status: &str) -> &'static str {
    if status.contains('?') || status.contains('A') {
        "A"
    } else if status.contains('M') {
        "M"
    } else if status.contains('D') {
        "D"
    } else if status.contains('U') {
        "U"
    } else {
        "?"
    }
}

fn render_stash_row(view: Entity<GitSparkApp>) -> AnyElement {
    h_flex()
        .id("stash-indicator")
        .w_full()
        .h(z(32.0))
        .px(z(10.0))
        .items_center()
        .gap(z(6.0))
        .bg(theme::surface_bg())
        .border_b_1()
        .border_color(theme::border())
        .flex_shrink_0()
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover_bg()))
        .child(
            Icon::new(IconName::Inbox)
                .size(z(14.0))
                .text_color(theme::text_muted()),
        )
        .child(
            div()
                .flex_1()
                .text_size(z(12.0))
                .text_color(theme::text_main())
                .child("Stashed Changes"),
        )
        .child(
            Icon::new(IconName::ChevronRight)
                .size(z(12.0))
                .text_color(theme::text_muted()),
        )
        .on_click(move |_evt, _win, cx| {
            view.update(cx, |app, cx| {
                app.show_restore_stash_dialog(cx);
            });
        })
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Public render entry point (interactive, with click handlers)
// ---------------------------------------------------------------------------

pub fn render_sidebar_interactive(
    app: &GitSparkApp,
    view: Entity<GitSparkApp>,
    cx: &mut Context<GitSparkApp>,
) -> Div {
    let snapshot = app.repo.snapshot.as_ref();
    let empty_changes: Vec<ChangeEntry> = vec![];
    let empty_history: Vec<CommitInfo> = vec![];
    let changes = snapshot
        .map(|s| s.changes.as_slice())
        .unwrap_or(&empty_changes);
    let history = app
        .repo
        .comparison
        .as_ref()
        .map(|comparison| comparison.commits.as_slice())
        .or_else(|| snapshot.map(|s| s.history.as_slice()))
        .unwrap_or(&empty_history);
    let sidebar_tab = app.nav.sidebar_tab;
    let selected_change = app.selection.selected_change.clone();
    let selected_commit = app.selection.selected_commit.clone();
    let change_count = changes.len();

    // Tab bar with click handlers
    let tab_bar = render_interactive_tab_bar(sidebar_tab, change_count, cx);

    // Content — virtualized with uniform_list
    let content: AnyElement = match sidebar_tab {
        SidebarTab::Changes => {
            if changes.is_empty() {
                let stash_row = if app.repo.has_stash {
                    render_stash_row(view.clone())
                } else {
                    div().into_any_element()
                };

                // Empty file list — just the header showing the zero-count state.
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        h_flex()
                            .w_full()
                            .h(z(28.0))
                            .px(z(10.0))
                            .items_center()
                            .gap(z(5.0))
                            .bg(theme::surface_bg())
                            .border_b_1()
                            .border_color(theme::border())
                            .flex_shrink_0()
                            .child(render_checkbox(false))
                            .child(
                                div()
                                    .text_size(z(11.0))
                                    .text_color(theme::text_muted())
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(crate::ui::labels::changed_files(0)),
                            ),
                    )
                    .child(stash_row)
                    .into_any_element()
            } else {
                let file_count = changes.len();
                let included_count = if app.commit.include_all {
                    file_count
                } else {
                    changes
                        .iter()
                        .filter(|c| app.commit.included_files.contains(&c.path))
                        .count()
                };

                // Tri-state: all, none, or mixed
                let check_state = if included_count == file_count {
                    CheckState::On
                } else if included_count == 0 {
                    CheckState::Off
                } else {
                    CheckState::Mixed
                };

                let header_label = if included_count == file_count {
                    crate::ui::labels::changed_files(file_count)
                } else {
                    crate::ui::labels::included_changed_files(included_count, file_count)
                };

                // Include-all header: checkbox + "N of M changed files"
                let include_header = h_flex()
                    .w_full()
                    .h(z(28.0))
                    .px(z(10.0))
                    .items_center()
                    .gap(z(5.0))
                    .bg(theme::surface_bg())
                    .border_b_1()
                    .border_color(theme::border())
                    .flex_shrink_0()
                    .child({
                        let vh = view.clone();
                        render_tristate_checkbox(check_state)
                            .id("include-all-checkbox")
                            .cursor_pointer()
                            .on_click(move |_evt, _win, cx| {
                                vh.update(cx, |app, cx| {
                                    if app.commit.include_all {
                                        // Switch to none
                                        app.commit.include_all = false;
                                        app.commit.included_files.clear();
                                    } else {
                                        // Switch to all
                                        app.commit.include_all = true;
                                        app.commit.included_files.clear();
                                    }
                                    cx.notify();
                                });
                            })
                    })
                    .child(
                        div()
                            .text_size(z(11.0))
                            .text_color(theme::text_muted())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(header_label),
                    );

                let changes_snapshot: Vec<ChangeEntry> = changes.to_vec();
                let sel = selected_change.clone();
                let cap_include_all = app.commit.include_all;
                let cap_included_files: std::collections::HashSet<String> =
                    app.commit.included_files.clone();

                let stash_row = if app.repo.has_stash {
                    render_stash_row(view.clone())
                } else {
                    div().into_any_element()
                };

                v_flex()
                    .flex_1()
                    .min_h_0()
                    .child(include_header)
                    .child(stash_row)
                    .child(
                        div()
                            .id("changes-scroll")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scrollbar()
                            .child(
                                uniform_list("changes-list", changes_snapshot.len(), {
                                    let view = view.clone();
                                    move |range, _win, _cx| {
                                        range
                                            .map(|ix| {
                                                let change = &changes_snapshot[ix];
                                                let is_selected =
                                                    sel.as_deref() == Some(change.path.as_str());
                                                let is_included = cap_include_all
                                                    || cap_included_files.contains(&change.path);
                                                let path = change.path.clone();
                                                let checkbox_path = change.path.clone();
                                                let checkbox_view = view.clone();
                                                let click_view = view.clone();
                                                let ctx_path = change.path.clone();
                                                changes_context_menu::bind_changes_context_click(
                                                    render_change_row(
                                                        change,
                                                        is_selected,
                                                        is_included,
                                                        checkbox_view,
                                                        checkbox_path,
                                                    )
                                                    .id(SharedString::from(format!(
                                                        "change-{}",
                                                        change.path
                                                    )))
                                                    .cursor_pointer()
                                                    .hover(|s| {
                                                        s.bg(if is_selected {
                                                            theme::selected_bg()
                                                        } else {
                                                            theme::list_hover_bg()
                                                        })
                                                    })
                                                    .on_click(move |_evt, _win, cx| {
                                                        let path = path.clone();
                                                        click_view.update(cx, |app, cx| {
                                                            if app
                                                                .selection
                                                                .selected_change
                                                                .as_deref()
                                                                != Some(path.as_str())
                                                            {
                                                                app.selection
                                                                    .selected_diff_lines
                                                                    .clear();
                                                            }
                                                            app.selection.selected_change =
                                                                Some(path.clone());
                                                            app.refresh_file_diff(path);
                                                            cx.notify();
                                                        });
                                                    }),
                                                    view.clone(),
                                                    ctx_path,
                                                )
                                                .into_any_element()
                                            })
                                            .collect()
                                    }
                                })
                                .flex_1()
                                .with_sizing_behavior(ListSizingBehavior::Infer),
                            )
                            .into_any_element(),
                    )
                    .into_any_element()
            }
        }
        SidebarTab::History => {
            if history.is_empty() {
                div()
                    .flex_1()
                    .child(render_empty_state("No history"))
                    .into_any_element()
            } else {
                let history_snapshot: Vec<CommitInfo> = history.to_vec();
                let sel = selected_commit.clone();
                // History during a branch comparison shows the OTHER branch's
                // commits, not this one's — "ahead of origin" doesn't apply to
                // them, so the arrow is real-history only. The list is newest
                // first, same order `ahead` counts from, so the first N rows
                // are exactly the commits git hasn't pushed yet.
                let ahead_count = if app.repo.comparison.is_none() {
                    snapshot.map(|s| s.repo.ahead).unwrap_or(0)
                } else {
                    0
                };
                div()
                    .id("history-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .child(
                        uniform_list("history-list", history_snapshot.len(), {
                            let view = view.clone();
                            move |range, _win, _cx| {
                                range
                                    .map(|ix| {
                                        let commit = &history_snapshot[ix];
                                        let is_selected =
                                            sel.as_deref() == Some(commit.oid.as_str());
                                        let is_unpushed = ix < ahead_count;
                                        let oid = commit.oid.clone();
                                        let click_view = view.clone();
                                        history_context_menu::bind_history_context_click(
                                            render_history_row(commit, is_selected, is_unpushed)
                                                .id(SharedString::from(format!(
                                                    "commit-{}",
                                                    commit.oid
                                                )))
                                                .cursor_pointer()
                                                .hover(move |s| {
                                                    s.bg(if is_selected {
                                                        theme::selected_bg()
                                                    } else {
                                                        theme::list_hover_bg()
                                                    })
                                                })
                                                .on_click(move |_evt, _win, cx| {
                                                    let oid = oid.clone();
                                                    click_view.update(cx, |app, cx| {
                                                        app.select_commit(oid, cx);
                                                    });
                                                }),
                                            view.clone(),
                                            commit.oid.clone(),
                                        )
                                        .into_any_element()
                                    })
                                    .collect()
                            }
                        })
                        .flex_1()
                        .with_sizing_behavior(ListSizingBehavior::Infer),
                    )
                    .into_any_element()
            }
        }
    };

    v_flex()
        .size_full()
        .bg(theme::panel_bg())
        .border_r_1()
        .border_color(theme::border())
        .child(tab_bar)
        .child(content)
}

fn render_interactive_tab_bar(
    active_tab: SidebarTab,
    change_count: usize,
    cx: &mut Context<GitSparkApp>,
) -> Div {
    let is_changes = active_tab == SidebarTab::Changes;

    // Changes tab content
    let mut changes_content = h_flex().items_center().justify_center().gap(z(4.0)).child(
        div()
            .text_size(z(theme::FONT_SIZE))
            .text_color(if is_changes {
                theme::text_main()
            } else {
                theme::text_muted()
            })
            .font_weight(FontWeight::SEMIBOLD)
            .child("Changes"),
    );

    if change_count > 0 {
        changes_content = changes_content.child(
            div()
                .px(z(6.0))
                .py(z(1.0))
                .rounded(z(10.0))
                .bg(theme::toolbar_badge_bg())
                .text_size(z(theme::FONT_SIZE_XS))
                .text_color(theme::text_main())
                .child(if change_count > 300 {
                    "300+".to_string()
                } else {
                    change_count.to_string()
                }),
        );
    }

    let changes_tab = h_flex()
        .id("tab-changes")
        .flex_1()
        .h(z(34.0))
        .items_center()
        .justify_center()
        .cursor_pointer()
        .border_b_2()
        .border_color(if is_changes {
            theme::accent()
        } else {
            gpui::transparent_black()
        })
        .hover(|s| s.bg(theme::hover_bg()))
        .on_click(cx.listener(|app, _evt, _win, cx| {
            app.nav.sidebar_tab = SidebarTab::Changes;
            app.repo.comparison = None;
            cx.notify();
        }))
        .child(changes_content);

    let history_tab = h_flex()
        .id("tab-history")
        .flex_1()
        .h(z(34.0))
        .items_center()
        .justify_center()
        .cursor_pointer()
        .border_b_2()
        .border_color(if !is_changes {
            theme::accent()
        } else {
            gpui::transparent_black()
        })
        .hover(|s| s.bg(theme::hover_bg()))
        .on_click(cx.listener(|app, _evt, _win, cx| {
            app.nav.sidebar_tab = SidebarTab::History;
            cx.notify();
        }))
        .child(
            div()
                .text_size(z(theme::FONT_SIZE))
                .text_color(if !is_changes {
                    theme::text_main()
                } else {
                    theme::text_muted()
                })
                .font_weight(FontWeight::SEMIBOLD)
                .child("History"),
        );

    h_flex()
        .w_full()
        .flex_shrink_0()
        .border_b_1()
        .border_color(theme::border())
        .child(changes_tab)
        .child(history_tab)
}

// ---------------------------------------------------------------------------
// Changes list
// ---------------------------------------------------------------------------

pub fn render_change_row(
    change: &ChangeEntry,
    selected: bool,
    included: bool,
    checkbox_view: Entity<GitSparkApp>,
    checkbox_path: String,
) -> Div {
    let bg = if selected {
        theme::selected_bg()
    } else {
        gpui::transparent_black()
    };

    let status_kind = status_label(&change.status);

    // Excluded files stay muted even when selected — that distinction carries
    // real meaning for the next commit, and the selection surface underneath
    // leaves it perfectly readable.
    let text_color = if included {
        theme::text_main()
    } else {
        theme::text_muted()
    };

    // Interactive checkbox
    let checkbox_id = stable_id_slug(&change.path);
    let checkbox = render_checkbox(included)
        .id(SharedString::from(format!("chk-{checkbox_id}")))
        .cursor_pointer()
        .on_click(move |_evt, _win, cx| {
            let path = checkbox_path.clone();
            checkbox_view.update(cx, |app, cx| {
                if app.commit.include_all {
                    // Switching from "all" to individual: include all except this one
                    app.commit.include_all = false;
                    if let Some(snapshot) = &app.repo.snapshot {
                        for c in &snapshot.changes {
                            if c.path != path {
                                app.commit.included_files.insert(c.path.clone());
                            }
                        }
                    }
                } else if app.commit.included_files.contains(&path) {
                    app.commit.included_files.remove(&path);
                } else {
                    app.commit.included_files.insert(path);
                    // Check if all are now included
                    if let Some(snapshot) = &app.repo.snapshot {
                        if app.commit.included_files.len() == snapshot.changes.len() {
                            app.commit.include_all = true;
                            app.commit.included_files.clear();
                        }
                    }
                }
                cx.notify();
            });
        });

    let row = h_flex()
        .w_full()
        .h(z(CHANGE_ROW_HEIGHT))
        .px(z(10.0))
        .items_center()
        .bg(bg);

    // The border is kept so the 2px it occupies comes out of the padding and
    // the label does not jump sideways on selection, but it matches the fill:
    // the spec's selected row is a plain surface with no accent edge.
    let row = if selected {
        row.border_l_2()
            .border_color(theme::selected_bg())
            .pl(z(8.0))
    } else {
        row
    };

    row.gap(z(5.0))
        .child(checkbox)
        .child(
            div().flex_1().overflow_x_hidden().child(
                div()
                    .text_size(z(12.0))
                    .text_color(text_color)
                    .whitespace_nowrap()
                    .child(change.path.clone()),
            ),
        )
        .child(render_status_icon(status_kind, selected))
}

fn render_checkbox(checked: bool) -> Div {
    let size = 14.0;
    if checked {
        h_flex()
            .w(z(size))
            .h(z(size))
            .rounded(z(3.0))
            .bg(theme::checkbox_selected_bg())
            .border_1()
            .border_color(theme::checkbox_selected_bg())
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .child(
                Icon::new(IconName::Check)
                    .size(z(10.0))
                    .text_color(theme::checkbox_selected_fg()),
            )
    } else {
        div()
            .w(z(size))
            .h(z(size))
            .rounded(z(3.0))
            .border_1()
            .border_color(theme::text_muted())
            .flex_shrink_0()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum CheckState {
    On,
    Off,
    Mixed,
}

fn render_tristate_checkbox(state: CheckState) -> Div {
    let size = 14.0;
    let check_bg = theme::checkbox_selected_bg();
    let check_fg = theme::checkbox_selected_fg();
    match state {
        CheckState::On => h_flex()
            .w(z(size))
            .h(z(size))
            .rounded(z(3.0))
            .bg(check_bg)
            .border_1()
            .border_color(check_bg)
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .child(
                Icon::new(IconName::Check)
                    .size(z(10.0))
                    .text_color(check_fg),
            ),
        CheckState::Mixed => h_flex()
            .w(z(size))
            .h(z(size))
            .rounded(z(3.0))
            .bg(check_bg)
            .border_1()
            .border_color(check_bg)
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .child(
                Icon::new(IconName::Minus)
                    .size(z(10.0))
                    .text_color(check_fg),
            ),
        CheckState::Off => div()
            .w(z(size))
            .h(z(size))
            .rounded(z(3.0))
            .border_1()
            .border_color(theme::text_muted())
            .flex_shrink_0(),
    }
}

pub fn render_no_changes_state(
    view: &Entity<GitSparkApp>,
    ahead: usize,
    behind: usize,
    remote: Option<&str>,
    has_github_remote: bool,
    _cx: &mut Context<GitSparkApp>,
) -> Div {
    let vh_publish = view.clone();
    let vh_push = view.clone();
    let vh_editor = view.clone();
    let vh_finder = view.clone();
    let vh_github = view.clone();

    let mut cards = v_flex().w_full().gap(z(8.0));

    // --- Card 1: Pull or push commits (highlighted blue when a sync action is available) ---
    if let Some(remote_name) = remote {
        let sync_action = if behind > 0 {
            Some((
                "card-pull",
                "card-pull-btn",
                crate::ui::domain_state::NetworkAction::Pull,
                "Pull",
                behind,
                format!(
                    "Pull {} from the {remote_name} remote",
                    if behind == 1 {
                        "1 commit".to_string()
                    } else {
                        format!("{behind} commits")
                    }
                ),
                format!(
                    "There {} {} remote {} on {remote_name} that {} not exist on your machine.",
                    if behind == 1 { "is" } else { "are" },
                    behind,
                    if behind == 1 { "commit" } else { "commits" },
                    if behind == 1 { "does" } else { "do" }
                ),
            ))
        } else if ahead > 0 {
            Some((
                "card-push",
                "card-push-btn",
                crate::ui::domain_state::NetworkAction::Push,
                "Push",
                ahead,
                format!(
                    "Push {} to the {remote_name} remote",
                    if ahead == 1 {
                        "1 commit".to_string()
                    } else {
                        format!("{ahead} commits")
                    }
                ),
                format!(
                    "You have {} local {} waiting to be pushed to GitHub.",
                    ahead,
                    if ahead == 1 { "commit" } else { "commits" }
                ),
            ))
        } else {
            None
        };

        if let Some((card_id, button_id, action, verb, count, title, subtitle)) = sync_action {
            let button_label = format!("{verb} {remote_name}");
            let helper = format!(
                "Always available in the toolbar when there {} {} {} to {} or",
                if count == 1 { "is" } else { "are" },
                count,
                if count == 1 { "commit" } else { "commits" },
                verb.to_ascii_lowercase()
            );

            cards = cards.child(
                h_flex()
                    .id(card_id)
                    .w_full()
                    .p(z(16.0))
                    .gap(z(12.0))
                    .items_start()
                    .rounded(z(theme::CORNER_RADIUS))
                    .bg(theme::push_card_bg())
                    .border_1()
                    .border_color(theme::push_card_border())
                    // Left: text content
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(z(4.0))
                            .child(
                                div()
                                    .text_size(z(14.0))
                                    .text_color(theme::text_main())
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_size(z(13.0))
                                    .text_color(theme::text_main())
                                    .child(subtitle),
                            )
                            .child({
                                let mut row =
                                    h_flex().gap(z(4.0)).items_center().flex_wrap().child(
                                        div()
                                            .text_size(z(13.0))
                                            .text_color(theme::push_card_text())
                                            .child(helper),
                                    );
                                row = row.child(kbd_badge("\u{2318}"));
                                row = row.child(kbd_badge("P"));
                                row
                            }),
                    )
                    // Right: action button
                    .child(
                        div().flex_shrink_0().child(
                            div()
                                .id(button_id)
                                .px(z(12.0))
                                .py(z(2.0))
                                .rounded(z(theme::CORNER_RADIUS))
                                .bg(theme::commit_button_bg())
                                .text_size(z(13.0))
                                .text_color(theme::on_accent())
                                .font_weight(FontWeight::SEMIBOLD)
                                .cursor_pointer()
                                .hover(|s| s.bg(theme::commit_button_hover_bg()))
                                .child(button_label)
                                .on_click(move |_evt, _win, cx| {
                                    vh_push.update(cx, |app, cx| {
                                        app.handle_toolbar_action(
                                            crate::ui::app::ToolbarAction::RunNetworkAction(action),
                                            cx,
                                        );
                                    });
                                }),
                        ),
                    ),
            );
        }
    }

    if remote.is_none() {
        cards = cards.child(suggestion_card(
            "no-changes-publish",
            "Publish your repository to GitHub",
            "Always available in the toolbar for local repositories or",
            &["\u{2318}", "P"],
            "Publish repository",
            move |_evt, _win, cx| {
                vh_publish.update(cx, |app, cx| {
                    app.handle_toolbar_action(
                        crate::ui::app::ToolbarAction::RunNetworkAction(
                            crate::ui::domain_state::NetworkAction::PublishRepository,
                        ),
                        cx,
                    );
                });
            },
        ));
    }

    // --- Card 2: Open in External Editor ---
    cards = cards.child(suggestion_card(
        "no-changes-editor",
        "Open the repository in your external editor",
        "Repository menu or",
        &["\u{2318}", "\u{21E7}", "A"],
        "Open in External Editor",
        move |_evt, _win, cx| {
            vh_editor.update(cx, |app, cx| {
                app.menu_open_external_editor(cx);
            });
        },
    ));

    // --- Card 3: Show in Finder ---
    cards = cards.child(suggestion_card(
        "no-changes-finder",
        "View the files of your repository in Finder",
        "Repository menu or",
        &["\u{2318}", "\u{21E7}", "F"],
        "Show in Finder",
        move |_evt, _win, cx| {
            vh_finder.update(cx, |app, cx| {
                app.menu_show_in_finder(cx);
            });
        },
    ));

    // --- Card 4: View on GitHub ---
    if has_github_remote {
        cards = cards.child(suggestion_card(
            "no-changes-github",
            "Open the repository page on GitHub in your browser",
            "Repository menu or",
            &["\u{2318}", "\u{21E7}", "G"],
            "View on GitHub",
            move |_evt, _win, cx| {
                vh_github.update(cx, |app, cx| {
                    app.menu_view_on_github(cx);
                });
            },
        ));
    }

    // Outer wrapper: full-size, scrollable, cards centered at max 600px
    div().size_full().child(
        div()
            .id("no-changes-scroll")
            .size_full()
            .min_h_0()
            .overflow_y_scrollbar()
            .child(
                h_flex()
                    .w_full()
                    .justify_center()
                    .child(
                        v_flex()
                            .w(px(600.0))
                            .p(z(20.0))
                            .gap(z(16.0))
                            // Header
                            .child(
                                v_flex()
                                    .gap(z(6.0))
                                    .child(
                                        div()
                                            .text_size(z(28.0))
                                            .text_color(theme::text_main())
                                            .font_weight(FontWeight::BOLD)
                                            .child("No local changes"),
                                    )
                                    .child(
                                        div()
                                            .text_size(z(14.0))
                                            .text_color(theme::text_muted())
                                            .child("There are no uncommitted changes in this repository. Here are some friendly suggestions for what to do next."),
                                    ),
                            )
                            // Cards
                            .child(cards),
                    ),
            ),
    )
}

pub fn render_no_repository_state(
    view: &Entity<GitSparkApp>,
    _cx: &mut Context<GitSparkApp>,
) -> Div {
    let vh_choose = view.clone();
    let vh_add = view.clone();

    div().size_full().child(
        div()
            .id("no-repository-state")
            .size_full()
            .min_h_0()
            .overflow_y_scrollbar()
            .child(
                h_flex()
                    .w_full()
                    .justify_center()
                    .child(
                        v_flex()
                            .w(px(560.0))
                            .p(z(20.0))
                            .gap(z(16.0))
                            .child(
                                v_flex()
                                    .gap(z(6.0))
                                    .child(
                                        div()
                                            .text_size(z(28.0))
                                            .text_color(theme::text_main())
                                            .font_weight(FontWeight::BOLD)
                                            .child("No repository selected"),
                                    )
                                    .child(
                                        div()
                                            .text_size(z(14.0))
                                            .text_color(theme::text_muted())
                                            .child("Choose a recent repository or add a local repository to get started."),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .gap(z(10.0))
                                    .child(
                                        div()
                                            .id("no-repository-choose")
                                            .px(z(12.0))
                                            .py(z(7.0))
                                            .rounded(z(theme::CORNER_RADIUS))
                                            .bg(theme::commit_button_bg())
                                            .text_color(theme::commit_button_text())
                                            .cursor_pointer()
                                            .hover(|s| s.bg(theme::commit_button_hover_bg()))
                                            .child(
                                                div()
                                                    .text_size(z(13.0))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child("Show Repository List"),
                                            )
                                            .on_click(move |_evt, _win, cx| {
                                                vh_choose.update(cx, |app, cx| {
                                                    app.menu_show_repository_list(cx);
                                                });
                                            }),
                                    )
                                    .child(
                                        div()
                                            .id("no-repository-add-local")
                                            .px(z(12.0))
                                            .py(z(7.0))
                                            .rounded(z(theme::CORNER_RADIUS))
                                            .border_1()
                                            .border_color(theme::surface_bg_alt())
                                            .bg(theme::surface_bg())
                                            .text_color(theme::text_main())
                                            .cursor_pointer()
                                            .hover(|s| s.bg(theme::toolbar_hover_bg()))
                                            .child(
                                                div()
                                                    .text_size(z(13.0))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child("Add Local Repository…"),
                                            )
                                            .on_click(move |_evt, _win, cx| {
                                                vh_add.update(cx, |app, cx| {
                                                    app.menu_open_repository(cx);
                                                });
                                            }),
                                    ),
                            ),
                    ),
            ),
    )
}

// ---------------------------------------------------------------------------
// Suggestion card — bordered card with title, subtitle, shortcut, action btn
// ---------------------------------------------------------------------------

fn suggestion_card(
    id: &str,
    title: &str,
    shortcut_prefix: &str,
    keys: &[&str],
    button_label: &str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let mut shortcut_row = h_flex().gap(z(4.0)).items_center().child(
        div()
            .text_size(z(13.0))
            .text_color(theme::text_muted())
            .child(shortcut_prefix.to_string()),
    );
    for key in keys {
        shortcut_row = shortcut_row.child(kbd_badge(key));
    }

    h_flex()
        .w_full()
        .p(z(16.0))
        .gap(z(12.0))
        .items_center()
        .rounded(z(theme::CORNER_RADIUS))
        .border_1()
        .border_color(theme::border())
        // Left: text content
        .child(
            v_flex()
                .flex_1()
                .gap(z(4.0))
                .child(
                    div()
                        .text_size(z(14.0))
                        .text_color(theme::text_main())
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.to_string()),
                )
                .child(shortcut_row),
        )
        // Right: action button
        .child(
            div().flex_shrink_0().child(
                div()
                    .id(SharedString::from(id.to_string()))
                    .px(z(12.0))
                    .py(z(2.0))
                    .rounded(z(theme::CORNER_RADIUS))
                    .bg(theme::surface_bg())
                    .border_1()
                    .border_color(theme::border())
                    .text_size(z(13.0))
                    .text_color(theme::text_main())
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::hover_bg()))
                    .child(button_label.to_string())
                    .on_click(on_click),
            ),
        )
}

fn kbd_badge(key: &str) -> Div {
    div()
        .px(z(4.0))
        .py(z(1.0))
        .rounded(z(3.0))
        .bg(theme::surface_bg())
        .border_1()
        .border_color(theme::border())
        .text_size(z(10.0))
        .text_color(theme::text_muted())
        .child(key.to_string())
}

// ---------------------------------------------------------------------------
// History list
// ---------------------------------------------------------------------------

/// One row of the history list, following GitHub Desktop's `.commit`
/// (`app/styles/ui/history/_commit-list.scss`) exactly.
///
/// The metrics are theirs: a 50px row (`RowHeight` in `commit-list.tsx`),
/// 10px of padding on the left and 15px on the right — the extra half-step
/// makes room for the scrollbar — a `--box-border-color` rule underneath, a
/// semibold summary, and a description line 3px below it. The `.info` block
/// carries a -4px top margin, which is what optically centres two lines of
/// different weights inside a 50px row.
pub fn render_history_row(commit: &CommitInfo, selected: bool, is_unpushed: bool) -> Div {
    // GitHub Desktop shows the blue only while the list has focus and a
    // neutral grey otherwise. This app does not track list focus, and the
    // blue is the state a user actually sees while working in the history,
    // so it is the one modelled here.
    let bg = if selected {
        theme::list_selected_active_bg()
    } else {
        gpui::transparent_black()
    };

    let (summary_color, meta_color) = if selected {
        (
            theme::list_selected_active_fg(),
            theme::list_selected_active_fg(),
        )
    } else {
        (theme::text_main(), theme::text_muted())
    };

    let meta = format!("{} \u{00b7} {}", commit.author_name, commit.date);

    // `.info` — the summary and description column.
    let info = v_flex()
        .flex_1()
        .min_w(z(50.0))
        .overflow_hidden()
        // .info { margin-top: -4px }
        .mt(z(-4.0))
        .child(
            div()
                .w_full()
                .text_size(z(theme::FONT_SIZE_BODY))
                .text_color(summary_color)
                .font_weight(FontWeight::SEMIBOLD)
                .whitespace_nowrap()
                .overflow_x_hidden()
                .child(commit.summary.clone()),
        )
        .child(
            // .description { display: flex; margin-top: 3px }
            h_flex()
                .w_full()
                .mt(z(3.0))
                .gap(z(5.0))
                .items_center()
                .overflow_hidden()
                .child(render_commit_avatar(&commit.author_name, selected))
                .child(
                    div()
                        .flex_1()
                        .text_size(z(theme::FONT_SIZE_BODY))
                        .text_color(meta_color)
                        .whitespace_nowrap()
                        .overflow_x_hidden()
                        .child(meta),
                ),
        );

    // `.commit-indicators` — tags and the HEAD marker, pinned right and
    // capped at half the row so a long tag cannot crowd out the summary.
    let mut indicators = h_flex()
        .flex_shrink_0()
        .ml(z(10.0))
        .h(z(16.0))
        .items_center()
        .justify_end()
        .gap(z(5.0));

    // Not yet on the remote. No badge shape — just the arrow, muted so it
    // doesn't compete with tags or HEAD, which are actual labels rather than
    // a transient state that clears itself the moment the commit is pushed.
    if is_unpushed {
        let arrow_color = if selected {
            theme::list_selected_active_fg()
        } else {
            theme::text_muted()
        };
        indicators = indicators.child(
            Icon::new(IconName::ArrowUp)
                .size(z(12.0))
                .text_color(arrow_color),
        );
    }

    for tag in &commit.tags {
        indicators = indicators.child(render_commit_badge(
            SharedString::from(tag.clone()),
            selected,
        ));
    }
    if commit.is_head {
        indicators = indicators.child(render_commit_badge(SharedString::from("HEAD"), selected));
    }

    h_flex()
        .w_full()
        .h(z(HISTORY_ROW_HEIGHT))
        .items_center()
        .pl(z(10.0))
        // padding-right: calc(var(--spacing) + var(--spacing-half))
        .pr(z(15.0))
        .bg(bg)
        .border_b_1()
        .border_color(theme::list_row_border())
        .child(info)
        .child(indicators)
}

/// The author bubble in a commit's description line.
///
/// GitHub Desktop renders a real Gravatar here through `AvatarStack`. Fetching
/// avatars would mean a network request per author, so this is an initial on a
/// disc at the same 16px `AvatarStack--small` size — the layout GitHub Desktop
/// has, without the network.
fn render_commit_avatar(author: &str, selected: bool) -> Div {
    let initial = author
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());

    let (bg, fg) = if selected {
        (
            theme::list_selected_badge_bg(),
            theme::list_selected_badge_fg(),
        )
    } else {
        (theme::accent(), theme::on_accent())
    };

    div()
        .flex_shrink_0()
        .w(z(16.0))
        .h(z(16.0))
        .rounded(z(999.0))
        .bg(bg)
        .flex()
        .items_center()
        .justify_center()
        .text_size(z(theme::FONT_SIZE_XS))
        .text_color(fg)
        .font_weight(FontWeight::SEMIBOLD)
        .child(initial)
}

/// A tag or HEAD pill — GitHub Desktop's `.tag-name`.
///
/// `padding: 0 var(--spacing-half)`, `border-radius: var(--border-radius)`,
/// on `--list-item-badge-background-color`. On a selected row the badge has to
/// sit on the blue fill, so it swaps to the light `selected` pair.
fn render_commit_badge(label: SharedString, selected: bool) -> Div {
    let (bg, fg) = if selected {
        (
            theme::list_selected_badge_bg(),
            theme::list_selected_badge_fg(),
        )
    } else {
        (theme::list_badge_bg(), theme::text_main())
    };

    div()
        .flex_shrink_0()
        .h(z(16.0))
        .px(z(5.0))
        .rounded(z(theme::CORNER_RADIUS))
        .bg(bg)
        .flex()
        .items_center()
        .text_size(z(theme::FONT_SIZE_SM))
        .text_color(fg)
        .whitespace_nowrap()
        .child(label)
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn render_empty_state(message: &str) -> Div {
    div()
        .w_full()
        .py(z(20.0))
        .items_center()
        .justify_center()
        .child(
            div()
                .text_size(z(12.0))
                .text_color(theme::text_muted())
                .child(message.to_string()),
        )
}
