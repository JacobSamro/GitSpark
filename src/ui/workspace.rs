use gpui::*;
use gpui_component::menu::{ContextMenuExt, PopupMenuItem};
use gpui_component::{Icon, IconName, h_flex, v_flex};
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;

use crate::models::DiffEntry;
use crate::ui::app::{DiffExpandDirection, GitSparkApp};

use crate::ui::binary_diff::render_binary_diff_panel;
use crate::ui::diff_line_selection::{DiffLineSelection, DiffLineSelectionKind};
use crate::ui::image_diff::render_image_diff_panel;
use crate::ui::submodule_diff::render_submodule_diff_panel;
use crate::ui::theme;
use crate::ui::theme::z;

// --- Constants ---

#[allow(dead_code)]
const MAX_INTRA_LINE_CHARS: usize = 1024;
const EXPAND_STEP: usize = 20;

/// Group name tying a diff gutter to the line-number cells inside it, so
/// hovering anywhere in the gutter highlights the cells that paint over it.
const DIFF_GUTTER_GROUP: &str = "diff-gutter";

// --- Hunk boundary info for expansion ---

#[derive(Clone, Debug)]
struct HunkBounds {
    /// Line number where this hunk starts in the new file (1-based).
    new_start: usize,
    /// Number of lines in this hunk in the new file.
    new_count: usize,
    /// Line number where this hunk starts in the old file (1-based).
    old_start: usize,
    /// Number of lines in this hunk in the old file.
    old_count: usize,
}

/// Virtualized-list state for the unified diff.
///
/// `gpui::list` needs its `ListState` to LIVE ACROSS RENDERS — it caches
/// measured item heights there — so the view owns this and hands out clones.
/// `uniform_list` was not an option: diff rows are `DIFF_ROW_HEIGHT` but hunk
/// headers are taller and wrapped lines taller still, and `uniform_list`
/// requires one height for every row.
///
/// The key is what the list was last built for. When it changes — different
/// file, different row count, split/unified toggled — the cached heights and
/// scroll offset belong to the old content and must be thrown away, or the
/// new diff opens scrolled into the middle of nowhere.
pub(crate) struct DiffListHandle {
    state: ListState,
    key: RefCell<String>,
}

impl Default for DiffListHandle {
    fn default() -> Self {
        Self {
            state: ListState::new(0, ListAlignment::Top, px(600.0)),
            key: RefCell::new(String::new()),
        }
    }
}

impl DiffListHandle {
    pub(crate) fn sync(&self, key: String, count: usize) -> ListState {
        let mut current = self.key.borrow_mut();
        if *current != key {
            self.state.reset(count);
            *current = key;
        }
        self.state.clone()
    }
}

/// One row of the unified diff, resolved ahead of virtualization.
///
/// The list renders arbitrary indices on demand, so nothing can depend on
/// walking the rows in order — the hunk index in particular used to be a
/// counter incremented by the loop, and is now baked into the row.
enum DiffRow {
    Hunk {
        line: DiffLine,
        index: usize,
        expansion: ExpansionType,
    },
    Line(DiffLine),
    WhitespaceHidden,
    ExpandEof {
        last_hunk: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ExpansionType {
    None,
    Up,
    #[allow(dead_code)]
    Down,
    Both,
    Short, // gap <= EXPAND_STEP lines, single unfold button
}

/// Determine expansion type for each hunk based on boundaries and file length.
/// Decide which expand affordance, if any, each hunk header should offer.
///
/// `can_expand` is false when the diff has no file contents to expand FROM —
/// a deleted file, or one that could not be read. The expansion action returns
/// early in that case, so offering the control anyway produced a visibly dead
/// button. No contents, no affordance.
fn compute_expansion_types(
    hunks: &[HunkBounds],
    _file_line_count: usize,
    can_expand: bool,
) -> Vec<ExpansionType> {
    if hunks.is_empty() || !can_expand {
        return vec![ExpansionType::None; hunks.len()];
    }
    let mut types = Vec::with_capacity(hunks.len());

    for (i, hunk) in hunks.iter().enumerate() {
        if i == 0 {
            // First hunk: can expand up if there's content above
            if hunk.new_start > 1 && hunk.old_start > 1 {
                types.push(ExpansionType::Up);
            } else {
                types.push(ExpansionType::None);
            }
        } else {
            // Distance from end of previous hunk to start of this hunk
            let prev = &hunks[i - 1];
            let prev_end = prev.new_start + prev.new_count;
            let gap = hunk.new_start.saturating_sub(prev_end);
            if gap == 0 {
                types.push(ExpansionType::None);
            } else if gap <= EXPAND_STEP {
                types.push(ExpansionType::Short);
            } else {
                types.push(ExpansionType::Both);
            }
        }
    }

    types
}

/// Expand a diff in-memory by inserting context lines from `file_lines`.
/// Returns new diff text with expanded context.
///
/// `file_lines` is 0-indexed; hunk line numbers are 1-based.
pub fn expand_diff_in_memory(
    diff_text: &str,
    file_lines: &[String],
    hunk_index: usize,
    direction: DiffExpandDirection,
) -> String {
    let diff_lines: Vec<&str> = diff_text.lines().collect();
    let total_file = file_lines.len();

    // Collect (diff_line_index, bounds) for each hunk header
    let mut hunks: Vec<(usize, HunkBounds)> = Vec::new();
    for (i, line) in diff_lines.iter().enumerate() {
        if let Some(b) = parse_hunk_header(line) {
            hunks.push((i, b));
        }
    }

    if hunks.is_empty() || total_file == 0 {
        return diff_text.to_string();
    }

    let target = hunk_index.min(hunks.len().saturating_sub(1));

    // Extract suffix text after @@ from original headers (e.g. " impl GitClient {")
    let hunk_suffixes: Vec<String> = hunks
        .iter()
        .map(|(dl_idx, _)| {
            let line = diff_lines[*dl_idx];
            // Find the closing @@ and take everything after it
            if let Some(pos) = line.find(" @@") {
                line[pos + 3..].to_string()
            } else {
                String::new()
            }
        })
        .collect();

    // Copy diff header lines (before first hunk)
    let first_hunk_line = hunks[0].0;
    let mut out: Vec<String> = diff_lines[..first_hunk_line]
        .iter()
        .map(|s| s.to_string())
        .collect();

    for (hi, (dl, bounds)) in hunks.iter().enumerate() {
        let next_dl = hunks
            .get(hi + 1)
            .map(|(i, _)| *i)
            .unwrap_or(diff_lines.len());

        if hi != target {
            // Copy hunk as-is
            for li in *dl..next_dl {
                out.push(diff_lines[li].to_string());
            }
            continue;
        }

        // --- Expand this hunk ---
        // file_lines is 0-based; new_start is 1-based.
        // The first line of this hunk in the file is at index (new_start - 1).
        // The last line of this hunk's content ends at index (new_start - 1 + new_count - 1).
        // Context lines ABOVE the hunk: file indices 0..new_start-1 (before the hunk).
        // Context lines BELOW the hunk: file indices (new_start-1+new_count)..total_file.

        let hunk_file_start = bounds.new_start.saturating_sub(1); // 0-based first line of hunk
        let hunk_file_end = hunk_file_start + bounds.new_count; // 0-based exclusive end

        match direction {
            DiffExpandDirection::Up => {
                let step = EXPAND_STEP.min(hunk_file_start);
                let ctx_start = hunk_file_start - step; // 0-based
                let suffix = hunk_suffixes.get(hi).map(|s| s.as_str()).unwrap_or("");

                // Updated header
                out.push(format!(
                    "@@ -{},{} +{},{} @@{suffix}",
                    bounds.old_start.saturating_sub(step),
                    bounds.old_count + step,
                    bounds.new_start.saturating_sub(step),
                    bounds.new_count + step,
                ));

                // New context lines from file (above)
                for fi in ctx_start..hunk_file_start {
                    out.push(format!(" {}", file_lines[fi]));
                }

                // Original hunk content lines (skip header)
                for li in (*dl + 1)..next_dl {
                    out.push(diff_lines[li].to_string());
                }
            }
            DiffExpandDirection::Down => {
                let room = total_file.saturating_sub(hunk_file_end);
                let step = EXPAND_STEP.min(room);
                let suffix = hunk_suffixes.get(hi).map(|s| s.as_str()).unwrap_or("");

                // Updated header
                out.push(format!(
                    "@@ -{},{} +{},{} @@{suffix}",
                    bounds.old_start,
                    bounds.old_count + step,
                    bounds.new_start,
                    bounds.new_count + step,
                ));

                // Original hunk content lines (skip header)
                for li in (*dl + 1)..next_dl {
                    out.push(diff_lines[li].to_string());
                }

                // New context lines from file (below)
                for fi in hunk_file_end..(hunk_file_end + step) {
                    out.push(format!(" {}", file_lines[fi]));
                }
            }
            DiffExpandDirection::All => {
                let above = hunk_file_start;
                let below = total_file.saturating_sub(hunk_file_end);
                let suffix = hunk_suffixes.get(hi).map(|s| s.as_str()).unwrap_or("");

                out.push(format!(
                    "@@ -{},{} +{},{} @@{suffix}",
                    1,
                    bounds.old_count + above + below,
                    1,
                    bounds.new_count + above + below,
                ));

                // All lines above
                for fi in 0..hunk_file_start {
                    out.push(format!(" {}", file_lines[fi]));
                }

                // Original hunk content (skip header)
                for li in (*dl + 1)..next_dl {
                    out.push(diff_lines[li].to_string());
                }

                // All lines below
                for fi in hunk_file_end..total_file {
                    out.push(format!(" {}", file_lines[fi]));
                }
            }
            DiffExpandDirection::MergeWithPrevious => {
                // Fill the gap between the previous hunk and this one.
                // Find where the previous hunk ends in the file.
                if hi > 0 {
                    let prev_bounds = &hunks[hi - 1].1;
                    let prev_end = prev_bounds.new_start.saturating_sub(1) + prev_bounds.new_count;
                    let gap_lines = hunk_file_start.saturating_sub(prev_end);

                    // Remove the last hunk header we pushed (prev hunk's header)
                    // and merge: extend prev hunk to include the gap + this hunk.
                    // Instead: don't emit a new header for this hunk, just add gap lines
                    // to the previous hunk's output and then this hunk's content.

                    // Insert gap context lines (these go at the end of prev hunk's output)
                    for fi in prev_end..hunk_file_start {
                        if fi < total_file {
                            out.push(format!(" {}", file_lines[fi]));
                        }
                    }

                    // Now update the previous hunk's header in `out` to include the gap + this hunk
                    // Find the last @@ line in out and update its counts
                    if let Some(last_hunk_pos) = out.iter().rposition(|l| l.starts_with("@@ ")) {
                        let new_old_count = prev_bounds.old_count + gap_lines + bounds.old_count;
                        let new_new_count = prev_bounds.new_count + gap_lines + bounds.new_count;
                        let prev_suffix =
                            hunk_suffixes.get(hi - 1).map(|s| s.as_str()).unwrap_or("");
                        out[last_hunk_pos] = format!(
                            "@@ -{},{} +{},{} @@{prev_suffix}",
                            prev_bounds.old_start,
                            new_old_count,
                            prev_bounds.new_start,
                            new_new_count,
                        );
                    }

                    // Append this hunk's content (skip its header)
                    for li in (*dl + 1)..next_dl {
                        out.push(diff_lines[li].to_string());
                    }
                } else {
                    // No previous hunk, just copy as-is
                    for li in *dl..next_dl {
                        out.push(diff_lines[li].to_string());
                    }
                }
            }
        }
    }

    out.join("\n")
}

fn parse_hunk_header(line: &str) -> Option<HunkBounds> {
    let rest = line.strip_prefix("@@ -")?;
    let parts: Vec<&str> = rest.splitn(2, " +").collect();
    if parts.len() != 2 {
        return None;
    }
    let old_parts: Vec<&str> = parts[0].split(',').collect();
    let old_start: usize = old_parts.first()?.parse().ok()?;
    let old_count: usize = old_parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);

    let new_part = parts[1].split(' ').next()?;
    let new_parts: Vec<&str> = new_part.split(',').collect();
    let new_start: usize = new_parts.first()?.parse().ok()?;
    let new_count: usize = new_parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);

    Some(HunkBounds {
        new_start,
        new_count,
        old_start,
        old_count,
    })
}

// --- Intra-line character range ---

#[derive(Clone, Debug)]
struct CharRange {
    start: usize,
    end: usize,
}

// --- Diff line classification ---

#[derive(Clone, Debug)]
enum DiffLineKind {
    Context,
    Added,
    Deleted,
    HunkHeader,
    /// A modified line has paired old/new content with optional intra-line highlights.
    #[allow(dead_code)]
    Modified {
        old_highlight: Option<CharRange>,
        new_highlight: Option<CharRange>,
    },
}

#[derive(Clone, Debug)]
struct DiffLine {
    kind: DiffLineKind,
    content: String,
    /// For Modified lines, the new (added) content.
    new_content: Option<String>,
    old_line: Option<usize>,
    new_line: Option<usize>,
}

// --- Intermediate parsed line used during pairing ---

#[derive(Clone, Debug)]
struct RawDiffLine {
    is_add: bool,
    is_del: bool,
    content: String,
    old_line: Option<usize>,
    new_line: Option<usize>,
}

/// Parse a unified diff string into classified lines with line numbers.
///
/// Detects paired delete/add blocks of equal length and converts them to
/// `Modified` lines with intra-line character-level diff highlights.
fn parse_diff(raw: &str) -> Vec<DiffLine> {
    // First pass: parse into RawDiffLines and hunk headers
    let mut raw_lines: Vec<Option<RawDiffLine>> = Vec::new(); // None = hunk header
    let mut hunk_header_texts: Vec<String> = Vec::new();
    let mut old_num: usize = 0;
    let mut new_num: usize = 0;

    for raw_line in raw.lines() {
        if raw_line.starts_with("@@") {
            if let Some(rest) = raw_line.strip_prefix("@@ -") {
                let parts: Vec<&str> = rest.splitn(2, " +").collect();
                if parts.len() == 2 {
                    if let Some(old_start_str) = parts[0].split(',').next() {
                        old_num = old_start_str.parse().unwrap_or(1);
                    }
                    let new_part = parts[1].split(' ').next().unwrap_or("");
                    if let Some(new_start_str) = new_part.split(',').next() {
                        new_num = new_start_str.parse().unwrap_or(1);
                    }
                }
            }
            hunk_header_texts.push(raw_line.to_string());
            raw_lines.push(None); // sentinel for hunk header
        } else if raw_line.starts_with("diff --git")
            || raw_line.starts_with("index ")
            || raw_line.starts_with("+++")
            || raw_line.starts_with("---")
        {
            // Meta / file header lines — skip
            continue;
        } else if raw_line.starts_with('+') {
            raw_lines.push(Some(RawDiffLine {
                is_add: true,
                is_del: false,
                content: raw_line[1..].to_string(),
                old_line: None,
                new_line: Some(new_num),
            }));
            new_num += 1;
        } else if raw_line.starts_with('-') {
            raw_lines.push(Some(RawDiffLine {
                is_add: false,
                is_del: true,
                content: raw_line[1..].to_string(),
                old_line: Some(old_num),
                new_line: None,
            }));
            old_num += 1;
        } else {
            let content = raw_line.strip_prefix(' ').unwrap_or(raw_line);
            raw_lines.push(Some(RawDiffLine {
                is_add: false,
                is_del: false,
                content: content.to_string(),
                old_line: Some(old_num),
                new_line: Some(new_num),
            }));
            old_num += 1;
            new_num += 1;
        }
    }

    // Second pass: detect paired delete/add blocks and build final DiffLines
    let mut result = Vec::new();
    let mut hunk_idx = 0usize;
    let mut i = 0usize;

    while i < raw_lines.len() {
        let Some(ref line) = raw_lines[i] else {
            // Hunk header
            result.push(DiffLine {
                kind: DiffLineKind::HunkHeader,
                content: hunk_header_texts.get(hunk_idx).cloned().unwrap_or_default(),
                new_content: None,
                old_line: None,
                new_line: None,
            });
            hunk_idx += 1;
            i += 1;
            continue;
        };

        if line.is_del {
            // Collect contiguous deleted lines
            let del_start = i;
            while i < raw_lines.len() && raw_lines[i].as_ref().map(|l| l.is_del).unwrap_or(false) {
                i += 1;
            }
            let del_end = i;

            // Collect contiguous added lines that follow
            let add_start = i;
            while i < raw_lines.len() && raw_lines[i].as_ref().map(|l| l.is_add).unwrap_or(false) {
                i += 1;
            }
            let add_end = i;

            let _del_count = del_end - del_start;
            let _add_count = add_end - add_start;

            // Always emit as separate Deleted then Added lines
            // (matches GitHub Desktop unified diff layout)
            for j in del_start..del_end {
                let l = raw_lines[j].as_ref().unwrap();
                result.push(DiffLine {
                    kind: DiffLineKind::Deleted,
                    content: l.content.clone(),
                    new_content: None,
                    old_line: l.old_line,
                    new_line: None,
                });
            }
            for j in add_start..add_end {
                let l = raw_lines[j].as_ref().unwrap();
                result.push(DiffLine {
                    kind: DiffLineKind::Added,
                    content: l.content.clone(),
                    new_content: None,
                    old_line: None,
                    new_line: l.new_line,
                });
            }
        } else if line.is_add {
            // Standalone added line (not preceded by deletes)
            result.push(DiffLine {
                kind: DiffLineKind::Added,
                content: line.content.clone(),
                new_content: None,
                old_line: None,
                new_line: line.new_line,
            });
            i += 1;
        } else {
            // Context line
            result.push(DiffLine {
                kind: DiffLineKind::Context,
                content: line.content.clone(),
                new_content: None,
                old_line: line.old_line,
                new_line: line.new_line,
            });
            i += 1;
        }
    }

    result
}

/// Find the character ranges that differ between two lines.
///
/// Returns `(old_range, new_range)` where each range marks the changed
/// character span. If a line is too long, returns `(None, None)`.
#[allow(dead_code)]
fn find_changed_ranges(old_line: &str, new_line: &str) -> (Option<CharRange>, Option<CharRange>) {
    let old_chars: Vec<char> = old_line.chars().collect();
    let new_chars: Vec<char> = new_line.chars().collect();

    if old_chars.len() > MAX_INTRA_LINE_CHARS || new_chars.len() > MAX_INTRA_LINE_CHARS {
        return (None, None);
    }

    // Find common prefix length
    let mut prefix = 0usize;
    while prefix < old_chars.len()
        && prefix < new_chars.len()
        && old_chars[prefix] == new_chars[prefix]
    {
        prefix += 1;
    }

    // Find common suffix length
    let mut old_suffix = 0usize;
    let mut new_suffix = 0usize;
    while old_suffix < old_chars.len().saturating_sub(prefix)
        && new_suffix < new_chars.len().saturating_sub(prefix)
        && old_chars[old_chars.len() - 1 - old_suffix]
            == new_chars[new_chars.len() - 1 - new_suffix]
    {
        old_suffix += 1;
        new_suffix += 1;
    }

    let old_end = old_chars.len().saturating_sub(old_suffix);
    let new_end = new_chars.len().saturating_sub(new_suffix);

    let old_range = (prefix < old_end).then_some(CharRange {
        start: prefix,
        end: old_end,
    });
    let new_range = (prefix < new_end).then_some(CharRange {
        start: prefix,
        end: new_end,
    });

    (old_range, new_range)
}

// --- Rendering ---

/// Render text with an optional highlighted character range.
///
/// Splits the text into up to 3 spans: before, highlighted, after.
fn render_highlighted_text(
    text: &str,
    highlight: Option<&CharRange>,
    base_color: Hsla,
    highlight_bg: Hsla,
) -> Div {
    let Some(range) = highlight else {
        return div().text_color(base_color).child(text.to_string());
    };

    let chars: Vec<char> = text.chars().collect();
    let before: String = chars[..range.start.min(chars.len())].iter().collect();
    let mid: String = chars[range.start.min(chars.len())..range.end.min(chars.len())]
        .iter()
        .collect();
    let after: String = chars[range.end.min(chars.len())..].iter().collect();

    h_flex()
        .child(div().text_color(base_color).child(before))
        .child(div().text_color(base_color).bg(highlight_bg).child(mid))
        .child(div().text_color(base_color).child(after))
}

fn diff_line_selection_target(file_path: &str, line: &DiffLine) -> Option<DiffLineSelection> {
    let kind = match line.kind {
        DiffLineKind::Added => DiffLineSelectionKind::Added,
        DiffLineKind::Deleted => DiffLineSelectionKind::Deleted,
        _ => return None,
    };

    Some(DiffLineSelection {
        path: file_path.to_string(),
        old_line: line.old_line,
        new_line: line.new_line,
        kind,
    })
}

fn diff_row_id(file_path: &str, line: &DiffLine) -> String {
    if let Some(target) = diff_line_selection_target(file_path, line) {
        return target.id();
    }

    let old_line = line
        .old_line
        .map(|line| line.to_string())
        .unwrap_or_else(|| "none".to_string());
    let new_line = line
        .new_line
        .map(|line| line.to_string())
        .unwrap_or_else(|| "none".to_string());
    format!(
        "diff-line-{}-context-{}-{}",
        crate::ui::ids::stable_id_slug(file_path),
        old_line,
        new_line
    )
}

/// Expand tab characters to `tab_size` spaces, column-aware (a tab advances
/// to the next multiple of `tab_size`, like a real terminal/editor, not a
/// flat replacement).
///
/// Runs before syntax highlighting, not after — `highlight_line` returns
/// byte-offset spans, and expanding post-highlight would desynchronize them
/// from the (now different-length) rendered string.
fn expand_tabs(content: &str, tab_size: u8) -> std::borrow::Cow<'_, str> {
    if !content.contains('\t') {
        return std::borrow::Cow::Borrowed(content);
    }
    let tab_size = (tab_size.max(1)) as usize;
    let mut result = String::with_capacity(content.len());
    let mut column = 0usize;
    for ch in content.chars() {
        if ch == '\t' {
            let spaces = tab_size - (column % tab_size);
            result.push_str(&" ".repeat(spaces));
            column += spaces;
        } else {
            result.push(ch);
            column += 1;
        }
    }
    std::borrow::Cow::Owned(result)
}

/// Render code text with syntax colours, falling back to one flat run.
///
/// `base` is what unclassified text keeps, so a deleted row can stay on its
/// own foreground rather than being forced to the shared body colour.
///
/// Returns a single child when there is nothing to colour — a file with no
/// known syntax, or a line the parser classified as entirely plain — so the
/// common case does not pay for a wrapper element per row.
fn render_code_text(content: &str, file_path: &str, base: Hsla, tab_size: u8) -> AnyElement {
    let expanded = expand_tabs(content, tab_size);
    let content = expanded.as_ref();

    let Some(syntax) = crate::ui::syntax::syntax_for_path(file_path) else {
        return div()
            .text_color(base)
            .child(content.to_string())
            .into_any_element();
    };

    let spans = crate::ui::syntax::highlight_line(content, syntax);

    // Fast path only when a single run would render in `base` anyway. Testing
    // just `len() <= 1` dropped the colour from any line that is ONE token all
    // the way across — every whole-line comment in the codebase, which is a
    // lot of them.
    let single_colour = match spans.as_slice() {
        [] => Some(base),
        [only] => Some(only.class.color().unwrap_or(base)),
        _ => None,
    };
    if let Some(colour) = single_colour {
        return div()
            .text_color(colour)
            .child(content.to_string())
            .into_any_element();
    }

    let mut row = h_flex().flex_shrink_0();
    let mut at = 0usize;
    for span in spans.iter() {
        let end = (at + span.len).min(content.len());
        // The spans are byte lengths from the same string, so this slice is
        // always on a char boundary; the guard is for a caller that ever
        // passes content the spans were not built from.
        if !content.is_char_boundary(at) || !content.is_char_boundary(end) {
            return div()
                .text_color(base)
                .child(content.to_string())
                .into_any_element();
        }
        let text = &content[at..end];
        at = end;
        if text.is_empty() {
            continue;
        }
        row = row.child(
            div()
                .flex_shrink_0()
                .text_color(span.class.color().unwrap_or(base))
                .child(text.to_string()),
        );
    }
    row.into_any_element()
}

/// Render a single diff line as a horizontal flex row.
fn render_diff_line(
    file_path: &str,
    line: &DiffLine,
    view: Option<&Entity<GitSparkApp>>,
    excluded_lines: &HashSet<DiffLineSelection>,
    hide_whitespace_changes: bool,
    tab_size: u8,
) -> Stateful<Div> {
    let mut row = h_flex()
        .id(SharedString::from(diff_row_id(file_path, line)))
        .w_full()
        .min_h(z(theme::DIFF_ROW_HEIGHT))
        .flex_shrink_0()
        .font_family(theme::mono_family())
        .text_size(z(12.0));
    // NOTE: no vertical padding on the row. The row paints the add/delete
    // wash, and the gutter paints its own fill on top; padding here insets the
    // gutter's fill by 2px top and bottom, leaving a stripe of the wash above
    // and below every gutter — which reads as gaps between highlighted lines.
    // The breathing room lives on the content children instead.

    let selection_target = diff_line_selection_target(file_path, line);
    let line_included = selection_target
        .as_ref()
        .is_some_and(|target| !excluded_lines.contains(target));

    let selectable = selection_target.is_some() && !hide_whitespace_changes;

    // Only the GUTTER toggles a line in or out of the commit — not the whole
    // row. Clicking the code itself is how you select text to copy, and
    // stealing that click meant every attempt to highlight a token silently
    // changed what was staged. GitHub Desktop scopes the affordance the same
    // way, to `.diff-line-gutter.includeable`.
    let mut gutter = render_unified_line_gutter(line, line_included, selectable).id(
        SharedString::from(format!("{}-gutter", diff_row_id(file_path, line))),
    );

    if let (Some(target), Some(vh)) = (selection_target.clone(), view)
        && selectable
    {
        let target_for_click = target.clone();
        let toggle_view = vh.clone();
        gutter = gutter
            .cursor_pointer()
            // A group, not `.hover()`: the line-number cells paint their own
            // background over this container, so hovering the container alone
            // would be invisible. The cells react to the group instead —
            // which is also what GitHub Desktop highlights (`.includeable:hover
            // .diff-line-number`).
            .group(DIFF_GUTTER_GROUP)
            .on_click(move |_evt, _win, cx| {
                let target = target_for_click.clone();
                toggle_view.update(cx, |app, cx| {
                    app.toggle_diff_line_selection(target, cx);
                });
            });
    }

    row = row.child(gutter);

    // Content — varies by line kind
    match &line.kind {
        DiffLineKind::Added => {
            row = row
                .bg(theme::diff_add_bg())
                .child(div().flex_1().py(z(2.0)).pl(z(5.0)).child(render_code_text(
                    &line.content,
                    file_path,
                    theme::diff_add_fg(),
                    tab_size,
                )));
        }
        DiffLineKind::Deleted => {
            row = row
                .bg(theme::diff_del_bg())
                .child(div().flex_1().py(z(2.0)).pl(z(5.0)).child(render_code_text(
                    &line.content,
                    file_path,
                    theme::diff_del_fg(),
                    tab_size,
                )));
        }
        DiffLineKind::HunkHeader => {
            row = row.bg(theme::diff_hunk_bg()).child(
                div()
                    .flex_1()
                    .py(z(2.0))
                    .pl(z(5.0))
                    .text_color(theme::diff_hunk_fg())
                    .child(line.content.clone()),
            );
        }
        DiffLineKind::Context => {
            row = row.child(
                div()
                    .flex_1()
                    .py(z(2.0))
                    .pl(z(5.0))
                    // --diff-text-color: var(--text-color)
                    .child(render_code_text(
                        &line.content,
                        file_path,
                        theme::text_main(),
                        tab_size,
                    )),
            );
        }
        DiffLineKind::Modified {
            old_highlight,
            new_highlight,
        } => {
            // Side-by-side old | new within the content area
            let old_content = render_highlighted_text(
                &line.content,
                old_highlight.as_ref(),
                theme::diff_del_fg(),
                theme::diff_del_highlight_bg(),
            );
            let new_text = line.new_content.as_deref().unwrap_or("");
            let new_content = render_highlighted_text(
                new_text,
                new_highlight.as_ref(),
                theme::diff_add_fg(),
                theme::diff_add_highlight_bg(),
            );
            row = row.child(
                h_flex()
                    .flex_1()
                    .child(
                        div()
                            .flex_1()
                            .pl(z(8.0))
                            .bg(theme::diff_del_bg())
                            .child(old_content),
                    )
                    .child(
                        div()
                            .flex_1()
                            .pl(z(8.0))
                            .bg(theme::diff_add_bg())
                            .child(new_content),
                    ),
            );
        }
    }

    row
}

fn render_unified_line_gutter(line: &DiffLine, selected: bool, selectable: bool) -> Div {
    // GitHub Desktop applies the selection blue to the LINE-NUMBER cells
    // (`.diff-line-gutter.diff-line-selected .diff-line-number`), not to the
    // check column and not to the row. That blue channel running down the
    // side is how a user sees at a glance what is going into the commit —
    // every line starts selected, there and here.
    let kind_gutter_bg = match line.kind {
        DiffLineKind::Added => theme::diff_add_gutter_bg(),
        DiffLineKind::Deleted => theme::diff_del_gutter_bg(),
        DiffLineKind::HunkHeader => theme::diff_hunk_bg(),
        _ => theme::diff_gutter_bg(),
    };

    let number_bg = if selected {
        theme::diff_selected_bg()
    } else {
        kind_gutter_bg
    };

    // The check column keeps the line's own kind tint; only the numbers go
    // blue, which is what keeps the add/delete signal visible beside it.
    let gutter_bg = kind_gutter_bg;
    let mark_bg = kind_gutter_bg;

    h_flex()
        .w(z(112.0))
        .flex_shrink_0()
        .min_h(z(theme::DIFF_ROW_HEIGHT))
        .items_center()
        .bg(gutter_bg)
        .text_size(z(12.0))
        .text_color(if selected {
            theme::diff_selected_fg()
        } else {
            theme::line_num_color()
        })
        .child(render_unified_selection_mark(selected, selectable, mark_bg))
        .child(
            h_flex()
                .w(z(46.0))
                .flex_shrink_0()
                .min_h(z(theme::DIFF_ROW_HEIGHT))
                .items_center()
                .justify_end()
                .pr(z(5.0))
                .bg(number_bg)
                .group_hover(DIFF_GUTTER_GROUP, |s| s.bg(theme::diff_hover_bg()))
                .whitespace_nowrap()
                .child(render_line_number_text(line.old_line)),
        )
        .child(
            h_flex()
                .w(z(46.0))
                .flex_shrink_0()
                .min_h(z(theme::DIFF_ROW_HEIGHT))
                .items_center()
                .justify_end()
                .pr(z(5.0))
                .bg(number_bg)
                .group_hover(DIFF_GUTTER_GROUP, |s| s.bg(theme::diff_hover_bg()))
                .whitespace_nowrap()
                .child(render_line_number_text(line.new_line)),
        )
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

fn render_unified_selection_mark(selected: bool, selectable: bool, bg: Hsla) -> Div {
    if !selectable {
        return div().w(z(20.0)).flex_shrink_0();
    }

    // h_flex, not div: `items_center`/`justify_center` need a flex container to
    // mean anything, and on a plain div the check drifted to the top-left
    // corner of its cell instead of sitting in the middle of the row.
    // Full row height so the selection tint on this column is a solid block
    // against the add/delete tint beside it, with no stripe above or below.
    let mark = h_flex()
        .w(z(20.0))
        .flex_shrink_0()
        .min_h(z(theme::DIFF_ROW_HEIGHT))
        .items_center()
        .justify_center()
        .bg(bg);

    if selected {
        // Bold: at 11px the plain check is a thin hairline that reads as a
        // smudge rather than a mark, and this is the control telling the user
        // what will actually go into the commit.
        mark.text_size(z(12.0))
            .text_color(theme::text_main())
            .font_weight(FontWeight::BOLD)
            .child("✓")
    } else {
        mark
    }
}

/// Render a hunk header row. The entire row is clickable to expand.
/// For Both type, the row is split into two clickable halves.
/// The "expand to end of file" affordance below the last hunk.
///
/// Extracted from the old inline loop so the virtualized list can build it on
/// demand like any other row.
fn render_expand_eof_row(
    file_path: &str,
    last_hunk: usize,
    view: Option<&Entity<GitSparkApp>>,
) -> AnyElement {
    let Some(view) = view else {
        return div().into_any_element();
    };
    let view = view.clone();
    let file_path = file_path.to_string();
    let row_h = z(theme::DIFF_ROW_HEIGHT);

    h_flex()
        .id("expand-eof")
        .w_full()
        .h(row_h)
        .flex_shrink_0()
        .bg(theme::diff_hunk_bg())
        .cursor_pointer()
        .hover(|s| s.bg(theme::accent()))
        .child(
            h_flex()
                .w(z(theme::DIFF_LINE_NUM_WIDTH * 2.0))
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .child(
                    gpui::svg()
                        .path("icons/chevrons-down.svg")
                        .size(z(16.0))
                        .text_color(theme::text_muted()),
                ),
        )
        .on_click(move |_evt, _win, cx| {
            let file_path = file_path.clone();
            view.update(cx, |app, cx| {
                app.expand_diff_context(file_path, last_hunk, DiffExpandDirection::Down);
                cx.notify();
            });
        })
        .into_any_element()
}

fn render_hunk_header(
    line: &DiffLine,
    hunk_index: usize,
    expansion_type: ExpansionType,
    file_path: &str,
    view: Option<&Entity<GitSparkApp>>,
    has_original_diff: bool,
) -> AnyElement {
    let row_h = z(theme::DIFF_ROW_HEIGHT);

    // Icon in the gutter area
    let icon_el = |icon_path: &'static str| -> Div {
        h_flex()
            .w(z(theme::DIFF_LINE_NUM_WIDTH * 2.0))
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .child(
                gpui::svg()
                    .path(icon_path)
                    .size(z(16.0))
                    .text_color(theme::text_muted()),
            )
    };

    // Hunk text (@@...@@) for the content area
    let hunk_text = div()
        .flex_1()
        .pl(z(5.0))
        .text_color(theme::text_muted())
        .child(line.content.clone());

    let hover_blue = |s: StyleRefinement| s.bg(theme::accent());

    if let Some(vh) = view {
        let content = line.content.clone();

        match expansion_type {
            ExpansionType::Up => {
                let vh_click = vh.clone();
                let fp = file_path.to_string();
                let hi = hunk_index;
                let row = h_flex()
                    .id(SharedString::from(format!("hunk-row-{hunk_index}")))
                    .w_full()
                    .h(row_h)
                    .flex_shrink_0()
                    .font_family(theme::mono_family())
                    .text_size(z(12.0))
                    .bg(theme::diff_hunk_bg())
                    .cursor_pointer()
                    .hover(hover_blue)
                    .child(icon_el("icons/chevrons-up.svg"))
                    .child(hunk_text)
                    .on_click(move |_evt, _win, cx| {
                        let fp = fp.clone();
                        vh_click.update(cx, |app, cx| {
                            app.expand_diff_context(fp, hi, DiffExpandDirection::Up);
                            cx.notify();
                        });
                    });
                return add_hunk_context_menu(row, vh, file_path, hunk_index, has_original_diff)
                    .into_any_element();
            }
            ExpansionType::Down => {
                let vh_click = vh.clone();
                let fp = file_path.to_string();
                let row = h_flex()
                    .id(SharedString::from(format!("hunk-row-{hunk_index}")))
                    .w_full()
                    .h(row_h)
                    .flex_shrink_0()
                    .font_family(theme::mono_family())
                    .text_size(z(12.0))
                    .bg(theme::diff_hunk_bg())
                    .cursor_pointer()
                    .hover(hover_blue)
                    .child(icon_el("icons/chevrons-down.svg"))
                    .child(hunk_text)
                    .on_click(move |_evt, _win, cx| {
                        let fp = fp.clone();
                        vh_click.update(cx, |app, cx| {
                            app.expand_diff_context(fp, hunk_index, DiffExpandDirection::Down);
                            cx.notify();
                        });
                    });
                return add_hunk_context_menu(row, vh, file_path, hunk_index, has_original_diff)
                    .into_any_element();
            }
            ExpansionType::Both => {
                // Top row: expand UP (fill gap above this hunk, shows vv icon)
                // Bottom row: expand DOWN (add context below, shows ^^ icon + @@ text)
                let vh_up = vh.clone();
                let vh_down = vh.clone();
                let fp_up = file_path.to_string();
                let fp_down = file_path.to_string();

                // Top row expands the PREVIOUS hunk downward (fills gap from above)
                let prev_hunk = hunk_index.saturating_sub(1);
                let expand_up_row = h_flex()
                    .id(SharedString::from(format!("hunk-up-gap-{hunk_index}")))
                    .w_full()
                    .h(row_h)
                    .flex_shrink_0()
                    .bg(theme::diff_hunk_bg())
                    .cursor_pointer()
                    .hover(hover_blue)
                    .child(icon_el("icons/chevrons-down.svg"))
                    .child(
                        div()
                            .flex_1()
                            .pl(z(5.0))
                            .font_family(theme::mono_family())
                            .text_size(z(12.0))
                            .text_color(theme::text_muted()),
                    )
                    .on_click(move |_evt, _win, cx| {
                        let fp = fp_up.clone();
                        vh_up.update(cx, |app, cx| {
                            app.expand_diff_context(fp, prev_hunk, DiffExpandDirection::Down);
                            cx.notify();
                        });
                    });

                let expand_down_row = h_flex()
                    .id(SharedString::from(format!("hunk-expand-up-{hunk_index}")))
                    .w_full()
                    .h(row_h)
                    .flex_shrink_0()
                    .font_family(theme::mono_family())
                    .text_size(z(12.0))
                    .bg(theme::diff_hunk_bg())
                    .cursor_pointer()
                    .hover(hover_blue)
                    .child(icon_el("icons/chevrons-up.svg"))
                    .child(
                        div()
                            .flex_1()
                            .pl(z(5.0))
                            .text_color(theme::text_muted())
                            .child(content),
                    )
                    .on_click(move |_evt, _win, cx| {
                        let fp = fp_down.clone();
                        vh_down.update(cx, |app, cx| {
                            app.expand_diff_context(fp, hunk_index, DiffExpandDirection::Up);
                            cx.notify();
                        });
                    });

                // `Both` renders two stacked affordances, so the menu binds
                // to the container rather than either row. Without an id this
                // is a plain Div and cannot carry one — which is why this arm
                // used to be one of the paths with no route back to collapse.
                let row = v_flex()
                    .id(SharedString::from(format!("hunk-both-{hunk_index}")))
                    .child(expand_up_row)
                    .child(expand_down_row);
                return add_hunk_context_menu(row, vh, file_path, hunk_index, has_original_diff)
                    .into_any_element();
            }
            ExpansionType::Short => {
                let vh_click = vh.clone();
                let fp = file_path.to_string();
                let row = h_flex()
                    .id(SharedString::from(format!("hunk-row-{hunk_index}")))
                    .w_full()
                    .h(row_h)
                    .flex_shrink_0()
                    .font_family(theme::mono_family())
                    .text_size(z(12.0))
                    .bg(theme::diff_hunk_bg())
                    .cursor_pointer()
                    .hover(hover_blue)
                    .child(icon_el("icons/unfold-vertical.svg"))
                    .child(hunk_text)
                    .on_click(move |_evt, _win, cx| {
                        let fp = fp.clone();
                        vh_click.update(cx, |app, cx| {
                            app.expand_diff_context(
                                fp,
                                hunk_index,
                                DiffExpandDirection::MergeWithPrevious,
                            );
                            cx.notify();
                        });
                    });
                return add_hunk_context_menu(row, vh, file_path, hunk_index, has_original_diff)
                    .into_any_element();
            }
            ExpansionType::None => {}
        }
    }

    // Fallback header: no inline expand affordance, but it must still carry
    // the context menu. `Both` and `None` used to return here bare, which was
    // the only route back to `collapse_diff` — so certain expansion sequences
    // left the diff expanded with no way in the UI to collapse it again.
    let row = h_flex()
        .id(SharedString::from(format!("hunk-plain-{hunk_index}")))
        .w_full()
        .h(row_h)
        .flex_shrink_0()
        .font_family(theme::mono_family())
        .text_size(z(12.0))
        .bg(theme::diff_hunk_bg())
        .child(
            h_flex()
                .w(z(theme::DIFF_LINE_NUM_WIDTH * 2.0))
                .flex_shrink_0(),
        )
        .child(hunk_text);

    match view {
        Some(vh) => add_hunk_context_menu(row, vh, file_path, hunk_index, has_original_diff)
            .into_any_element(),
        None => row.into_any_element(),
    }
}

/// Add right-click context menu to a hunk row.
fn add_hunk_context_menu(
    row: Stateful<Div>,
    vh: &Entity<GitSparkApp>,
    file_path: &str,
    _hunk_index: usize,
    has_original_diff: bool,
) -> gpui_component::menu::ContextMenu<Stateful<Div>> {
    let vh_ctx = vh.clone();
    let fp = file_path.to_string();
    row.context_menu(move |menu, _window, _cx| {
        let vh = vh_ctx.clone();
        let fp = fp.clone();
        if has_original_diff {
            let vh2 = vh.clone();
            let fp2 = fp.clone();
            menu.item(PopupMenuItem::new("Collapse Expanded Lines").on_click(
                move |_evt, _win, cx| {
                    let fp = fp2.clone();
                    vh2.update(cx, |app, cx| {
                        app.collapse_diff(fp);
                        cx.notify();
                    });
                },
            ))
        } else {
            let vh2 = vh.clone();
            let fp2 = fp.clone();
            menu.item(
                PopupMenuItem::new("Expand Whole File").on_click(move |_evt, _win, cx| {
                    let fp = fp2.clone();
                    vh2.update(cx, |app, cx| {
                        app.expand_diff_context(fp, 0, DiffExpandDirection::All);
                        cx.notify();
                    });
                }),
            )
        }
    })
}

/// Render a dummy EOF row with expand-down. Entire row is clickable.
/// Render the diff header bar showing the selected file path.
fn render_diff_header(
    file_path: &str,
    is_image: bool,
    view: Option<&Entity<GitSparkApp>>,
    diff_options_view: Option<&Entity<GitSparkApp>>,
) -> Div {
    let mut header = h_flex()
        .w_full()
        .h(z(theme::DIFF_HEADER_HEIGHT))
        .flex_shrink_0()
        .bg(theme::surface_bg())
        .border_b_1()
        .border_color(theme::border())
        .px(z(14.0))
        .items_center()
        .relative();

    let file_label = div()
        .flex_1()
        .min_w_0()
        .text_color(theme::text_main())
        .text_size(z(12.0))
        .child(file_path.to_string());
    header = header.child(file_label);

    if is_image && let Some(vh) = view {
        let reveal_view = vh.clone();
        let reveal_path = file_path.to_string();
        header = header.child(diff_header_button(
            "diff-image-reveal",
            crate::ui::labels::reveal_in_file_manager_menu(),
            move |_evt, _win, cx| {
                let path = reveal_path.clone();
                reveal_view.update(cx, |app, cx| {
                    app.reveal_in_finder(&path);
                    cx.notify();
                });
            },
        ));

        let open_view = vh.clone();
        let open_path = file_path.to_string();
        header = header.child(diff_header_button(
            "diff-image-open-default",
            "Open Image",
            move |_evt, _win, cx| {
                let path = open_path.clone();
                open_view.update(cx, |app, cx| {
                    app.open_with_default_program(&path);
                    cx.notify();
                });
            },
        ));
    }

    if let Some(vh) = diff_options_view {
        let menu_view = vh.clone();
        // A real 16px gear plus a caret, borderless — GitHub Desktop's
        // `.diff-options-component` renders `octicons.gear` next to
        // `octicons.triangleDown` with no border or fill. This used to be the
        // literal string "⚙ ▾" set at 11px inside a bordered pill, which drew
        // a gear a third the size of every other icon in the app.
        header = header.child(
            h_flex()
                .id("diff-options-menu")
                .flex_none()
                .h(z(24.0))
                .px(z(theme::SPACE_3))
                .mr(z(8.0))
                .gap(z(theme::SPACE_1))
                .items_center()
                .justify_center()
                .rounded(z(theme::CORNER_RADIUS_SM))
                .cursor_pointer()
                .hover(|style| style.bg(theme::toolbar_hover_bg()))
                .child(
                    Icon::new(IconName::Settings)
                        .size(z(16.0))
                        .text_color(theme::text_main()),
                )
                .child(
                    Icon::new(IconName::ChevronDown)
                        .size(z(12.0))
                        .text_color(theme::text_muted()),
                )
                .on_click(move |_evt, _win, cx| {
                    menu_view.update(cx, |app, cx| {
                        app.nav.show_diff_options_menu = !app.nav.show_diff_options_menu;
                        cx.notify();
                    });
                }),
        );
    }

    header
}

fn render_diff_options_menu(
    hide_whitespace_changes: bool,
    show_side_by_side: bool,
    view: &Entity<GitSparkApp>,
) -> Stateful<Div> {
    let unified_view = view.clone();
    let split_view = view.clone();
    let whitespace_view = view.clone();

    v_flex()
        .id("diff-options-popover")
        .absolute()
        .top(z(theme::DIFF_HEADER_HEIGHT + 4.0))
        .right(z(12.0))
        .w(z(230.0))
        .p(z(12.0))
        .gap(z(8.0))
        .rounded(z(theme::CORNER_RADIUS))
        .border_1()
        .border_color(theme::border())
        .bg(theme::surface_bg())
        .shadow_md()
        .text_size(z(12.0))
        .text_color(theme::text_main())
        .child(
            div()
                .text_size(z(13.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child("Diff Settings"),
        )
        .child(div().text_color(theme::text_muted()).child("Diff display"))
        .child(diff_option_row(
            "diff-option-unified",
            "Unified",
            !show_side_by_side,
            move |_evt, _win, cx| {
                unified_view.update(cx, |app, cx| {
                    app.nav.diff_options.show_side_by_side = false;
                    app.nav.show_diff_options_menu = false;
                    cx.notify();
                });
            },
        ))
        .child(diff_option_row(
            "diff-option-side-by-side",
            "Split",
            show_side_by_side,
            move |_evt, _win, cx| {
                split_view.update(cx, |app, cx| {
                    app.nav.diff_options.show_side_by_side = true;
                    app.nav.show_diff_options_menu = false;
                    cx.notify();
                });
            },
        ))
        .child(div().text_color(theme::text_muted()).child("Whitespace"))
        .child(diff_option_row(
            "diff-option-hide-whitespace",
            "Hide Whitespace Changes",
            hide_whitespace_changes,
            move |_evt, _win, cx| {
                whitespace_view.update(cx, |app, cx| {
                    app.toggle_hide_whitespace_changes(cx);
                    app.nav.show_diff_options_menu = false;
                });
            },
        ))
        .child(
            div()
                .text_size(z(11.0))
                .text_color(theme::text_muted())
                .child("Interacting with individual lines or hunks will be disabled while hiding whitespace."),
        )
}

fn diff_option_row(
    id: &'static str,
    label: &'static str,
    selected: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    h_flex()
        .id(id)
        .h(z(24.0))
        .items_center()
        .gap(z(8.0))
        .cursor_pointer()
        .text_color(theme::text_main())
        .child(if selected { "●" } else { "○" })
        .child(label)
        .on_click(on_click)
}

fn diff_header_button(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    h_flex()
        .id(id)
        .flex_none()
        .h(z(24.0))
        .px(z(10.0))
        .mr(z(8.0))
        .items_center()
        .justify_center()
        .rounded(z(theme::CORNER_RADIUS_SM))
        .border_1()
        .border_color(theme::border())
        .bg(theme::surface_bg())
        .text_size(z(11.0))
        .text_color(theme::text_muted())
        .cursor_pointer()
        .hover(|style| style.bg(theme::toolbar_hover_bg()))
        .child(label)
        .on_click(on_click)
}

/// Render the empty state when no file is selected.
fn render_empty_state() -> Div {
    div()
        .w_full()
        .h_full()
        .flex_1()
        .bg(theme::bg())
        .items_center()
        .justify_center()
        .child(
            div()
                .text_color(theme::text_muted())
                .text_size(z(14.0))
                .child("Select a file to view its diff"),
        )
}

/// Render the workspace diff viewer.
///
/// Fills the remaining horizontal space (flex-1) and displays either
/// a unified diff for the selected file or a placeholder message.
#[allow(clippy::too_many_arguments)]
pub fn render_workspace(
    repo_path: Option<&Path>,
    selected_file: Option<&str>,
    diff: Option<&DiffEntry>,
    hide_whitespace_changes: bool,
    show_side_by_side: bool,
    show_diff_options_menu: bool,
    excluded_lines: &HashSet<DiffLineSelection>,
    view: Option<&Entity<GitSparkApp>>,
    diff_list: &DiffListHandle,
    tab_size: u8,
) -> Div {
    let Some(file_path) = selected_file else {
        return render_empty_state();
    };
    let diff_options_view = view.filter(|_| {
        diff.is_some_and(|entry| {
            !entry.is_binary
                && !entry.is_image
                && !entry.is_submodule
                && !entry.diff.trim().is_empty()
        })
    });

    let diff_content: AnyElement = match diff {
        Some(entry) if entry.is_image => div()
            .w_full()
            .flex_1()
            .flex()
            .flex_col()
            .min_h_0()
            .child(render_image_diff_panel(repo_path, file_path))
            .into_any_element(),
        Some(entry) if entry.is_submodule => div()
            .w_full()
            .flex_1()
            .items_center()
            .justify_center()
            .child(render_submodule_diff_panel(file_path, entry, view))
            .into_any_element(),
        Some(entry) if entry.is_binary => div()
            .w_full()
            .flex_1()
            .items_center()
            .justify_center()
            .child(render_binary_diff_panel(file_path, view))
            .into_any_element(),
        Some(entry) if entry.diff.trim().is_empty() => div()
            .w_full()
            .flex_1()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_color(theme::text_muted())
                    .text_size(z(14.0))
                    .child("No diff text available."),
            )
            .into_any_element(),
        Some(entry) => {
            let parsed = parse_diff(&entry.diff);
            let visible_lines = visible_diff_lines(&parsed, hide_whitespace_changes);

            // Collect hunk boundaries for expansion type computation
            let hunk_bounds: Vec<HunkBounds> = entry
                .diff
                .lines()
                .filter_map(|l| parse_hunk_header(l))
                .collect();
            let file_line_count = entry.file_contents.as_ref().map(|c| c.len()).unwrap_or(0);
            // Expansion reads from the working-tree file. Without it the
            // action is a no-op, so the affordances must not appear either.
            let can_expand = entry.file_contents.is_some();
            let expansion_types =
                compute_expansion_types(&hunk_bounds, file_line_count, can_expand);

            if show_side_by_side {
                // Virtualized too; `list` is its own scroller, so there is no
                // outer overflow container here.
                div()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .child(crate::ui::side_by_side_diff::render_side_by_side_diff(
                        file_path,
                        &entry.diff,
                        hide_whitespace_changes,
                        excluded_lines,
                        view,
                        diff_list,
                    ))
                    .into_any_element()
            } else {
                // Flatten every row up front, then virtualize. Building the
                // element for each of these cost ~15ms on a 4000-row diff, on
                // EVERY frame — including every frame of a scroll — which is
                // the whole frame budget at 60fps. Now only the visible
                // handful are built.
                let mut rows: Vec<DiffRow> = Vec::with_capacity(visible_lines.len() + 2);
                let mut hunk_index = 0usize;
                for line in &visible_lines {
                    if matches!(line.kind, DiffLineKind::HunkHeader) {
                        rows.push(DiffRow::Hunk {
                            line: line.clone(),
                            index: hunk_index,
                            expansion: expansion_types
                                .get(hunk_index)
                                .copied()
                                .unwrap_or(ExpansionType::None),
                        });
                        hunk_index += 1;
                    } else {
                        rows.push(DiffRow::Line(line.clone()));
                    }
                }

                if meaningful_diff_line_count(&visible_lines) == 0 && hide_whitespace_changes {
                    rows.push(DiffRow::WhitespaceHidden);
                }

                if let Some(last_bounds) = hunk_bounds.last() {
                    let last_end = last_bounds.new_start + last_bounds.new_count;
                    if last_end <= file_line_count
                        && entry.file_contents.is_some()
                        && view.is_some()
                    {
                        rows.push(DiffRow::ExpandEof {
                            last_hunk: hunk_index.saturating_sub(1),
                        });
                    }
                }

                let key = format!(
                    "{file_path}|{}|{}|{}|{}",
                    rows.len(),
                    hide_whitespace_changes,
                    entry.original_diff.is_some(),
                    tab_size
                );
                let state = diff_list.sync(key, rows.len());

                let rows = Rc::new(rows);
                let owned_path = file_path.to_string();
                let owned_excluded = excluded_lines.clone();
                let owned_view = view.cloned();
                let has_original = entry.original_diff.is_some();

                let list_element = list(state, move |ix, _window, _cx| {
                    let Some(row) = rows.get(ix) else {
                        return div().into_any_element();
                    };
                    match row {
                        DiffRow::Hunk {
                            line,
                            index,
                            expansion,
                        } => render_hunk_header(
                            line,
                            *index,
                            *expansion,
                            &owned_path,
                            owned_view.as_ref(),
                            has_original,
                        ),
                        DiffRow::Line(line) => render_diff_line(
                            &owned_path,
                            line,
                            owned_view.as_ref(),
                            &owned_excluded,
                            hide_whitespace_changes,
                            tab_size,
                        )
                        .into_any_element(),
                        DiffRow::WhitespaceHidden => h_flex()
                            .id("diff-whitespace-hidden-empty")
                            .w_full()
                            .h(z(theme::DIFF_ROW_HEIGHT * 2.0))
                            .items_center()
                            .justify_center()
                            .text_size(z(12.0))
                            .text_color(theme::text_muted())
                            .child("Only whitespace changes hidden.")
                            .into_any_element(),
                        DiffRow::ExpandEof { last_hunk } => {
                            render_expand_eof_row(&owned_path, *last_hunk, owned_view.as_ref())
                        }
                    }
                })
                .with_sizing_behavior(ListSizingBehavior::Auto);

                div()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .child(list_element.w_full().h_full())
                    .into_any_element()
            }
        }
        None => div()
            .w_full()
            .flex_1()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_color(theme::text_muted())
                    .text_size(z(14.0))
                    .child("No diff available for this file."),
            )
            .into_any_element(),
    };

    let mut workspace = v_flex()
        .w_full()
        .flex_1()
        .min_h_0()
        .items_start()
        .bg(theme::bg())
        .relative()
        .child(render_diff_header(
            file_path,
            diff.is_some_and(|entry| entry.is_image),
            view,
            diff_options_view,
        ))
        .child(diff_content);

    if show_diff_options_menu && let Some(vh) = diff_options_view {
        workspace = workspace.child(render_diff_options_menu(
            hide_whitespace_changes,
            show_side_by_side,
            vh,
        ));
    }

    workspace
}

pub fn visible_diff_line_count(diff_text: &str, hide_whitespace_changes: bool) -> usize {
    let parsed = parse_diff(diff_text);
    meaningful_diff_line_count(&visible_diff_lines(&parsed, hide_whitespace_changes))
}

pub(crate) fn selectable_diff_line_targets(
    file_path: &str,
    diff_text: &str,
    hide_whitespace_changes: bool,
) -> Vec<DiffLineSelection> {
    if hide_whitespace_changes {
        return Vec::new();
    }

    let parsed = parse_diff(diff_text);
    visible_diff_lines(&parsed, false)
        .iter()
        .filter_map(|line| diff_line_selection_target(file_path, line))
        .collect()
}

fn visible_diff_lines(lines: &[DiffLine], hide_whitespace_changes: bool) -> Vec<DiffLine> {
    if !hide_whitespace_changes {
        return lines.to_vec();
    }

    let mut result = Vec::with_capacity(lines.len());
    let mut index = 0usize;
    while index < lines.len() {
        let line = &lines[index];
        if matches!(line.kind, DiffLineKind::Deleted) {
            let del_start = index;
            while index < lines.len() && matches!(lines[index].kind, DiffLineKind::Deleted) {
                index += 1;
            }
            let add_start = index;
            while index < lines.len() && matches!(lines[index].kind, DiffLineKind::Added) {
                index += 1;
            }
            let del_block = &lines[del_start..add_start];
            let add_block = &lines[add_start..index];
            if del_block.len() == add_block.len()
                && del_block.iter().zip(add_block.iter()).all(|(old, new)| {
                    whitespace_normalized(&old.content) == whitespace_normalized(&new.content)
                })
            {
                continue;
            }
            result.extend_from_slice(del_block);
            result.extend_from_slice(add_block);
        } else {
            result.push(line.clone());
            index += 1;
        }
    }

    result
}

fn meaningful_diff_line_count(lines: &[DiffLine]) -> usize {
    lines
        .iter()
        .filter(|line| !matches!(line.kind, DiffLineKind::HunkHeader))
        .count()
}

fn whitespace_normalized(value: &str) -> String {
    value.chars().filter(|ch| !ch.is_whitespace()).collect()
}

#[cfg(test)]
mod tests {
    use super::{ExpansionType, HunkBounds, compute_expansion_types, expand_tabs};

    #[test]
    fn leaves_tab_free_content_untouched() {
        assert_eq!(expand_tabs("no tabs here", 4), "no tabs here");
    }

    #[test]
    fn expands_a_leading_tab_to_the_full_width() {
        assert_eq!(expand_tabs("\tfoo", 4), "    foo");
    }

    #[test]
    fn expands_to_the_next_tab_stop_not_a_flat_width() {
        // A tab after 2 columns of text only needs 2 more spaces to reach
        // the next 4-column stop, not a flat 4 — matches real terminals and
        // editors, not a naive "\t" -> "    " replacement.
        assert_eq!(expand_tabs("ab\tcd", 4), "ab  cd");
    }

    #[test]
    fn respects_a_custom_tab_size() {
        assert_eq!(expand_tabs("\tx", 2), "  x");
        assert_eq!(expand_tabs("\tx", 8), "        x");
    }

    #[test]
    fn treats_a_zero_tab_size_as_one() {
        assert_eq!(expand_tabs("\tx", 0), " x");
    }

    fn hunk(new_start: usize, new_count: usize) -> HunkBounds {
        HunkBounds {
            new_start,
            new_count,
            old_start: new_start,
            old_count: new_count,
        }
    }

    #[test]
    fn offers_expansion_when_file_contents_are_available() {
        // A hunk starting well past line 1 has room to expand upwards.
        let hunks = vec![hunk(40, 5)];
        let types = compute_expansion_types(&hunks, 200, true);
        assert_eq!(types.len(), 1);
        assert_ne!(
            types[0],
            ExpansionType::None,
            "a mid-file hunk with contents should offer an expand control"
        );
    }

    #[test]
    fn offers_nothing_when_file_contents_are_missing() {
        // Deleted or unreadable file: the expand action returns early, so the
        // control would be visibly dead. Same hunks as above.
        let hunks = vec![hunk(40, 5), hunk(120, 5)];
        let types = compute_expansion_types(&hunks, 200, false);
        assert_eq!(
            types,
            vec![ExpansionType::None, ExpansionType::None],
            "no contents to expand from means no affordance"
        );
    }

    #[test]
    fn returns_one_entry_per_hunk_in_both_modes() {
        // The caller indexes this by hunk position, so a short vec would
        // silently mis-assign controls to the wrong hunks.
        let hunks = vec![hunk(10, 2), hunk(60, 2), hunk(140, 2)];
        assert_eq!(
            compute_expansion_types(&hunks, 300, true).len(),
            hunks.len()
        );
        assert_eq!(
            compute_expansion_types(&hunks, 300, false).len(),
            hunks.len()
        );
    }

    #[test]
    fn handles_an_empty_hunk_list() {
        assert!(compute_expansion_types(&[], 0, true).is_empty());
        assert!(compute_expansion_types(&[], 0, false).is_empty());
    }
}
