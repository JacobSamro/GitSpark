import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { assert } from "../support/assertions.mjs";
import { nodeById } from "../support/tree.mjs";

const exec = promisify(execFile);

export async function testStashEdgeCases(app) {
  const repo = await makeStashRepo();
  await app.openRepo(repo);
  await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.path === repo,
    { timeoutMs: 15_000 },
  );

  await fs.writeFile(path.join(repo, "main-only.txt"), "main stash one\n");
  await app.command({ command: "refresh_repo" });
  await waitForChanges(app, ["main-only.txt"]);
  await app.command({ command: "stash_all" });
  await app.waitForSnapshot(
    (snapshot) => snapshot.active_dialog === "stash_changes",
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("stash-changes-confirm").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.stash_count === 1 && snapshot.repo.changes.length === 0,
    { timeoutMs: 10_000 },
  );

  await fs.writeFile(path.join(repo, "main-only.txt"), "main stash replacement\n");
  await app.command({ command: "refresh_repo" });
  await waitForChanges(app, ["main-only.txt"]);
  await app.command({ command: "stash_all" });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "stash_changes" &&
      nodeById(snapshot.test_tree, "stash-changes-replace-warning")?.visible === true,
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("stash-changes-confirm").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.stash_count === 1 && snapshot.repo.changes.length === 0,
    { timeoutMs: 10_000 },
  );
  assert(
    (await stashSubjects(repo)).filter((subject) =>
      subject.endsWith("GitSpark stash for main"),
    ).length === 1,
    "replacing a GitSpark stash keeps one branch-scoped stash",
  );

  await fs.writeFile(path.join(repo, "user-stash.txt"), "user stash\n");
  await exec("git", ["stash", "push", "-u", "-m", "User stash"], { cwd: repo });
  assert(
    (await stashSubjects(repo)).some((subject) => subject.endsWith("User stash")),
    "fixture has a user-created stash",
  );

  await app.getByTestId("branch-feature-stash").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.current_branch === "feature/stash" &&
      snapshot.repo.stash_count === 0,
    { timeoutMs: 15_000 },
  );
  await fs.writeFile(path.join(repo, "feature-only.txt"), "feature stash\n");
  await app.command({ command: "refresh_repo" });
  await waitForChanges(app, ["feature-only.txt"]);
  await app.command({ command: "stash_all" });
  await app.waitForSnapshot(
    (snapshot) => snapshot.active_dialog === "stash_changes",
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("stash-changes-confirm").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.stash_count === 1 && snapshot.repo.changes.length === 0,
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("branch-main").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.current_branch === "main" &&
      snapshot.repo.stash_count === 1,
    { timeoutMs: 15_000 },
  );
  await app.getByTestId("stash-indicator").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "restore_stash" &&
      nodeById(snapshot.test_tree, "restore-stash-file-main-only-txt"),
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("restore-stash-confirm").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Restored stash complete." &&
      snapshot.repo?.changes.some((change) => change.path === "main-only.txt") &&
      snapshot.repo.stash_count === 0,
    { timeoutMs: 10_000 },
  );
  assert(
    (await fs.readFile(path.join(repo, "main-only.txt"), "utf8")) ===
      "main stash replacement\n",
    "branch-scoped restore uses the current branch stash, not newer stashes",
  );
  assert(
    (await stashSubjects(repo)).some((subject) => subject.endsWith("User stash")),
    "restoring a GitSpark stash leaves user-created stashes untouched",
  );

  await app.command({
    command: "change_action",
    path: "main-only.txt",
    action: "discard",
  });
  await app.waitForSnapshot(
    (snapshot) => !snapshot.repo?.changes.some((change) => change.path === "main-only.txt"),
    { timeoutMs: 10_000 },
  );

  await fs.writeFile(path.join(repo, "multi-a.txt"), "multi a\n");
  await fs.writeFile(path.join(repo, "multi-b.txt"), "multi b\n");
  await app.command({ command: "refresh_repo" });
  await waitForChanges(app, ["multi-a.txt", "multi-b.txt"]);
  await app.command({ command: "stash_all" });
  await app.waitForSnapshot(
    (snapshot) => snapshot.active_dialog === "stash_changes",
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("stash-changes-confirm").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.stash_count === 1,
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("stash-indicator").click();
  await app.getByTestId("restore-stash-discard").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "discard_stash" &&
      nodeById(snapshot.test_tree, "discard-stash-file-multi-a-txt") &&
      nodeById(snapshot.test_tree, "discard-stash-file-multi-b-txt"),
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("discard-stash-cancel").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "none" && snapshot.repo?.stash_count === 1,
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("stash-indicator").click();
  await app.getByTestId("restore-stash-discard").click();
  await app.getByTestId("discard-stash-confirm").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Discarded stash complete." &&
      snapshot.repo?.stash_count === 0,
    { timeoutMs: 10_000 },
  );

  await fs.writeFile(path.join(repo, "rename-old.txt"), "rename stash\n");
  await fs.writeFile(path.join(repo, "delete-stash.txt"), "delete stash\n");
  await exec("git", ["add", "--all"], { cwd: repo });
  await exec("git", ["commit", "-m", "stash rename baseline"], { cwd: repo });
  await exec("git", ["mv", "rename-old.txt", "rename-new.txt"], { cwd: repo });
  await fs.rm(path.join(repo, "delete-stash.txt"));
  await app.command({ command: "refresh_repo" });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.changes.some((change) => change.path === "rename-new.txt") &&
      snapshot.repo?.changes.some((change) => change.path === "delete-stash.txt"),
    { timeoutMs: 10_000 },
  );
  await app.command({ command: "stash_all" });
  await app.waitForSnapshot(
    (snapshot) => snapshot.active_dialog === "stash_changes",
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("stash-changes-confirm").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.stash_count === 1,
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("stash-indicator").click();
  await app.waitForSnapshot(
    (snapshot) =>
      nodeById(snapshot.test_tree, "restore-stash-file-rename-new-txt") &&
      nodeById(snapshot.test_tree, "restore-stash-file-delete-stash-txt"),
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("restore-stash-cancel").click();

  await testStashPopConflict(app);
}

async function makeStashRepo() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-stash-e2e-"));
  const repo = path.join(root, "repo");
  await exec("git", ["init", "-b", "main", repo]);
  await exec("git", ["config", "user.name", "GitSpark E2E"], { cwd: repo });
  await exec("git", ["config", "user.email", "e2e@gitspark.local"], {
    cwd: repo,
  });
  await fs.writeFile(path.join(repo, "README.md"), "stash\n");
  await exec("git", ["add", "--all"], { cwd: repo });
  await exec("git", ["commit", "-m", "initial"], { cwd: repo });
  await exec("git", ["branch", "feature/stash"], { cwd: repo });
  return await fs.realpath(repo);
}

async function waitForChanges(app, paths) {
  await app.waitForSnapshot(
    (snapshot) =>
      paths.every((path) =>
        snapshot.repo?.changes.some((change) => change.path === path),
      ),
    { timeoutMs: 10_000 },
  );
}

async function testStashPopConflict(app) {
  const repo = await makeStashConflictRepo();
  await app.openRepo(repo);
  await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.path === repo,
    { timeoutMs: 15_000 },
  );

  await fs.writeFile(path.join(repo, "conflict.txt"), "stashed\n");
  await app.command({ command: "refresh_repo" });
  await waitForChanges(app, ["conflict.txt"]);
  await app.command({ command: "stash_all" });
  await app.waitForSnapshot(
    (snapshot) => snapshot.active_dialog === "stash_changes",
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("stash-changes-confirm").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.stash_count === 1 && snapshot.repo.changes.length === 0,
    { timeoutMs: 10_000 },
  );

  await fs.writeFile(path.join(repo, "conflict.txt"), "current\n");
  await exec("git", ["add", "conflict.txt"], { cwd: repo });
  await exec("git", ["commit", "-m", "current conflicting edit"], { cwd: repo });
  await app.command({ command: "refresh_repo" });
  await app.getByTestId("stash-indicator").click();
  await app.getByTestId("restore-stash-confirm").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.error_message.startsWith("Restored stash failed:") &&
      snapshot.error_message.includes("failed to pop stash"),
    { timeoutMs: 15_000 },
  );

  const { stdout } = await exec("git", ["diff", "--name-only", "--diff-filter=U"], {
    cwd: repo,
  });
  assert(
    stdout.split("\n").includes("conflict.txt"),
    "stash pop conflict leaves the conflicted file visible to Git",
  );
}

async function makeStashConflictRepo() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-stash-conflict-e2e-"));
  const repo = path.join(root, "repo");
  await exec("git", ["init", "-b", "main", repo]);
  await exec("git", ["config", "user.name", "GitSpark E2E"], { cwd: repo });
  await exec("git", ["config", "user.email", "e2e@gitspark.local"], {
    cwd: repo,
  });
  await fs.writeFile(path.join(repo, "conflict.txt"), "base\n");
  await exec("git", ["add", "--all"], { cwd: repo });
  await exec("git", ["commit", "-m", "initial"], { cwd: repo });
  return await fs.realpath(repo);
}

async function stashSubjects(repo) {
  const { stdout } = await exec("git", ["stash", "list", "--format=%s"], {
    cwd: repo,
  });
  return stdout
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}
