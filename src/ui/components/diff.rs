use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use crate::ui::components::syntax::{SYNTAX_SET, get_syntax, syntax_theme};
use crate::ui::theme::{
    ACCENT, ACCENT_MUTED, BORDER, DIFF_ADD_BG, DIFF_ADD_FG, DIFF_DEL_BG, DIFF_DEL_FG,
    DIFF_HUNK_BG, SURFACE_BG_MUTED, TEXT_MAIN, TEXT_MUTED,
};
use eframe::egui::{self, Color32, Stroke, TextWrapMode, Vec2};
use egui_phosphor::regular as icons;
use once_cell::sync::Lazy;
use regex::Regex;
use syntect::easy::HighlightLines;
use syntect::highlighting::Style;

const MAX_SYNTAX_BYTES: usize = 256 * 1024;
const MAX_INTRA_LINE_CHARS: usize = 1024;
const ROW_HEIGHT: f32 = 20.0;
const MIN_GUTTER_DIGITS: usize = 3;
const MAX_CACHE_ENTRIES: usize = 32;
const CODE_FONT_SIZE: f32 = 12.5;
const SELECTION_GUTTER_WIDTH: f32 = 18.0;
const LINE_CHECK_WIDTH: f32 = 20.0;
const GUTTER_GAP: f32 = 4.0;

static HUNK_HEADER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@(?: .*)?$")
        .expect("invalid hunk header regex")
});

static DIFF_CACHE: Lazy<Mutex<HashMap<DiffCacheKey, CachedDiff>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CodeLineKind {
    Context,
    Added,
    Deleted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum DiffSide {
    Before,
    After,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CharRange {
    start: usize,
    end: usize,
}

#[derive(Clone, Copy, Debug)]
struct SyntaxSpan {
    start: usize,
    end: usize,
    style: Style,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HunkHeader {
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
}

#[derive(Clone, Debug)]
struct ParsedCodeLine {
    kind: CodeLineKind,
    text: String,
    old_line_num: Option<usize>,
    new_line_num: Option<usize>,
    no_newline: bool,
    syntax_index: usize,
}

#[derive(Clone, Debug)]
struct ParsedHunk {
    header_text: String,
    lines: Vec<ParsedCodeLine>,
}

#[derive(Clone, Debug)]
enum ParsedItem {
    Meta(String),
    Hunk(ParsedHunk),
}

#[derive(Clone, Debug)]
struct ParsedDiff {
    items: Vec<ParsedItem>,
    syntax_inputs: Vec<String>,
    max_line_number: usize,
}

#[derive(Clone, Debug)]
enum DiffRow {
    Meta {
        text: String,
        prominent: bool,
    },
    Hunk {
        text: String,
    },
    Context {
        old_line_num: usize,
        new_line_num: usize,
        text: String,
        syntax_index: usize,
        no_newline: bool,
    },
    Added {
        new_line_num: usize,
        text: String,
        syntax_index: usize,
        no_newline: bool,
    },
    Deleted {
        old_line_num: usize,
        text: String,
        syntax_index: usize,
        no_newline: bool,
    },
    Modified {
        old_line_num: usize,
        old_text: String,
        old_syntax_index: usize,
        old_highlight: Option<CharRange>,
        old_no_newline: bool,
        new_line_num: usize,
        new_text: String,
        new_syntax_index: usize,
        new_highlight: Option<CharRange>,
        new_no_newline: bool,
    },
}

#[derive(Clone, Debug)]
struct DiffDocument {
    rows: Vec<DiffRow>,
    syntax_inputs: Vec<String>,
    gutter_digits: usize,
    selection_scope: u64,
    row_group_ids: Vec<Option<usize>>,
    row_groups: Vec<DiffRowGroup>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct LineSelectionTarget {
    row_index: usize,
    side: DiffSide,
}

#[derive(Clone, Debug)]
struct DiffRowGroup {
    start_row: usize,
    end_row: usize,
    targets: Vec<LineSelectionTarget>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GroupSelectionState {
    All,
    Partial,
    None,
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct DiffCacheKey {
    file_path: String,
    diff_hash: u64,
    diff_len: usize,
}

enum SyntaxTokensState {
    Disabled,
    Pending(mpsc::Receiver<Vec<Vec<SyntaxSpan>>>),
    Ready(Arc<Vec<Vec<SyntaxSpan>>>),
}

struct CachedDiff {
    document: Arc<DiffDocument>,
    syntax_tokens: SyntaxTokensState,
}

struct PreparedDiff {
    document: Arc<DiffDocument>,
    syntax_tokens: Option<Arc<Vec<Vec<SyntaxSpan>>>>,
}

impl DiffCacheKey {
    fn new(file_path: &str, diff_text: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        diff_text.hash(&mut hasher);
        Self {
            file_path: file_path.to_string(),
            diff_hash: hasher.finish(),
            diff_len: diff_text.len(),
        }
    }
}

impl CachedDiff {
    fn new(diff_text: &str, file_path: &str) -> Self {
        let document = Arc::new(build_document(diff_text, file_path));
        let syntax_tokens =
            if diff_text.len() > MAX_SYNTAX_BYTES || document.syntax_inputs.is_empty() {
                SyntaxTokensState::Disabled
            } else {
                let file_path = file_path.to_string();
                let syntax_inputs = document.syntax_inputs.clone();
                let (tx, rx) = mpsc::channel();
                thread::spawn(move || {
                    let tokens = tokenize_syntax_lines(&file_path, syntax_inputs);
                    let _ = tx.send(tokens);
                });
                SyntaxTokensState::Pending(rx)
            };

        Self {
            document,
            syntax_tokens,
        }
    }

    fn prepare(&mut self, ctx: &egui::Context) -> PreparedDiff {
        let syntax_tokens = match &mut self.syntax_tokens {
            SyntaxTokensState::Disabled => None,
            SyntaxTokensState::Ready(tokens) => Some(tokens.clone()),
            SyntaxTokensState::Pending(receiver) => match receiver.try_recv() {
                Ok(tokens) => {
                    let tokens = Arc::new(tokens);
                    self.syntax_tokens = SyntaxTokensState::Ready(tokens.clone());
                    Some(tokens)
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(16));
                    None
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.syntax_tokens = SyntaxTokensState::Disabled;
                    None
                }
            },
        };

        PreparedDiff {
            document: self.document.clone(),
            syntax_tokens,
        }
    }
}

pub fn render_diff_text(ui: &mut egui::Ui, diff_text: &str, file_path: &str) {
    let prepared = prepare_diff(ui.ctx(), diff_text, file_path);
    let document = prepared.document;
    let syntax_tokens = prepared.syntax_tokens;

    ui.spacing_mut().item_spacing = Vec2::ZERO;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(ui, ROW_HEIGHT, document.rows.len(), |ui, row_range| {
            let visible_start = row_range.start;
            for row_index in row_range {
                render_row(
                    ui,
                    &document,
                    row_index,
                    syntax_tokens.as_deref(),
                    visible_start,
                );
            }
        });
}

fn prepare_diff(ctx: &egui::Context, diff_text: &str, file_path: &str) -> PreparedDiff {
    let key = DiffCacheKey::new(file_path, diff_text);
    let mut cache = DIFF_CACHE.lock().expect("diff cache poisoned");

    if cache.len() >= MAX_CACHE_ENTRIES && !cache.contains_key(&key) {
        cache.clear();
    }

    cache
        .entry(key)
        .or_insert_with(|| CachedDiff::new(diff_text, file_path))
        .prepare(ctx)
}

fn build_document(diff_text: &str, file_path: &str) -> DiffDocument {
    let parsed = parse_diff(diff_text);
    let rows = build_rows(&parsed.items);
    let (row_group_ids, row_groups) = build_row_groups(&rows);
    DiffDocument {
        rows,
        syntax_inputs: parsed.syntax_inputs,
        gutter_digits: digits_for_line_number(parsed.max_line_number),
        selection_scope: selection_scope(file_path, diff_text),
        row_group_ids,
        row_groups,
    }
}

fn parse_diff(diff_text: &str) -> ParsedDiff {
    let mut items = Vec::new();
    let mut syntax_inputs = Vec::new();
    let mut current_hunk: Option<ParsedHunk> = None;
    let mut old_line_num = 0usize;
    let mut new_line_num = 0usize;
    let mut max_line_number = 0usize;

    for raw_line in diff_text.lines() {
        if let Some(header) = parse_hunk_header(raw_line) {
            if let Some(hunk) = current_hunk.take() {
                items.push(ParsedItem::Hunk(hunk));
            }

            old_line_num = header.old_start;
            new_line_num = header.new_start;
            current_hunk = Some(ParsedHunk {
                header_text: raw_line.to_string(),
                lines: Vec::new(),
            });
            continue;
        }

        if raw_line.starts_with('\\') {
            if let Some(hunk) = current_hunk.as_mut() {
                if let Some(previous) = hunk.lines.last_mut() {
                    previous.no_newline = true;
                    continue;
                }
            }

            items.push(ParsedItem::Meta(raw_line.to_string()));
            continue;
        }

        if let Some(hunk) = current_hunk.as_mut() {
            let Some(prefix) = raw_line.chars().next() else {
                items.push(ParsedItem::Meta(String::new()));
                continue;
            };

            let text = raw_line.get(1..).unwrap_or_default().to_string();
            let syntax_index = syntax_inputs.len();
            let (kind, old_num, new_num) = match prefix {
                ' ' => {
                    let old_num = old_line_num;
                    let new_num = new_line_num;
                    old_line_num += 1;
                    new_line_num += 1;
                    (CodeLineKind::Context, Some(old_num), Some(new_num))
                }
                '+' => {
                    let new_num = new_line_num;
                    new_line_num += 1;
                    (CodeLineKind::Added, None, Some(new_num))
                }
                '-' => {
                    let old_num = old_line_num;
                    old_line_num += 1;
                    (CodeLineKind::Deleted, Some(old_num), None)
                }
                _ => {
                    let finished_hunk = current_hunk.take().expect("current hunk missing");
                    items.push(ParsedItem::Hunk(finished_hunk));
                    items.push(ParsedItem::Meta(raw_line.to_string()));
                    continue;
                }
            };

            syntax_inputs.push(text.clone());
            max_line_number = max_line_number
                .max(old_num.unwrap_or(0))
                .max(new_num.unwrap_or(0));
            hunk.lines.push(ParsedCodeLine {
                kind,
                text,
                old_line_num: old_num,
                new_line_num: new_num,
                no_newline: false,
                syntax_index,
            });
            continue;
        }

        items.push(ParsedItem::Meta(raw_line.to_string()));
    }

    if let Some(hunk) = current_hunk.take() {
        items.push(ParsedItem::Hunk(hunk));
    }

    ParsedDiff {
        items,
        syntax_inputs,
        max_line_number,
    }
}

fn build_rows(items: &[ParsedItem]) -> Vec<DiffRow> {
    let mut rows = Vec::new();

    for item in items {
        match item {
            ParsedItem::Meta(text) => {
                if should_render_meta(text) {
                    rows.push(DiffRow::Meta {
                        text: text.trim_start_matches("### ").to_string(),
                        prominent: text.starts_with("### "),
                    });
                }
            }
            ParsedItem::Hunk(hunk) => {
                rows.push(DiffRow::Hunk {
                    text: hunk.header_text.clone(),
                });

                let mut index = 0usize;
                while index < hunk.lines.len() {
                    match hunk.lines[index].kind {
                        CodeLineKind::Context => {
                            let line = &hunk.lines[index];
                            rows.push(DiffRow::Context {
                                old_line_num: line.old_line_num.unwrap_or(0),
                                new_line_num: line.new_line_num.unwrap_or(0),
                                text: line.text.clone(),
                                syntax_index: line.syntax_index,
                                no_newline: line.no_newline,
                            });
                            index += 1;
                        }
                        CodeLineKind::Deleted => {
                            let deleted_start = index;
                            while index < hunk.lines.len()
                                && hunk.lines[index].kind == CodeLineKind::Deleted
                            {
                                index += 1;
                            }

                            let added_start = index;
                            while index < hunk.lines.len()
                                && hunk.lines[index].kind == CodeLineKind::Added
                            {
                                index += 1;
                            }

                            let deleted_block = &hunk.lines[deleted_start..added_start];
                            let added_block = &hunk.lines[added_start..index];

                            if !deleted_block.is_empty() && deleted_block.len() == added_block.len()
                            {
                                for (deleted, added) in deleted_block.iter().zip(added_block.iter())
                                {
                                    let (old_highlight, new_highlight) =
                                        find_changed_ranges(&deleted.text, &added.text);
                                    rows.push(DiffRow::Modified {
                                        old_line_num: deleted.old_line_num.unwrap_or(0),
                                        old_text: deleted.text.clone(),
                                        old_syntax_index: deleted.syntax_index,
                                        old_highlight,
                                        old_no_newline: deleted.no_newline,
                                        new_line_num: added.new_line_num.unwrap_or(0),
                                        new_text: added.text.clone(),
                                        new_syntax_index: added.syntax_index,
                                        new_highlight,
                                        new_no_newline: added.no_newline,
                                    });
                                }
                            } else {
                                for deleted in deleted_block {
                                    rows.push(DiffRow::Deleted {
                                        old_line_num: deleted.old_line_num.unwrap_or(0),
                                        text: deleted.text.clone(),
                                        syntax_index: deleted.syntax_index,
                                        no_newline: deleted.no_newline,
                                    });
                                }

                                for added in added_block {
                                    rows.push(DiffRow::Added {
                                        new_line_num: added.new_line_num.unwrap_or(0),
                                        text: added.text.clone(),
                                        syntax_index: added.syntax_index,
                                        no_newline: added.no_newline,
                                    });
                                }
                            }
                        }
                        CodeLineKind::Added => {
                            let line = &hunk.lines[index];
                            rows.push(DiffRow::Added {
                                new_line_num: line.new_line_num.unwrap_or(0),
                                text: line.text.clone(),
                                syntax_index: line.syntax_index,
                                no_newline: line.no_newline,
                            });
                            index += 1;
                        }
                    }
                }
            }
        }
    }

    rows
}

fn build_row_groups(rows: &[DiffRow]) -> (Vec<Option<usize>>, Vec<DiffRowGroup>) {
    let mut row_group_ids = vec![None; rows.len()];
    let mut row_groups = Vec::new();
    let mut row_index = 0usize;

    while row_index < rows.len() {
        if !is_changed_row(&rows[row_index]) {
            row_index += 1;
            continue;
        }

        let group_id = row_groups.len();
        let start_row = row_index;
        let mut targets = Vec::new();

        while row_index < rows.len() && is_changed_row(&rows[row_index]) {
            row_group_ids[row_index] = Some(group_id);
            targets.extend(row_selection_targets(row_index, &rows[row_index]));
            row_index += 1;
        }

        row_groups.push(DiffRowGroup {
            start_row,
            end_row: row_index.saturating_sub(1),
            targets,
        });
    }

    (row_group_ids, row_groups)
}

fn is_changed_row(row: &DiffRow) -> bool {
    matches!(
        row,
        DiffRow::Added { .. } | DiffRow::Deleted { .. } | DiffRow::Modified { .. }
    )
}

fn row_selection_targets(row_index: usize, row: &DiffRow) -> Vec<LineSelectionTarget> {
    match row {
        DiffRow::Added { .. } => vec![LineSelectionTarget {
            row_index,
            side: DiffSide::After,
        }],
        DiffRow::Deleted { .. } => vec![LineSelectionTarget {
            row_index,
            side: DiffSide::Before,
        }],
        DiffRow::Modified { .. } => vec![
            LineSelectionTarget {
                row_index,
                side: DiffSide::Before,
            },
            LineSelectionTarget {
                row_index,
                side: DiffSide::After,
            },
        ],
        _ => Vec::new(),
    }
}

fn row_line_targets(
    row_index: usize,
    row: &DiffRow,
) -> (Option<LineSelectionTarget>, Option<LineSelectionTarget>) {
    match row {
        DiffRow::Added { .. } => (
            None,
            Some(LineSelectionTarget {
                row_index,
                side: DiffSide::After,
            }),
        ),
        DiffRow::Deleted { .. } => (
            Some(LineSelectionTarget {
                row_index,
                side: DiffSide::Before,
            }),
            None,
        ),
        DiffRow::Modified { .. } => (
            Some(LineSelectionTarget {
                row_index,
                side: DiffSide::Before,
            }),
            Some(LineSelectionTarget {
                row_index,
                side: DiffSide::After,
            }),
        ),
        _ => (None, None),
    }
}

fn line_selection_id(
    ui: &egui::Ui,
    selection_scope: u64,
    target: LineSelectionTarget,
) -> egui::Id {
    ui.id().with((
        "diff-line-selection",
        selection_scope,
        target.row_index,
        target.side,
    ))
}

fn get_line_selected(ui: &mut egui::Ui, selection_scope: u64, target: LineSelectionTarget) -> bool {
    let selection_id = line_selection_id(ui, selection_scope, target);
    ui.data_mut(|data| data.get_temp::<bool>(selection_id).unwrap_or(true))
}

fn set_line_selected(
    ui: &mut egui::Ui,
    selection_scope: u64,
    target: LineSelectionTarget,
    selected: bool,
) {
    let selection_id = line_selection_id(ui, selection_scope, target);
    ui.data_mut(|data| data.insert_temp(selection_id, selected));
}

fn get_group_selection_state(
    ui: &mut egui::Ui,
    selection_scope: u64,
    group: &DiffRowGroup,
) -> GroupSelectionState {
    let selected_count = group
        .targets
        .iter()
        .filter(|target| get_line_selected(ui, selection_scope, **target))
        .count();

    if selected_count == 0 {
        GroupSelectionState::None
    } else if selected_count == group.targets.len() {
        GroupSelectionState::All
    } else {
        GroupSelectionState::Partial
    }
}

fn set_group_selection(
    ui: &mut egui::Ui,
    selection_scope: u64,
    group: &DiffRowGroup,
    selected: bool,
) {
    for target in &group.targets {
        set_line_selected(ui, selection_scope, *target, selected);
    }
}

fn line_cell_base_bg(row: &DiffRow, side: DiffSide) -> Color32 {
    match row {
        DiffRow::Added { .. } => match side {
            DiffSide::Before => SURFACE_BG_MUTED,
            DiffSide::After => DIFF_ADD_BG,
        },
        DiffRow::Deleted { .. } => match side {
            DiffSide::Before => DIFF_DEL_BG,
            DiffSide::After => SURFACE_BG_MUTED,
        },
        DiffRow::Modified { .. } => match side {
            DiffSide::Before => DIFF_DEL_BG,
            DiffSide::After => DIFF_ADD_BG,
        },
        DiffRow::Hunk { .. } => DIFF_HUNK_BG,
        _ => Color32::TRANSPARENT,
    }
}

fn parse_hunk_header(line: &str) -> Option<HunkHeader> {
    let captures = HUNK_HEADER_RE.captures(line)?;
    Some(HunkHeader {
        old_start: captures.get(1)?.as_str().parse().ok()?,
        old_count: captures
            .get(2)
            .and_then(|value| value.as_str().parse().ok())
            .unwrap_or(1),
        new_start: captures.get(3)?.as_str().parse().ok()?,
        new_count: captures
            .get(4)
            .and_then(|value| value.as_str().parse().ok())
            .unwrap_or(1),
    })
}

fn should_render_meta(text: &str) -> bool {
    if text.starts_with("diff --git")
        || text.starts_with("index ")
        || text.starts_with("--- ")
        || text.starts_with("+++ ")
    {
        return false;
    }

    true
}

fn find_changed_ranges(old_line: &str, new_line: &str) -> (Option<CharRange>, Option<CharRange>) {
    let old_len = old_line.chars().count();
    let new_len = new_line.chars().count();

    if old_len > MAX_INTRA_LINE_CHARS || new_len > MAX_INTRA_LINE_CHARS {
        return (None, None);
    }

    let old_chars: Vec<char> = old_line.chars().collect();
    let new_chars: Vec<char> = new_line.chars().collect();

    let mut prefix = 0usize;
    while prefix < old_chars.len()
        && prefix < new_chars.len()
        && old_chars[prefix] == new_chars[prefix]
    {
        prefix += 1;
    }

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

fn tokenize_syntax_lines(file_path: &str, lines: Vec<String>) -> Vec<Vec<SyntaxSpan>> {
    let first_line = lines
        .iter()
        .find(|line| !line.trim().is_empty())
        .map(String::as_str);
    let syntax = get_syntax(file_path, first_line);
    let mut highlighter = HighlightLines::new(syntax, syntax_theme());

    lines
        .iter()
        .map(|line| {
            let mut char_index = 0usize;
            highlighter
                .highlight_line(line, &SYNTAX_SET)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|(style, token_text)| {
                    let token_len = token_text.chars().count();
                    if token_len == 0 {
                        return None;
                    }

                    let span = SyntaxSpan {
                        start: char_index,
                        end: char_index + token_len,
                        style,
                    };
                    char_index += token_len;
                    Some(span)
                })
                .collect()
        })
        .collect()
}

fn render_row(
    ui: &mut egui::Ui,
    document: &DiffDocument,
    row_index: usize,
    syntax_tokens: Option<&Vec<Vec<SyntaxSpan>>>,
    visible_start: usize,
) {
    let row = &document.rows[row_index];
    let width = ui.available_width();
    let (row_rect, _) = ui.allocate_exact_size(Vec2::new(width, ROW_HEIGHT), egui::Sense::hover());
    let gutter = layout_gutter(row_rect, document.gutter_digits);
    let (before_target, after_target) = row_line_targets(row_index, row);
    let group = document
        .row_group_ids
        .get(row_index)
        .and_then(|group_id| group_id.and_then(|group_id| document.row_groups.get(group_id)));
    let show_group_handle = group.is_some_and(|group| group.targets.len() > 1);

    if show_group_handle {
        let group_id = group.expect("group missing");
        let group_response = ui.interact(
            gutter.group_rect,
            ui.id()
                .with(("diff-group-selection", document.selection_scope, group_id.start_row)),
            egui::Sense::click(),
        );
        if group_response.hovered() {
            ui.output_mut(|output| output.cursor_icon = egui::CursorIcon::PointingHand);
        }
        if group_response.clicked() {
            let next_selected =
                get_group_selection_state(ui, document.selection_scope, group_id)
                    != GroupSelectionState::All;
            set_group_selection(ui, document.selection_scope, group_id, next_selected);
        }
    }

    if let Some(target) = before_target {
        let response = ui.interact(
            gutter.before_rect,
            line_selection_id(ui, document.selection_scope, target),
            egui::Sense::click(),
        );
        if response.hovered() {
            ui.output_mut(|output| output.cursor_icon = egui::CursorIcon::PointingHand);
        }
        if response.clicked() {
            let selected = get_line_selected(ui, document.selection_scope, target);
            set_line_selected(ui, document.selection_scope, target, !selected);
        }
    }

    if let Some(target) = after_target {
        let response = ui.interact(
            gutter.after_rect,
            line_selection_id(ui, document.selection_scope, target),
            egui::Sense::click(),
        );
        if response.hovered() {
            ui.output_mut(|output| output.cursor_icon = egui::CursorIcon::PointingHand);
        }
        if response.clicked() {
            let selected = get_line_selected(ui, document.selection_scope, target);
            set_line_selected(ui, document.selection_scope, target, !selected);
        }
    }

    let before_selected =
        before_target.map(|target| get_line_selected(ui, document.selection_scope, target));
    let after_selected =
        after_target.map(|target| get_line_selected(ui, document.selection_scope, target));
    let group_state = group
        .map(|group| get_group_selection_state(ui, document.selection_scope, group))
        .unwrap_or(GroupSelectionState::None);
    let show_group_icon = group.is_some_and(|group| {
        show_group_handle
            && (row_index == group.start_row
                || (group.start_row < visible_start && row_index == visible_start))
    });
    let painter = ui.painter();

    match row {
        DiffRow::Meta { text, prominent } => {
            painter.rect_filled(row_rect, 0.0, Color32::TRANSPARENT);
            render_full_width_text(
                ui,
                row_rect,
                if *prominent { TEXT_MAIN } else { TEXT_MUTED },
                text,
            );
        }
        DiffRow::Hunk { text } => {
            painter.rect_filled(row_rect, 0.0, DIFF_HUNK_BG);
            painter.rect_filled(gutter.content_rect, 0.0, DIFF_HUNK_BG);
            painter.hline(
                row_rect.x_range(),
                row_rect.bottom(),
                Stroke::new(1.0, BORDER),
            );
            render_gutter(
                painter,
                &gutter,
                LineCellState::inactive(None, line_cell_base_bg(row, DiffSide::Before)),
                LineCellState::inactive(None, line_cell_base_bg(row, DiffSide::After)),
                GroupHandleState::inactive(),
            );
            render_hunk_text(ui, row_rect, document.gutter_digits, text);
        }
        DiffRow::Context {
            old_line_num,
            new_line_num,
            text,
            syntax_index,
            no_newline,
        } => {
            painter.rect_filled(row_rect, 0.0, Color32::TRANSPARENT);
            render_gutter(
                painter,
                &gutter,
                LineCellState::inactive(
                    Some(*old_line_num),
                    line_cell_base_bg(row, DiffSide::Before),
                ),
                LineCellState::inactive(
                    Some(*new_line_num),
                    line_cell_base_bg(row, DiffSide::After),
                ),
                GroupHandleState::inactive(),
            );
            render_code_cell(
                ui,
                gutter.content_rect,
                ' ',
                text,
                TEXT_MUTED,
                syntax_tokens.and_then(|tokens| tokens.get(*syntax_index).map(Vec::as_slice)),
                None,
                None,
                *no_newline,
            );
        }
        DiffRow::Added {
            new_line_num,
            text,
            syntax_index,
            no_newline,
        } => {
            painter.rect_filled(row_rect, 0.0, DIFF_ADD_BG);
            render_gutter(
                painter,
                &gutter,
                LineCellState::inactive(None, line_cell_base_bg(row, DiffSide::Before)),
                LineCellState::selectable(
                    Some(*new_line_num),
                    after_selected.unwrap_or(true),
                    line_cell_base_bg(row, DiffSide::After),
                ),
                GroupHandleState::new(group_state, show_group_icon),
            );
            render_code_cell(
                ui,
                gutter.content_rect,
                '+',
                text,
                DIFF_ADD_FG,
                syntax_tokens.and_then(|tokens| tokens.get(*syntax_index).map(Vec::as_slice)),
                None,
                None,
                *no_newline,
            );
        }
        DiffRow::Deleted {
            old_line_num,
            text,
            syntax_index,
            no_newline,
        } => {
            painter.rect_filled(row_rect, 0.0, DIFF_DEL_BG);
            render_gutter(
                painter,
                &gutter,
                LineCellState::selectable(
                    Some(*old_line_num),
                    before_selected.unwrap_or(true),
                    line_cell_base_bg(row, DiffSide::Before),
                ),
                LineCellState::inactive(None, line_cell_base_bg(row, DiffSide::After)),
                GroupHandleState::new(group_state, show_group_icon),
            );
            render_code_cell(
                ui,
                gutter.content_rect,
                '-',
                text,
                DIFF_DEL_FG,
                syntax_tokens.and_then(|tokens| tokens.get(*syntax_index).map(Vec::as_slice)),
                None,
                None,
                *no_newline,
            );
        }
        DiffRow::Modified {
            old_line_num,
            old_text,
            old_syntax_index,
            old_highlight,
            old_no_newline,
            new_line_num,
            new_text,
            new_syntax_index,
            new_highlight,
            new_no_newline,
        } => {
            render_gutter(
                painter,
                &gutter,
                LineCellState::selectable(
                    Some(*old_line_num),
                    before_selected.unwrap_or(true),
                    line_cell_base_bg(row, DiffSide::Before),
                ),
                LineCellState::selectable(
                    Some(*new_line_num),
                    after_selected.unwrap_or(true),
                    line_cell_base_bg(row, DiffSide::After),
                ),
                GroupHandleState::new(group_state, show_group_icon),
            );
            let divider_x = gutter.content_rect.center().x;
            let divider_stroke = Stroke::new(1.0, BORDER);
            let left_rect = egui::Rect::from_min_max(
                gutter.content_rect.min,
                egui::pos2(divider_x - 2.0, gutter.content_rect.max.y),
            );
            let right_rect = egui::Rect::from_min_max(
                egui::pos2(divider_x + 2.0, gutter.content_rect.min.y),
                gutter.content_rect.max,
            );

            painter.rect_filled(left_rect, 0.0, DIFF_DEL_BG);
            painter.rect_filled(right_rect, 0.0, DIFF_ADD_BG);
            painter.vline(divider_x, row_rect.y_range(), divider_stroke);

            render_code_cell(
                ui,
                left_rect,
                '-',
                old_text,
                DIFF_DEL_FG,
                syntax_tokens.and_then(|tokens| tokens.get(*old_syntax_index).map(Vec::as_slice)),
                *old_highlight,
                Some(Color32::from_rgba_premultiplied(218, 54, 51, 150)),
                *old_no_newline,
            );
            render_code_cell(
                ui,
                right_rect,
                '+',
                new_text,
                DIFF_ADD_FG,
                syntax_tokens.and_then(|tokens| tokens.get(*new_syntax_index).map(Vec::as_slice)),
                *new_highlight,
                Some(Color32::from_rgba_premultiplied(3, 201, 105, 150)),
                *new_no_newline,
            );
        }
    }
}

struct GutterLayout {
    gutter_rect: egui::Rect,
    before_rect: egui::Rect,
    after_rect: egui::Rect,
    group_rect: egui::Rect,
    content_rect: egui::Rect,
}

fn layout_gutter(row_rect: egui::Rect, gutter_digits: usize) -> GutterLayout {
    let gutter_width = total_gutter_width(gutter_digits);
    let gutter_rect = egui::Rect::from_min_max(
        row_rect.min,
        egui::pos2(row_rect.left() + gutter_width, row_rect.bottom()),
    );
    let line_cell_width = line_number_cell_width(gutter_digits);
    let before_rect = egui::Rect::from_min_max(
        gutter_rect.min,
        egui::pos2(
            gutter_rect.left() + line_cell_width,
            gutter_rect.bottom(),
        ),
    );
    let after_rect = egui::Rect::from_min_max(
        egui::pos2(before_rect.right() + GUTTER_GAP, gutter_rect.top()),
        egui::pos2(
            before_rect.right() + GUTTER_GAP + line_cell_width,
            gutter_rect.bottom(),
        ),
    );
    let group_rect = egui::Rect::from_min_max(
        egui::pos2(after_rect.right() + GUTTER_GAP, gutter_rect.top()),
        egui::pos2(
            after_rect.right() + GUTTER_GAP + SELECTION_GUTTER_WIDTH,
            gutter_rect.bottom(),
        ),
    );

    GutterLayout {
        gutter_rect,
        before_rect,
        after_rect,
        group_rect,
        content_rect: egui::Rect::from_min_max(
            egui::pos2(gutter_rect.right(), row_rect.top()),
            row_rect.max,
        ),
    }
}

fn line_number_cell_width(gutter_digits: usize) -> f32 {
    line_number_width(gutter_digits) + LINE_CHECK_WIDTH
}

struct LineCellState {
    line_number: Option<usize>,
    selectable: bool,
    selected: bool,
    base_bg: Color32,
}

impl LineCellState {
    fn inactive(line_number: Option<usize>, base_bg: Color32) -> Self {
        Self {
            line_number,
            selectable: false,
            selected: false,
            base_bg,
        }
    }

    fn selectable(line_number: Option<usize>, selected: bool, base_bg: Color32) -> Self {
        Self {
            line_number,
            selectable: true,
            selected,
            base_bg,
        }
    }
}

struct GroupHandleState {
    selection_state: GroupSelectionState,
    show_icon: bool,
}

impl GroupHandleState {
    fn new(selection_state: GroupSelectionState, show_icon: bool) -> Self {
        Self {
            selection_state,
            show_icon,
        }
    }

    fn inactive() -> Self {
        Self {
            selection_state: GroupSelectionState::None,
            show_icon: false,
        }
    }
}

fn render_gutter(
    painter: &egui::Painter,
    gutter: &GutterLayout,
    before: LineCellState,
    after: LineCellState,
    group: GroupHandleState,
) {
    painter.rect_filled(gutter.gutter_rect, 0.0, SURFACE_BG_MUTED);
    render_line_cell(painter, gutter.before_rect, before);
    render_line_cell(painter, gutter.after_rect, after);
    render_group_handle(painter, gutter.group_rect, group);
    painter.vline(
        gutter.gutter_rect.right(),
        gutter.gutter_rect.y_range(),
        Stroke::new(1.0, BORDER),
    );
    painter.vline(
        gutter.before_rect.right(),
        gutter.before_rect.y_range(),
        Stroke::new(1.0, Color32::from_black_alpha(50)),
    );
    painter.vline(
        gutter.after_rect.right(),
        gutter.after_rect.y_range(),
        Stroke::new(1.0, Color32::from_black_alpha(50)),
    );
    painter.vline(
        gutter.group_rect.right(),
        gutter.group_rect.y_range(),
        Stroke::new(1.0, Color32::from_black_alpha(50)),
    );
}

fn render_line_cell(painter: &egui::Painter, rect: egui::Rect, cell: LineCellState) {
    let background = if cell.selectable && cell.selected {
        selection_fill(false)
    } else {
        cell.base_bg
    };
    painter.rect_filled(rect, 0.0, background);

    if cell.selectable && cell.selected {
        let check_rect = egui::Rect::from_min_max(
            rect.min,
            egui::pos2(rect.left() + LINE_CHECK_WIDTH, rect.bottom()),
        );
        painter.text(
            check_rect.center(),
            egui::Align2::CENTER_CENTER,
            icons::CHECK,
            egui::FontId::proportional(12.0),
            Color32::WHITE,
        );
    }

    if let Some(line_number) = cell.line_number {
        painter.text(
            rect.right_center() - Vec2::new(8.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            line_number.to_string(),
            egui::FontId::monospace(11.0),
            if cell.selectable && cell.selected {
                Color32::WHITE
            } else {
                TEXT_MUTED
            },
        );
    }
}

fn render_group_handle(painter: &egui::Painter, rect: egui::Rect, group: GroupHandleState) {
    let background = if group.selection_state != GroupSelectionState::None {
        selection_fill(false)
    } else {
        SURFACE_BG_MUTED
    };
    painter.rect_filled(rect, 0.0, background);

    if group.show_icon {
        let icon = match group.selection_state {
            GroupSelectionState::All => icons::CHECK,
            GroupSelectionState::Partial => icons::MINUS,
            GroupSelectionState::None => "",
        };

        if !icon.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                icon,
                egui::FontId::proportional(12.0),
                Color32::WHITE,
            );
        }
    }
}

fn render_hunk_text(ui: &mut egui::Ui, row_rect: egui::Rect, gutter_digits: usize, text: &str) {
    let content_rect = egui::Rect::from_min_max(
        egui::pos2(
            row_rect.left() + total_gutter_width(gutter_digits) + 8.0,
            row_rect.top(),
        ),
        row_rect.max,
    );
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(content_rect), |ui| {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            let mut job = egui::text::LayoutJob::default();
            job.append(
                text,
                0.0,
                egui::text::TextFormat::simple(egui::FontId::monospace(CODE_FONT_SIZE), ACCENT),
            );
            ui.add(egui::Label::new(job).wrap_mode(TextWrapMode::Extend));
        });
    });
}

fn selection_fill(hovered: bool) -> Color32 {
    if hovered { ACCENT } else { ACCENT_MUTED }
}

fn render_full_width_text(ui: &mut egui::Ui, row_rect: egui::Rect, color: Color32, text: &str) {
    let content_rect = egui::Rect::from_min_max(
        egui::pos2(row_rect.left() + 8.0, row_rect.top()),
        row_rect.max,
    );
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(content_rect), |ui| {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            let mut job = egui::text::LayoutJob::default();
            job.append(
                text,
                0.0,
                egui::text::TextFormat::simple(egui::FontId::monospace(CODE_FONT_SIZE), color),
            );
            ui.add(egui::Label::new(job).wrap_mode(TextWrapMode::Extend));
        });
    });
}

fn render_code_cell(
    ui: &mut egui::Ui,
    cell_rect: egui::Rect,
    prefix: char,
    text: &str,
    base_color: Color32,
    syntax_spans: Option<&[SyntaxSpan]>,
    diff_highlight: Option<CharRange>,
    diff_highlight_bg: Option<Color32>,
    no_newline: bool,
) {
    let inner_rect = egui::Rect::from_min_max(
        egui::pos2(cell_rect.left() + 8.0, cell_rect.top()),
        egui::pos2(cell_rect.right() - 8.0, cell_rect.bottom()),
    );
    let job = build_code_layout_job(
        text,
        prefix,
        base_color,
        syntax_spans,
        diff_highlight,
        diff_highlight_bg,
        no_newline,
    );

    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(inner_rect), |ui| {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.add(egui::Label::new(job).wrap_mode(TextWrapMode::Extend));
        });
    });
}

fn build_code_layout_job(
    text: &str,
    prefix: char,
    base_color: Color32,
    syntax_spans: Option<&[SyntaxSpan]>,
    diff_highlight: Option<CharRange>,
    diff_highlight_bg: Option<Color32>,
    no_newline: bool,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let base_format =
        egui::text::TextFormat::simple(egui::FontId::monospace(CODE_FONT_SIZE), base_color);
    let prefix_text = prefix.to_string();
    job.append(&prefix_text, 0.0, base_format.clone());

    let char_len = text.chars().count();
    if char_len == 0 {
        if no_newline {
            append_no_newline_marker(&mut job);
        }
        return job;
    }

    let mut boundaries = vec![0usize, char_len];
    if let Some(highlight) = diff_highlight {
        boundaries.push(highlight.start.min(char_len));
        boundaries.push(highlight.end.min(char_len));
    }

    if let Some(spans) = syntax_spans {
        for span in spans {
            boundaries.push(span.start.min(char_len));
            boundaries.push(span.end.min(char_len));
        }
    }

    boundaries.sort_unstable();
    boundaries.dedup();

    let offsets = char_offsets(text);
    for window in boundaries.windows(2) {
        let start = window[0];
        let end = window[1];
        if start >= end {
            continue;
        }

        let mut format = base_format.clone();
        if let Some(style) = style_for_char_range(start, syntax_spans) {
            format.color =
                Color32::from_rgb(style.foreground.r, style.foreground.g, style.foreground.b);
        }

        if diff_highlight.is_some_and(|highlight| start >= highlight.start && end <= highlight.end)
        {
            format.background = diff_highlight_bg.unwrap_or(Color32::TRANSPARENT);
        }

        job.append(slice_by_chars(text, &offsets, start, end), 0.0, format);
    }

    if no_newline {
        append_no_newline_marker(&mut job);
    }

    job
}

fn append_no_newline_marker(job: &mut egui::text::LayoutJob) {
    job.append(
        " [no newline]",
        0.0,
        egui::text::TextFormat::simple(egui::FontId::monospace(CODE_FONT_SIZE), TEXT_MUTED),
    );
}

fn style_for_char_range(start: usize, syntax_spans: Option<&[SyntaxSpan]>) -> Option<Style> {
    syntax_spans?
        .iter()
        .find(|span| start >= span.start && start < span.end)
        .map(|span| span.style)
}

fn char_offsets(text: &str) -> Vec<usize> {
    let mut offsets = text.char_indices().map(|(idx, _)| idx).collect::<Vec<_>>();
    offsets.push(text.len());
    offsets
}

fn slice_by_chars<'a>(text: &'a str, offsets: &[usize], start: usize, end: usize) -> &'a str {
    &text[offsets[start]..offsets[end]]
}

fn selection_scope(file_path: &str, diff_text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    file_path.hash(&mut hasher);
    diff_text.hash(&mut hasher);
    hasher.finish()
}

fn digits_for_line_number(max_line_number: usize) -> usize {
    max_line_number
        .max(10_usize.pow((MIN_GUTTER_DIGITS - 1) as u32))
        .to_string()
        .len()
}

fn line_number_width(gutter_digits: usize) -> f32 {
    gutter_digits.max(MIN_GUTTER_DIGITS) as f32 * 10.0 + 5.0
}

fn total_gutter_width(gutter_digits: usize) -> f32 {
    let number_width = line_number_cell_width(gutter_digits);
    SELECTION_GUTTER_WIDTH + number_width * 2.0 + GUTTER_GAP * 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hunk_headers_with_optional_counts() {
        assert_eq!(
            parse_hunk_header("@@ -12,4 +20,5 @@ fn main()"),
            Some(HunkHeader {
                old_start: 12,
                old_count: 4,
                new_start: 20,
                new_count: 5,
            })
        );
        assert_eq!(
            parse_hunk_header("@@ -1 +3 @@"),
            Some(HunkHeader {
                old_start: 1,
                old_count: 1,
                new_start: 3,
                new_count: 1,
            })
        );
    }

    #[test]
    fn pairs_balanced_delete_add_blocks_as_modified_rows() {
        let document = build_document(
            "@@ -1,2 +1,2 @@\n-const foo = 42;\n-let left = 1;\n+const bar = 42;\n+let right = 1;\n",
            "src/lib.rs",
        );

        assert!(matches!(document.rows[0], DiffRow::Hunk { .. }));

        match &document.rows[1] {
            DiffRow::Modified {
                old_highlight,
                new_highlight,
                ..
            } => {
                assert_eq!(*old_highlight, Some(CharRange { start: 6, end: 9 }));
                assert_eq!(*new_highlight, Some(CharRange { start: 6, end: 9 }));
            }
            row => panic!("expected modified row, got {row:?}"),
        }

        assert!(matches!(document.rows[2], DiffRow::Modified { .. }));
    }

    #[test]
    fn leaves_unbalanced_blocks_as_added_and_deleted_rows() {
        let document = build_document(
            "@@ -1,2 +1,3 @@\n-old one\n-old two\n+new one\n+new two\n+new three\n",
            "src/lib.rs",
        );

        assert!(matches!(document.rows[1], DiffRow::Deleted { .. }));
        assert!(matches!(document.rows[2], DiffRow::Deleted { .. }));
        assert!(matches!(document.rows[3], DiffRow::Added { .. }));
        assert!(matches!(document.rows[4], DiffRow::Added { .. }));
        assert!(matches!(document.rows[5], DiffRow::Added { .. }));
    }

    #[test]
    fn flags_the_previous_line_for_missing_newline_markers() {
        let parsed = parse_diff("@@ -1 +1 @@\n-foo\n\\ No newline at end of file\n+bar\n");

        let ParsedItem::Hunk(hunk) = &parsed.items[0] else {
            panic!("expected hunk");
        };

        assert_eq!(hunk.lines.len(), 2);
        assert!(hunk.lines[0].no_newline);
        assert!(!hunk.lines[1].no_newline);
    }

    #[test]
    fn skips_git_file_headers_but_keeps_section_labels() {
        let document = build_document(
            "### Staged\ndiff --git a/src/lib.rs b/src/lib.rs\nindex 123..456 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n",
            "src/lib.rs",
        );

        assert!(matches!(
            &document.rows[0],
            DiffRow::Meta { text, prominent: true } if text == "Staged"
        ));
        assert!(matches!(document.rows[1], DiffRow::Hunk { .. }));
    }
}
