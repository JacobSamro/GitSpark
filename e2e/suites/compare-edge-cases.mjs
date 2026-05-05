import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { assert } from "../support/assertions.mjs";
import { nodeById } from "../support/tree.mjs";

const exec = promisify(execFile);

export async function testCompareEdgeCases(app) {
  const repo = await makeCompareRepo();
  await app.openRepo(repo);
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.path === repo && snapshot.repo.current_branch === "main",
    { timeoutMs: 15_000 },
  );

  await app.command({ command: "compare_branch", name: "feature/complex" });
  let snapshot = await app.waitForSnapshot(
    (snapshot) =>
      snapshot.sidebar_tab === "history" &&
      snapshot.compare?.target_branch === "feature/complex" &&
      snapshot.compare.files.some((file) => file.path === "binary.dat") &&
      snapshot.compare.files.some((file) => file.path === "docs/renamed.txt") &&
      nodeById(snapshot.test_tree, "compare-merge-button")?.enabled === true &&
      nodeById(snapshot.test_tree, "commit-file-list-viewport")?.visible === true,
    { timeoutMs: 10_000 },
  );
  assert(
    snapshot.selected_commit_file === snapshot.compare.files[0].path,
    "compare selects the first changed file",
  );

  await app.command({ command: "refresh_repo" });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.current_branch === "main" &&
      snapshot.compare?.target_branch === "feature/complex",
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("tab-changes").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.sidebar_tab === "changes" && snapshot.compare === null,
    { timeoutMs: 10_000 },
  );

  await app.command({ command: "compare_branch", name: "feature/complex" });
  await app.getByTestId("compare-exit-button").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.sidebar_tab === "history" &&
      snapshot.compare === null &&
      nodeById(snapshot.test_tree, "compare-merge-button") === null,
    { timeoutMs: 10_000 },
  );

  await app.command({ command: "compare_branch", name: "same-as-main" });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.compare?.target_branch === "same-as-main" &&
      snapshot.compare.files.length === 0 &&
      snapshot.compare.commits.length === 0 &&
      nodeById(snapshot.test_tree, "compare-merge-button")?.enabled === false,
    { timeoutMs: 10_000 },
  );

  await app.command({ command: "compare_branch", name: "feature/complex" });
  await app.getByTestId("button-branch-selector").click();
  await app.getByTestId("branch-feature-complex").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.current_branch === "feature/complex" &&
      snapshot.compare === null,
    { timeoutMs: 15_000 },
  );

  await app.getByTestId("branch-main").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.current_branch === "main",
    { timeoutMs: 15_000 },
  );
  await app.command({ command: "compare_branch", name: "delete-target" });
  await exec("git", ["branch", "-D", "delete-target"], { cwd: repo });
  await app.command({ command: "refresh_repo" });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Repository refreshed." &&
      snapshot.compare === null &&
      !snapshot.repo?.branches.some((branch) => branch.name === "delete-target"),
    { timeoutMs: 10_000 },
  );
}

async function makeCompareRepo() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-compare-e2e-"));
  const repo = path.join(root, "repo");
  await exec("git", ["init", "-b", "main", repo]);
  await gitConfig(repo);
  await fs.mkdir(path.join(repo, "docs"), { recursive: true });
  await fs.writeFile(path.join(repo, "docs", "original.txt"), "one\n");
  await fs.writeFile(path.join(repo, "README.md"), "compare base\n");
  await exec("git", ["add", "--all"], { cwd: repo });
  await exec("git", ["commit", "-m", "initial"], { cwd: repo });
  await exec("git", ["branch", "same-as-main"], { cwd: repo });

  await exec("git", ["switch", "-c", "feature/complex"], { cwd: repo });
  await exec("git", ["mv", "docs/original.txt", "docs/renamed.txt"], {
    cwd: repo,
  });
  await fs.writeFile(path.join(repo, "docs", "renamed.txt"), "one\ntwo\n");
  await fs.writeFile(path.join(repo, "binary.dat"), Buffer.from([0, 1, 2, 3]));
  await exec("git", ["add", "--all"], { cwd: repo });
  await exec("git", ["commit", "-m", "feature: complex compare files"], {
    cwd: repo,
  });

  await exec("git", ["switch", "main"], { cwd: repo });
  await exec("git", ["switch", "-c", "delete-target"], { cwd: repo });
  await fs.writeFile(path.join(repo, "delete-target.txt"), "temporary\n");
  await exec("git", ["add", "--all"], { cwd: repo });
  await exec("git", ["commit", "-m", "feature: deleted compare target"], {
    cwd: repo,
  });
  await exec("git", ["switch", "main"], { cwd: repo });
  return await fs.realpath(repo);
}

async function gitConfig(repo) {
  await exec("git", ["config", "user.name", "GitSpark E2E"], { cwd: repo });
  await exec("git", ["config", "user.email", "e2e@gitspark.local"], {
    cwd: repo,
  });
}
