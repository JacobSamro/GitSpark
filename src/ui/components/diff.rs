use crate::ui::components::syntax::{SYNTAX_SET, THEME_SET};
use crate::ui::theme::{
    ACCENT, DIFF_ADD_BG, DIFF_ADD_FG, DIFF_DEL_BG, DIFF_DEL_FG, DIFF_HUNK_BG, TEXT_MAIN, TEXT_MUTED,
};
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use egui_phosphor::regular as icons;
use syntect::easy::HighlightLines;
use syntect::highlighting::Style;

#[derive(Clone, PartialEq)]
enum LineType {
    Context,
    Added,
    Deleted,
    HunkHeader,
    Empty,
}

struct ParsedLine<'a> {
    line_type: LineType,
    text: &'a str,
    old_line_num: Option<usize>,
    new_line_num: Option<usize>,
    char_highlight: Option<(usize, usize)>, // (start_idx, end_idx) for character highlighting
    syntax_tokens: Vec<(syntect::highlighting::Style, &'a str)>,
}

fn find_highlight(
    old_line: &str,
    new_line: &str,
) -> (Option<(usize, usize)>, Option<(usize, usize)>) {
    let old_chars: Vec<char> = old_line.chars().collect();
    let new_chars: Vec<char> = new_line.chars().collect();

    let mut prefix = 0;
    while prefix < old_chars.len()
        && prefix < new_chars.len()
        && old_chars[prefix] == new_chars[prefix]
    {
        prefix += 1;
    }

    let mut old_suffix = 0;
    let mut new_suffix = 0;
    while old_suffix < old_chars.len() - prefix
        && new_suffix < new_chars.len() - prefix
        && old_chars[old_chars.len() - 1 - old_suffix]
            == new_chars[new_chars.len() - 1 - new_suffix]
    {
        old_suffix += 1;
        new_suffix += 1;
    }

    let old_end = old_chars.len() - old_suffix;
    let new_end = new_chars.len() - new_suffix;

    let old_hl = if prefix < old_end {
        Some((prefix, old_end))
    } else {
        None
    };
    let new_hl = if prefix < new_end {
        Some((prefix, new_end))
    } else {
        None
    };

    (old_hl, new_hl)
}

pub fn render_diff_text(ui: &mut egui::Ui, diff_text: &str, file_path: &str) {
    let mut old_line = 0;
    let mut new_line = 0;
    let mut in_hunk = false;

    let syntax = crate::ui::components::syntax::get_syntax(file_path)
        .unwrap_or_else(|| crate::ui::components::syntax::SYNTAX_SET.find_syntax_plain_text());
    let theme = &crate::ui::components::syntax::THEME_SET.themes["base16-ocean.dark"];
    let mut highlighter = syntect::easy::HighlightLines::new(syntax, theme);

    let mut parsed_lines = Vec::new();

    for line in diff_text.lines() {
        let is_hunk_header = line.starts_with("@@ ");
        if is_hunk_header {
            if let Some(hunk_info) = line.split("@@").nth(1) {
                let parts: Vec<&str> = hunk_info.trim().split(' ').collect();
                if parts.len() >= 2 {
                    old_line = parts[0]
                        .trim_start_matches('-')
                        .split(',')
                        .next()
                        .unwrap_or("0")
                        .parse()
                        .unwrap_or(0);
                    new_line = parts[1]
                        .trim_start_matches('+')
                        .split(',')
                        .next()
                        .unwrap_or("0")
                        .parse()
                        .unwrap_or(0);
                    in_hunk = true;
                }
            }
        } else if line.starts_with("diff --git")
            || line.starts_with("index ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
        {
            in_hunk = false;
        }

        let line_type = if is_hunk_header {
            LineType::HunkHeader
        } else if in_hunk && line.starts_with('+') {
            LineType::Added
        } else if in_hunk && line.starts_with('-') {
            LineType::Deleted
        } else if in_hunk && line.starts_with('\\') {
            LineType::Empty
        } else if in_hunk {
            LineType::Context
        } else {
            LineType::Empty
        };

        let mut current_old_num = None;
        let mut current_new_num = None;

        if in_hunk && !is_hunk_header {
            if line_type == LineType::Added {
                current_new_num = Some(new_line);
                new_line += 1;
            } else if line_type == LineType::Deleted {
                current_old_num = Some(old_line);
                old_line += 1;
            } else if line_type == LineType::Context {
                current_old_num = Some(old_line);
                current_new_num = Some(new_line);
                old_line += 1;
                new_line += 1;
            }
        }

        let mut syntax_tokens = Vec::new();
        if line_type == LineType::Added
            || line_type == LineType::Context
            || line_type == LineType::Deleted
        {
            let syntax_text = line
                .strip_prefix('+')
                .or_else(|| line.strip_prefix('-'))
                .or_else(|| line.strip_prefix(' '))
                .unwrap_or(line);
            syntax_tokens = highlighter
                .highlight_line(syntax_text, &crate::ui::components::syntax::SYNTAX_SET)
                .unwrap_or_default();
        }

        parsed_lines.push(ParsedLine {
            line_type,
            text: line,
            old_line_num: current_old_num,
            new_line_num: current_new_num,
            char_highlight: None,
            syntax_tokens,
        });
    }

    // Pass 2: Find pairs for character highlighting
    let mut i = 0;
    while i < parsed_lines.len() {
        if parsed_lines[i].line_type == LineType::Deleted
            && i + 1 < parsed_lines.len()
            && parsed_lines[i + 1].line_type == LineType::Added
        {
            let old_text = parsed_lines[i]
                .text
                .strip_prefix('-')
                .unwrap_or(parsed_lines[i].text);
            let new_text = parsed_lines[i + 1]
                .text
                .strip_prefix('+')
                .unwrap_or(parsed_lines[i + 1].text);

            let (old_hl, new_hl) = find_highlight(old_text, new_text);

            // Adjust indices because text includes the +/- prefix
            parsed_lines[i].char_highlight = old_hl.map(|(s, e)| (s + 1, e + 1));
            parsed_lines[i + 1].char_highlight = new_hl.map(|(s, e)| (s + 1, e + 1));

            i += 2;
        } else {
            i += 1;
        }
    }

    // Render
    let diff_add_inner_bg = Color32::from_rgba_premultiplied(3, 201, 105, 120);
    let diff_del_inner_bg = Color32::from_rgba_premultiplied(218, 54, 51, 120);

    let mut line_idx = 0;

    for p_line in &parsed_lines {
        let (bg_color, text_color, hl_bg) = match p_line.line_type {
            LineType::Added => (DIFF_ADD_BG, DIFF_ADD_FG, Some(diff_add_inner_bg)),
            LineType::Deleted => (DIFF_DEL_BG, DIFF_DEL_FG, Some(diff_del_inner_bg)),
            LineType::HunkHeader => (DIFF_HUNK_BG, ACCENT, None),
            LineType::Context => (Color32::TRANSPARENT, TEXT_MUTED, None),
            LineType::Empty => (Color32::TRANSPARENT, TEXT_MUTED, None),
        };

        let old_num = p_line
            .old_line_num
            .map(|n| n.to_string())
            .unwrap_or_default();
        let new_num = p_line
            .new_line_num
            .map(|n| n.to_string())
            .unwrap_or_default();

        let selectable =
            p_line.line_type == LineType::Added || p_line.line_type == LineType::Deleted;

        // Mock selection state using egui memory
        let id = ui.id().with(line_idx);
        let mut is_selected = ui.data_mut(|d| d.get_temp::<bool>(id).unwrap_or(true));

        egui::Frame::default()
            .fill(bg_color)
            .inner_margin(egui::Margin::symmetric(0, 0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;

                    // Gutter (Line numbers + Selection area)
                    let gutter_width = 80.0;
                    let row_height = 20.0;
                    let (gutter_rect, gutter_resp) = ui.allocate_exact_size(
                        Vec2::new(gutter_width, row_height),
                        if selectable {
                            egui::Sense::click_and_drag()
                        } else {
                            egui::Sense::hover()
                        },
                    );

                    if gutter_resp.clicked() {
                        is_selected = !is_selected;
                        ui.data_mut(|d| d.insert_temp(id, is_selected));
                    }

                    // Gutter background & Hover
                    let mut gutter_bg = Color32::from_black_alpha(40);
                    if gutter_resp.hovered() && selectable {
                        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
                        gutter_bg = bg_color; // Match the row color on hover
                    }

                    ui.painter().rect_filled(gutter_rect, 0.0, gutter_bg);

                    // Separator line
                    ui.painter().vline(
                        gutter_rect.right(),
                        gutter_rect.y_range(),
                        Stroke::new(1.0, Color32::from_black_alpha(80)),
                    );

                    // Checkmark if selected
                    if selectable && is_selected {
                        let icon_pos = gutter_rect.left_center() + Vec2::new(8.0, 0.0);
                        ui.painter().text(
                            icon_pos,
                            egui::Align2::LEFT_CENTER,
                            icons::CHECK,
                            egui::FontId::proportional(14.0),
                            Color32::WHITE,
                        );
                    }

                    // Line numbers
                    ui.painter().text(
                        gutter_rect.left_center() + Vec2::new(42.0, 0.0),
                        egui::Align2::RIGHT_CENTER,
                        old_num,
                        egui::FontId::monospace(11.0),
                        Color32::from_gray(140),
                    );
                    ui.painter().text(
                        gutter_rect.right_center() - Vec2::new(8.0, 0.0),
                        egui::Align2::RIGHT_CENTER,
                        new_num,
                        egui::FontId::monospace(11.0),
                        Color32::from_gray(140),
                    );

                    // Hunk Handle
                    let handle_width = 16.0;
                    let (handle_rect, handle_resp) = ui.allocate_exact_size(
                        Vec2::new(handle_width, row_height),
                        if selectable {
                            egui::Sense::click()
                        } else {
                            egui::Sense::hover()
                        },
                    );

                    if handle_resp.hovered() && selectable {
                        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
                        ui.painter()
                            .rect_filled(handle_rect, 0.0, Color32::from_white_alpha(20));
                    }
                    ui.painter().vline(
                        handle_rect.right(),
                        handle_rect.y_range(),
                        Stroke::new(1.0, Color32::from_black_alpha(40)),
                    );

                    // Content Prefix padding
                    ui.add_space(8.0);

                    // Content
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let mut job = egui::text::LayoutJob::default();

                        if p_line.line_type == LineType::HunkHeader
                            || p_line.line_type == LineType::Empty
                        {
                            let fmt = egui::text::TextFormat::simple(
                                egui::FontId::monospace(12.5),
                                text_color,
                            );
                            job.append(p_line.text, 0.0, fmt);
                        } else {
                            // Add prefix explicitly since we stripped it for syntax highlighting
                            let prefix_str = match p_line.line_type {
                                LineType::Added => "+",
                                LineType::Deleted => "-",
                                LineType::Context => " ",
                                _ => "",
                            };
                            if !prefix_str.is_empty() {
                                let fmt = egui::text::TextFormat::simple(
                                    egui::FontId::monospace(12.5),
                                    text_color,
                                );
                                job.append(prefix_str, 0.0, fmt);
                            }

                            // Render syntax tokens, overlaying char_highlight if present
                            let mut current_char_idx = 0;
                            for (style, token_text) in &p_line.syntax_tokens {
                                let token_len = token_text.chars().count();
                                let token_end = current_char_idx + token_len;

                                let base_color = Color32::from_rgb(
                                    style.foreground.r,
                                    style.foreground.g,
                                    style.foreground.b,
                                );

                                // Simple approach for now: if this token overlaps with the diff highlight, apply the background.
                                // A perfect implementation would split the token at the highlight boundaries.
                                let mut fmt = egui::text::TextFormat::simple(
                                    egui::FontId::monospace(12.5),
                                    base_color,
                                );

                                if let Some((hl_start, hl_end)) = p_line.char_highlight {
                                    // Adjust indices because highlight calculation included the +/- prefix
                                    let adjusted_hl_start = hl_start.saturating_sub(1);
                                    let adjusted_hl_end = hl_end.saturating_sub(1);

                                    // Overlap check
                                    if current_char_idx < adjusted_hl_end
                                        && token_end > adjusted_hl_start
                                    {
                                        fmt.background = hl_bg.unwrap_or(Color32::TRANSPARENT);
                                    }
                                }

                                job.append(token_text, 0.0, fmt);
                                current_char_idx = token_end;
                            }
                        }

                        ui.add(egui::Label::new(job));
                    });
                });
            });

        line_idx += 1;
    }
}
