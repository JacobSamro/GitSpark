# Changelog

## [0.5.0] - 2026-08-21

169 commits since 0.4.0. Grouped by theme rather than listed individually.

### Performance

The app used to run git commands on the UI thread during render, and rebuild
every diff row on every frame. Both are fixed, and both were measured rather
than guessed at.

- **Diff views are virtualized.** Building a row element for every line cost
  14-21ms per frame on a 4000-line diff -- the entire 60fps budget, on every
  frame of every scroll. Now 0.08-0.19ms. `uniform_list` did not apply because
  hunk headers and wrapped lines are taller than diff rows, so both the unified
  and split views use GPUI's variable-height `list`.
- **No git on the render path.** A traced 20-second session ran 206 git
  invocations, 162 of them on the UI thread, blocking for 1.75 seconds. One
  triple repeated ~47 times because `repo_has_github_remote()` shelled out to
  re-derive a value the snapshot already carried, and render called it once per
  visible row that binds a context menu. Now zero main-thread git calls during
  interaction.
- **Reads are backed by [gitoxide](https://github.com/GitoxideLabs/gitoxide).**
  Repo discovery, remote lookup, branches and history run in-process; every
  shell-out costs ~10ms in process spawn alone. Writes stay on the git binary,
  where matching its exact behaviour around hooks, config precedence and
  credential helpers is worth more than milliseconds. `GITSPARK_TRACE_GIT=1`
  logs every invocation with its duration and thread.
- **Build profiles** -- thin LTO and strip on release, line-tables-only debug
  info on dev and test, no debug info for dependencies.

### Features

- **Worktrees** -- a Current Worktree section in the toolbar with a filterable
  picker, plus add and prune. Selecting a worktree opens that directory, which
  is what a worktree is. Branches checked out in another worktree are greyed and
  tagged with the worktree holding them, because git refuses those checkouts.
- **Light and dark themes** -- Zed One Dark, deepened, with a One Light arm.
  Settings > Appearance switches live between System, Light and Dark. Defaults
  to Dark, so existing installs are unchanged.
- **Diff line selection** -- select individual lines to commit or discard, in
  both unified and split views.
- **Split diff mode**, whitespace-change toggle, binary diff fallback, image
  diff preview, and a submodule diff panel.
- **Compare branch view** with ahead/behind, plus compare-on-GitHub.
- **Conflict-aware operations** -- merge and rebase surface conflicted files
  with continue, skip and abort, and open-in-editor per file.
- **Repository create and clone flows**, remote settings, and ignored-files
  settings.
- **Tag creation and deletion**, and branch rename.

### Design system

- **`design.md`** documents the visual language: tokens, component specs,
  elevation, the type ramp, layout rules and build profiles.
- **`src/ui/kit/`** holds the components the app kept hand-rolling -- buttons,
  the modal dialog shell, surfaces, tags, pills, empty states, and the
  filterable picker shared by the repository, worktree, branch and AI-model
  selectors.
- Every colour now resolves through `theme.rs`, which is what makes the light
  arm a token swap rather than a fork.

### Bug fixes

112 fixes, mostly native menu behaviour, settings scoping between global and
per-repository, stash previews and confirmations, and input validation for
branch and tag names. The ones worth calling out:

- **Selected rows drew text with a literal white** in fourteen places. The dark
  accent is a light blue, so white on it was barely legible; in light mode the
  literal ignored the palette entirely.
- **Typing an accented character or emoji into a filter could panic.** Both
  hand-rolled filter fields sliced at a byte offset that clamped to the string
  length but was not guaranteed to be a character boundary.
- **Diff line selection** shaded the whole gutter in a saturated blue on every
  selected line -- and every line starts selected -- making the gutter the
  loudest thing in the diff.

### Removed

- `src/ui/components/` and `src/ui/primitives/` -- 21 tracked files, declared in
  no module, referenced by nothing, and still written against egui. They
  predated the GPUI rewrite and were a trap for anyone looking for the component
  layer.

## [0.4.0] - 2026-04-11

### Features

- **Diff Context Expansion** -- Expand/collapse diff hunks in-memory matching GitHub Desktop behavior. Click hunk headers to expand up/down with 20-line steps. Right-click for "Expand Whole File" / "Collapse Expanded Lines". Supports Up, Down, Both, Short (auto-merge), and EOF expansion types.
- **No-Changes Suggestion Cards** -- When there are no local changes, the workspace shows GitHub Desktop-style suggestion cards (Push commits, Open in Editor, Show in Finder, View on GitHub) with keyboard shortcut badges.
- **Window Position Persistence** -- Window size, position, and display are saved on every move/resize and restored on next launch. Multi-monitor aware via display ID tracking.
- **Text Selection & Clipboard** -- Full text selection support in commit summary/description and all settings fields. Cmd+A (select all), Cmd+C (copy), Cmd+X (cut), Cmd+V (paste replaces selection), Shift+Arrow/Home/End (extend selection). Blue highlight for selected text.
- **Reusable Text Field Component** -- Extracted `text_field` module with `TextFieldState`, `handle_text_key`, and `render_text_content` used across commit form and settings modal.
- **Network Dropdown** -- Fetch origin dropdown with description text, backdrop dismiss, and proper overlay positioning. Rotate-cw icon with spin animation during fetch.
- **Settings Modal Redesign** -- Horizontal tab bar navigation (Git, AI Commit, Appearance, Integrations). Collapsible model picker for OpenRouter. Click-outside-to-close behavior.
- **Custom SVG Icons** -- Git-branch, lock, rotate-cw, chevrons-up/down, unfold-vertical, dot-square icons from Lucide via asset embedding system.
- **Auto-Refresh on Focus** -- Git changes automatically refresh when the application window gains focus.
- **Minimum Window Size** -- Set to 720x480px to prevent unusable layouts.
- **File Diff Refresh on Click** -- Clicking a file in the Changes list re-fetches its diff from disk for up-to-date content.

### Improvements

- **Toolbar Icons** -- Repository icon varies by type (lock for remote repos, folder for local). Branch icon uses git-branch SVG instead of GitHub logo.
- **Active Selection Highlighting** -- Selected items in file list, commit list, and commit file list use blue accent background with white text. Hover uses a subtle lighter background that doesn't override selection.
- **Checkbox Styling** -- Light blue background (#58a6ff) with dark tick mark, properly centered using flex layout.
- **File Status Icons** -- Replaced text-based tags (M/A/D) with dot-square SVG icons colored by status (orange modified, green added, red deleted, white when selected).
- **Push/Pull Button** -- Reduced width, proper badge spacing, correct pending action label (shows "Fetching..." not "Pushing..." when fetching).
- **Commit Button** -- Enabled when summary is non-empty regardless of file count.
- **Font Size** -- Default increased from 12px to 13px.
- **Description Field** -- Scrollable when content exceeds container height.
- **Placeholder Persistence** -- Input placeholders remain visible when focused until text is typed.
- **Duplicate Headers Removed** -- Repository and branch selector panels no longer duplicate the toolbar header.

### Bug Fixes

- **Untracked File Diffs** -- New/untracked files now show their full content as added lines instead of "No textual diff available".
- **Root Commit Diffs** -- `git diff-tree --root` flag added so the initial commit shows changed files.
- **Git Config Reading** -- Removed `--local` flag so user.name/email from global git config are displayed.
- **AI Error Handling** -- OpenRouter API errors now show the actual error message from the response body instead of a generic "401". Added HTTP-Referer and X-Title headers.
- **Network Action Labels** -- Toolbar shows the actual running action's pending title (e.g. "Fetching..." when fetch is active, not "Pushing...").
- **Scroll Clipping** -- Added 300px bottom padding to diff viewer to prevent last lines from being clipped by the scroll container.
- **Button Centering** -- "Add" and "New Branch" buttons use h_flex for proper vertical text centering.
- **Percentage Panic** -- Fixed `Transformation::rotate(percentage(...))` crash by passing 0.0-1.0 range instead of 0-100.

### Technical

- Default branch option now persisted in local settings file alongside window configuration.
- `DiffEntry` stores `file_contents` and `original_diff` for in-memory expansion and collapse.
- `expand_diff_in_memory` function handles Up/Down/All/MergeWithPrevious directions with proper 0-based/1-based index conversion.
- Hunk header suffixes (function context text) preserved during expansion.
- `ExpansionType` enum: None, Up, Down, Both, Short -- computed from hunk boundaries and gap analysis.

## [0.3.3] - 2026-04-10

- macOS titlebar double-click support
- UI selection and layout improvements

## [0.3.2] - 2026-04-09

- Windows title bar controls fix
- App icons added
- Prevent Git command windows on Windows

## [0.3.1] - 2026-04-08

- Branch and changes context menus
- Tags in commit info
- Improved commit UI and sidebar interactions
