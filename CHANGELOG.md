# Changelog

## [Unreleased]

## [0.8.2] - 2026-08-23

### Fixed

- **Multi-monitor window restore was broken.** Closing the app on a secondary
  display and clicking the Dock icon reopened the window back on the primary
  display instead. Window state (size, position, display) is now properly
  restored through the same channel the initial launch uses.

- **Commit by Cmd+Enter (Ctrl+Enter) failed on Linux CI and would fail on
  real Linux and Windows users.** The test sent a literal `"cmd-enter"`
  keystroke, which GPUI resolved to the Cmd/Super modifier specifically, not
  the cross-platform "secondary" modifier the app checks. Added the correct
  keystroke string for the fix; also found and repaired two real race
  conditions in the stash E2E tests (clicking dialogs before waiting for
  them to open).

- **Config reads crashed on Windows.** Checking whether a git config key was
  set (like `pull.rebase`, or a user identity field) threw a spurious hard
  error on Windows instead of treating the missing key as absent. The code
  pattern that detects a missing key matches `std::process::ExitStatus`'s
  string representation, which reads `"exit status: 1"` on Unix but `"exit
  code: 1"` on Windows; only the Unix wording was matched. Real Windows users
  would have hit this anywhere the app checks an unset config key.

- **File content assertions failed on Windows.** Four tests write a file with
  an explicit newline, clone or check it out, and compare the bytes back —
  Git for Windows defaults `core.autocrlf` to true (pinned false on
  Linux/macOS builds), silently converting `\n` to `\r\n` on checkout. The
  bytes would never match on Windows. Git for Windows now has autocrlf
  disabled in CI to match the behavior everywhere else.

## [0.8.1] - 2026-08-22

### Fixed

- **Closing the window and clicking the Dock icon did nothing.** Closing the
  last window correctly left the app running, same as any other Mac app, but
  clicking the Dock icon to bring it back did nothing — the only way in was
  Force Quit and relaunch. GPUI installs the reopen delegate method
  regardless of whether the app hooks it, which replaces macOS's own default
  handling; with no handler registered, the click had nothing to do. A
  window now reopens, reusing the existing view, so a half-typed commit or
  an open tab survives the round trip.

- **Every failure toast hid the actual git error.** `Pull origin failed:
  failed to pull from 'origin'` — and 48 messages just like it — showed only
  a context wrapper repeating itself, never git's own reason. The real
  cause was already attached the whole time through `anyhow`'s context
  chain; the code printing it was just dropping everything but the
  outermost message. Every failure toast in the app now shows the full
  chain, so a rejected push or a failed pull says why.

## [0.8.0] - 2026-08-22

Push and pull now show you something is happening, history tells you what
hasn't shipped yet, and a background refresh can no longer knock you out of
what you were doing.

### Features

- **Push and pull animate now.** The arrow used to swap to a static icon and
  sit there until the operation finished — no feedback for the whole
  duration. It now nudges toward the direction commits are actually
  travelling, and a thin accent rail sweeps along the bottom of the button
  while the operation is in flight. Indeterminate, since git gives no
  byte-level progress to report.

- **Unpushed commits show an up arrow in history.** Muted grey on an
  unselected row, white on a selected one, so it doesn't compete with tags or
  HEAD — those are real labels; this is a state that clears itself the
  moment the commit is pushed. History is newest-first, the same order
  `ahead` counts from, so the first *N* rows are exactly the commits git
  hasn't pushed yet; no new field needed on top of what the snapshot already
  carries.

- **`⌘R` / `Ctrl+R` reloads the repository.** A local-only re-read of the
  working tree and history — the same thing the watcher already does on a
  change it notices itself, just on demand for whatever it hasn't noticed
  yet. No network call, so it works with no remote configured. Also in the
  Repository menu as **Reload**.

- **New app icon.** The old one was a busy, AI-generated rocket with a
  rainbow trail and a gradient hull that never read at Dock or menu-bar
  size. Replaced with a single flat rocket silhouette on the app's own
  background colour, angled bottom-left to top-right, with its fins drawn as
  small mirrored bolts — a nod back to the name.

### Fixed

- **A background refresh could reset what you were looking at.** Any
  filesystem change near the repository — including a commit you made
  yourself in a terminal — triggered a refresh that always jumped the
  Changes tab to the first file and reloaded the selected commit's diff,
  which blanked the diff pane and snapped the file selection back to the top
  even when nothing you cared about had changed. Both now only reset when
  the previous selection has actually disappeared from the new snapshot.

- **Occasional `Unable to create '.git/index.lock'` errors while GitSpark was
  running.** Every git call GitSpark makes — status, log, diff, the
  watcher's own polling — opportunistically rewrote the on-disk index to
  cache refreshed file-stat info, which briefly takes the index lock to do
  it. If your own `git add`/`commit`/`checkout` landed inside that window,
  your command lost the race. GitSpark's own reads now pass
  `--no-optional-locks` and never take the lock at all; writes you make in
  the app still take the locks they actually need.

- **The worktree, branch, and network dropdowns could open in the wrong
  place.** All three positioned themselves as if worktree, branch, and
  network were still one row in one container, measuring left offsets from
  the window's own edge — true before the toolbar split into two
  independently resizable panels, wrong since. Each now opens directly
  under the section that owns it, regardless of where the sidebar divider
  currently sits.

## [0.7.0] - 2026-08-21

Several repositories open at once, and a diff that finally looks like one.

### Features

- **Repository tabs.** One tab per open repository, in the window's title-bar
  row beside the traffic lights, with a changed-file badge and a `+` that opens
  the repository list. `⌘T` opens the list, `⌘W` closes the active tab, and
  `⌃Tab` / `⌃⇧Tab` or `⌘⇧]` / `⌘⇧[` cycle. **Not** `⌘1`–`⌘9` — those already
  switch Changes and History, a far more frequent move.

  Tabs and the active one persist, so a relaunch reopens them. Only the active
  repository keeps a filesystem watcher; a background tab reloads when switched
  to, rather than costing a watcher and a status refresh while nobody is
  looking at it. A half-typed commit message belongs to its tab and survives
  switching away and back.

  The toolbar's "Current Repository" section is gone — the strip replaced it,
  and keeping both would give one piece of state two controls that can
  disagree. The worktree section took the vacated slot, since it sits directly
  above the sidebar and the sidebar lists that worktree's changes.

- **Drag a folder onto the window to open it.** Whatever is dropped resolves to
  its work-tree root first, so a file or a nested directory opens the repository
  containing it. Each one opens as a tab and lands in the recents list. Anything
  outside a repository is reported rather than silently ignored.

- **Syntax highlighting in the diff.** `syntect` had been a dependency with no
  uses in the codebase; it is now wired, mapping syntect *scopes* onto the
  design's six hues rather than using a syntect theme, so highlighting survives
  a theme switch. Memoised per line, because diff parsing runs on the render
  path. The syntax set comes from `two-face` rather than syntect's own, which
  ships 75 languages and carries neither Swift nor TypeScript.

### Fixed

- **The diff was not monospaced.** `font_family("monospace")` never resolved on
  macOS — GPUI looks up one family name verbatim, so the CSS generic matched
  nothing and every diff, SHA and path rendered in the proportional system
  font. That is why columns never lined up. The split view had the same bug in
  a louder form, passing an entire CSS fallback list as a single family name.

- **The diff palette now comes from GitHub Desktop**, read from its source
  rather than approximated. Three values were the *opposite* of what we had:
  the gutter is darker than the row, deleted text is tinted, and selection is a
  real blue filling the line-number cells. Deriving the diff from the Zed hues
  had kept producing something visibly duller — Zed's created/deleted are muted
  olive and brick where GitHub's are saturated true green and red.

- **Clicking a diff line no longer toggles it.** The whole row was a toggle, so
  selecting a token to copy silently changed what was staged. Only the gutter
  acts now, and hovering it says so.

- **A tab switch could delete a tab.** Repository loads run on a worker thread
  and were adopted without checking which repository they answered, so
  switching faster than a load completed pointed the active tab at the wrong
  repository — where it looked like a duplicate and was removed.

- The history row follows GitHub Desktop's commit list exactly: 50px rows,
  their padding and separator, a semibold summary over an author line, and
  their badge pills.
- The UI scales a step larger. GPUI draws with grayscale antialiasing where
  Chromium leaves macOS subpixel AA on, so the same font at the same size came
  out thinner and read smaller.
- The diff options control is a real gear rather than an 11px `⚙` glyph.
- Traffic lights sat above centre; their offset and the title-bar height now
  derive from one constant.
- **The macOS bundle had no icon**, so the Dock and Finder drew the generic
  blank document. It now carries a real `.icns` built from the source art at
  every size from 16 to 512 with @2x twins, referenced by `CFBundleIconFile`.

### Build

- **CI and releases cache their dependencies.** Every release rebuilt GPUI's
  entire dependency graph from scratch on three platforms, which was most of an
  18-minute run. `sccache` now caches compiled crates across runs, alongside the
  cargo registry and target directory, keyed on `Cargo.lock` so a dependency
  change misses and nothing else does.

## [0.6.1] - 2026-08-21

Security patches, and the fix that makes v0.6.0's updater actually work.

### Fixed

- **The update manifest never published.** The release workflow generated and
  signed it correctly, then skipped committing it: the guard used
  `git diff --quiet -- updates`, and `git diff` only reports *tracked* files. On
  the first release the whole `updates/` tree is untracked, so it reported "no
  changes" and exited before the `git add` below it ever ran. The first manifest
  could therefore never publish, which is the one case that had to work. It now
  stages first and compares the index, and rebases before pushing so a release
  that takes minutes to build cannot lose the manifest to a race on `master`.

  **v0.6.0 shipped with no manifest and cannot see this release.** 0.6.1 needs a
  manual install; updates are automatic from there.

### Security

Six advisories closed in the dependency graph. Lockfile only, no API changes.

- **rustls-webpki 0.103.9 → 0.103.14** — a high-severity denial of service via a
  panic on a malformed CRL BIT STRING, plus three lower name-constraint and
  CRL-matching issues. This backs TLS for every AI request *and* every update
  check, so it is the one that mattered most — and v0.6.0 shipped with it.
- **quinn-proto 0.11.14 → 0.11.17** — high-severity remote memory exhaustion from
  unbounded out-of-order stream reassembly.
- **rand 0.8.5 → 0.8.7 and 0.9.2 → 0.9.5** — an unsoundness advisory affecting
  both majors present in the graph.

Not fixed: the `grid` integer overflow needs grid 1.0.1, but taffy 0.9 requires
grid `^0.18` and gpui 0.2.2 pins taffy 0.9, so it cannot move until gpui does. It
is also unreachable here — the overflow is on taffy's CSS Grid layout path, and
this app lays out entirely with flex and never sets `Display::Grid`.

## [0.6.0] - 2026-08-21

Signed auto-update, plus the seven issues closed since v0.5.0.

### Features

- **Auto-update, Zed-style.** A check runs ten seconds after launch, downloads
  and verifies in the background, and puts a **Restart to update** button at the
  top right of the title bar. Clicking it replaces the installed application and
  relaunches into it. Nothing is applied without the user asking: the download is
  silent, the restart is not.

  Two gates protect it, in this order. The channel manifest carries a detached
  Ed25519 signature verified against a key compiled into the binary, so whoever
  serves the metadata cannot substitute their own; then the artifact's SHA-256 is
  checked against the digest in that verified manifest before anything is
  unpacked. A build with no key baked in **refuses to check** rather than
  accepting an unsigned manifest — accepting one would hand the metadata host the
  ability to choose what code runs. The release workflow re-derives the public key
  from the signing secret and fails the release if it does not match the key
  shipped in the app.

  Two channels, derived from the running build's own version rather than a stored
  preference: a prerelease follows beta, everything else follows stable. There is
  no setting to get out of sync.

  **Note for existing installs:** v0.5.0 shipped without the updater, so it cannot
  update itself to this release. 0.6.0 needs a manual install; updates are
  automatic from here.

### Performance

- **Batched diff calls** (#7) — `build_diffs` ran two `git diff` subprocesses
  per changed file. It now runs two in total and splits the output per path.
  Measured with 20 modified files: **40 diff subprocesses down to 2**.
- **OS-level file watching** (#8) — replaced the 3-second `git status` poll with
  FSEvents / inotify / ReadDirectoryChangesW via `notify`. Measured on an idle
  repository: **zero git calls across 12 seconds**, where the poll ran four, and
  a new file is detected in ~700ms rather than up to 3s. The poll survives as a
  fallback for filesystems the watcher refuses.
- **Window bounds no longer written from `render()`** (#9) — persisting ran a
  `create_dir_all`, a TOML serialize and a blocking `fs::write` on the UI thread
  on essentially every frame of a resize. Writes are now debounced onto a
  background thread: **one write across 40 resize steps**.

### Bug fixes

- **AI: OpenAI-compatible provider was unusable** (#5) — switching provider only
  reset the endpoint when it was empty, so going OpenRouter → OpenAI-compatible
  left requests pointed at `openrouter.ai` carrying an OpenAI key. A value
  matching a known provider default is now replaced; a genuinely custom endpoint
  (local llama.cpp, vLLM) is preserved.
- **AI requests could hang forever** (#6) — no HTTP timeouts, so an unreachable
  endpoint blocked its worker thread permanently and the UI sat on "Generating
  commit details..." Connect 10s, send 30s, receive 120s. The model-catalogue
  fetch had the same defect and is fixed too.
- **Untracked-file diffs were malformed** (#10) — the body was truncated to 400
  lines while the hunk header reported the file's full line count. The header now
  describes what is actually emitted, and the cap rises to 5000 now that the diff
  view is virtualized.
- **Collapse could become unreachable** (#11) — the "Collapse Expanded Lines"
  menu was attached in three of five hunk-header paths, so some expansion
  sequences left no route back.
- **Dead expand controls** (#12) — expansion reads the working-tree file, so for
  a deleted or unreadable one the action returned early while the control was
  still drawn. No contents, no affordance.

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
