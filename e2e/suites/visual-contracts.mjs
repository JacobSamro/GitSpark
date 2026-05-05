import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { assert } from "../support/assertions.mjs";
import { findNode, flattenNodes, nodeById } from "../support/tree.mjs";

const exec = promisify(execFile);

const LONG_BRANCH =
  "feature/very-long-branch-name-with-many-segments-and-descriptive-text-for-layout-checks";
const LONG_FILE =
  "deeply/nested/path/with spaces/and-a-very-long-file-name-for-layout-contract-checks.txt";

export async function testVisualContracts(app) {
  const repo = await makeVisualRepo();
  await app.openRepo(repo);
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.path === repo &&
      snapshot.sidebar_tab === "changes" &&
      snapshot.repo.changes.length === 0,
    { timeoutMs: 15_000 },
  );

  await assertCleanChangesEmptyState(app);
  await assertSettingsModalContract(app);
  await assertRepositorySelectorContract(app);
  await assertBranchSelectorContract(app);
  await assertLongPathChangeContract(app, repo);
  await assertCompareContract(app);
}

async function assertCleanChangesEmptyState(app) {
  const snapshot = await app.snapshot();
  assert(nodeById(snapshot.test_tree, "changes-list")?.visible !== false, "changes list is visible in the clean empty state");
  assert(nodeById(snapshot.test_tree, "no-changes-publish")?.visible !== false, "clean state exposes publish CTA");
  assert(nodeById(snapshot.test_tree, "no-changes-editor")?.visible !== false, "clean state exposes editor shortcut");
  assert(nodeById(snapshot.test_tree, "no-changes-finder")?.visible !== false, "clean state exposes file-manager shortcut");
  assert(nodeById(snapshot.test_tree, "commit-all")?.enabled === false, "commit button is disabled when there are no changes");
}

async function assertSettingsModalContract(app) {
  await app.command({ command: "show_global_settings", show: true });
  let snapshot = await app.waitForSnapshot(
    (snapshot) =>
      snapshot.show_settings === true &&
      nodeById(snapshot.test_tree, "settings-tab-git") &&
      nodeById(snapshot.test_tree, "settings-tab-ai") &&
      nodeById(snapshot.test_tree, "settings-tab-appearance") &&
      nodeById(snapshot.test_tree, "settings-tab-integrations") &&
      nodeById(snapshot.test_tree, "settings-save-git"),
    { timeoutMs: 10_000 },
  );
  assert(
    !flattenNodes(snapshot.test_tree).some(
      (node) => node.role === "button" && node.text === "Close",
    ),
    "settings modal does not expose a footer Close button",
  );

  await app.getByTestId("settings-tab-ai").click();
  snapshot = await app.waitForSnapshot(
    (snapshot) =>
      snapshot.show_settings === true &&
      nodeById(snapshot.test_tree, "settings-provider-openai-compatible") &&
      nodeById(snapshot.test_tree, "settings-ai-model"),
    { timeoutMs: 10_000 },
  );
  assert(nodeById(snapshot.test_tree, "settings-save-ai")?.visible !== false, "AI settings save button is visible");

  await app.getByTestId("settings-provider-openrouter").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.show_settings === true &&
      nodeById(snapshot.test_tree, "settings-provider-openrouter")?.selected === true &&
      nodeById(snapshot.test_tree, "settings-ai-model"),
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("settings-ai-model").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.show_settings === true &&
      nodeById(snapshot.test_tree, "settings-openrouter-model-filter"),
    { timeoutMs: 10_000 },
  );
  await app.command({ command: "show_global_settings", show: false });
  await app.waitForSnapshot((snapshot) => snapshot.show_settings === false);
}

async function assertRepositorySelectorContract(app) {
  await app.getByTestId("button-repo-selector").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.show_repo_selector === true &&
      nodeById(snapshot.test_tree, "repo-list") &&
      nodeById(snapshot.test_tree, "repo-selector-add"),
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("input-repo-filter").fill("definitely-no-repository-matches-this-filter");
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.show_repo_selector === true &&
      nodeById(snapshot.test_tree, "repo-selector-empty")?.text ===
        "Sorry, I can't find that repository",
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("input-repo-filter").press("escape");
  await app.waitForSnapshot((snapshot) => snapshot.show_repo_selector === false);
}

async function assertBranchSelectorContract(app) {
  await app.getByTestId("button-branch-selector").click();
  let snapshot = await app.waitForSnapshot(
    (snapshot) =>
      snapshot.show_branch_selector === true &&
      nodeById(snapshot.test_tree, "branch-list") &&
      findNode(snapshot.test_tree, (node) => node.text === LONG_BRANCH),
    { timeoutMs: 10_000 },
  );
  assert(nodeById(snapshot.test_tree, "input-branch-filter")?.visible !== false, "branch filter remains visible with a long branch list");

  await app.getByTestId("input-branch-filter").fill("missing-long-branch-filter");
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.show_branch_selector === true &&
      nodeById(snapshot.test_tree, "branch-selector-empty")?.text ===
        "Sorry, I can't find that branch",
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("input-branch-filter").press("escape");
  await app.waitForSnapshot((snapshot) => snapshot.show_branch_selector === false);
}

async function assertLongPathChangeContract(app, repo) {
  await fs.mkdir(path.dirname(path.join(repo, LONG_FILE)), { recursive: true });
  await fs.writeFile(path.join(repo, LONG_FILE), "long path change\n");
  await app.command({ command: "refresh_repo" });
  const snapshot = await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.changes.some((change) => change.path === LONG_FILE) &&
      findNode(snapshot.test_tree, (node) => node.text === LONG_FILE),
    { timeoutMs: 15_000 },
  );
  const changeNode = findNode(snapshot.test_tree, (node) => node.text === LONG_FILE);
  assert(changeNode?.id?.startsWith("change-"), "long file path change has a stable row ID");
}

async function assertCompareContract(app) {
  await app.command({ command: "compare_branch", name: "compare/visual-contract" });
  const snapshot = await app.waitForSnapshot(
    (snapshot) =>
      snapshot.sidebar_tab === "history" &&
      snapshot.compare?.target_branch === "compare/visual-contract" &&
      nodeById(snapshot.test_tree, "compare-exit-button") &&
      nodeById(snapshot.test_tree, "compare-merge-button") &&
      nodeById(snapshot.test_tree, "commit-file-list-viewport") &&
      findNode(snapshot.test_tree, (node) => node.text === "compare-output.txt"),
    { timeoutMs: 10_000 },
  );
  assert(
    nodeById(snapshot.test_tree, "compare-merge-button")?.enabled === true,
    "compare merge CTA is visually/semantically enabled when target has commits",
  );
}

async function makeVisualRepo() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-visual-contract-e2e-"));
  const repo = path.join(root, "repo");
  await exec("git", ["init", "-b", "main", repo]);
  await exec("git", ["config", "user.name", "GitSpark E2E"], { cwd: repo });
  await exec("git", ["config", "user.email", "e2e@gitspark.local"], {
    cwd: repo,
  });
  await fs.writeFile(path.join(repo, "README.md"), "visual contracts\n");
  await exec("git", ["add", "--all"], { cwd: repo });
  await exec("git", ["commit", "-m", "initial"], { cwd: repo });

  await exec("git", ["branch", LONG_BRANCH], { cwd: repo });
  await exec("git", ["switch", "-c", "compare/visual-contract"], { cwd: repo });
  await fs.writeFile(path.join(repo, "compare-output.txt"), "compare branch output\n");
  await exec("git", ["add", "--all"], { cwd: repo });
  await exec("git", ["commit", "-m", "compare visual contract"], { cwd: repo });
  await exec("git", ["switch", "main"], { cwd: repo });
  return await fs.realpath(repo);
}
