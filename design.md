# GitSpark — Desktop Design System

The visual language for the GPUI desktop client. It is **Zed One Dark,
deepened**, with a **One Light** arm — chosen because GPUI *is* Zed's
framework, so every measurement here is one the renderer already ships in
production, and the implementation has a reference you can open and compare
against.

Surfaces are a darkened derivation: stock One Dark bottoms out at `#282c33`,
a fairly light "dark", so the ramp sits about two stops below it. The *hues* —
`created` / `modified` / `deleted` / `player` and the syntax set — are Zed's
literal values in both arms.

The goal is a client that reads as a first-class editor-adjacent tool: dense
information, quiet chrome, one blue accent, hairline separators, no
decoration.

> **Scope of this doc:** tokens + component specs + how they map to GPUI /
> gpui-component. It defines the *what*. `src/ui/theme.rs` is the tokens made
> executable; `src/ui/kit/` is the components made executable. When this doc
> and the code disagree, the code is the bug.

---

## 1. Principles

1. **Density over comfort.** This is a tool people keep open all day next to an
   editor. Rows are 22–32px, body text is 13px, padding is tight. Whitespace is
   earned, not default.
2. **One accent.** `accent()` — Zed's `player[0]` — is the single interactive
   blue, and the commit CTA reuses it. Nothing else is blue. Green/red/yellow
   are **status only**, never chrome.
3. **Structure by hairline, not by shadow.** A 1px `border()` line separates
   surfaces. Shadows exist only for things that genuinely float (dialogs,
   dropdowns, context menus) — see §7.
4. **Chrome recedes, content leads.** The toolbar, sidebar, and status bar use
   darker surfaces than the diff. The diff is the brightest thing on screen.
5. **No motion.** GitSpark has no transitions, no spinners that bounce, no
   fades. State changes are instant. The only animated element is the
   indeterminate `LoaderCircle` during network operations.
6. **Everything scales.** Zoom is a first-class feature (§5.1). Any pixel value
   a user can perceive goes through `theme::z()`. Values that are not
   perceptual (1px hairlines) do not.

---

## 2. Surfaces

The window is: toolbar across the top, sidebar left, workspace right, status
bar across the bottom. Each step of the ramp is a token, and the ordering is
the hierarchy.

| Surface | Token | Dark | Light | Notes |
|---|---|---|---|---|
| Diff body / buffer | `bg()` | `#0e1013` | `#ffffff` | The reading surface. |
| Chrome — toolbar, sidebar, dialogs, status bar | `panel_bg()` / `toolbar_bg()` | `#15171b` | `#f2f2f3` | |
| Raised control — inputs, buttons, hunk headers | `surface_bg()` | `#1a1d22` | `#eaeaeb` | |
| Raised border / pressed | `surface_bg_alt()` | `#262a31` | `#dfdfe0` | |
| Recessed | `surface_bg_muted()` | `#0a0c0f` | `#f7f7f8` | |
| Border | `border()` | `#2a2f37` | `#c9c9ca` | |

**Depth inverts between the arms.** In dark the buffer is the *darkest* thing
on screen and the chrome sits above it; in light it is the *brightest*. Depth
means "furthest from the chrome", not "darker than it". Getting this backwards
is the single most common way a light arm ends up looking broken.

**Rule:** a surface never sits on a surface of the same value. If two adjacent
regions share a token, one of them needs a `border()` hairline instead.

---

## 3. Color tokens

All colors live in `src/ui/theme.rs` and are exposed as `fn() -> Hsla`. They
are functions, not consts, so a light theme stays a token swap and never a
component fork (§13). **No component may write `rgb(0x…)`.**

### 3.1 Text

| Token | Dark | Light | Use |
|---|---|---|---|
| `text_main()` | `#d3d7de` | `#383a41` | Primary labels, file names, commit summaries, diff code. |
| `text_muted()` | `#868d99` | `#6b6d76` | Metadata, timestamps, paths, placeholders, idle icons. |
| `line_num_color()` | `#59606b` | `#9a9ca3` | Diff line numbers only. The faintest readable step. |
| `commit_button_text()` | `#0e1013` | `#ffffff` | Text on the accent fill — see the note below. |

`commit_button_text()` is **not** a constant white. The dark arm's accent is
light enough to need near-black text on it; the light arm's needs white. A
token that hardcodes white here is a light-mode bug.

Three steps of text is the whole ramp. If a label needs a fourth, it is
probably the wrong size or the wrong surface.

### 3.2 Accent & interactive

| Token | Dark | Light | Use |
|---|---|---|---|
| `accent()` | `#74ade8` | `#4257c9` | Zed's `player[0]`. Selection, focus, links, the commit CTA. |
| `accent_muted()` | `#5b93cc` | `#35489f` | Pressed accent. |
| `commit_button_bg()` | = `accent()` | = `accent()` | The CTA reuses the one accent. |
| `commit_button_hover_bg()` | `#8cbcec` | `#3a4eb8` | Note the direction flips: dark hovers *lighter*, light hovers *darker*. |
| `checkbox_selected_bg()` | = `accent()` | = `accent()` | |
| `checkbox_selected_fg()` | = `bg()` | = `bg()` | The check glyph knocked out of the fill — resolving to `bg()` is correct in both arms. |
| `text_selection_bg()` | `#2f4c6b` | `#d8deef` | Text selection inside fields. |
| `hover_bg()` | `#22262d` | `#e6e6e8` | Zed's `element.hover`. |
| `list_hover_bg()` | `#262a31` | `#e0e0e3` | Rows sit on `panel_bg`, so one step further. |
| `toolbar_hover_bg()` | = `hover_bg()` | = `hover_bg()` | |

**The accent cannot be shared between arms.** `#74ade8` measures about 1.9:1
on white — unreadable. Any "token swap" that changes surfaces and leaves the
accent alone produces an unusable light mode.

### 3.3 Status

| Token | Dark | Light | Use |
|---|---|---|---|
| `success()` | `#a1c181` | `#3f8a3a` | Zed's `created`. Added files, ahead count, clean state. |
| `warning()` | `#dec184` | `#b07a08` | Zed's `modified`. Conflicts, detached HEAD, stale branch. |
| `warning_bg()` | `#2a2415` | `#fbf3df` | Fill behind a warning banner. |
| `danger()` | `#d07277` | `#c0392e` | Zed's `deleted`. Deleted files, destructive buttons, errors. |
| `danger_hover()` | `#dc8a8e` | `#a82f26` | Hover on a destructive button. |

Status colors carry meaning. A red button means data loss; a yellow banner
means the repo is in a state the user must resolve. Never use them for emphasis.

### 3.4 Diff

The diff has its own palette because it needs four simultaneous backgrounds
(add / delete / context / hunk) that all stay legible under 13px mono text.

| Token | Dark | Light | Use |
|---|---|---|---|
| `diff_add_bg()` / `diff_add_gutter_bg()` | `#212721` / `#1b2419` | `#ecf3eb` / `#e3efe1` | Added line body / its gutter. |
| `diff_del_bg()` / `diff_del_gutter_bg()` | `#271d20` / `#2e1e20` | `#f9ebea` / `#f5dfdd` | Deleted line body / its gutter. |
| `diff_add_fg()` / `diff_del_fg()` | = `text_main()` | = `text_main()` | The background carries the signal, not the text. |
| `diff_hunk_bg()` | = `surface_bg()` | = `surface_bg()` | `@@` hunk header strip. |
| `diff_gutter_bg()` | `#121519` | `#fafafb` | Line-number gutter on context lines. |
| `diff_selected_bg()` | `#1c2a39` | `#dbe4f4` | Line-selection for partial commits. |

Row tints are the hue at ~13% over the buffer in dark and ~10% in light, then
flattened to an opaque value. The two are **not** the same number: a 10% green
over `#282c33` and over `#0e1013` are not the same signal — the deeper ground
eats it.

`diff_selected_bg()` is deliberately quiet. It fills the *whole gutter* on
every selected line and every line starts selected, so a saturated value turns
the gutter into the loudest thing in the diff — backwards, since selection is
the default state and the code is the content.

### 3.5 Push suggestion card

| Token | Dark | Light |
|---|---|---|
| `push_card_bg()` | `#17232e` | `#e8edfa` |
| `push_card_border()` | = `accent()` | = `accent()` |
| `push_card_text()` | `#9fc4e4` | `#3a4b8f` |

One component, three tokens. Do not reuse them elsewhere; if a second
highlighted card appears, generalize these into `info_*` first.

### 3.6 Utilities

- `with_alpha(color, a)` — the only sanctioned way to make a translucent
  variant. Never write a second hex for "the same color but faded".
- `blend(from, to, t)` — interpolation, for intra-line diff highlighting.

---

## 4. Typography

System font (San Francisco on macOS) via GPUI's default. Do not name or bundle
a face. The diff uses GPUI's monospace family.

Every size below is a **base** value; render it as `theme::z(SIZE)` so zoom
applies (§5.1).

| Role | Const | px | Weight | Use |
|---|---|---|---|---|
| Display | `FONT_SIZE_LG` | 28 | SEMIBOLD | Empty-state headline, welcome screen. |
| Title | `FONT_SIZE_MD` | 14 | SEMIBOLD | Dialog titles, section headings. |
| Row title | `FONT_SIZE` | 13 | NORMAL | The prominent line in a row: file, branch, repo, commit summary. Also diff code. |
| Body | `FONT_SIZE_BODY` | 12 | NORMAL | **The default.** Labels, buttons, dialog copy. |
| Secondary | `FONT_SIZE_SM` | 11 | NORMAL | Metadata, timestamps, paths, hints. |
| Micro | `FONT_SIZE_XS` | 9 | SEMIBOLD | Status tags (A/M/D), badge counts. |

Six rungs, and **12 is the workhorse** — it is what most UI text renders at, so
it gets the plainest name. 13 is a deliberate step up for the one line in a row
the eye should land on first. Reaching for 13 as a general default inflates the
whole layout; reaching for 12 in a row title flattens it.

**Numerals.** SF's default figures are proportional, so digits that update in
place jitter. Apply `theme::tabular_nums()` to ahead/behind counts, line
numbers, file counts, and anything else whose digits change under a stable
label.

**Truncation.** Single-line labels truncate with `.truncate()`, never wrap.
File paths truncate at the *front* (`labels::truncate_path_start`) so the file
name stays visible; everything else truncates at the end.

---

## 5. Spacing & geometry

### 5.1 Zoom — the `z()` contract

GitSpark scales its whole layout with a user zoom factor. `theme::z(v)` returns
`px(v * zoom())`.

**Use `z()` for:** padding, gaps, font sizes, icon sizes, row heights, corner
radii, fixed widths — anything whose apparent size should follow zoom.

**Use raw `px()` for:** 1px hairline borders (a 1.3px border renders as a
smeared 2px line), and nothing else. `border_1()` already does the right thing.

A `z()` call with a literal is a smell — reach for the scale below instead.

### 5.2 Spacing scale

| Token | px | Typical use |
|---|---|---|
| `SPACE_1` | 2 | Icon nudge, tag inner padding. |
| `SPACE_2` | 4 | Icon-to-label gap, close-button padding. |
| `SPACE_3` | 6 | Button vertical padding, tight row gaps. |
| `SPACE_4` | 8 | The default gap. Row gaps, button gaps, footer gaps. |
| `SPACE_5` | 10 | Toolbar section inner padding, list gaps. |
| `SPACE_6` | 12 | Button horizontal padding, dialog header vertical padding. |
| `SPACE_7` | 16 | Dialog body padding, section padding. |
| `SPACE_8` | 24 | Empty-state padding, welcome-screen breathing room. |

Eight steps, and 8 is the default. If a layout needs a value that is not on the
scale, the layout is wrong before the scale is.

### 5.3 Radius

| Token | px | Applied to |
|---|---|---|
| `CORNER_RADIUS_SM` | 4 | Tags, badges, checkboxes, close buttons, hover chips. |
| `CORNER_RADIUS` | 6 | Buttons, text fields, list rows, cards, dialogs, menus. |
| `RADIUS_PILL` | 999 | Count pills, branch chips. |

Two real radii. A third would be noticed.

### 5.4 Frame dimensions

| Token | px | Notes |
|---|---|---|
| `TOOLBAR_HEIGHT` | 50 | Fixed. Matches GitHub Desktop. |
| `STATUS_BAR_HEIGHT` | 26 | Fixed. |
| `SIDEBAR_WIDTH` / `SIDEBAR_MIN_WIDTH` | 260 / 220 | Resizable via `h_resizable`. |
| `WORKTREE_SECTION_WIDTH` | 220 | Toolbar section 2. |
| `BRANCH_SECTION_WIDTH` | 300 | Toolbar section 3. |
| `NETWORK_SECTION_WIDTH` | 231 | Toolbar section 4. |
| `ROW_HEIGHT` / `ROW_HEIGHT_COMPACT` | 32 / 28 | List rows. `uniform_list` needs these exact. |
| `CONTROL_HEIGHT` | 34 | Buttons, fields, dropdown triggers. |
| `TAB_HEIGHT` | 34 | Sidebar tab bar. |
| `DIFF_ROW_HEIGHT` | 22 | Must be exact — `uniform_list` computes scroll from it. |
| `DIFF_HEADER_HEIGHT` | 32 | File header strip in the diff. |
| `DIFF_LINE_NUM_WIDTH` | 50 | Both gutters. |
| `FILTER_BAR_HEIGHT` | 32 | Sidebar filter field. |

---

## 6. Iconography

- **Source:** `gpui_component::Icon` + `IconName` first; custom SVGs in
  `assets/icons/` for anything missing.
- **Sizes:** toolbar 16 · dialog title 16 · list row 14 · close/chevron 14 ·
  caret 10. All through `z()`.
- **Color:** idle `text_muted()`; active/selected `text_main()`; status icons
  take their status color.
- **Constraint:** `Button::new().icon(IconName::X)` renders nothing without
  `Root` (§10). Icon buttons are `div()` + `Icon::new()`, which is exactly what
  `kit::icon_button` gives you.

---

## 7. Elevation

Four levels. Most surfaces are flat with a hairline and no shadow at all.

`kit::Surface` is a trait over every `Styled` element, so the helpers chain:
`v_flex().w(z(440.0)).dialog()`.

| Level | Helper | Shadow | Use |
|---|---|---|---|
| `e0` | none — `border_1() + border_color(border())` | — | Sidebar, rows, chips, fields. |
| `e1` | `.card()` | `0 1px 2px / 20%` | Push suggestion card, settings groups. |
| `e2` | `.overlay()` | `0 6px 16px / 40%` | Dropdowns, context menus, autocomplete. |
| `e3` | `.dialog()` | `0 12px 32px / 55%` | Modal dialogs, the settings modal. |

Each helper applies the fill, the outline, **and** the shadow together. A 1px
border on top of a drop shadow reads as an outline rather than as elevation, so
do not add `border_1()` after calling one.

Shadow offsets and blur go through `z()` like everything else — a shadow that
stayed put while the layout grew under zoom reads as a rendering bug.

---

## 8. Component catalog

Each entry: what it is → key specs → where it lives. Everything here exists in
`src/ui/kit/`; the specs are the contract, the code is the truth.

Migration status: `dialog`, `button`, `icon_button`, and `Surface` are built
and in use by the delete-branch, reset-to-commit, and discard-stash dialogs.
`tag`, `pill`, `empty_state`, and `section_header` are built but not yet
adopted — the app still draws those by hand. `row` is specified below and not
yet built. Unused kit code is marked `#[allow(dead_code)]` on purpose: a
component library carries its vocabulary ahead of its consumers, so "unused"
here means "not migrated yet", not "backlog".

### 8.1 Button — `kit::button`

`CONTROL_HEIGHT` tall, `SPACE_6` horizontal padding, `CORNER_RADIUS`,
`FONT_SIZE` label. Four variants:

| Variant | Rest | Hover | Use |
|---|---|---|---|
| `Primary` | `commit_button_bg()` bg, white text | `commit_button_hover_bg()` | The one affirmative action per dialog. |
| `Secondary` | `surface_bg()` bg, `surface_bg_alt()` border, `text_main()` | `toolbar_hover_bg()` | Cancel, and every non-primary action. |
| `Danger` | `danger()` bg, white text | `danger_hover()` | Delete, discard, force-push. |
| `Ghost` | transparent, `text_muted()` | `hover_bg()` | Tertiary links inside a body. |

**Disabled** (`button_state(.., false)`) drops to neutral gray —
`surface_bg()` fill, `surface_bg_alt()` border, `text_muted()` label — for all
four variants, and loses hover and the pointer cursor. It is *not* the variant
color at reduced alpha: a translucent red still reads as "destructive", where
gray reads as "unavailable", which is the thing that needs communicating. The
footprint stays identical so the footer doesn't shift under the cursor.

Every button takes a stable id (§9).

### 8.2 Icon button — `kit::icon_button`

Square, `SPACE_2` padding, `CORNER_RADIUS_SM`, 14px icon in `text_muted()`,
hover fills `hover_bg()`. Dialog close buttons, row affordances, toolbar
extras.

### 8.3 Dialog — `kit::dialog`

The single modal shell. `panel_bg()`, `CORNER_RADIUS`, `e3` elevation, fixed
width (440 default; 520 wide; 720 for settings).

- **Header:** `SPACE_7`/`SPACE_6` padding, hairline bottom. Optional status
  icon (16px), title at Title role, close `icon_button` pinned right.
- **Body:** `SPACE_7` padding, `SPACE_5` gap. Body text at Body role,
  explanatory text at Secondary role in `text_muted()`.
- **Footer:** `SPACE_7`/`SPACE_6` padding, hairline top, right-aligned,
  `SPACE_4` gap. **Cancel is always leftmost, the affirmative action rightmost**
  — macOS order.

A dialog that hand-rolls its own header/footer is a bug. Fourteen files did
this before the kit existed; that is precisely why it exists.

### 8.4 Row — `kit::row`

The list-row primitive behind changes, history, branches, stashes, and repos.
`ROW_HEIGHT` (or `ROW_HEIGHT_COMPACT`), `SPACE_5` horizontal padding,
`SPACE_4` gap, `CORNER_RADIUS`.

- Rest: transparent · Hover: `list_hover_bg()` · Selected: `accent()` fill with
  `text_main()` label and icons (GitHub Desktop fills the whole row).
- Leading slot (checkbox / status tag / avatar), title (truncating, `flex_1`),
  trailing slot (metadata, count, chevron).
- Every row is built inside `uniform_list` and carries a stable id derived from
  its file path / SHA / branch name.

### 8.5 Tag — `kit::tag`

The A/M/D status square and the HEAD marker. 16px square, `CORNER_RADIUS_SM`,
Micro role SEMIBOLD, centered glyph. Colors: added `success()`, modified
`warning()`, deleted `danger()`, renamed `accent()`, conflicted `danger()` with
a `warning_bg()` fill.

### 8.6 Count pill — `kit::pill`

Inline counters in the tab bar and toolbar. `RADIUS_PILL`,
`toolbar_badge_bg()`, Secondary role, `SPACE_3`/`SPACE_1` padding,
`tabular_nums`. **Not** `gpui_component::Badge` — that is an absolutely
positioned overlay dot, not an inline counter (§10).

### 8.7 Empty state — `kit::empty_state`

Centered, `SPACE_8` padding. Optional 28px icon in `text_muted()`, headline at
Body role in `text_main()`, one line of guidance at Secondary role in
`text_muted()`. Optional single `Ghost` button. No illustrations.

### 8.8 Section header — `kit::section_header`

Sidebar and settings group labels. Secondary role, SEMIBOLD, `text_muted()`,
`SPACE_5`/`SPACE_3` padding. Not uppercase — GitHub Desktop is not.

### 8.9 Picker — `kit::picker`

Repository, worktree and branch selection are one control: an overlay panel
with a filter field on top and a list below. Panel is `panel_bg()`,
`CORNER_RADIUS`, `e3`; rows are 40px with a 16px leading icon, a title at Body
role and an optional subtitle at Secondary; the current row takes
`surface_bg_alt()` plus a 7px `accent()` dot.

`kit::filter_input` is the extracted part, and it is the reason the module
exists: `gpui_component::Input` is unusable (§10.1), so every consumer
hand-rolled the same native `FocusHandle` field with its own painted caret.
It returns with `track_focus` applied; the callsite adds `.key_context(..)`
and `.on_key_down(..)`, so the kit never mentions `GitSparkApp`.

**Splitting at the caret must snap to a char boundary.** Both hand-rolled
copies sliced `&text[..cursor]` after clamping to `len()`, which is not
enough — a byte index inside a multi-byte character is in range but not a
boundary, and slicing there panics. Typing an accented character into either
filter took the app down. `split_at_cursor` handles it, with tests.

### 8.10 Toolbar sections

Four, left to right: **Repository · Worktree · Branch · Status**. Each is an
icon plus a stacked label and value; the first three open a picker and carry a
caret, Status is a button and does not. Full-height 1px dividers between them.

The **Worktree** section names the open working directory. A worktree is a
second checkout of the same repository in its own folder, so selecting one is
*opening a different path* — the row click goes to `open_repo_with_notify`,
the same call the Repository picker makes, not a branch checkout.

The list loads lazily on open rather than on every refresh: it is a separate
`git worktree list` shell-out, and the toolbar can name the current worktree
from the snapshot alone. The call is synchronous because it reads a few
administrative files and returns in single-digit milliseconds — threading it
would make the panel open empty and fill in late.

### 8.11 Text field

Native GPUI `FocusHandle` + `on_key_down`, per `src/ui/text_field.rs`.
`surface_bg()`, `CORNER_RADIUS`, `CONTROL_HEIGHT`, `SPACE_6` padding, Body
role. Focused: `accent()` border. Placeholder: `text_muted()`.
`gpui_component::Input` is unusable here (§10) — do not reach for it.

---

## 9. Stable ID rule

Every repeated row, popup, selector, checkbox-like control, text input, and
scroll area carries an explicit stable id. Preferred sources, in order: repo
path, file path, commit SHA, branch name, enum key. Build them with
`SharedString::from(format!("prefix-{key}"))`.

Index-based ids (`format!("row-{i}")`) are a bug: they re-bind to a different
item when the list is filtered or reordered, which loses hover, focus, and
scroll position.

---

## 10. gpui-component constraints

These are hard limits of the dependency, not preferences. They are why the kit
exists at all.

1. **`Input` requires `Root`. Do not use either.** `Input` calls `Root::read()`
   and panics without it; wrapping the window in `Root` costs a blink-cursor
   timer, font overrides, and constant repaints. Native `FocusHandle` +
   `on_key_down` instead (§8.9).
2. **Button icons need `Root`.** `Button::new().icon(..)` renders an empty box.
   Use `kit::icon_button`.
3. **`Popover::trigger()` requires `Selectable`,** which `Stateful<Div>` does
   not implement. Dropdowns and menus are hand-rolled — see
   `src/ui/primitives`-era code and the kit overlay helpers.
4. **`Badge` is an absolute overlay,** not an inline counter. Use `kit::pill`.
5. **gpui-component's theme must be driven explicitly.** Left alone it follows
   the system appearance, which will disagree with our resolved preference the
   moment a user picks Light on a dark Mac. `Theme::change(..)` is called from
   `main.rs` at startup and from `set_appearance` at runtime, both pointed at
   the same `theme::resolve` answer (§13).

What we *do* use from gpui-component: `TabBar`/`Tab`, `Divider::vertical()`,
`Icon`/`IconName`, `h_flex`/`v_flex`, `ResizablePanelGroup`/`resizable_panel`,
and label-only `Button`.

---

## 11. Layout gotchas

**Vertical centering in scroll containers.** `v_flex().flex_1().overflow_y_scroll()`
centers its children when content is shorter than the container. Separate the
scroller from the content:

```rust
// BAD — centers children
v_flex().flex_1().overflow_y_scroll().child(a).child(b)

// GOOD — content stays at top
div().flex_1().overflow_y_scroll().child(
    v_flex().w_full().child(a).child(b)
)
```

**The diff is virtualized with `list`, not `uniform_list`.** Diff rows are
`DIFF_ROW_HEIGHT`, hunk headers are taller, and wrapped lines taller still —
`uniform_list` needs one height for every row, so it does not apply. GPUI's
`list` + `ListState` handles variable heights.

`ListState` must live **across renders** (the view owns it — it caches
measured heights there) and must be **reset when the content changes**, or a
new diff opens scrolled into the middle of nowhere. `DiffListHandle` does
both, keyed on file + row count + options. Changes and History hold separate
handles because they can show the same file path.

Measured on a 4001-row diff, in release: building every row cost **14–21ms per
frame** — the entire 60fps budget, on every frame of every scroll. Virtualized,
the same diff flattens in **0.08–0.19ms**. Parsing the diff text was never the
problem at 0.3ms; it was constructing elements nobody could see.

**Known debt:** side-by-side diff is still built eagerly and needs the same
treatment.

**Long lists are `uniform_list`.** Any list that can exceed ~20 items —
changes, history, files, branches — is virtualized. A `for` loop that pushes
hundreds of children is a bug. `uniform_list` needs an exact row height, which
is why §5.4 pins `ROW_HEIGHT` and `DIFF_ROW_HEIGHT`.

**Resizable panels fill their parent.** Do not hardcode widths on the content
inside a `resizable_panel` — use `.size_full()`.

---

## 12. Composition rules

`src/ui/app.rs` and `src/ui/app/*` hold app state, the event loop, action
dispatch, and top-level screen composition. They do **not** hold custom
dropdown implementations, custom text-field styling, repeated row rendering, or
repeated popup lifecycle code. Those live in `src/ui/kit/`.

**If a visual pattern appears twice, it is a candidate for extraction. Three
times and it is not a candidate any more.**

### Decisions taken during the picker/worktree build

Recorded because each was a real fork in the road, and the reasoning is not
recoverable from the code:

- **Selecting a worktree opens a path; it does not check out a branch.** A
  worktree is a separate working directory, so the row click routes to
  `open_repo_with_notify`. The branch changes as a *consequence* of the
  directory changing, which is git's model, not ours.
- **A branch checked out in another worktree is unavailable here.** Git
  enforces this — `add_worktree` returns an error, covered by
  `refuses_to_check_out_a_branch_already_checked_out_elsewhere`. Each worktree
  row therefore names its branch, so the constraint is visible before it is
  hit. Greying the branch out in the Branch picker is the follow-up.
- **The worktree list is lazy, the label is not.** The toolbar reads the
  current worktree name from the snapshot; the list is fetched when the picker
  opens. One shell-out per open beats one per refresh.
- **The duplicate Worktree section stays.** With one worktree it repeats the
  repository name. A stable toolbar position is worth more than the reclaimed
  space, and the section is where a second worktree becomes discoverable —
  hiding it until one exists means nobody finds the feature.

### Known debt

- **`src/ui/components/` and `src/ui/primitives/` are dead.** They are tracked
  in git but declared in no `mod.rs`, so nothing compiles them, and they are
  written against **egui** (`use eframe::egui`) — they predate the GPUI
  rewrite. They are not an earlier version of this kit and must not be
  mistaken for one. Delete them.
- **Dialog geometry is still split.** `kit::dialog` owns the width, but
  `app::dialogs` also carries a hand-maintained *height* per dialog for
  centering. Migrated dialogs share the width constant; the heights are still
  guesses typed in one place and rendered in another.
- **Off-scale dialog widths.** `420` and `576` are still in the centering
  table (§8.3 defines 440 / 500 / 720). Fold them in as those dialogs migrate.

---

## 13. Appearance (implemented)

Light is a **token swap**, exactly as this section always demanded — and it is
now built. `theme.rs` holds a preference (`System` / `Light` / `Dark`, in
`AppSettings.appearance`) and a resolved `DARK` flag; every color token is a
`fn` that reads that flag through `pick(dark, light)`. No component was forked.

`main.rs` resolves the preference against `cx.window_appearance()` before the
first frame, and points gpui-component's own `Theme` at the same answer so its
stock widgets don't stay on the old palette. `GitSparkApp::set_appearance`
does the same at runtime and persists.

**The default is `Dark`, not `System`,** deliberately: the app shipped
dark-only, so following the OS would silently flip every existing user on a
light Mac. System is one click away in Settings ▸ Appearance.

### What the light arm actually cost

The plumbing was trivial. The audit was not — and that is the part worth
budgeting for if you add a third arm:

1. **Every hardcoded color blocks the swap.** Any `rgb(0x…)` outside
   `theme.rs` is a light-mode bug, which is why §3 forbids it.
2. **The accent cannot be reused.** `#74ade8` is ~1.9:1 on white. Light
   darkens to `#4257c9`.
3. **Text on the accent flips.** `commit_button_text()` is near-black in dark
   and white in light. A constant white is wrong.
4. **Depth inverts.** The buffer is the darkest surface in dark and the
   brightest in light (§2).
5. **Diff tints are not the same number** in both arms — 13% dark, 10% light.
6. **Shadows lose their black.** Pure black on a light ground reads as dirt;
   it should take the ink hue at lower opacity.
7. **Syntax must be re-picked, not lightened.** One Light chosen for white,
   not One Dark brightened — the pastels vanish otherwise.

---

## 14. Build profiles

GPUI's dependency graph is large enough that build configuration is part of the
development experience, so it is specified here alongside everything else.

| Profile | Setting | Why |
|---|---|---|
| `dev` | `opt-level = 1` | Debug GPUI is unusably slow to run. |
| `dev` | `debug = "line-tables-only"` | Backtraces and panic locations stay accurate; full DWARF costs seconds per link and hundreds of MB. |
| `dev` | `split-debuginfo = "unpacked"` | Skips `dsymutil` on every build. |
| `dev.package."*"` | `opt-level = 2`, `debug = false` | We never step into gpui/syntect/regex. |
| `test` | `debug = "line-tables-only"`, `lto = "off"` | Tests link often and are never profiled. |
| `release` | `lto = "thin"` | Most of fat LTO's cross-crate inlining, parallel across codegen units — a fraction of the link time. |
| `release` | `codegen-units = 16`, `strip = true` | Parallel codegen; thin LTO recovers the inlining. Stripping halves the shipped binary. |

Not adopted: an alternative linker (`lld`/`mold` are not installed, and Apple's
`ld-prime` is already fast on this toolchain) and `panic = "abort"` (GPUI
relies on unwinding).

---

## 15. Do / Don't

**Do**
- route every color through a `theme.rs` token
- use the spacing scale (§5.2) instead of free-handing `z(13.0)`
- wrap perceptual sizes in `z()` and leave hairlines at raw 1px
- use `uniform_list` for anything that can exceed ~20 rows
- give every repeated interactive element a stable, content-derived id
- reach for `kit::` before writing a `div()` chain that looks like a control

**Don't**
- write `rgb(0x…)` outside `theme.rs`
- use `gpui_component::Input`, `Root`, `Badge`, or `Button::icon`
- introduce a second interactive accent, a gradient on chrome, or a heavy shadow
- put a border and a shadow on the same raised surface
- animate anything decoratively
- hardcode a panel width inside a `resizable_panel`

---

## 16. Definition of done

A UI change is not done unless:

- repeated patterns are extracted into `kit::`
- ids are stable and content-derived
- hover / active / selected states are explicit
- geometry comes from tokens, not literals
- lists are virtualized
- it was checked **visually in the native app**, not only through automation —
  hit targets, modal placement, contrast, scrolling, and real keyboard input.
  The automation snapshot must agree with what is on screen (if the UI shows
  `1 changed file`, `test_tree` reports one change).
- `src/ui/app.rs` is simpler, not more crowded
