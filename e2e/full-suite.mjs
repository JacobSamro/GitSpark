import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { GitSparkAutomation, expect } from "./gitspark.mjs";

const exec = promisify(execFile);

const fixture = await makeFreshSampleRepo();
const aiServer = await startMockAiServer();
const app = await GitSparkAutomation.launch({
  env: {
    GITSPARK_AI_ENDPOINT: aiServer.url,
    GITSPARK_CONFIG_DIR: fixture.configDir,
    GITSPARK_OPEN_COMMAND: "/usr/bin/true",
    GITSPARK_OPEN_URL_COMMAND: fixture.openUrlCommand,
    GITSPARK_REVEAL_COMMAND: "/usr/bin/true",
  },
});

try {
  await testAutomationBasics(app);
  await testShellControls(app);
  await testRepositoryFlows(app, fixture);
  await testAiValidation(app);
  await testSettingsPersistence(app, fixture);
  await testAiSuccess(app, aiServer);
  await testCommitFlow(app);
  await testCreateBranchDialog(app);
  await testHistoryAndBranchFlows(app, fixture);
  await testNetworkFlows(app, fixture);
  await testStashFlows(app, fixture);
  await testStashAndSwitchDialog(app, fixture);
  await testGithubOpenActions(app, fixture);
  await testDiscardConfirmationDialog(app, fixture);
  await testChangeFileActions(app, fixture);
  await testUndoLastCommit(app, fixture);
} finally {
  await app.close();
  await aiServer.close();
}

async function testAutomationBasics(app) {
  const ping = await app.ping();
  assert(ping.pong === true, "ping returns pong");

  const tree = await app.testTree();
  assert(tree.test_id === "gitspark-root", "test tree exposes root node");
  await expect(app.getByTestId("gitspark-root")).toBeVisible();
}

async function testShellControls(app) {
  await app.getByTestId("button-repo-selector").click();
  await expect(app.getByTestId("input-repo-filter")).toBeVisible();
  await app.getByTestId("input-repo-filter").fill("sample");
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.show_repo_selector === true &&
      snapshot.repo_filter_text === "sample",
  );

  await app.getByTestId("button-settings").click();
  await app.waitForSnapshot((snapshot) => snapshot.show_settings === true);
  await app.getByTestId("button-settings").click();
  await app.waitForSnapshot((snapshot) => snapshot.show_settings === false);
}

async function testRepositoryFlows(app, fixture) {
  await app.openRepo(fixture.workRepo);
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.path === fixture.workRepo &&
      snapshot.status_message === "Repository loaded.",
    { timeoutMs: 15_000 },
  );

  await expect(app.getByTestId("change-src-main-rs")).toBeVisible({
    timeoutMs: 10_000,
  });
  await app.getByTestId("change-src-main-rs").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.selected_change === "src/main.rs",
  );

  await app.getByTestId("tab-history").click();
  await app.waitForSnapshot((snapshot) => snapshot.sidebar_tab === "history");
  await app.getByTestId("tab-changes").click();
  await app.waitForSnapshot((snapshot) => snapshot.sidebar_tab === "changes");

  await app.command({ command: "refresh_repo" });
  await app.waitForSnapshot(
    (snapshot) => snapshot.status_message === "Repository refreshed.",
    { timeoutMs: 10_000 },
  );
}

async function testCommitFlow(app) {
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

async function testCreateBranchDialog(app) {
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
}

async function testAiValidation(app) {
  await app.command({ command: "generate_ai_commit" });
  await expect(
    app.getByText(
      "AI generation failed: AI API key is missing. Add one in settings before generating commit messages.",
    ),
  ).toBeVisible({ timeoutMs: 10_000 });
}

async function testSettingsPersistence(app, fixture) {
  await app.getByTestId("button-settings").click();
  await app.getByTestId("settings-tab-git").click();
  await app.getByTestId("settings-git-user-name").fill("GitSpark Precise");
  await app
    .getByTestId("settings-git-user-email")
    .fill("precise@gitspark.local");
  await app.getByTestId("settings-git-default-branch").fill("main");
  await app.getByTestId("settings-save-git").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Git config saved." &&
      snapshot.error_message === "",
    { timeoutMs: 10_000 },
  );

  assert(
    (await gitOutput(fixture.workRepo, ["config", "user.name"])) ===
      "GitSpark Precise",
    "git user.name persisted to repository config",
  );
  assert(
    (await gitOutput(fixture.workRepo, ["config", "user.email"])) ===
      "precise@gitspark.local",
    "git user.email persisted to repository config",
  );

  await app.getByTestId("settings-tab-ai").click();
  await app.getByTestId("settings-provider-openai-compatible").click();
  await app.getByTestId("settings-ai-model").fill("gpt-e2e-precise");
  await app.getByTestId("settings-ai-api-key").fill("sk-e2e-test-key");
  await app
    .getByTestId("settings-ai-system-prompt")
    .fill("Return a precise JSON commit suggestion.");
  await app.getByTestId("settings-save-ai").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "AI settings saved." &&
      snapshot.error_message === "",
    { timeoutMs: 10_000 },
  );

  const settingsToml = await fs.readFile(fixture.settingsPath, "utf8");
  assert(
    settingsToml.includes('model = "gpt-e2e-precise"'),
    "AI model persisted to isolated settings file",
  );
  assert(
    settingsToml.includes('api_key = "sk-e2e-test-key"'),
    "AI API key persisted to isolated settings file",
  );
  assert(
    settingsToml.includes('system_prompt = "Return a precise JSON commit suggestion."'),
    "AI system prompt persisted to isolated settings file",
  );

  await app.getByTestId("button-settings").click();
  await app.waitForSnapshot((snapshot) => snapshot.show_settings === false);
}

async function testAiSuccess(app, aiServer) {
  await app.command({ command: "generate_ai_commit" });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Generated commit suggestion." &&
      snapshot.error_message === "" &&
      snapshot.commit_summary === "test: mocked ai summary" &&
      snapshot.commit_body === "Mocked body from local e2e server.",
    { timeoutMs: 10_000 },
  );

  assert(aiServer.requests.length === 1, "mock AI server received one request");
  const request = aiServer.requests[0];
  assert(
    request.headers.authorization === "Bearer sk-e2e-test-key",
    "AI request sends configured bearer token",
  );
  assert(
    request.body.messages.some((message) =>
      message.content.includes("src/main.rs"),
    ),
    "AI request includes the working-tree diff",
  );
}

async function testHistoryAndBranchFlows(app, fixture) {
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
      snapshot.status_message ===
        `Cherry-picked commit ${shortOid(fixture.cherryPickOid)}.` &&
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

async function testNetworkFlows(app, fixture) {
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

async function testGithubOpenActions(app, fixture) {
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

async function testStashFlows(app, fixture) {
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
      snapshot.status_message === "Stashed changes complete." &&
      snapshot.repo?.stash_count === 1 &&
      !snapshot.repo?.changes.some((change) => change.path === "README.md"),
    { timeoutMs: 10_000 },
  );
  await expect(app.getByTestId("stash-indicator")).toBeVisible({
    timeoutMs: 10_000,
  });

  await app.command({ command: "stash_pop" });
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

async function testStashAndSwitchDialog(app, fixture) {
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
      snapshot.repo?.current_branch === "main",
    { timeoutMs: 15_000 },
  );
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
  await expect(app.getByTestId("stash-switch")).toBeVisible({
    timeoutMs: 15_000,
  });
  await app.getByTestId("stash-switch").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "none" &&
      snapshot.status_message === "Switched to branch 'switch/conflict'." &&
      snapshot.repo?.current_branch === "switch/conflict" &&
      snapshot.repo?.stash_count === 1 &&
      !snapshot.repo?.changes.some((change) => change.path === "README.md"),
    { timeoutMs: 15_000 },
  );

  await app.getByTestId("branch-main").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.current_branch === "main",
    { timeoutMs: 15_000 },
  );
  await app.command({ command: "stash_pop" });
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

async function readOpenUrlLog(logPath) {
  try {
    const text = await fs.readFile(logPath, "utf8");
    return text.split("\n").filter(Boolean);
  } catch (error) {
    if (error.code === "ENOENT") {
      return [];
    }
    throw error;
  }
}

async function waitForOpenUrl(logPath, expectedUrl, message) {
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) {
    const openedUrls = await readOpenUrlLog(logPath);
    if (openedUrls.includes(expectedUrl)) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(message);
}

async function testDiscardConfirmationDialog(app, fixture) {
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

async function testChangeFileActions(app, fixture) {
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
}

async function testUndoLastCommit(app, fixture) {
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

  await app.getByTestId("button-undo-last-commit").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Undid last commit." &&
      snapshot.repo?.history[0]?.summary !== "test: undo coverage" &&
      snapshot.repo?.changes.some((change) => change.path === "README.md"),
    { timeoutMs: 15_000 },
  );
}

async function makeFreshSampleRepo() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-full-e2e-"));
  const remote = path.join(root, "origin.git");
  const workRepo = path.join(root, "sample-repo");
  const remoteClone = path.join(root, "remote-clone");
  const configDir = path.join(root, "config");
  const settingsPath = path.join(configDir, "settings.toml");
  const openUrlLog = path.join(root, "opened-urls.log");
  const openUrlScript = path.join(root, "open-url.sh");
  const githubBaseUrl = "https://github.com/gitspark/e2e-fixture";
  const githubRemote = `${githubBaseUrl}.git`;

  await fs.writeFile(
    openUrlScript,
    `#!/bin/sh\nprintf '%s\\n' "$1" >> ${shellQuote(openUrlLog)}\n`,
    { mode: 0o755 },
  );

  await exec("git", ["init", "--bare", remote]);
  await exec("git", ["init", "-b", "main", workRepo]);
  await gitConfig(workRepo);
  await exec("git", ["config", "core.editor", "/usr/bin/true"], {
    cwd: workRepo,
  });

  await fs.mkdir(path.join(workRepo, "src"));
  await fs.writeFile(path.join(workRepo, "src", "main.rs"), "fn main() {}\n");
  await fs.writeFile(
    path.join(workRepo, "README.md"),
    "# GitSpark E2E Sample\n",
  );
  await exec("git", ["add", "--all"], { cwd: workRepo });
  await exec("git", ["commit", "-m", "initial sample"], { cwd: workRepo });
  await exec("git", ["branch", "feature/update"], { cwd: workRepo });
  await exec("git", ["branch", "delete/me"], { cwd: workRepo });
  await exec("git", ["switch", "-c", "switch/conflict"], { cwd: workRepo });
  await fs.writeFile(
    path.join(workRepo, "README.md"),
    "# GitSpark E2E Sample\n\nconflict branch version\n",
  );
  await exec("git", ["add", "README.md"], { cwd: workRepo });
  await exec("git", ["commit", "-m", "branch: add conflicting readme"], {
    cwd: workRepo,
  });
  await exec("git", ["switch", "main"], { cwd: workRepo });
  await exec("git", ["switch", "-c", "cherry/source"], { cwd: workRepo });
  await fs.writeFile(path.join(workRepo, "cherry.txt"), "cherry-pick fixture\n");
  await exec("git", ["add", "cherry.txt"], { cwd: workRepo });
  await exec("git", ["commit", "-m", "feature: add cherry pick fixture"], {
    cwd: workRepo,
  });
  const cherryPickOid = await gitOutput(workRepo, ["rev-parse", "HEAD"]);
  await exec("git", ["switch", "main"], { cwd: workRepo });
  await exec("git", ["switch", "-c", "merge/source"], { cwd: workRepo });
  await fs.writeFile(path.join(workRepo, "merge.txt"), "merge fixture\n");
  await exec("git", ["add", "merge.txt"], { cwd: workRepo });
  await exec("git", ["commit", "-m", "feature: add merge fixture"], {
    cwd: workRepo,
  });
  await exec("git", ["switch", "main"], { cwd: workRepo });
  await exec("git", ["remote", "add", "origin", remote], { cwd: workRepo });
  await exec("git", ["push", "-u", "origin", "main"], { cwd: workRepo });
  await exec("git", ["push", "origin", "feature/update"], { cwd: workRepo });

  await fs.writeFile(
    path.join(workRepo, "src", "main.rs"),
    "fn main() {\n    println!(\"fresh sample repo\");\n}\n",
  );

  await exec("git", ["clone", remote, remoteClone]);
  await gitConfig(remoteClone);

  return {
    root: await fs.realpath(root),
    remote: await fs.realpath(remote),
    workRepo: await fs.realpath(workRepo),
    remoteClone: await fs.realpath(remoteClone),
    configDir,
    settingsPath,
    cherryPickOid,
    githubBaseUrl,
    githubRemote,
    openUrlCommand: shellQuote(openUrlScript),
    openUrlLog,
  };
}

function shortOid(oid) {
  return oid.slice(0, 7);
}

function shellQuote(value) {
  return `'${value.replaceAll("'", "'\"'\"'")}'`;
}

async function createRemoteOnlyCommit(remoteClone) {
  await exec("git", ["checkout", "main"], { cwd: remoteClone });
  await exec("git", ["fetch", "origin", "main"], { cwd: remoteClone });
  await exec("git", ["reset", "--hard", "origin/main"], { cwd: remoteClone });
  await fs.writeFile(
    path.join(remoteClone, "upstream.txt"),
    `upstream change ${Date.now()}\n`,
  );
  await exec("git", ["add", "--all"], { cwd: remoteClone });
  await exec("git", ["commit", "-m", "remote: add upstream change"], {
    cwd: remoteClone,
  });
  await exec("git", ["push", "origin", "main"], { cwd: remoteClone });
}

async function gitConfig(repo) {
  await exec("git", ["config", "user.name", "GitSpark E2E"], { cwd: repo });
  await exec("git", ["config", "user.email", "e2e@gitspark.local"], {
    cwd: repo,
  });
}

async function gitOutput(repo, args) {
  const { stdout } = await exec("git", args, { cwd: repo });
  return stdout.trim();
}

async function startMockAiServer() {
  const requests = [];
  const server = http.createServer(async (request, response) => {
    let raw = "";
    for await (const chunk of request) {
      raw += chunk;
    }

    requests.push({
      headers: request.headers,
      body: JSON.parse(raw),
    });

    response.writeHead(200, { "Content-Type": "application/json" });
    response.end(
      JSON.stringify({
        choices: [
          {
            message: {
              content: JSON.stringify({
                subject: "test: mocked ai summary",
                body: "Mocked body from local e2e server.",
              }),
            },
          },
        ],
      }),
    );
  });

  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address();
  return {
    requests,
    url: `http://127.0.0.1:${port}/v1/chat/completions`,
    close: () => new Promise((resolve) => server.close(resolve)),
  };
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}
