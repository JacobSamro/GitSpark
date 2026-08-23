import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { assert } from "../support/assertions.mjs";
import { nodeById } from "../support/tree.mjs";

const exec = promisify(execFile);

export async function testKeyboardFocusPaths(app, fixture) {
  const repo = await createKeyboardRepo();
  await app.openRepo(repo);
  let snapshot = await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.path === repo &&
      snapshot.sidebar_tab === "changes" &&
      snapshot.repo.changes.some((change) => change.path === "README.md"),
    { timeoutMs: 10_000 },
  );
  if (snapshot.active_dialog !== "none") {
    await app.getByTestId("dialog-cancel").click();
    await app.waitForSnapshot(
      (snapshot) => snapshot.active_dialog === "none",
      { timeoutMs: 10_000 },
    );
  }

  await app.getByTestId("input-commit-summary").typeText("Keyboard commit");
  await app
    .getByTestId("input-commit-body")
    .typeText("Typed with real keys\nSecond line");
  snapshot = await app.snapshot();
  assert(snapshot.commit_summary === "Keyboard commit", "summary accepts real key typing");
  assert(
    snapshot.commit_body === "Typed with real keys\nSecond line",
    "body accepts multiline real key typing",
  );

  await app.getByTestId("input-commit-summary").press("right");
  snapshot = await app.snapshot();
  assert(snapshot.sidebar_tab === "changes", "arrow keys do not switch tabs while summary is focused");

  // "secondary" resolves to cmd on macOS and ctrl on Linux/Windows, matching
  // what the app actually checks (Modifiers::secondary()) — the native E2E
  // job runs on Linux, where a literal "cmd-enter" only sets the platform
  // (Super) modifier and never fires the commit shortcut.
  await app.getByTestId("input-commit-summary").press("secondary-enter");
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Commit created." &&
      snapshot.repo?.changes.length === 0,
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("button-branch-selector").click();
  await app.getByTestId("input-branch-filter").typeText("feature");
  snapshot = await app.waitForSnapshot(
    (snapshot) => snapshot.branch_filter_text === "feature",
    { timeoutMs: 10_000 },
  );
  assert(snapshot.show_branch_selector === true, "branch selector stays open while typing filter");
  await app.getByTestId("input-branch-filter").press("escape");
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.show_branch_selector === false && snapshot.branch_filter_text === "",
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("button-repo-selector").click();
  await app.getByTestId("input-repo-filter").fill("");
  await app.getByTestId("input-repo-filter").typeText("keyboard");
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.show_repo_selector === true && snapshot.repo_filter_text === "keyboard",
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("button-repo-selector").click();
  await app.waitForSnapshot((snapshot) => snapshot.show_repo_selector === false);

  await app.command({ command: "show_global_settings", show: true });
  await app.getByTestId("settings-tab-git").click();
  await app.getByTestId("settings-git-user-name").fill("");
  await app.getByTestId("settings-git-user-name").typeText("Keyboard User");
  await app.waitForSnapshot(
    (snapshot) => snapshot.git_user_name.endsWith("Keyboard User"),
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("settings-git-user-name").press("escape");
  await app.waitForSnapshot((snapshot) => snapshot.show_settings === false);

  await app.getByTestId("button-branch-selector").click();
  await app.getByTestId("button-branch-new").click();
  await app.getByTestId("input-new-branch-name").press("enter");
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "create_branch" &&
      nodeById(snapshot.test_tree, "dialog-create-branch")?.enabled === false,
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("input-new-branch-name").typeText("keyboard-input");
  await app.getByTestId("input-new-branch-name").press("enter");
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "none" &&
      snapshot.repo?.current_branch === "keyboard-input" &&
      snapshot.status_message === "Switched to branch 'keyboard-input'.",
    { timeoutMs: 15_000 },
  );

  snapshot = await app.snapshot();
  const head = snapshot.repo.history[0];
  await app.command({
    command: "history_action",
    oid: head.oid,
    action: "create_tag",
  });
  await app.getByTestId("create-tag-name-input").press("enter");
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "create_tag" &&
      nodeById(snapshot.test_tree, "create-tag-confirm")?.enabled === false,
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("create-tag-name-input").typeText("keyboard-tag");
  await app.getByTestId("create-tag-name-input").press("enter");
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "none" &&
      snapshot.status_message === "Created tag 'keyboard-tag' complete." &&
      snapshot.repo?.history.some((commit) =>
        commit.tags.includes("keyboard-tag"),
      ),
    { timeoutMs: 15_000 },
  );

  await app.openRepo(fixture.workRepo);
  await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.path === fixture.workRepo,
    { timeoutMs: 10_000 },
  );
}

async function createKeyboardRepo() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-keyboard-e2e-"));
  const repo = path.join(root, "keyboard-repo");
  await exec("git", ["init", "-b", "main", repo]);
  await exec("git", ["config", "user.name", "GitSpark E2E"], { cwd: repo });
  await exec("git", ["config", "user.email", "e2e@gitspark.local"], { cwd: repo });
  await fs.writeFile(path.join(repo, "README.md"), "# Keyboard\n");
  await exec("git", ["add", "README.md"], { cwd: repo });
  await exec("git", ["commit", "-m", "initial"], { cwd: repo });
  await exec("git", ["branch", "feature/keyboard"], { cwd: repo });
  await fs.writeFile(path.join(repo, "README.md"), "# Keyboard\n\nchanged\n");
  return fs.realpath(repo);
}
