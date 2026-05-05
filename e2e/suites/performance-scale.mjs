import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { assert } from "../support/assertions.mjs";
import { nodeById } from "../support/tree.mjs";

const exec = promisify(execFile);

export async function testPerformanceScaleSmoke(app) {
  const repo = await makeScaleRepo();
  await app.openRepo(repo);
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.path === repo &&
      snapshot.repo.changes.length >= 1001 &&
      snapshot.repo.history.length >= 100,
    { timeoutMs: 30_000 },
  );

  let snapshot = await app.snapshot();
  assert(
    nodeById(snapshot.test_tree, "changes-list")?.children.length >= 1001,
    "1,000 changed files are represented in the virtualized changes list contract",
  );
  assert(
    snapshot.repo.history[0].summary === "scale commit 1000",
    "repository with 1,000 commits loads the latest commit promptly",
  );

  await app.command({ command: "select_change", path: "huge-diff.txt" });
  await app.waitForSnapshot(
    (snapshot) => snapshot.selected_change === "huge-diff.txt",
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("button-branch-selector").click();
  await app.getByTestId("input-branch-filter").fill("scale/branch-099");
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.branch_filter_text === "scale/branch-099" &&
      snapshot.repo?.branches.some((branch) => branch.name === "scale/branch-099"),
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("button-branch-selector").click();

  for (let i = 0; i < 25; i += 1) {
    await fs.writeFile(path.join(repo, `burst-${i}.txt`), `burst ${i}\n`);
  }
  await app.command({ command: "refresh_repo" });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.changes.some((change) => change.path === "burst-24.txt"),
    { timeoutMs: 60_000 },
  );

  for (let i = 0; i < 5; i += 1) {
    await app.command({ command: "refresh_repo" });
    snapshot = await app.waitForSnapshot(
      (snapshot) =>
        snapshot.status_message === "Repository refreshed." &&
        snapshot.repo?.changes.length >= 1026,
      { timeoutMs: 60_000 },
    );
  }
  assert(
    snapshot.repo.changes.length >= 1026,
    "repeated refreshes keep the scale repo state stable",
  );
}

async function makeScaleRepo() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-scale-e2e-"));
  const repo = path.join(root, "repo");
  await exec("git", ["init", "-b", "main", repo]);
  await exec("git", ["config", "user.name", "GitSpark E2E"], { cwd: repo });
  await exec("git", ["config", "user.email", "e2e@gitspark.local"], {
    cwd: repo,
  });
  await fs.writeFile(path.join(repo, "README.md"), "scale\n");
  await fs.writeFile(path.join(repo, "huge-diff.txt"), makeLines("base", 5000));
  await exec("git", ["add", "--all"], { cwd: repo });
  await exec("git", ["commit", "-m", "initial"], { cwd: repo });

  for (let i = 1; i <= 1000; i += 1) {
    await fs.writeFile(path.join(repo, "commit-counter.txt"), `${i}\n`);
    await exec("git", ["add", "commit-counter.txt"], { cwd: repo });
    await exec("git", ["commit", "-m", `scale commit ${i}`], { cwd: repo });
  }

  for (let i = 0; i < 150; i += 1) {
    await exec("git", ["branch", `scale/branch-${String(i).padStart(3, "0")}`], {
      cwd: repo,
    });
  }

  const writes = [];
  for (let i = 0; i < 1000; i += 1) {
    writes.push(fs.writeFile(path.join(repo, `changed-${i}.txt`), `changed ${i}\n`));
  }
  await Promise.all(writes);
  await fs.writeFile(path.join(repo, "huge-diff.txt"), makeLines("changed", 5000));
  return await fs.realpath(repo);
}

function makeLines(prefix, count) {
  return Array.from({ length: count }, (_, index) => `${prefix} ${index}`).join("\n") + "\n";
}
