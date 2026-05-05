import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import path from "node:path";
import { promisify } from "node:util";

import { expect } from "../gitspark.mjs";
import { assert, shortOid } from "../support/assertions.mjs";
import { createRemoteOnlyCommit } from "../support/fixtures.mjs";
import { waitForOpenUrl } from "../support/url-log.mjs";

const exec = promisify(execFile);

export async function testCommitFlow(app) {
  await app.getByTestId("input-commit-summary").fill("");
  await app.command({ command: "commit_all" });
  await expect(app.getByText("Commit summary cannot be empty.")).toBeVisible({
    timeoutMs: 10_000,
  });

  await app
    .getByTestId("input-commit-summary")
    .fill("test: selector full suite");
  await app
    .getByTestId("input-commit-body")
    .fill("Covers selector-driven e2e commit flow.");
  await app.getByTestId("button-commit-all").click();
  await expect(app.getByText("Commit created.")).toBeVisible({
    timeoutMs: 15_000,
  });

  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.changes.length === 0 &&
      snapshot.repo?.history[0]?.summary === "test: selector full suite",
    { timeoutMs: 10_000 },
  );
}

export async function testCreateBranchDialog(app) {
  await app.getByTestId("button-branch-selector").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.show_branch_selector === true,
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("input-branch-filter").fill("dialog-created");
  await app.getByTestId("button-branch-new").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.active_dialog === "create_branch",
    { timeoutMs: 10_000 },
  );
  await expect(app.getByTestId("dialog-cancel")).toBeVisible({
    timeoutMs: 10_000,
  });
  await app.getByTestId("dialog-cancel").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "none" &&
      !snapshot.repo?.branches.some((branch) => branch.name === "dialog-created"),
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("button-branch-new").click();
  await expect(app.getByTestId("dialog-create-branch")).toBeVisible({
    timeoutMs: 10_000,
  });
  await app.getByTestId("dialog-create-branch").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Switched to branch 'dialog-created'." &&
      snapshot.repo?.current_branch === "dialog-created" &&
      snapshot.repo?.branches.some((branch) => branch.name === "dialog-created"),
    { timeoutMs: 15_000 },
  );

  await app.getByTestId("branch-main").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.current_branch === "main",
    { timeoutMs: 15_000 },
  );

  await app.getByTestId("button-branch-selector").click();
  await app.getByTestId("input-branch-filter").fill("dialog-created");
  await app.getByTestId("button-branch-new").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.active_dialog === "create_branch",
    { timeoutMs: 10_000 },
  );
  await expect(
    app.getByText("A branch named dialog-created already exists."),
  ).toBeVisible({ timeoutMs: 10_000 });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.test_tree?.children?.some(
        (node) => node.id === "dialog-create-branch" && node.enabled === false,
      ),
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("dialog-cancel").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.active_dialog === "none",
    { timeoutMs: 10_000 },
  );

  await app.command({
    command: "branch_action",
    name: "dialog-created",
    action: "rename",
  });
  await app.waitForSnapshot(
    (snapshot) => snapshot.active_dialog === "rename_branch",
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("rename-branch-name-input").fill("main");
  await expect(app.getByText("A branch named main already exists.")).toBeVisible({
    timeoutMs: 10_000,
  });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.test_tree?.children?.some(
        (node) => node.id === "rename-branch-confirm" && node.enabled === false,
      ),
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("rename-branch-cancel").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.active_dialog === "none",
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("button-branch-selector").click();
  await app.getByTestId("input-branch-filter").fill("feature branch?");
  await app.getByTestId("button-branch-new").click();
  await expect(
    app.getByText(
      "Will be created as feature-branch-. Spaces and invalid characters have been replaced by hyphens.",
    ),
  ).toBeVisible({ timeoutMs: 10_000 });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "create_branch" &&
      snapshot.test_tree?.children?.some(
        (node) => node.id === "dialog-create-branch" && node.enabled === true,
      ),
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("dialog-cancel").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.active_dialog === "none",
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("button-branch-selector").click();
  await app.getByTestId("input-branch-filter").fill("");
  await app.getByTestId("button-branch-new").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.active_dialog === "create_branch",
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("input-new-branch-name").fill("modal-typed");
  await app.getByTestId("dialog-create-branch").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Switched to branch 'modal-typed'." &&
      snapshot.repo?.current_branch === "modal-typed" &&
      snapshot.repo?.branches.some((branch) => branch.name === "modal-typed"),
    { timeoutMs: 15_000 },
  );
  await app.getByTestId("branch-main").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.current_branch === "main",
    { timeoutMs: 15_000 },
  );
}

export async function testHistoryAndBranchFlows(app, fixture) {
  await app.getByTestId("tab-history").click();
  await expect(app.getByText("test: selector full suite")).toBeVisible({
    timeoutMs: 10_000,
  });
  await app.getByText("test: selector full suite").click();
  const selectedSnapshot = await app.waitForSnapshot((snapshot) =>
    snapshot.repo?.history.some(
      (commit) => commit.oid === snapshot.selected_commit,
    ),
  );
  const selectedCommit = selectedSnapshot.repo.history.find(
    (commit) => commit.oid === selectedSnapshot.selected_commit,
  );
  assert(selectedCommit, "selected history commit is present in snapshot");

  await app
    .getByTestId(`commit-${selectedCommit.short_oid}-copy-sha`)
    .click();
  assert(
    (await app.clipboardText()).text === selectedCommit.oid,
    "copy SHA writes selected commit oid to clipboard",
  );

  await app
    .getByTestId(`commit-${selectedCommit.short_oid}-copy-diff`)
    .click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === `Copied diff for ${selectedCommit.short_oid}.` &&
      snapshot.error_message === "",
    { timeoutMs: 10_000 },
  );
  assert(
    (await app.clipboardText()).text.includes("src/main.rs"),
    "copy diff writes selected commit diff to clipboard",
  );

  await app
    .getByTestId(`commit-${selectedCommit.short_oid}-create-tag`)
    .click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.active_dialog === "create_tag",
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("create-tag-name-input").fill("e2e-tag");
  await app.getByTestId("create-tag-confirm").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Created tag 'e2e-tag' complete." &&
      snapshot.repo?.history.some((commit) =>
        commit.tags.includes("e2e-tag"),
      ),
    { timeoutMs: 15_000 },
  );
  const e2eTaggedCommit = await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.history.find((commit) => commit.tags.includes("e2e-tag")),
    { timeoutMs: 10_000 },
  );
  const e2eTaggedHistoryCommit = e2eTaggedCommit.repo.history.find((commit) =>
    commit.tags.includes("e2e-tag"),
  );
  assert(
    e2eTaggedCommit.test_tree.children.some(
      (node) =>
        node.id === `commit-${e2eTaggedHistoryCommit.short_oid}-delete-tag` &&
        node.text === "Delete tag e2e-tag" &&
        node.enabled === true,
    ),
    "delete tag action shows the target tag name and is enabled",
  );
  await app
    .getByTestId(`commit-${e2eTaggedHistoryCommit.short_oid}-delete-tag`)
    .click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.active_dialog === "delete_tag",
    { timeoutMs: 10_000 },
  );
  await expect(app.getByTestId("delete-tag-confirm")).toBeVisible({
    timeoutMs: 10_000,
  });
  await app.getByTestId("delete-tag-cancel").click();

  await app
    .getByTestId(`commit-${selectedCommit.short_oid}-create-tag`)
    .click();
  await app
    .getByTestId("create-tag-name-input")
    .fill("x".repeat(246));
  await expect(
    app.getByText("The tag name cannot be longer than 245 characters"),
  ).toBeVisible({ timeoutMs: 10_000 });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "create_tag" &&
      snapshot.test_tree.children.some(
        (node) => node.id === "create-tag-confirm" && node.enabled === false,
      ),
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("create-tag-cancel").click();

  await app
    .getByTestId(`commit-${selectedCommit.short_oid}-create-tag`)
    .click();
  await app.getByTestId("create-tag-name-input").fill("e2e-tag");
  await expect(
    app.getByText("A tag named e2e-tag already exists."),
  ).toBeVisible({ timeoutMs: 10_000 });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "create_tag" &&
      snapshot.test_tree.children.some(
        (node) => node.id === "create-tag-confirm" && node.enabled === false,
      ),
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("create-tag-cancel").click();

  await app
    .getByTestId(`commit-${selectedCommit.short_oid}-create-branch`)
    .click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.active_dialog === "create_branch",
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("dialog-create-branch").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === `Switched to branch 'branch-${selectedCommit.short_oid}'.` &&
      snapshot.repo?.current_branch === `branch-${selectedCommit.short_oid}` &&
      snapshot.repo?.head_oid === selectedCommit.oid,
    { timeoutMs: 15_000 },
  );
  await app.getByTestId("branch-main").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.current_branch === "main",
    { timeoutMs: 15_000 },
  );

  await app.command({
    command: "history_action",
    oid: selectedCommit.oid,
    action: "checkout_commit",
  });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message ===
        `Checked out commit ${selectedCommit.short_oid}.` &&
      snapshot.repo?.head_oid === selectedCommit.oid,
    { timeoutMs: 15_000 },
  );

  await app.getByTestId("branch-feature-update").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.current_branch === "feature/update",
    { timeoutMs: 15_000 },
  );

  await exec("git", ["switch", "main"], { cwd: fixture.workRepo });
  await app.command({ command: "refresh_repo" });
  await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.current_branch === "main",
    { timeoutMs: 15_000 },
  );

  await app.getByTestId("branch-delete-me-delete").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Deleted branch 'delete/me' complete." &&
      !snapshot.repo?.branches.some((branch) => branch.name === "delete/me"),
    { timeoutMs: 15_000 },
  );

  await app.command({
    command: "history_action",
    oid: selectedCommit.oid,
    action: "revert_changes_in_commit",
  });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === `Reverted commit ${selectedCommit.short_oid}.` &&
      snapshot.repo?.history[0]?.summary ===
        'Revert "test: selector full suite"',
    { timeoutMs: 15_000 },
  );

  await app.command({
    command: "history_action",
    oid: fixture.cherryPickOid,
    action: "cherry_pick_commit",
  });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.show_branch_selector === true &&
      snapshot.status_message ===
        `Choose a branch to cherry-pick ${shortOid(fixture.cherryPickOid)} into.`,
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("branch-main").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message ===
        `Cherry-picked commit ${shortOid(fixture.cherryPickOid)} into 'main'.` &&
      snapshot.repo?.history[0]?.summary === "feature: add cherry pick fixture",
    { timeoutMs: 15_000 },
  );

  await app.command({ command: "create_branch", name: "e2e-created" });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Switched to branch 'e2e-created'." &&
      snapshot.repo?.current_branch === "e2e-created" &&
      snapshot.repo?.branches.some((branch) => branch.name === "e2e-created"),
    { timeoutMs: 15_000 },
  );

  await app.command({ command: "merge_branch", name: "merge/source" });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Merged 'merge/source'." &&
      snapshot.repo?.history[0]?.summary === "Merge branch 'merge/source' into e2e-created",
    { timeoutMs: 15_000 },
  );

  await app.getByTestId("branch-main").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.current_branch === "main",
    { timeoutMs: 15_000 },
  );
}

export async function testNetworkFlows(app, fixture) {
  await app.getByTestId("button-network-push").click();
  await expect(app.getByText("Push origin complete.")).toBeVisible({
    timeoutMs: 20_000,
  });

  await createRemoteOnlyCommit(fixture.remoteClone);

  await app.getByTestId("button-network-fetch").click();
  await expect(app.getByText("Fetch origin complete.")).toBeVisible({
    timeoutMs: 20_000,
  });

  await app.getByTestId("button-network-pull").click();
  await expect(app.getByText("Pull origin complete.")).toBeVisible({
    timeoutMs: 20_000,
  });

  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.behind === 0 &&
      snapshot.repo?.history.some(
        (commit) => commit.summary === "remote: add upstream change",
      ),
    { timeoutMs: 10_000 },
  );
}

export async function testGithubOpenActions(app, fixture) {
  await exec("git", ["remote", "set-url", "origin", fixture.githubRemote], {
    cwd: fixture.workRepo,
  });

  const snapshot = await app.snapshot();
  const commit = snapshot.repo?.history[0];
  assert(commit, "history commit exists for GitHub open action");

  await app.command({
    command: "history_action",
    oid: commit.oid,
    action: "view_on_github",
  });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === `Opened commit ${commit.short_oid} on GitHub.` &&
      snapshot.error_message === "",
    { timeoutMs: 10_000 },
  );

  await app.command({
    command: "branch_action",
    name: "main",
    action: "view_on_github",
  });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Opened branch 'main' on GitHub." &&
      snapshot.error_message === "",
    { timeoutMs: 10_000 },
  );

  await waitForOpenUrl(
    fixture.openUrlLog,
    `${fixture.githubBaseUrl}/commit/${commit.oid}`,
    "commit GitHub action opens URL from configured remote",
  );
  await waitForOpenUrl(
    fixture.openUrlLog,
    `${fixture.githubBaseUrl}/tree/main`,
    "branch GitHub action opens URL from configured remote",
  );
}

export async function testStashFlows(app, fixture) {
  await app.getByTestId("tab-changes").click();
  const readmePath = path.join(fixture.workRepo, "README.md");
  await fs.appendFile(readmePath, "\nstash coverage edit\n");
  await app.command({ command: "refresh_repo" });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Repository refreshed." &&
      snapshot.repo?.changes.some((change) => change.path === "README.md"),
    { timeoutMs: 10_000 },
  );

  await app.command({ command: "stash_all" });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "stash_changes" &&
      snapshot.repo?.changes.some((change) => change.path === "README.md"),
    { timeoutMs: 10_000 },
  );
  await expect(app.getByTestId("stash-changes-file-readme-md")).toBeVisible({
    timeoutMs: 10_000,
  });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.test_tree?.children?.some(
        (node) => node.id === "stash-changes-file-readme-md",
      ) &&
      snapshot.test_tree?.children?.some(
        (node) => node.id === "stash-changes-confirm" && node.enabled !== false,
      ),
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("stash-changes-confirm").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Stashed changes complete." &&
      snapshot.repo?.stash_count === 1 &&
      !snapshot.repo?.changes.some((change) => change.path === "README.md"),
    { timeoutMs: 10_000 },
  );
  await expect(app.getByTestId("stash-indicator")).toBeVisible({
    timeoutMs: 10_000,
  });

  await app.getByTestId("stash-indicator").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "restore_stash" &&
      snapshot.repo?.stash_count === 1,
    { timeoutMs: 10_000 },
  );
  await expect(app.getByTestId("restore-stash-file-readme-md")).toBeVisible({
    timeoutMs: 10_000,
  });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.test_tree?.children?.some(
        (node) => node.id === "restore-stash-file-readme-md",
      ) &&
      snapshot.test_tree?.children?.some(
        (node) => node.id === "restore-stash-confirm" && node.enabled === true,
      ),
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("restore-stash-confirm").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Restored stash complete." &&
      snapshot.repo?.stash_count === 0 &&
      snapshot.repo?.changes.some((change) => change.path === "README.md"),
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("change-readme-md-discard").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Discarded changes for 'README.md'." &&
      !snapshot.repo?.changes.some((change) => change.path === "README.md"),
    { timeoutMs: 10_000 },
  );
}

export async function testStashAndSwitchDialog(app, fixture) {
  await app.getByTestId("tab-changes").click();
  const readmePath = path.join(fixture.workRepo, "README.md");
  const originalReadme = await fs.readFile(readmePath, "utf8");
  await fs.writeFile(readmePath, `${originalReadme}\nconflicting branch switch edit\n`);
  await app.command({ command: "refresh_repo" });
  await expect(app.getByTestId("change-readme-md")).toBeVisible({
    timeoutMs: 10_000,
  });

  await app.getByTestId("branch-switch-conflict").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "stash_and_switch" &&
      snapshot.error_message === "Branch switch needs a clean working tree." &&
      snapshot.repo?.current_branch === "main" &&
      snapshot.test_tree?.children?.some(
        (node) => node.id === "branch-switch-file-readme-md",
      ),
    { timeoutMs: 15_000 },
  );
  await expect(app.getByTestId("branch-switch-file-readme-md")).toBeVisible({
    timeoutMs: 10_000,
  });
  await expect(app.getByTestId("stash-cancel")).toBeVisible({
    timeoutMs: 10_000,
  });
  await app.getByTestId("stash-cancel").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "none" &&
      snapshot.repo?.current_branch === "main" &&
      snapshot.repo?.changes.some((change) => change.path === "README.md"),
    { timeoutMs: 10_000 },
  );
  assert(
    (await fs.readFile(readmePath, "utf8")).includes("conflicting branch switch edit"),
    "stash-and-switch cancel keeps dirty tracked changes",
  );

  await app.getByTestId("branch-switch-conflict").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "stash_and_switch" &&
      snapshot.test_tree?.children?.some(
        (node) => node.id === "branch-switch-file-readme-md",
      ),
    { timeoutMs: 15_000 },
  );
  await expect(app.getByTestId("branch-switch-file-readme-md")).toBeVisible({
    timeoutMs: 10_000,
  });
  await expect(app.getByTestId("stash-switch")).toBeVisible({
    timeoutMs: 15_000,
  });
  await app
    .getByTestId("input-commit-summary")
    .fill("stale include state guard");
  await app.getByTestId("stash-switch").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "none" &&
      snapshot.status_message === "Switched to branch 'switch/conflict'." &&
      snapshot.repo?.current_branch === "switch/conflict" &&
      snapshot.repo?.stash_count === 0 &&
      !snapshot.repo?.changes.some((change) => change.path === "README.md"),
    { timeoutMs: 15_000 },
  );
  const cleanBranchCommitButton = (await app.getByTestId("button-commit-all").all())[0];
  assert(
    cleanBranchCommitButton?.enabled === false,
    "stash-and-switch disables commit button when target branch has no local changes",
  );
  await app.getByTestId("branch-main").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.current_branch === "main",
    { timeoutMs: 15_000 },
  );
  await expect(app.getByTestId("stash-indicator")).toBeVisible({
    timeoutMs: 10_000,
  });
  await app.getByTestId("stash-indicator").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "restore_stash" &&
      snapshot.repo?.stash_count === 1,
    { timeoutMs: 10_000 },
  );
  await expect(app.getByTestId("restore-stash-file-readme-md")).toBeVisible({
    timeoutMs: 10_000,
  });
  await app.getByTestId("restore-stash-confirm").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Restored stash complete." &&
      snapshot.repo?.stash_count === 0 &&
      snapshot.repo?.changes.some((change) => change.path === "README.md"),
    { timeoutMs: 15_000 },
  );
  await app.getByTestId("change-readme-md-discard").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Discarded changes for 'README.md'." &&
      !snapshot.repo?.changes.some((change) => change.path === "README.md"),
    { timeoutMs: 10_000 },
  );
  assert(
    (await fs.readFile(readmePath, "utf8")) === originalReadme,
    "stash-and-switch cleanup restores README",
  );
}

export async function testDiscardConfirmationDialog(app, fixture) {
  const readmePath = path.join(fixture.workRepo, "README.md");
  const originalReadme = await fs.readFile(readmePath, "utf8");
  await fs.writeFile(readmePath, `${originalReadme}\ndiscard dialog edit\n`);
  await app.command({ command: "refresh_repo" });
  await expect(app.getByTestId("change-readme-md")).toBeVisible({
    timeoutMs: 10_000,
  });

  await app.getByTestId("change-readme-md-prompt-discard").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.active_dialog === "discard_changes",
    { timeoutMs: 10_000 },
  );
  await expect(app.getByTestId("discard-cancel")).toBeVisible({
    timeoutMs: 10_000,
  });
  await app.getByTestId("discard-cancel").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "none" &&
      snapshot.repo?.changes.some((change) => change.path === "README.md"),
    { timeoutMs: 10_000 },
  );
  assert(
    (await fs.readFile(readmePath, "utf8")).includes("discard dialog edit"),
    "discard cancel keeps tracked file changes",
  );

  await app.getByTestId("change-readme-md-prompt-discard").click();
  await expect(app.getByTestId("discard-confirm")).toBeVisible({
    timeoutMs: 10_000,
  });
  await app.getByTestId("discard-confirm").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "none" &&
      snapshot.status_message === "Discarded changes for 'README.md'." &&
      !snapshot.repo?.changes.some((change) => change.path === "README.md"),
    { timeoutMs: 10_000 },
  );
  assert(
    (await fs.readFile(readmePath, "utf8")) === originalReadme,
    "discard confirm restores tracked file contents",
  );
}

export async function testChangeFileActions(app, fixture) {
  await fs.writeFile(path.join(fixture.workRepo, "scratch.log"), "scratch\n");
  await app.command({ command: "refresh_repo" });
  await expect(app.getByTestId("change-scratch-log")).toBeVisible({
    timeoutMs: 10_000,
  });

  await app.getByTestId("change-scratch-log-copy-relative-path").click();
  assert(
    (await app.clipboardText()).text === "scratch.log",
    "copy relative path writes clipboard text",
  );

  await app.getByTestId("change-scratch-log-copy-full-path").click();
  assert(
    (await app.clipboardText()).text ===
      path.join(fixture.workRepo, "scratch.log"),
    "copy full path writes clipboard text",
  );

  await app.getByTestId("change-scratch-log-ignore-extension").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Added '*.log' to .gitignore." &&
      snapshot.repo?.changes.some((change) => change.path === ".gitignore") &&
      !snapshot.repo?.changes.some((change) => change.path === "scratch.log"),
    { timeoutMs: 10_000 },
  );
  assert(
    (await fs.readFile(path.join(fixture.workRepo, ".gitignore"), "utf8")).includes(
      "*.log",
    ),
    "ignore extension appends .gitignore pattern",
  );

  await fs.writeFile(path.join(fixture.workRepo, "local.tmp"), "tmp\n");
  await app.command({ command: "refresh_repo" });
  await expect(app.getByTestId("change-local-tmp")).toBeVisible({
    timeoutMs: 10_000,
  });
  await app.getByTestId("change-local-tmp-ignore-path").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Added 'local.tmp' to .gitignore." &&
      !snapshot.repo?.changes.some((change) => change.path === "local.tmp"),
    { timeoutMs: 10_000 },
  );
  assert(
    (await fs.readFile(path.join(fixture.workRepo, ".gitignore"), "utf8")).includes(
      "local.tmp",
    ),
    "ignore path appends .gitignore pattern",
  );

  await fs.mkdir(path.join(fixture.workRepo, "nested"), { recursive: true });
  await fs.writeFile(
    path.join(fixture.workRepo, "nested", "ignored.tmp"),
    "folder ignore\n",
  );
  await app.command({ command: "refresh_repo" });
  await expect(app.getByTestId("change-nested-ignored-tmp")).toBeVisible({
    timeoutMs: 10_000,
  });
  await app.getByTestId("change-nested-ignored-tmp-ignore-folder").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Added 'nested/' to .gitignore." &&
      !snapshot.repo?.changes.some(
        (change) => change.path === "nested/ignored.tmp",
      ),
    { timeoutMs: 10_000 },
  );
  assert(
    (await fs.readFile(path.join(fixture.workRepo, ".gitignore"), "utf8")).includes(
      "nested/",
    ),
    "ignore folder appends .gitignore pattern",
  );

  const readmePath = path.join(fixture.workRepo, "README.md");
  const originalReadme = await fs.readFile(readmePath, "utf8");
  await fs.writeFile(readmePath, `${originalReadme}\ntransient edit\n`);
  await app.command({ command: "refresh_repo" });
  await expect(app.getByTestId("change-readme-md")).toBeVisible({
    timeoutMs: 10_000,
  });
  await app.getByTestId("change-readme-md-discard").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Discarded changes for 'README.md'." &&
      !snapshot.repo?.changes.some((change) => change.path === "README.md"),
    { timeoutMs: 10_000 },
  );
  assert(
    (await fs.readFile(readmePath, "utf8")) === originalReadme,
    "discard restores tracked file contents",
  );

  await app.getByTestId("change-gitignore-open-in-editor").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Opened '.gitignore' in external editor." &&
      snapshot.error_message === "",
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("change-gitignore-reveal-in-finder").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Revealed '.gitignore' in Finder." &&
      snapshot.error_message === "",
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("change-gitignore-open-with-default").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Opened '.gitignore' with the default program." &&
      snapshot.error_message === "",
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("change-gitignore-view-on-github").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Opened '.gitignore' on GitHub." &&
      snapshot.error_message === "",
    { timeoutMs: 10_000 },
  );
  await waitForOpenUrl(
    fixture.openUrlLog,
    `${fixture.githubBaseUrl}/blob/main/.gitignore`,
    "changed-file GitHub action opens blob URL from configured remote",
  );
}

export async function testUndoLastCommit(app, fixture) {
  await app.getByTestId("tab-changes").click();
  await fs.appendFile(
    path.join(fixture.workRepo, "README.md"),
    "\nundo coverage edit\n",
  );
  await app.command({ command: "refresh_repo" });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Repository refreshed." &&
      snapshot.repo?.changes.some((change) => change.path === "README.md"),
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("input-commit-summary").fill("test: undo coverage");
  await app
    .getByTestId("input-commit-body")
    .fill("Covers undoing the last local commit.");
  await app.getByTestId("button-commit-all").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Commit created." &&
      snapshot.repo?.changes.length === 0 &&
      snapshot.repo?.history[0]?.summary === "test: undo coverage",
    { timeoutMs: 15_000 },
  );

  const taggedHeadSnapshot = await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.history[0]?.summary === "test: undo coverage",
    { timeoutMs: 10_000 },
  );
  await app.command({
    command: "history_action",
    oid: taggedHeadSnapshot.repo.history[0].oid,
    action: "create_tag",
  });
  await app.getByTestId("create-tag-name-input").fill("undo-guard-tag");
  await app.getByTestId("create-tag-confirm").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Created tag 'undo-guard-tag' complete." &&
      snapshot.repo?.history[0]?.tags.includes("undo-guard-tag") &&
      !snapshot.test_tree.children.some(
        (node) => node.id === "undo-last-commit" && node.visible,
      ),
    { timeoutMs: 15_000 },
  );

  await fs.appendFile(
    path.join(fixture.workRepo, "README.md"),
    "\nuntagged undo coverage edit\n",
  );
  await app.command({ command: "refresh_repo" });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Repository refreshed." &&
      snapshot.repo?.changes.some((change) => change.path === "README.md"),
    { timeoutMs: 10_000 },
  );

  await app
    .getByTestId("input-commit-summary")
    .fill("test: undo untagged coverage");
  await app
    .getByTestId("input-commit-body")
    .fill("Covers undoing a newer untagged local commit.");
  await app.getByTestId("button-commit-all").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Commit created." &&
      snapshot.repo?.changes.length === 0 &&
      snapshot.repo?.history[0]?.summary === "test: undo untagged coverage" &&
      snapshot.test_tree.children.some(
        (node) => node.id === "undo-last-commit" && node.visible,
      ),
    { timeoutMs: 15_000 },
  );

  await app.getByTestId("button-undo-last-commit").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Undid last commit." &&
      snapshot.repo?.history[0]?.summary !== "test: undo untagged coverage" &&
      snapshot.repo?.changes.some((change) => change.path === "README.md"),
    { timeoutMs: 15_000 },
  );
}
