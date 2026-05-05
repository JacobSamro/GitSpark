import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import path from "node:path";
import { promisify } from "node:util";

import { assert } from "../support/assertions.mjs";

const exec = promisify(execFile);

export async function testConflictFlows(app, fixture) {
  const mergeRepo = await createConflictRepo(fixture.root, "merge-conflict");
  await app.openRepo(mergeRepo);
  await waitForRepo(app, mergeRepo, "feature");

  await startConflictedMerge(mergeRepo);
  await app.command({ command: "refresh_repo" });
  await assertConflictBanner(app, {
    kind: "merge",
    hasSkip: false,
    canContinue: false,
  });

  await app.getByTestId("operation-conflict-open-editor-conflict-txt").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Opened 'conflict.txt' in external editor." &&
      snapshot.error_message === "",
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("operation-conflict-reveal-conflict-txt").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Revealed 'conflict.txt' in Finder." &&
      snapshot.error_message === "",
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("operation-abort").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.operation === null &&
      snapshot.repo?.current_branch === "feature" &&
      snapshot.status_message === "Abort merge complete.",
    { timeoutMs: 15_000 },
  );
  assert(
    !(await fs.readFile(path.join(mergeRepo, "conflict.txt"), "utf8")).includes(
      "<<<<<<<",
    ),
    "abort merge removes conflict markers from the worktree",
  );

  await startConflictedMerge(mergeRepo);
  await app.command({ command: "refresh_repo" });
  await assertConflictBanner(app, {
    kind: "merge",
    hasSkip: false,
    canContinue: false,
  });
  await fs.writeFile(path.join(mergeRepo, "conflict.txt"), "resolved merge\n");
  await app.getByTestId("operation-conflict-mark-resolved-conflict-txt").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.operation?.kind === "merge" &&
      snapshot.operation?.can_continue === true &&
      snapshot.operation?.conflicted_files.length === 0 &&
      hasNode(snapshot.test_tree, "operation-continue", (node) => node.enabled),
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("operation-continue").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.operation === null &&
      snapshot.repo?.current_branch === "feature" &&
      snapshot.status_message === "Continue merge complete.",
    { timeoutMs: 15_000 },
  );

  const rebaseAbortRepo = await createConflictRepo(fixture.root, "rebase-abort");
  await app.openRepo(rebaseAbortRepo);
  await waitForRepo(app, rebaseAbortRepo, "feature");
  await startConflictedRebase(rebaseAbortRepo);
  await app.command({ command: "refresh_repo" });
  await assertConflictBanner(app, {
    kind: "rebase",
    hasSkip: true,
    canContinue: false,
  });
  await app.getByTestId("operation-abort").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.operation === null &&
      snapshot.repo?.current_branch === "feature" &&
      snapshot.status_message === "Abort rebase complete.",
    { timeoutMs: 15_000 },
  );

  const rebaseSkipRepo = await createConflictRepo(fixture.root, "rebase-skip");
  await app.openRepo(rebaseSkipRepo);
  await waitForRepo(app, rebaseSkipRepo, "feature");
  await startConflictedRebase(rebaseSkipRepo);
  await app.command({ command: "refresh_repo" });
  await assertConflictBanner(app, {
    kind: "rebase",
    hasSkip: true,
    canContinue: false,
  });
  await app.getByTestId("operation-skip").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.operation === null &&
      snapshot.repo?.current_branch === "feature" &&
      snapshot.status_message === "Skip rebase complete.",
    { timeoutMs: 15_000 },
  );

  await app.openRepo(fixture.workRepo);
  await waitForRepo(app, fixture.workRepo);
}

async function createConflictRepo(root, name) {
  const repo = path.join(root, name);
  await exec("git", ["init", "-b", "main", repo]);
  await exec("git", ["config", "user.name", "GitSpark E2E"], { cwd: repo });
  await exec("git", ["config", "user.email", "e2e@gitspark.local"], {
    cwd: repo,
  });
  await exec("git", ["config", "core.editor", "/usr/bin/true"], { cwd: repo });

  await fs.writeFile(path.join(repo, "conflict.txt"), "base\n");
  await exec("git", ["add", "--all"], { cwd: repo });
  await exec("git", ["commit", "-m", "initial conflict fixture"], {
    cwd: repo,
  });

  await exec("git", ["switch", "-c", "feature"], { cwd: repo });
  await fs.writeFile(path.join(repo, "conflict.txt"), "feature\n");
  await exec("git", ["add", "conflict.txt"], { cwd: repo });
  await exec("git", ["commit", "-m", "feature conflict edit"], { cwd: repo });

  await exec("git", ["switch", "main"], { cwd: repo });
  await fs.writeFile(path.join(repo, "conflict.txt"), "main\n");
  await exec("git", ["add", "conflict.txt"], { cwd: repo });
  await exec("git", ["commit", "-m", "main conflict edit"], { cwd: repo });

  await exec("git", ["switch", "feature"], { cwd: repo });
  return repo;
}

async function startConflictedMerge(repo) {
  await gitExpectFailure(repo, ["merge", "--no-ff", "main"]);
}

async function startConflictedRebase(repo) {
  await gitExpectFailure(repo, ["rebase", "main"]);
}

async function gitExpectFailure(repo, args) {
  try {
    await exec("git", args, { cwd: repo });
  } catch {
    return;
  }
  throw new Error(`expected git ${args.join(" ")} to fail`);
}

async function waitForRepo(app, repo, branch = null) {
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.path === repo &&
      (branch === null || snapshot.repo?.current_branch === branch),
    { timeoutMs: 15_000 },
  );
}

async function assertConflictBanner(app, { kind, hasSkip, canContinue }) {
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.operation?.kind === kind &&
      snapshot.operation?.conflicted_files.some(
        (file) => file.path === "conflict.txt",
      ) &&
      snapshot.operation?.can_continue === canContinue &&
      hasNode(snapshot.test_tree, "operation-conflict-banner") &&
      hasNode(snapshot.test_tree, "operation-conflict-files") &&
      hasNode(snapshot.test_tree, "operation-conflict-file-conflict-txt") &&
      hasNode(snapshot.test_tree, "operation-conflict-open-editor-conflict-txt") &&
      hasNode(snapshot.test_tree, "operation-conflict-reveal-conflict-txt") &&
      hasNode(
        snapshot.test_tree,
        "operation-conflict-mark-resolved-conflict-txt",
      ) &&
      hasNode(
        snapshot.test_tree,
        "operation-continue",
        (node) => node.enabled === canContinue,
      ) &&
      hasNode(snapshot.test_tree, "operation-abort") &&
      hasNode(snapshot.test_tree, "operation-skip") === hasSkip,
    { timeoutMs: 10_000 },
  );
}

function hasNode(node, id, predicate = () => true) {
  if (!node) {
    return false;
  }
  if (node.id === id && predicate(node)) {
    return true;
  }
  return (node.children || []).some((child) => hasNode(child, id, predicate));
}
