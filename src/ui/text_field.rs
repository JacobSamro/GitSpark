//! Reusable text field component with cursor, selection, and keyboard handling.
//!
//! Used by both the commit form (summary/description) and settings modal fields.

use gpui::*;
use gpui_component::h_flex;

use crate::ui::theme;
use crate::ui::theme::z;

/// State for a single text field instance.
#[derive(Clone, Debug)]
pub struct TextFieldState {
    pub cursor: usize,
    pub selection: Option<usize>, // anchor position; selected range is anchor..cursor
}

impl Default for TextFieldState {
    fn default() -> Self {
        Self {
            cursor: 0,
            selection: None,
        }
    }
}

impl TextFieldState {
    pub fn clamp(&mut self, len: usize) {
        self.cursor = self.cursor.min(len);
        if let Some(sel) = &mut self.selection {
            *sel = (*sel).min(len);
        }
    }
}

/// Ordered range from selection anchor and cursor.
fn ordered(a: usize, b: usize) -> (usize, usize) {
    if a <= b { (a, b) } else { (b, a) }
}

fn prev_boundary(s: &str, pos: usize) -> usize {
    if pos == 0 { return 0; }
    let mut p = pos - 1;
    while p > 0 && !s.is_char_boundary(p) { p -= 1; }
    p
}

fn next_boundary(s: &str, pos: usize) -> usize {
    if pos >= s.len() { return s.len(); }
    let mut p = pos + 1;
    while p < s.len() && !s.is_char_boundary(p) { p += 1; }
    p
}

/// Handle a key event on a text field. Mutates value + state in place.
/// Returns true if the event was handled.
pub fn handle_text_key(
    value: &mut String,
    state: &mut TextFieldState,
    multiline: bool,
    event: &KeyDownEvent,
    cx: &mut App,
) -> bool {
    let ks = &event.keystroke;

    if ks.modifiers.secondary() {
        match ks.key.as_str() {
            "a" => {
                state.selection = Some(0);
                state.cursor = value.len();
                return true;
            }
            "c" => {
                if let Some(sel) = state.selection {
                    let (s, e) = ordered(sel, state.cursor);
                    let selected = &value[s..e];
                    if !selected.is_empty() {
                        cx.write_to_clipboard(ClipboardItem::new_string(selected.to_string()));
                    }
                }
                return true;
            }
            "x" => {
                if let Some(sel) = state.selection {
                    let (s, e) = ordered(sel, state.cursor);
                    let selected = &value[s..e];
                    if !selected.is_empty() {
                        cx.write_to_clipboard(ClipboardItem::new_string(selected.to_string()));
                        delete_selection(value, state);
                    }
                }
                return true;
            }
            "v" => {
                if let Some(item) = cx.read_from_clipboard() {
                    if let Some(text) = item.text() {
                        let text = if multiline {
                            text.to_string()
                        } else {
                            text.replace(['\n', '\r'], " ")
                        };
                        delete_selection(value, state);
                        value.insert_str(state.cursor, &text);
                        state.cursor += text.len();
                    }
                }
                return true;
            }
            _ => return false,
        }
    }

    match ks.key.as_str() {
        "backspace" => {
            if state.selection.is_some() {
                delete_selection(value, state);
            } else if state.cursor > 0 {
                let new_pos = prev_boundary(value, state.cursor);
                value.drain(new_pos..state.cursor);
                state.cursor = new_pos;
            }
            true
        }
        "delete" => {
            if state.selection.is_some() {
                delete_selection(value, state);
            } else if state.cursor < value.len() {
                let end = next_boundary(value, state.cursor);
                value.drain(state.cursor..end);
            }
            true
        }
        "left" => {
            if ks.modifiers.shift {
                if state.selection.is_none() {
                    state.selection = Some(state.cursor);
                }
            } else {
                state.selection = None;
            }
            if state.cursor > 0 {
                state.cursor = prev_boundary(value, state.cursor);
            }
            true
        }
        "right" => {
            if ks.modifiers.shift {
                if state.selection.is_none() {
                    state.selection = Some(state.cursor);
                }
            } else {
                state.selection = None;
            }
            if state.cursor < value.len() {
                state.cursor = next_boundary(value, state.cursor);
            }
            true
        }
        "home" => {
            if ks.modifiers.shift {
                if state.selection.is_none() {
                    state.selection = Some(state.cursor);
                }
            } else {
                state.selection = None;
            }
            state.cursor = 0;
            true
        }
        "end" => {
            if ks.modifiers.shift {
                if state.selection.is_none() {
                    state.selection = Some(state.cursor);
                }
            } else {
                state.selection = None;
            }
            state.cursor = value.len();
            true
        }
        "enter" if multiline => {
            delete_selection(value, state);
            value.insert(state.cursor, '\n');
            state.cursor += 1;
            true
        }
        _ => {
            if let Some(ref ch) = ks.key_char {
                if !ks.modifiers.control
                    && (multiline || (!ch.contains('\n') && !ch.contains('\r')))
                {
                    delete_selection(value, state);
                    value.insert_str(state.cursor, ch);
                    state.cursor += ch.len();
                    return true;
                }
            }
            false
        }
    }
}

fn delete_selection(value: &mut String, state: &mut TextFieldState) {
    if let Some(sel) = state.selection.take() {
        let (s, e) = ordered(sel, state.cursor);
        if s < e && e <= value.len() {
            value.drain(s..e);
            state.cursor = s;
        }
    }
}

/// Render the text content with cursor and optional selection highlight.
pub fn render_text_content(
    value: &str,
    cursor: usize,
    selection: Option<usize>,
    focused: bool,
    placeholder: &str,
    multiline: bool,
) -> Div {
    let is_empty = value.is_empty();
    let sel_bg: Hsla = gpui::rgb(0x264f78).into();

    if is_empty && !focused {
        return div()
            .text_size(z(12.0))
            .text_color(theme::text_muted())
            .child(placeholder.to_string());
    }

    if is_empty && focused {
        return h_flex()
            .items_center()
            .text_size(z(12.0))
            .child(div().w(px(1.0)).h(px(14.0)).bg(theme::text_main()).flex_shrink_0())
            .child(div().text_color(theme::text_muted()).child(placeholder.to_string()));
    }

    if !focused {
        return if multiline {
            div().text_size(z(12.0)).text_color(theme::text_main()).child(value.to_string())
        } else {
            div().text_size(z(12.0)).text_color(theme::text_main()).truncate().child(value.to_string())
        };
    }

    // Focused with text
    let cursor_pos = cursor.min(value.len());

    if let Some(sel_anchor) = selection {
        let (sel_start, sel_end) = ordered(sel_anchor.min(value.len()), cursor_pos);
        let before_sel = &value[..sel_start];
        let selected = &value[sel_start..sel_end];
        let after_sel = &value[sel_end..];
        let nowrap = !multiline;

        let mut row = if multiline {
            h_flex().items_start().text_size(z(12.0)).flex_wrap()
        } else {
            h_flex().items_center().overflow_x_hidden().text_size(z(12.0))
        };

        if !before_sel.is_empty() {
            let mut el = div().text_color(theme::text_main()).child(before_sel.to_string());
            if nowrap { el = el.whitespace_nowrap(); }
            row = row.child(el);
        }
        if cursor_pos == sel_start {
            row = row.child(div().w(px(1.0)).h(px(14.0)).bg(theme::text_main()).flex_shrink_0());
        }
        if !selected.is_empty() {
            let mut el = div().text_color(gpui::white()).bg(sel_bg).child(selected.to_string());
            if nowrap { el = el.whitespace_nowrap(); }
            row = row.child(el);
        }
        if cursor_pos == sel_end {
            row = row.child(div().w(px(1.0)).h(px(14.0)).bg(theme::text_main()).flex_shrink_0());
        }
        if !after_sel.is_empty() {
            let mut el = div().text_color(theme::text_main()).child(after_sel.to_string());
            if nowrap { el = el.whitespace_nowrap(); }
            row = row.child(el);
        }
        row
    } else {
        let before = &value[..cursor_pos];
        let after = &value[cursor_pos..];
        if multiline {
            h_flex()
                .items_start()
                .text_size(z(12.0))
                .child(div().text_color(theme::text_main()).child(before.to_string()))
                .child(div().w(px(1.0)).h(px(14.0)).bg(theme::text_main()).flex_shrink_0())
                .child(div().text_color(theme::text_main()).child(after.to_string()))
        } else {
            h_flex()
                .items_center()
                .overflow_x_hidden()
                .text_size(z(12.0))
                .child(div().text_color(theme::text_main()).whitespace_nowrap().child(before.to_string()))
                .child(div().w(px(1.0)).h(px(14.0)).bg(theme::text_main()).flex_shrink_0())
                .child(div().text_color(theme::text_main()).whitespace_nowrap().child(after.to_string()))
        }
    }
}
