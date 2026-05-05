use std::cmp::Ordering;
use std::collections::HashSet;

use crate::ui::diff_line_selection::{DiffLineSelection, DiffLineSelectionKind};

#[derive(Clone, Debug)]
enum TextEdit {
    Remove {
        index: usize,
        content: String,
    },
    Insert {
        index: usize,
        content: String,
        sequence: usize,
    },
}

impl TextEdit {
    fn index(&self) -> usize {
        match self {
            Self::Remove { index, .. } | Self::Insert { index, .. } => *index,
        }
    }
}

pub(crate) fn discard_selected_lines_in_text(
    file_path: &str,
    diff_text: &str,
    file_text: &str,
    selections: &HashSet<DiffLineSelection>,
) -> Result<String, String> {
    if selections.is_empty() {
        return Err("Select at least one changed line to discard.".to_string());
    }

    let edits = selected_line_edits(file_path, diff_text, selections);
    if edits.is_empty() {
        return Err("No selected changed lines are present in the current diff.".to_string());
    }

    let had_final_newline = file_text.ends_with('\n');
    let mut lines: Vec<String> = file_text.lines().map(ToString::to_string).collect();
    let mut edits = edits;
    edits.sort_by(compare_edits_for_application);

    for edit in edits {
        match edit {
            TextEdit::Remove { index, content } => {
                let Some(current) = lines.get(index) else {
                    return Err(format!("Line {} is no longer available.", index + 1));
                };
                if current != &content {
                    return Err(format!(
                        "Line {} has changed since the diff was loaded.",
                        index + 1
                    ));
                }
                lines.remove(index);
            }
            TextEdit::Insert { index, content, .. } => {
                let index = index.min(lines.len());
                lines.insert(index, content);
            }
        }
    }

    let mut next = lines.join("\n");
    if had_final_newline && !next.is_empty() {
        next.push('\n');
    }
    Ok(next)
}

fn selected_line_edits(
    file_path: &str,
    diff_text: &str,
    selections: &HashSet<DiffLineSelection>,
) -> Vec<TextEdit> {
    let mut edits = Vec::new();
    let mut old_line = 0usize;
    let mut new_line = 0usize;
    let mut sequence = 0usize;

    for raw_line in diff_text.lines() {
        if let Some((old_start, new_start)) = parse_hunk_starts(raw_line) {
            old_line = old_start;
            new_line = new_start;
            continue;
        }

        if raw_line.starts_with("diff --git")
            || raw_line.starts_with("index ")
            || raw_line.starts_with("---")
            || raw_line.starts_with("+++")
            || raw_line.starts_with("\\ ")
        {
            continue;
        }

        if let Some(content) = raw_line.strip_prefix('+') {
            let target = DiffLineSelection {
                path: file_path.to_string(),
                old_line: None,
                new_line: Some(new_line),
                kind: DiffLineSelectionKind::Added,
            };
            if selections.contains(&target) {
                edits.push(TextEdit::Remove {
                    index: new_line.saturating_sub(1),
                    content: content.to_string(),
                });
            }
            new_line += 1;
        } else if let Some(content) = raw_line.strip_prefix('-') {
            let target = DiffLineSelection {
                path: file_path.to_string(),
                old_line: Some(old_line),
                new_line: None,
                kind: DiffLineSelectionKind::Deleted,
            };
            if selections.contains(&target) {
                edits.push(TextEdit::Insert {
                    index: new_line.saturating_sub(1),
                    content: content.to_string(),
                    sequence,
                });
            }
            old_line += 1;
            sequence += 1;
        } else {
            old_line += 1;
            new_line += 1;
        }
    }

    edits
}

fn compare_edits_for_application(left: &TextEdit, right: &TextEdit) -> Ordering {
    right
        .index()
        .cmp(&left.index())
        .then_with(|| edit_priority(left).cmp(&edit_priority(right)))
        .then_with(|| match (left, right) {
            (TextEdit::Insert { sequence: a, .. }, TextEdit::Insert { sequence: b, .. }) => {
                b.cmp(a)
            }
            _ => Ordering::Equal,
        })
}

fn edit_priority(edit: &TextEdit) -> usize {
    match edit {
        TextEdit::Remove { .. } => 0,
        TextEdit::Insert { .. } => 1,
    }
}

fn parse_hunk_starts(line: &str) -> Option<(usize, usize)> {
    let rest = line.strip_prefix("@@ -")?;
    let (old_part, rest) = rest.split_once(" +")?;
    let new_part = rest.split_once(" @@").map(|(part, _)| part).unwrap_or(rest);
    let old_start = old_part.split(',').next()?.parse().ok()?;
    let new_start = new_part.split(',').next()?.parse().ok()?;
    Some((old_start, new_start))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(path: &str, kind: DiffLineSelectionKind, line: usize) -> DiffLineSelection {
        DiffLineSelection {
            path: path.to_string(),
            old_line: matches!(kind, DiffLineSelectionKind::Deleted).then_some(line),
            new_line: matches!(kind, DiffLineSelectionKind::Added).then_some(line),
            kind,
        }
    }

    #[test]
    fn discards_selected_added_line() {
        let diff = "@@ -1,2 +1,3 @@\n one\n+two\n three";
        let mut selected = HashSet::new();
        selected.insert(target("code.txt", DiffLineSelectionKind::Added, 2));

        let next = discard_selected_lines_in_text("code.txt", diff, "one\ntwo\nthree\n", &selected)
            .unwrap();
        assert_eq!(next, "one\nthree\n");
    }

    #[test]
    fn discards_selected_deleted_line() {
        let diff = "@@ -1,3 +1,2 @@\n one\n-two\n three";
        let mut selected = HashSet::new();
        selected.insert(target("code.txt", DiffLineSelectionKind::Deleted, 2));

        let next =
            discard_selected_lines_in_text("code.txt", diff, "one\nthree\n", &selected).unwrap();
        assert_eq!(next, "one\ntwo\nthree\n");
    }

    #[test]
    fn discards_selected_replacement_pair() {
        let diff = "@@ -1,3 +1,3 @@\n one\n-two\n+two changed\n three";
        let mut selected = HashSet::new();
        selected.insert(target("code.txt", DiffLineSelectionKind::Deleted, 2));
        selected.insert(target("code.txt", DiffLineSelectionKind::Added, 2));

        let next = discard_selected_lines_in_text(
            "code.txt",
            diff,
            "one\ntwo changed\nthree\n",
            &selected,
        )
        .unwrap();
        assert_eq!(next, "one\ntwo\nthree\n");
    }
}
