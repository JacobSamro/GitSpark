# GitSpark UI Agent Guide

GitSpark is a native Rust desktop app built on GPUI (Zed's GPU-accelerated framework) with `gpui-component` v0.5.1 for higher-level UI primitives.

Reference UI: GitHub Desktop (source in `tmp/`).

This repo should not be treated like a web app:
- no DOM
- no CSS cascade
- no browser layout engine
- GPUI uses retained-mode element trees, not immediate mode

## Core Rule

Do not add more ad hoc UI to `src/ui/app.rs` when a reusable primitive or component should exist.

If a pattern appears twice, it is a candidate for extraction.

## Current Architecture

- `src/ui/app.rs` — App state, event loop, action dispatch, top-level composition (~2000 lines)
- `src/ui/sidebar.rs` — Sidebar with TabBar/Tag, virtualized changes/history lists
- `src/ui/toolbar.rs` — Toolbar sections with Icon/Divider components
- `src/ui/workspace.rs` — Diff viewer with intra-line highlighting
- `src/ui/theme.rs` — Centralized color/geometry tokens (GitHub Dark palette)
- `src/ui/status_bar.rs` — Bottom status bar
- `src/ui/ui_state.rs` — NavState, FilterState, SidebarTab, etc.
- `src/ui/domain_state.rs` — CommitState, NetworkState, RepoState, SelectionState

## gpui-component Usage

Components we use from `gpui-component`:
- **TabBar / Tab** — sidebar tab switching
- **Tag** — status tags (A/M/D), HEAD indicator
- **Button + ButtonCustomVariant** — commit button with GitHub blue (#0969da)
- **Divider::vertical()** — toolbar section separators
- **Icon / IconName** — toolbar section icons (FolderOpen, GitHub, ArrowUp, ChevronDown, etc.)
- **ResizablePanelGroup / resizable_panel** — sidebar and file list resizing via `h_resizable()`

## gpui-component Constraints (CRITICAL)

### Input requires Root — DO NOT USE
`gpui_component::Input` calls `Root::read()` internally which panics unless the window root is `gpui_component::Root`. Wrapping in `Root` causes severe performance regression (blink cursor timers, font overrides, constant repaints). We use native GPUI `FocusHandle` + `on_key_down()` for text input instead.

### Button icons need Root — USE UNICODE INSTEAD
`Button::new().icon(IconName::X)` won't render SVG icons without `Root`. Use Unicode characters or plain divs for icon buttons. Only use `Button` for label-only buttons (e.g. commit button).

### Popover requires Selectable trait
`Popover.trigger()` requires the `Selectable` trait. `Stateful<Div>` doesn't implement it. Hand-roll dropdowns/overlays instead.

### Badge is an absolute overlay
`Badge::new().count(N)` renders as an absolutely positioned notification dot — not an inline counter. For inline count pills (like tab bar counters), use a styled `div()` with pill styling.

### Theme must be forced dark
Call `gpui_component::Theme::change(ThemeMode::Dark, None, cx)` in `main.rs` before opening the window. Without this, components follow system appearance which may be light.

## Text Input Pattern (Native GPUI)

Since `gpui_component::Input` is unusable without Root:

```rust
// State on the app struct:
summary_focus: FocusHandle,    // cx.focus_handle() in new()
summary_cursor: usize,         // byte offset into the string

// In render, check focus:
let focused = self.summary_focus.is_focused(window);

// Element:
div()
    .id("field-id")
    .track_focus(&self.summary_focus)  // makes it focusable
    .on_key_down(cx.listener(Self::handle_key))  // captures input
    .child(text_with_cursor)

// Key handling:
fn handle_key(&mut self, event: &KeyDownEvent, _win: &mut Window, cx: &mut Context<Self>) {
    let ks = &event.keystroke;
    if ks.modifiers.secondary() { /* Cmd+V paste, etc. */ }
    match ks.key.as_str() {
        "backspace" => { /* delete before cursor */ }
        "left" | "right" => { /* move cursor */ }
        _ => {
            if let Some(ref ch) = ks.key_char {
                // insert ch at cursor position
            }
        }
    }
}
```

Key details:
- `Keystroke.modifiers.secondary()` = Cmd on macOS (NOT `.command`)
- `Keystroke.key_char` = typed character; `.key` = key name ("backspace", "enter", etc.)
- `cx.read_from_clipboard()` for paste support
- Track cursor as byte offset; use char boundary helpers for safe movement
- Every visible text field must have `.track_focus(...)`, `.key_context("text-field")`, and `.on_key_down(...)`. If a filter/input renders a placeholder but cannot accept real keyboard input on macOS, it is not implemented correctly.

## Mac Visual Verification

Mac UI work must be tested visually in the native app, not only through automation. Verify UI behavior with both:
- Real native-app visual checks for hit targets, modal placement, loading/error messages, contrast, scrolling, and actual keyboard input.
- The app's native automation state for semantic comparison (`test_tree`, `snapshot`, stable IDs, enabled/selected state).

When checking a feature, compare what is visible with the automation state. Example: if the UI shows `1 changed files`, the automation snapshot should report one change; if a button looks disabled, the automation node should be disabled too.

Mac UI work is not done if only automation passes. It must also be checked visually in the native app.

## Flex Layout Gotchas (CRITICAL)

### Vertical centering in scroll containers
`v_flex().flex_1().overflow_y_scroll()` will vertically center children when content is shorter than the container. Fix: separate the scroll container from the content wrapper:

```rust
// BAD — centers children:
v_flex().flex_1().overflow_y_scroll().child(item1).child(item2)

// GOOD — content stays at top:
div().flex_1().overflow_y_scroll().child(
    div().flex().flex_col().w_full()
        .child(item1)
        .child(item2)
)
```

### Use uniform_list for long lists
All repeated lists (changes, history, file lists) must use `uniform_list` for viewport-aware rendering. Never use a plain `for` loop to add hundreds of children.

```rust
uniform_list("list-id", items.len(), move |range, _win, _cx| {
    range.map(|ix| render_row(&items[ix]).into_any_element()).collect()
})
.flex_1()
.with_sizing_behavior(ListSizingBehavior::Infer)
```

### Resizable panels
Use `gpui_component::resizable::{h_resizable, resizable_panel}`:

```rust
h_resizable("panel-group-id")
    .child(resizable_panel().size(px(260.0)).size_range(px(200.0)..px(400.0)).child(sidebar))
    .child(resizable_panel().child(workspace))
```

Panels fill their parent. Don't hardcode widths on child content — use `.size_full()`.

## Theme Color Mapping (GitHub Desktop Dark)

All colors are in `src/ui/theme.rs`. Key mappings from GitHub Desktop's SCSS:

| Token | Hex | GitHub Desktop Variable |
|-------|-----|------------------------|
| `toolbar_bg()` | #0a0e14 | `darken($gray-900, 3%)` |
| `toolbar_button_border()` | #141414 | `--box-border-color` |
| `toolbar_hover_bg()` | #30363d | `--toolbar-button-hover-background-color` |
| `commit_button_bg()` | #0969da | `$blue` |
| `accent()` | #1f6feb | `--focus-color` |
| `bg()` | #0d1117 | `$gray-900` |
| `panel_bg()` | #161b22 | sidebar/panel background |
| `text_main()` | #c9d1d9 | `--text-color` |
| `text_muted()` | #8b949e | `--text-secondary-color` |

Font: `system-ui` (San Francisco on macOS), base size 12px.

## Stable ID Rule

Every repeated row, popup, selector, checkbox-like control, text input, and scroll area must have an explicit stable ID.

Preferred ID sources: repo path, file path, commit SHA, branch name, enum key.

Use `SharedString::from(format!("prefix-{}", key))` for dynamic IDs.

## Composition Rules

`src/ui/app.rs` should be:
- app state
- event loop
- action dispatch
- top-level screen composition

It should NOT keep:
- custom dropdown implementations
- custom text field styling
- repeated row rendering logic
- repeated popup lifecycle code

## Do / Don't

Do:
- reuse `src/ui/theme.rs` tokens for all colors, sizes, radii
- use `uniform_list` for any list that could exceed ~20 items
- use `h_resizable` / `resizable_panel` for adjustable panel layouts
- use `cx.listener()` for entity callbacks on gpui-component widgets
- thread `&Window` through render methods when `FocusHandle::is_focused()` is needed
- prefer small Rust functions over giant render closures

Do not:
- use `gpui_component::Input` or `Root` (performance killer)
- use `Button::new().icon(IconName::X)` without Root (icons won't render)
- use `Badge` for inline counters (it's an absolute overlay)
- add `v_flex().flex_1().overflow_y_scroll()` for lists (causes centering)
- hardcode panel widths when using resizable panels
- leave repeated interactive widgets without explicit IDs

## Definition Of Done For UI Work

A UI change is not done unless:
- repeated patterns are extracted when appropriate
- IDs are stable
- hover/active/selected states are explicit
- geometry comes from tokens or helpers
- lists use `uniform_list` for virtualization
- `src/ui/app.rs` is simpler, not more crowded


## Guidelines

- Do not change existing comments, remove them only if its irrelavent
