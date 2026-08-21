use std::collections::HashSet;
use std::rc::Rc;

use gpui::*;
use gpui_component::h_flex;

use crate::ui::GitSparkApp;
use crate::ui::diff_line_selection::{DiffLineSelection, DiffLineSelectionKind};
use crate::ui::theme;
use crate::ui::theme::z;
use crate::ui::workspace::DiffListHandle;

#[derive(Clone)]
struct RawDiffLine {
    is_add: bool,
    is_del: bool,
    content: String,
    old_line: Option<usize>,
    new_line: Option<usize>,
    selection_target: Option<DiffLineSelection>,
}

#[derive(Clone)]
enum SideBySideRow {
    Hunk(String),
    Pair {
        old_line: Option<usize>,
        old_text: Option<String>,
        old_is_deleted: bool,
        old_target: Option<DiffLineSelection>,
        new_line: Option<usize>,
        new_text: Option<String>,
        new_is_added: bool,
        new_target: Option<DiffLineSelection>,
    },
}

/// Virtualized split diff.
///
/// Same reasoning as the unified view (design.md §11): building every row on
/// every frame costs the whole frame budget on a large file, and rows here
/// are variable height too, so this is `list` rather than `uniform_list`.
pub fn render_side_by_side_diff(
    file_path: &str,
    diff_text: &str,
    hide_whitespace_changes: bool,
    excluded_lines: &HashSet<DiffLineSelection>,
    view: Option<&Entity<GitSparkApp>>,
    diff_list: &DiffListHandle,
) -> Div {
    let rows = side_by_side_rows(file_path, diff_text, hide_whitespace_changes);

    // Keyed on the same axes as the unified view, plus a marker so toggling
    // split/unified for one file still counts as a content change.
    let key = format!("split|{file_path}|{}|{hide_whitespace_changes}", rows.len());
    let state = diff_list.sync(key, rows.len());

    let rows = Rc::new(rows);
    let owned_excluded = excluded_lines.clone();
    let owned_view = view.cloned();

    let list_element = list(state, move |ix, _window, _cx| {
        let Some(row) = rows.get(ix) else {
            return div().into_any_element();
        };
        render_side_by_side_row(
            row,
            ix,
            hide_whitespace_changes,
            &owned_excluded,
            owned_view.as_ref(),
        )
    })
    .with_sizing_behavior(ListSizingBehavior::Auto);

    div()
        .w_full()
        .h_full()
        .child(list_element.w_full().h_full())
}

fn side_by_side_rows(
    file_path: &str,
    diff_text: &str,
    hide_whitespace_changes: bool,
) -> Vec<SideBySideRow> {
    let raw = parse_raw_diff(file_path, diff_text);
    let mut rows = Vec::new();
    let mut index = 0usize;
    let mut hunk_index = 0usize;

    while index < raw.len() {
        match &raw[index] {
            None => {
                if let Some(header) = hunk_header_at(diff_text, hunk_index) {
                    rows.push(SideBySideRow::Hunk(header));
                }
                hunk_index += 1;
                index += 1;
            }
            Some(line) if line.is_del => {
                let del_start = index;
                while index < raw.len() && raw[index].as_ref().is_some_and(|line| line.is_del) {
                    index += 1;
                }
                let add_start = index;
                while index < raw.len() && raw[index].as_ref().is_some_and(|line| line.is_add) {
                    index += 1;
                }

                let deleted = raw[del_start..add_start]
                    .iter()
                    .filter_map(Option::as_ref)
                    .collect::<Vec<_>>();
                let added = raw[add_start..index]
                    .iter()
                    .filter_map(Option::as_ref)
                    .collect::<Vec<_>>();

                if hide_whitespace_changes
                    && deleted.len() == added.len()
                    && deleted.iter().zip(added.iter()).all(|(old, new)| {
                        whitespace_normalized(&old.content) == whitespace_normalized(&new.content)
                    })
                {
                    continue;
                }

                let len = deleted.len().max(added.len());
                for ix in 0..len {
                    let old = deleted.get(ix);
                    let new = added.get(ix);
                    rows.push(SideBySideRow::Pair {
                        old_line: old.and_then(|line| line.old_line),
                        old_text: old.map(|line| line.content.clone()),
                        old_is_deleted: old.is_some(),
                        old_target: old.and_then(|line| line.selection_target.clone()),
                        new_line: new.and_then(|line| line.new_line),
                        new_text: new.map(|line| line.content.clone()),
                        new_is_added: new.is_some(),
                        new_target: new.and_then(|line| line.selection_target.clone()),
                    });
                }
            }
            Some(line) if line.is_add => {
                rows.push(SideBySideRow::Pair {
                    old_line: None,
                    old_text: None,
                    old_is_deleted: false,
                    old_target: None,
                    new_line: line.new_line,
                    new_text: Some(line.content.clone()),
                    new_is_added: true,
                    new_target: line.selection_target.clone(),
                });
                index += 1;
            }
            Some(line) => {
                rows.push(SideBySideRow::Pair {
                    old_line: line.old_line,
                    old_text: Some(line.content.clone()),
                    old_is_deleted: false,
                    old_target: None,
                    new_line: line.new_line,
                    new_text: Some(line.content.clone()),
                    new_is_added: false,
                    new_target: None,
                });
                index += 1;
            }
        }
    }

    rows
}

fn parse_raw_diff(file_path: &str, diff_text: &str) -> Vec<Option<RawDiffLine>> {
    let mut raw = Vec::new();
    let mut old_num = 0usize;
    let mut new_num = 0usize;

    for line in diff_text.lines() {
        if line.starts_with("@@") {
            if let Some((old_start, new_start)) = parse_hunk_starts(line) {
                old_num = old_start;
                new_num = new_start;
            }
            raw.push(None);
        } else if line.starts_with("diff --git")
            || line.starts_with("index ")
            || line.starts_with("+++")
            || line.starts_with("---")
        {
            continue;
        } else if let Some(content) = line.strip_prefix('+') {
            raw.push(Some(RawDiffLine {
                is_add: true,
                is_del: false,
                content: content.to_string(),
                old_line: None,
                new_line: Some(new_num),
                selection_target: Some(DiffLineSelection {
                    path: file_path.to_string(),
                    old_line: None,
                    new_line: Some(new_num),
                    kind: DiffLineSelectionKind::Added,
                }),
            }));
            new_num += 1;
        } else if let Some(content) = line.strip_prefix('-') {
            raw.push(Some(RawDiffLine {
                is_add: false,
                is_del: true,
                content: content.to_string(),
                old_line: Some(old_num),
                new_line: None,
                selection_target: Some(DiffLineSelection {
                    path: file_path.to_string(),
                    old_line: Some(old_num),
                    new_line: None,
                    kind: DiffLineSelectionKind::Deleted,
                }),
            }));
            old_num += 1;
        } else {
            let content = line.strip_prefix(' ').unwrap_or(line);
            raw.push(Some(RawDiffLine {
                is_add: false,
                is_del: false,
                content: content.to_string(),
                old_line: Some(old_num),
                new_line: Some(new_num),
                selection_target: None,
            }));
            old_num += 1;
            new_num += 1;
        }
    }

    raw
}

fn hunk_header_at(diff_text: &str, hunk_index: usize) -> Option<String> {
    diff_text
        .lines()
        .filter(|line| line.starts_with("@@"))
        .nth(hunk_index)
        .map(ToString::to_string)
}

fn parse_hunk_starts(line: &str) -> Option<(usize, usize)> {
    let rest = line.strip_prefix("@@ -")?;
    let (old_part, rest) = rest.split_once(" +")?;
    let new_part = rest.split(' ').next()?;
    let old_start = old_part.split(',').next()?.parse().ok()?;
    let new_start = new_part.split(',').next()?.parse().ok()?;
    Some((old_start, new_start))
}

fn render_side_by_side_row(
    row: &SideBySideRow,
    index: usize,
    hide_whitespace_changes: bool,
    excluded_lines: &HashSet<DiffLineSelection>,
    view: Option<&Entity<GitSparkApp>>,
) -> AnyElement {
    match row {
        SideBySideRow::Hunk(text) => h_flex()
            .id(SharedString::from(format!("side-by-side-hunk-{index}")))
            .w_full()
            .h(z(theme::DIFF_ROW_HEIGHT))
            .flex_shrink_0()
            .bg(theme::diff_hunk_bg())
            .items_center()
            .px(z(8.0))
            .text_size(z(12.0))
            .text_color(theme::text_muted())
            .child(text.clone())
            .into_any_element(),
        SideBySideRow::Pair {
            old_line,
            old_text,
            old_is_deleted,
            old_target,
            new_line,
            new_text,
            new_is_added,
            new_target,
        } => h_flex()
            .id(SharedString::from(format!("side-by-side-row-{index}")))
            .w_full()
            .min_h(z(theme::DIFF_ROW_HEIGHT))
            .flex_shrink_0()
            .child(render_side_segment(
                old_text.as_deref(),
                *old_is_deleted,
                false,
                *old_line,
                old_target.as_ref().filter(|_| !hide_whitespace_changes),
                old_target
                    .as_ref()
                    .is_some_and(|target| !excluded_lines.contains(target)),
                view,
            ))
            .child(render_side_segment(
                new_text.as_deref(),
                false,
                *new_is_added,
                *new_line,
                new_target.as_ref().filter(|_| !hide_whitespace_changes),
                new_target
                    .as_ref()
                    .is_some_and(|target| !excluded_lines.contains(target)),
                view,
            ))
            .into_any_element(),
    }
}

fn render_side_segment(
    text: Option<&str>,
    deleted: bool,
    added: bool,
    line: Option<usize>,
    target: Option<&DiffLineSelection>,
    selected: bool,
    view: Option<&Entity<GitSparkApp>>,
) -> AnyElement {
    let bg = if deleted {
        theme::diff_del_bg()
    } else if added {
        theme::diff_add_bg()
    } else {
        theme::bg()
    };
    let fg = if deleted {
        theme::diff_del_fg()
    } else if added {
        theme::diff_add_fg()
    } else {
        theme::text_main()
    };

    h_flex()
        .flex_1()
        .min_w_0()
        .min_h(z(theme::DIFF_ROW_HEIGHT))
        .bg(bg)
        .text_color(fg)
        .child(render_side_line_number(
            line, deleted, added, target, selected, view,
        ))
        .child(render_side_cell(text, bg, fg))
        .into_any_element()
}

fn render_side_cell(text: Option<&str>, bg: Hsla, fg: Hsla) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .min_h(z(theme::DIFF_ROW_HEIGHT))
        .px(z(8.0))
        .py(z(4.0))
        .bg(bg)
        .text_size(z(12.0))
        .font_family(theme::mono_family())
        .text_color(fg)
        .child(text.unwrap_or("").to_string())
}

fn render_side_line_number(
    line: Option<usize>,
    deleted: bool,
    added: bool,
    target: Option<&DiffLineSelection>,
    selected: bool,
    view: Option<&Entity<GitSparkApp>>,
) -> AnyElement {
    let gutter_bg = if selected {
        theme::diff_selected_bg()
    } else if deleted {
        theme::diff_del_gutter_bg()
    } else if added {
        theme::diff_add_gutter_bg()
    } else {
        theme::diff_gutter_bg()
    };

    let fg = if selected {
        theme::text_main()
    } else {
        theme::line_num_color()
    };

    let base = h_flex()
        .w(z(55.0))
        .flex_shrink_0()
        .min_h(z(theme::DIFF_ROW_HEIGHT))
        .items_center()
        .bg(gutter_bg)
        .text_size(z(12.0))
        .text_color(fg)
        .child(render_selection_mark(selected, target.is_some()))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .px(z(6.0))
                .text_align(gpui::TextAlign::Right)
                .whitespace_nowrap()
                .child(render_line_number_text(line)),
        );

    if let Some(target) = target {
        let mut interactive_gutter = base.id(SharedString::from(target.id())).cursor_pointer();

        if let Some(vh) = view {
            let target_for_click = target.clone();
            let toggle_view = vh.clone();
            interactive_gutter = interactive_gutter.on_click(move |_evt, _win, cx| {
                let target = target_for_click.clone();
                toggle_view.update(cx, |app, cx| {
                    app.toggle_diff_line_selection(target, cx);
                });
            });
        }

        return interactive_gutter.into_any_element();
    }

    base.into_any_element()
}

fn render_line_number_text(line: Option<usize>) -> Div {
    let mut text = h_flex().items_center().justify_end().whitespace_nowrap();
    if let Some(line) = line {
        for ch in line.to_string().chars() {
            text = text.child(ch.to_string());
        }
    }
    text
}

fn render_selection_mark(selected: bool, selectable: bool) -> Div {
    if !selectable {
        return div().w(z(20.0)).flex_shrink_0();
    }

    let mark = div()
        .w(z(20.0))
        .flex_shrink_0()
        .items_center()
        .justify_center();

    if selected {
        mark.text_size(z(11.0))
            .text_color(theme::text_main())
            .child("✓")
    } else {
        mark
    }
}

fn whitespace_normalized(value: &str) -> String {
    value.chars().filter(|ch| !ch.is_whitespace()).collect()
}
