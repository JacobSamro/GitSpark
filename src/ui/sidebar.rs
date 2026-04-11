use gpui::*;
use gpui_component::scroll::ScrollableElement;
use gpui_component::tag::Tag;
use gpui_component::{Icon, IconName, Sizable, h_flex, v_flex};

use crate::models::{ChangeEntry, CommitInfo};
use crate::ui::app::GitSparkApp;
use crate::ui::changes_context_menu;
use crate::ui::history_context_menu;
use crate::ui::theme;
use crate::ui::theme::z;
use crate::ui::ui_state::SidebarTab;

// ---------------------------------------------------------------------------
// Row heights (fixed, for uniform_list)
// ---------------------------------------------------------------------------

const CHANGE_ROW_HEIGHT: f32 = 29.0;
const HISTORY_ROW_HEIGHT: f32 = 40.0; // summary + meta + padding

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

fn accent_selection_bg() -> Hsla {
    theme::with_alpha(theme::accent_muted(), 0.2)
}

// ---------------------------------------------------------------------------
// Status helpers
// ---------------------------------------------------------------------------

fn status_tag(label: &str) -> Tag {
    let tag = match label {
        "A" => Tag::success(),
        "M" => Tag::warning(),
        "D" => Tag::danger(),
        _ => Tag::secondary(),
    };
    tag.outline().xsmall().child(label.to_string())
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
    let history = snapshot
        .map(|s| s.history.as_slice())
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
                // No changes empty state with suggestion cards
                let ahead = snapshot.map(|s| s.repo.ahead).unwrap_or(0);
                let remote = snapshot.and_then(|s| s.repo.remote_name.as_deref());
                render_no_changes_state(&view, ahead, remote, cx).into_any_element()
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
                    format!("{file_count} changed files")
                } else {
                    format!("{included_count} of {file_count} changed files")
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

                // Stash indicator
                let mut stash_row: AnyElement = div().into_any_element();
                if app.repo.has_stash {
                    let stash_view = view.clone();
                    stash_row = h_flex()
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
                            stash_view.update(cx, |app, cx| {
                                app.messages.status_message = "Restoring stash...".to_string();
                                if let Some(path) = app.repo_path().map(std::path::PathBuf::from) {
                                    let tx = app.event_tx.clone();
                                    let git = crate::git::GitClient::new();
                                    std::thread::spawn(move || {
                                        let res = git.stash_pop(&path).map_err(|e| e.to_string());
                                        tx.send(crate::ui::app::AppEvent::NetworkActionCompleted(
                                            res,
                                            "Restored stash".to_string(),
                                        ));
                                    });
                                }
                                cx.notify();
                            });
                        })
                        .into_any_element();
                }

                v_flex().flex_1().min_h_0().child(include_header).child(stash_row).child(
                div().id("changes-scroll").flex_1().min_h_0().overflow_y_scrollbar().child(
                    uniform_list("changes-list", changes_snapshot.len(), {
                        let view = view.clone();
                        move |range, _win, _cx| {
                            range
                                .map(|ix| {
                                    let change = &changes_snapshot[ix];
                                    let is_selected = sel.as_deref() == Some(change.path.as_str());
                                    let is_included = cap_include_all
                                        || cap_included_files.contains(&change.path);
                                    let path = change.path.clone();
                                    let checkbox_path = change.path.clone();
                                    let checkbox_view = view.clone();
                                    let click_view = view.clone();
                                    let ctx_path = change.path.clone();
                                    changes_context_menu::bind_changes_context_click(
                                        render_change_row(change, is_selected, is_included, checkbox_view, checkbox_path)
                                            .id(SharedString::from(format!("change-{}", change.path)))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(theme::hover_bg()))
                                            .on_click(move |_evt, _win, cx| {
                                                let path = path.clone();
                                                click_view.update(cx, |app, cx| {
                                                    app.selection.selected_change = Some(path);
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
                ).into_any_element()
                ).into_any_element()
            }
        }
        SidebarTab::History => {
            if history.is_empty() {
                div().flex_1().child(render_empty_state("No history")).into_any_element()
            } else {
                let history_snapshot: Vec<CommitInfo> = history.to_vec();
                let sel = selected_commit.clone();
                div().id("history-scroll").flex_1().min_h_0().overflow_y_scrollbar().child(
                    uniform_list("history-list", history_snapshot.len(), {
                        let view = view.clone();
                        move |range, _win, _cx| {
                            range
                                .map(|ix| {
                                    let commit = &history_snapshot[ix];
                                    let is_selected = sel.as_deref() == Some(commit.oid.as_str());
                                    let oid = commit.oid.clone();
                                    let click_view = view.clone();
                                    history_context_menu::bind_history_context_click(
                                        render_history_row(commit, is_selected)
                                        .id(SharedString::from(format!("commit-{}", commit.oid)))
                                        .cursor_pointer()
                                        .hover(|s| s.bg(theme::hover_bg()))
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
                ).into_any_element()
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
                .child(if change_count > 300 { "300+".to_string() } else { change_count.to_string() }),
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
        theme::hover_bg()
    } else {
        gpui::transparent_black()
    };

    let badge_label = status_label(&change.status);

    let text_color = if selected {
        gpui::white().into()
    } else if !included {
        theme::text_muted() // dim excluded files
    } else {
        theme::text_main()
    };

    // Interactive checkbox
    let checkbox = render_checkbox(included)
        .id(SharedString::from(format!("chk-{}", change.path)))
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

    h_flex()
        .w_full()
        .h(z(CHANGE_ROW_HEIGHT))
        .px(z(10.0))
        .items_center()
        .bg(bg)
        .border_l_2()
        .border_color(if selected {
            theme::accent()
        } else {
            gpui::transparent_black()
        })
        .gap(z(5.0))
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
        .child(status_tag(badge_label))
}

fn render_checkbox(checked: bool) -> Div {
    let size = 14.0;
    if checked {
        div()
            .w(z(size))
            .h(z(size))
            .rounded(z(3.0))
            .bg(theme::accent())
            .border_1()
            .border_color(theme::accent())
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .child(
                Icon::new(IconName::Check)
                    .size(z(10.0))
                    .text_color(gpui::white()),
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
    match state {
        CheckState::On => div()
            .w(z(size))
            .h(z(size))
            .rounded(z(3.0))
            .bg(theme::accent())
            .border_1()
            .border_color(theme::accent())
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .child(
                Icon::new(IconName::Check)
                    .size(z(10.0))
                    .text_color(gpui::white()),
            ),
        CheckState::Mixed => div()
            .w(z(size))
            .h(z(size))
            .rounded(z(3.0))
            .bg(theme::accent())
            .border_1()
            .border_color(theme::accent())
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .child(
                Icon::new(IconName::Minus)
                    .size(z(10.0))
                    .text_color(gpui::white()),
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

fn render_no_changes_state(
    view: &Entity<GitSparkApp>,
    ahead: usize,
    remote: Option<&str>,
    cx: &mut Context<GitSparkApp>,
) -> Div {
    let vh_push = view.clone();
    let vh_editor = view.clone();
    let vh_finder = view.clone();
    let vh_github = view.clone();

    let mut cards = v_flex().w_full().gap(z(8.0));

    // --- Card 1: Push commits (highlighted blue, only when commits to push) ---
    if ahead > 0 && remote.is_some() {
        let push_title = format!(
            "Push {} to the origin remote",
            if ahead == 1 { "1 commit".to_string() } else { format!("{ahead} commits") }
        );
        let push_subtitle = format!(
            "You have {} local {} waiting to be pushed to GitHub.",
            ahead,
            if ahead == 1 { "commit" } else { "commits" }
        );

        cards = cards.child(
            v_flex()
                .id("card-push")
                .w_full()
                .p(z(12.0))
                .gap(z(6.0))
                .rounded(z(theme::CORNER_RADIUS))
                .bg(theme::push_card_bg())
                .border_1()
                .border_color(theme::push_card_border())
                // Title
                .child(
                    div()
                        .text_size(z(12.0))
                        .text_color(theme::text_main())
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(push_title),
                )
                // Subtitle
                .child(
                    div()
                        .text_size(z(11.0))
                        .text_color(theme::push_card_text())
                        .child(push_subtitle),
                )
                // Shortcut hint
                .child(
                    h_flex()
                        .gap(z(4.0))
                        .items_center()
                        .child(
                            div()
                                .text_size(z(11.0))
                                .text_color(theme::push_card_text())
                                .child("Always available in the toolbar or"),
                        )
                        .child(kbd_badge("\u{2318}"))
                        .child(kbd_badge("P")),
                )
                // Action button
                .child(
                    h_flex().justify_end().child(
                        div()
                            .id("push-btn")
                            .px(z(12.0))
                            .py(z(4.0))
                            .rounded(z(theme::CORNER_RADIUS))
                            .bg(theme::commit_button_bg())
                            .text_size(z(12.0))
                            .text_color(gpui::white())
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::commit_button_hover_bg()))
                            .child("Push origin")
                            .on_click(move |_evt, _win, cx| {
                                vh_push.update(cx, |app, cx| {
                                    app.handle_toolbar_action(
                                        crate::ui::app::ToolbarAction::RunNetworkAction(
                                            crate::ui::domain_state::NetworkAction::Push,
                                        ),
                                        cx,
                                    );
                                });
                            }),
                    ),
                ),
        );
    }

    // --- Card 2: Open in External Editor ---
    cards = cards.child(suggestion_card(
        "no-changes-editor",
        "Open the repository in your external editor",
        "Repository menu or",
        &["\u{2318}", "\u{21E7}", "A"],
        "Open in External Editor",
        move |_evt, _win, cx| {
            vh_editor.update(cx, |app, _cx| {
                if let Some(path) = app.repo_path() {
                    let _ = open::that_detached(path);
                }
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
            vh_finder.update(cx, |app, _cx| {
                if let Some(path) = app.repo_path() {
                    let _ = open::that_detached(path);
                }
            });
        },
    ));

    // --- Card 4: View on GitHub ---
    cards = cards.child(suggestion_card(
        "no-changes-github",
        "Open the repository page on GitHub in your browser",
        "Repository menu or",
        &["\u{2318}", "\u{21E7}", "G"],
        "View on GitHub",
        move |_evt, _win, cx| {
            vh_github.update(cx, |app, _cx| {
                if let Some(snapshot) = &app.repo.snapshot {
                    let name = &snapshot.repo.name;
                    let _ = open::that_detached(format!("https://github.com/{name}"));
                }
            });
        },
    ));

    // Outer wrapper: scroll the whole thing, content at top
    div().flex_1().child(
        div()
        .id("no-changes-scroll")
        .flex_1()
        .min_h_0()
        .overflow_y_scrollbar()
        .child(
            v_flex()
                .w_full()
                .p(z(20.0))
                .gap(z(16.0))
                // Header
                .child(
                    v_flex()
                        .gap(z(4.0))
                        .child(
                            div()
                                .text_size(z(14.0))
                                .text_color(theme::text_main())
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("No local changes"),
                        )
                        .child(
                            div()
                                .text_size(z(12.0))
                                .text_color(theme::text_muted())
                                .child("There are no uncommitted changes in this repository. Here are some friendly suggestions for what to do next."),
                        ),
                )
                // Cards
                .child(cards),
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
    let mut shortcut_row = h_flex()
        .gap(z(4.0))
        .items_center()
        .child(
            div()
                .text_size(z(11.0))
                .text_color(theme::text_muted())
                .child(shortcut_prefix.to_string()),
        );
    for key in keys {
        shortcut_row = shortcut_row.child(kbd_badge(key));
    }

    v_flex()
        .w_full()
        .p(z(12.0))
        .gap(z(6.0))
        .rounded(z(theme::CORNER_RADIUS))
        .border_1()
        .border_color(theme::border())
        // Title
        .child(
            div()
                .text_size(z(12.0))
                .text_color(theme::text_main())
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_string()),
        )
        // Shortcut hint
        .child(shortcut_row)
        // Action button — right-aligned
        .child(
            h_flex().justify_end().child(
                div()
                    .id(SharedString::from(id.to_string()))
                    .px(z(12.0))
                    .py(z(4.0))
                    .rounded(z(theme::CORNER_RADIUS))
                    .bg(theme::surface_bg())
                    .border_1()
                    .border_color(theme::border())
                    .text_size(z(12.0))
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

pub fn render_history_row(commit: &CommitInfo, selected: bool) -> Div {
    let bg = if selected {
        accent_selection_bg()
    } else {
        gpui::transparent_black()
    };

    let summary_color = if selected {
        gpui::white().into()
    } else {
        theme::text_main()
    };

    let meta = format!("{} \u{00b7} {}", commit.author_name, commit.date);

    let mut summary_row = h_flex().gap(z(6.0)).child(
        div().flex_1().overflow_x_hidden().child(
            div()
                .text_size(z(12.0))
                .text_color(summary_color)
                .font_weight(FontWeight::SEMIBOLD)
                .whitespace_nowrap()
                .child(commit.summary.clone()),
        ),
    );

    // Version tags
    for tag in &commit.tags {
        summary_row = summary_row.child(
            Tag::secondary().xsmall().child(tag.clone()),
        );
    }

    if commit.is_head {
        summary_row = summary_row.child(Tag::primary().xsmall().child("HEAD"));
    }

    v_flex()
        .w_full()
        .px(z(10.0))
        .py(z(6.0))
        .bg(bg)
        .border_b_1()
        .border_color(theme::toolbar_button_border())
        .gap(z(2.0))
        .child(summary_row)
        .child(
            div()
                .text_size(z(11.0))
                .text_color(theme::text_muted())
                .whitespace_nowrap()
                .child(meta),
        )
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
