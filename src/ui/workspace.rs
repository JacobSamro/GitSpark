use gpui::*;
use gpui_component::menu::{ContextMenuExt, PopupMenuItem};
use gpui_component::{h_flex, v_flex};

use crate::models::DiffEntry;
use crate::ui::app::{DiffExpandDirection, GitSparkApp};
use gpui_component::scroll::ScrollableElement;

use crate::ui::theme;
use crate::ui::theme::z;

// --- Constants ---

#[allow(dead_code)]
const MAX_INTRA_LINE_CHARS: usize = 1024;
const EXPAND_STEP: usize = 20;

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
fn compute_expansion_types(hunks: &[HunkBounds], _file_line_count: usize) -> Vec<ExpansionType> {
    if hunks.is_empty() {
        return vec![];
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

/// Brighter highlight background for intra-line changed characters.
fn diff_add_highlight_bg() -> Hsla {
    gpui::rgb(0x1a5c2e).into()
}

fn diff_del_highlight_bg() -> Hsla {
    gpui::rgb(0x6e2b25).into()
}

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

/// Render a single diff line as a horizontal flex row.
fn render_diff_line(line: &DiffLine) -> Div {
    // Format line number strings. Hunk headers show no numbers.
    let old_num_str = match line.old_line {
        Some(n) => format!("{n}"),
        None => String::new(),
    };
    let new_num_str = match line.new_line {
        Some(n) => format!("{n}"),
        None => String::new(),
    };

    let mut row = h_flex()
        .w_full()
        .min_h(z(theme::DIFF_ROW_HEIGHT))
        .flex_shrink_0()
        .font_family("monospace")
        .text_size(z(12.0))
        .py(z(2.0)); // match GitHub Desktop: padding 2px 0

    // Old line number gutter
    row = row.child(
        div()
            .w(z(theme::DIFF_LINE_NUM_WIDTH))
            .flex_shrink_0()
            .text_color(theme::line_num_color())
            .px(z(4.0))
            .child(old_num_str),
    );

    // New line number gutter
    row = row.child(
        div()
            .w(z(theme::DIFF_LINE_NUM_WIDTH))
            .flex_shrink_0()
            .text_color(theme::line_num_color())
            .px(z(4.0))
            .child(new_num_str),
    );

    // Content — varies by line kind
    match &line.kind {
        DiffLineKind::Added => {
            row = row.bg(theme::diff_add_bg()).child(
                div()
                    .flex_1()
                    .pl(z(5.0))
                    .text_color(theme::diff_add_fg())
                    .child(line.content.clone()),
            );
        }
        DiffLineKind::Deleted => {
            row = row.bg(theme::diff_del_bg()).child(
                div()
                    .flex_1()
                    .pl(z(5.0))
                    .text_color(theme::diff_del_fg())
                    .child(line.content.clone()),
            );
        }
        DiffLineKind::HunkHeader => {
            row = row.bg(theme::diff_hunk_bg()).child(
                div()
                    .flex_1()
                    .pl(z(5.0))
                    .text_color(theme::text_muted())
                    .child(line.content.clone()),
            );
        }
        DiffLineKind::Context => {
            row = row.child(
                div()
                    .flex_1()
                    .pl(z(5.0))
                    .text_color(theme::text_main()) // --diff-text-color: var(--text-color)
                    .child(line.content.clone()),
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
                diff_del_highlight_bg(),
            );
            let new_text = line.new_content.as_deref().unwrap_or("");
            let new_content = render_highlighted_text(
                new_text,
                new_highlight.as_ref(),
                theme::diff_add_fg(),
                diff_add_highlight_bg(),
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

/// Render a hunk header row. The entire row is clickable to expand.
/// For Both type, the row is split into two clickable halves.
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

    let hover_blue = |s: StyleRefinement| s.bg(gpui::rgb(0x0969da));

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
                    .font_family("monospace")
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
                    .font_family("monospace")
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
                            .font_family("monospace")
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
                    .font_family("monospace")
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

                return v_flex()
                    .child(expand_up_row)
                    .child(expand_down_row)
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
                    .font_family("monospace")
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

    // Fallback: non-interactive hunk header
    h_flex()
        .w_full()
        .h(row_h)
        .flex_shrink_0()
        .font_family("monospace")
        .text_size(z(12.0))
        .bg(theme::diff_hunk_bg())
        .child(
            h_flex()
                .w(z(theme::DIFF_LINE_NUM_WIDTH * 2.0))
                .flex_shrink_0(),
        )
        .child(hunk_text)
        .into_any_element()
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
    hide_whitespace_changes: bool,
    show_side_by_side: bool,
    view: Option<&Entity<GitSparkApp>>,
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
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_color(theme::text_main())
                .text_size(z(12.0))
                .child(file_path.to_string()),
        );

    if let Some(vh) = view {
        let vh_split = vh.clone();
        header = header.child(
            h_flex()
                .id("diff-option-side-by-side")
                .h(z(24.0))
                .px(z(10.0))
                .mr(z(8.0))
                .items_center()
                .justify_center()
                .rounded(z(theme::CORNER_RADIUS_SM))
                .border_1()
                .border_color(if show_side_by_side {
                    theme::accent()
                } else {
                    theme::border()
                })
                .bg(if show_side_by_side {
                    theme::toolbar_hover_bg()
                } else {
                    theme::surface_bg()
                })
                .text_size(z(11.0))
                .text_color(if show_side_by_side {
                    theme::text_main()
                } else {
                    theme::text_muted()
                })
                .cursor_pointer()
                .hover(|style| style.bg(theme::toolbar_hover_bg()))
                .child("Split")
                .on_click(move |_evt, _win, cx| {
                    vh_split.update(cx, |app, cx| {
                        app.toggle_side_by_side_diff(cx);
                    });
                }),
        );

        let vh_whitespace = vh.clone();
        header = header.child(
            h_flex()
                .id("diff-option-hide-whitespace")
                .h(z(24.0))
                .px(z(10.0))
                .items_center()
                .justify_center()
                .rounded(z(theme::CORNER_RADIUS_SM))
                .border_1()
                .border_color(if hide_whitespace_changes {
                    theme::accent()
                } else {
                    theme::border()
                })
                .bg(if hide_whitespace_changes {
                    theme::toolbar_hover_bg()
                } else {
                    theme::surface_bg()
                })
                .text_size(z(11.0))
                .text_color(if hide_whitespace_changes {
                    theme::text_main()
                } else {
                    theme::text_muted()
                })
                .cursor_pointer()
                .hover(|style| style.bg(theme::toolbar_hover_bg()))
                .child("Hide whitespace")
                .on_click(move |_evt, _win, cx| {
                    vh_whitespace.update(cx, |app, cx| {
                        app.toggle_hide_whitespace_changes(cx);
                    });
                }),
        );
    }

    header
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
pub fn render_workspace(
    selected_file: Option<&str>,
    diff: Option<&DiffEntry>,
    hide_whitespace_changes: bool,
    show_side_by_side: bool,
    view: Option<&Entity<GitSparkApp>>,
) -> Div {
    let Some(file_path) = selected_file else {
        return render_empty_state();
    };
    let diff_options_view = view
        .filter(|_| diff.is_some_and(|entry| !entry.is_binary && !entry.diff.trim().is_empty()));

    let diff_content: AnyElement = match diff {
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
            let expansion_types = compute_expansion_types(&hunk_bounds, file_line_count);

            let mut scroll_content = if show_side_by_side {
                crate::ui::side_by_side_diff::render_side_by_side_diff(
                    &entry.diff,
                    hide_whitespace_changes,
                )
            } else {
                div().flex().flex_col().w_full()
            };
            let mut hunk_index = 0usize;
            if !show_side_by_side {
                for line in &visible_lines {
                    if matches!(line.kind, DiffLineKind::HunkHeader) {
                        let exp_type = expansion_types
                            .get(hunk_index)
                            .copied()
                            .unwrap_or(ExpansionType::None);
                        scroll_content = scroll_content.child(render_hunk_header(
                            line,
                            hunk_index,
                            exp_type,
                            file_path,
                            view,
                            entry.original_diff.is_some(),
                        ));
                        hunk_index += 1;
                    } else {
                        scroll_content = scroll_content.child(render_diff_line(line));
                    }
                }
            }

            if meaningful_diff_line_count(&visible_lines) == 0 && hide_whitespace_changes {
                scroll_content = scroll_content.child(
                    h_flex()
                        .id("diff-whitespace-hidden-empty")
                        .w_full()
                        .h(z(theme::DIFF_ROW_HEIGHT * 2.0))
                        .items_center()
                        .justify_center()
                        .text_size(z(12.0))
                        .text_color(theme::text_muted())
                        .child("Only whitespace changes hidden."),
                );
            }

            // Expand-down row after the last hunk if file has more lines
            if !show_side_by_side && let Some(last_bounds) = hunk_bounds.last() {
                let last_end = last_bounds.new_start + last_bounds.new_count;
                if last_end <= file_line_count && entry.file_contents.is_some() {
                    if let Some(vh) = view {
                        let vh_click = vh.clone();
                        let fp = file_path.to_string();
                        let last_hi = hunk_index.saturating_sub(1);
                        let row_h = z(theme::DIFF_ROW_HEIGHT);
                        let hover_blue = |s: StyleRefinement| s.bg(gpui::rgb(0x0969da));
                        scroll_content = scroll_content.child(
                            h_flex()
                                .id("expand-eof")
                                .w_full()
                                .h(row_h)
                                .flex_shrink_0()
                                .bg(theme::diff_hunk_bg())
                                .cursor_pointer()
                                .hover(hover_blue)
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
                                    let fp = fp.clone();
                                    vh_click.update(cx, |app, cx| {
                                        app.expand_diff_context(
                                            fp,
                                            last_hi,
                                            DiffExpandDirection::Down,
                                        );
                                        cx.notify();
                                    });
                                })
                                .into_any_element(),
                        );
                    }
                }
            }

            div()
                .w_full()
                .flex_1()
                .min_h_0()
                .child(
                    div()
                        .id("diff-scroll")
                        .size_full()
                        .overflow_y_scrollbar()
                        .child(scroll_content.pb(z(300.0))),
                )
                .into_any_element()
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

    v_flex()
        .w_full()
        .flex_1()
        .min_h_0()
        .items_start()
        .bg(theme::bg())
        .child(render_diff_header(
            file_path,
            hide_whitespace_changes,
            show_side_by_side,
            diff_options_view,
        ))
        .child(diff_content)
}

pub fn visible_diff_line_count(diff_text: &str, hide_whitespace_changes: bool) -> usize {
    let parsed = parse_diff(diff_text);
    meaningful_diff_line_count(&visible_diff_lines(&parsed, hide_whitespace_changes))
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

fn render_binary_diff_panel(file_path: &str, view: Option<&Entity<GitSparkApp>>) -> Div {
    let mut panel = v_flex()
        .items_center()
        .gap(z(10.0))
        .text_size(z(14.0))
        .text_color(theme::text_muted())
        .child("This binary file has changed.");

    if let Some(vh) = view {
        let vh_click = vh.clone();
        let path = file_path.to_string();
        panel = panel.child(
            h_flex()
                .id("diff-binary-open-default")
                .h(z(28.0))
                .px(z(12.0))
                .items_center()
                .justify_center()
                .rounded(z(theme::CORNER_RADIUS_SM))
                .border_1()
                .border_color(theme::border())
                .bg(theme::surface_bg())
                .text_size(z(12.0))
                .text_color(theme::text_main())
                .cursor_pointer()
                .hover(|style| style.bg(theme::toolbar_hover_bg()))
                .child("Open file in external program")
                .on_click(move |_evt, _win, cx| {
                    let path = path.clone();
                    vh_click.update(cx, |app, cx| {
                        app.open_with_default_program(&path);
                        cx.notify();
                    });
                }),
        );
    }

    panel
}
