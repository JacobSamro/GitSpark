# GitSpark E2E Automation

GitSpark exposes a small native automation channel for end-to-end tests. It is disabled by default and only starts when the app is launched with `GITSPARK_AUTOMATION`.

The automation channel is JSON-lines over TCP. Each request is one JSON object followed by a newline, and each response is one JSON object followed by a newline.

## Launch

```sh
GITSPARK_AUTOMATION=1 cargo run
```

By default the app listens on `127.0.0.1:7878`.

Use a fixed custom port:

```sh
GITSPARK_AUTOMATION=9000 cargo run
```

Use an explicit bind address and write the bound address to a ready file:

```sh
GITSPARK_AUTOMATION_ADDR=127.0.0.1:0 \
GITSPARK_AUTOMATION_READY_FILE=/tmp/gitspark-automation.addr \
cargo run
```

The Node launcher waits up to five minutes for the automation ready file by default. Override this for slower or faster environments:

```sh
GITSPARK_E2E_LAUNCH_TIMEOUT_MS=600000 node e2e/full-suite.mjs
```

Use an isolated settings file or config directory for tests:

```sh
GITSPARK_CONFIG_DIR=/tmp/gitspark-test-config cargo run
GITSPARK_SETTINGS_PATH=/tmp/gitspark-settings.toml cargo run
```

Use safe command overrides for external launch actions during automation:

```sh
GITSPARK_REVEAL_COMMAND=/usr/bin/true cargo run
GITSPARK_OPEN_COMMAND=/usr/bin/true cargo run
GITSPARK_OPEN_URL_COMMAND=/usr/bin/true cargo run
```

## Client

The repo includes a tiny client for manual checks:

```sh
cargo run --example automation_client -- '{"command":"ping"}'
cargo run --example automation_client -- '{"command":"snapshot"}'
```

Selector-driven tests can use the Node client in `e2e/gitspark.mjs`:

```js
import { GitSparkAutomation, expect } from "./e2e/gitspark.mjs";

const app = await GitSparkAutomation.launch();
await app.openRepo("/absolute/path/to/repo");
await app.getByTestId("change-src-main-rs").click();
await app.getByTestId("input-commit-summary").fill("test: commit");
await app.getByTestId("button-commit-all").click();
await expect(app.getByText("Commit created.")).toBeVisible();
await app.close();
```

There is also a smoke flow that creates a temporary git repo, opens the real app, commits a change, and asserts the status text:

```sh
node e2e/selector-smoke.mjs
```

For broader local coverage, run the full suite. It creates a fresh temporary sample repo plus a local bare `origin` and local mock AI server, then exercises selectors, repo loading, refresh, file watcher auto-sync, tabs, filters, settings visibility and persistence, AI validation/success, change/history selection, history checkout/revert/cherry-pick through branch selection, history copy-SHA/copy-diff, commit/branch/file GitHub URL actions through a safe URL opener, branch switching, create-branch dialog cancel/fill/confirm, create branch from commit, branch creation, branch merging, branch deletion, stash/restore, commit validation, commit success, undo last commit, fetch/pull/push, clipboard path copy, ignore path/extension, discard confirmation cancel/confirm, direct discard, safe external-editor dispatch, safe reveal-in-Finder dispatch, and safe default-app dispatch:

```sh
node e2e/full-suite.mjs
```

`e2e/full-suite.mjs` is only the runner. Maintainable coverage lives under `e2e/suites/`, with shared fixture, mock AI, assertion, and URL-log helpers under `e2e/support/`.

CI runs `node --check` for all E2E scripts and runs the native full suite on `ubuntu-24.04` under `xvfb` so the real GPUI desktop app is built, launched, and driven through the automation channel.

Current full-suite coverage:

```text
Covered: automation startup, test tree, getByTestId, getByText, click, fill, repo selector visibility/filter state, settings visibility, fresh repo open, refresh, file watcher auto-sync, change selection, sidebar tab switching, commit validation error, commit success with body, history selection, branch switching, push, fetch, pull.
Covered: isolated app settings persistence, repository Git config persistence, AI missing-key validation, AI success through a local OpenAI-compatible mock server, copy relative path, copy full path, copy commit SHA, copy commit diff, checkout commit, revert commit, cherry-pick branch selector flow, commit/branch/file GitHub URL actions using a safe URL opener, create-branch dialog cancel/fill/confirm, create branch from commit, branch creation, branch merge, branch deletion, stash/restore, stash-and-switch dialog cancel/confirm for conflicting branch switches, ignore extension, ignore path, discard confirmation cancel/confirm, direct discard tracked-file changes, external-editor dispatch using a safe test editor, reveal-in-Finder/default-app dispatch using safe command overrides, undo last commit.
Not covered yet: real external AI provider availability, actual Finder/default app GUI behavior after OS handoff, visual regression, and cross-platform OS automation permissions.
```

When using a non-default port:

```sh
GITSPARK_AUTOMATION_ADDR=127.0.0.1:9000 \
cargo run --example automation_client -- '{"command":"snapshot"}'
```

## Commands

```json
{"command":"ping"}
{"command":"snapshot"}
{"command":"test_tree"}
{"command":"clipboard_text"}
{"command":"query","selector":{"by":"test_id","value":"change-src-main-rs"}}
{"command":"query","selector":{"by":"text","value":"Commit created."}}
{"command":"click","selector":{"by":"test_id","value":"change-src-main-rs"}}
{"command":"fill","selector":{"by":"test_id","value":"input-commit-summary"},"text":"test commit"}
{"command":"open_repo","path":"/absolute/path/to/repo"}
{"command":"refresh_repo"}
{"command":"select_tab","tab":"changes"}
{"command":"select_tab","tab":"history"}
{"command":"select_change","path":"src/main.rs"}
{"command":"select_commit","oid":"abc123..."}
{"command":"set_commit_message","summary":"test commit","body":"optional body"}
{"command":"commit_all"}
{"command":"undo_last_commit"}
{"command":"stash_all"}
{"command":"stash_pop"}
{"command":"show_settings","show":true}
{"command":"show_repo_selector","show":true}
{"command":"set_repo_filter","text":"rust"}
{"command":"set_branch_filter","text":"main"}
{"command":"set_settings_section","section":"git"}
{"command":"set_settings_field","field":"ai_model","text":"gpt-4.1-mini"}
{"command":"set_settings_field","field":"ai_endpoint","text":"https://api.openai.com/v1/chat/completions"}
{"command":"save_settings","section":"ai"}
{"command":"change_ai_provider","provider":"openai_compatible"}
{"command":"generate_ai_commit"}
{"command":"change_action","path":"scratch.log","action":"ignore_extension"}
{"command":"change_action","path":"README.md","action":"prompt_discard"}
{"command":"change_action","path":".gitignore","action":"reveal_in_finder"}
{"command":"change_action","path":".gitignore","action":"open_in_editor"}
{"command":"change_action","path":".gitignore","action":"open_with_default"}
{"command":"history_action","oid":"abc123...","action":"checkout_commit"}
{"command":"history_action","oid":"abc123...","action":"revert_changes_in_commit"}
{"command":"history_action","oid":"abc123...","action":"cherry_pick_commit"}
{"command":"history_action","oid":"abc123...","action":"view_on_github"}
{"command":"history_action","oid":"abc123...","action":"copy_sha"}
{"command":"history_action","oid":"abc123...","action":"copy_diff"}
{"command":"branch_action","name":"delete/me","action":"delete"}
{"command":"branch_action","name":"main","action":"view_on_github"}
{"command":"create_branch","name":"e2e-created"}
{"command":"merge_branch","name":"merge/source"}
{"command":"network_action","action":"fetch"}
{"command":"network_action","action":"pull"}
{"command":"network_action","action":"push"}
```

Responses have this shape:

```json
{"ok":true,"result":{}}
```

Errors have this shape:

```json
{"ok":false,"error":"message"}
```

Long-running operations such as `open_repo`, `commit_all`, and network actions return after dispatching the app action. Tests should poll `snapshot` until `status_message`, `error_message`, `repo`, `repo.changes`, or `repo.history` reaches the expected state.

## Test IDs

The automation layer exposes a semantic test tree derived from app state. It is not a DOM, but it gives tests stable selector targets and actions.

Stable IDs include:

```text
gitspark-root
tab-changes
tab-history
input-commit-summary
input-commit-body
button-commit-all
button-undo-last-commit
button-settings
button-generate-ai-commit
button-repo-selector
button-branch-selector
button-branch-new
input-repo-filter
input-branch-filter
input-new-branch-name
status-message
error-message
stash-indicator
dialog-cancel
dialog-create-branch
discard-cancel
discard-confirm
stash-cancel
stash-switch
settings-tab-git
settings-tab-ai
settings-git-user-name
settings-git-user-email
settings-git-default-branch
settings-ai-model
settings-ai-endpoint
settings-ai-api-key
settings-ai-system-prompt
settings-openrouter-model-filter
settings-save-git
settings-save-ai
changes-list
history-list
branch-list
button-network-fetch
button-network-pull
button-network-push
```

Repeated rows use slugged domain keys:

```text
change-src-main-rs
change-src-main-rs-copy-relative-path
change-src-main-rs-copy-full-path
change-src-main-rs-reveal-in-finder
change-src-main-rs-ignore-path
change-src-main-rs-ignore-extension
change-src-main-rs-discard
change-src-main-rs-prompt-discard
change-src-main-rs-open-in-editor
change-src-main-rs-open-with-default
change-src-main-rs-view-on-github
commit-abc1234
commit-abc1234-checkout
commit-abc1234-revert
commit-abc1234-cherry-pick
commit-abc1234-create-branch
commit-abc1234-copy-sha
commit-abc1234-copy-diff
commit-abc1234-view-on-github
branch-main
branch-delete-me-delete
branch-delete-me-view-on-github
```
