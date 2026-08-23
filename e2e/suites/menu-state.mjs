import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { assert } from "../support/assertions.mjs";

const exec = promisify(execFile);

export async function testMenuStateWithoutRepository(app) {
  const snapshot = await app.snapshot();
  assert(snapshot.repo === null, "menu no-repo check starts without a repository");
  assert(snapshot.menu_availability.has_repository === false, "no repo disables repository menus");
  assert(snapshot.menu_availability.fetch === false, "no repo disables Fetch");
  assert(snapshot.menu_availability.pull === false, "no repo disables Pull");
  assert(snapshot.menu_availability.push === false, "no repo disables Push");
  assert(snapshot.menu_availability.create_branch === false, "no repo disables branch actions");
  assert(snapshot.menu_availability.view_repository_on_github === false, "no repo disables GitHub actions");

  await app.command({
    command: "change_action",
    path: "missing.txt",
    action: "reveal_in_finder",
  });
  await app.waitForSnapshot(
    (snapshot) => snapshot.error_message === "No repository selected.",
    { timeoutMs: 10_000 },
  );
  await app.command({
    command: "change_action",
    path: "missing.txt",
    action: "open_in_editor",
  });
  await app.waitForSnapshot(
    (snapshot) => snapshot.error_message === "No repository selected.",
    { timeoutMs: 10_000 },
  );

  await app.command({ command: "open_in_terminal" });
  await app.waitForSnapshot(
    (snapshot) => snapshot.error_message === "No repository selected.",
    { timeoutMs: 10_000 },
  );
}

export async function testMenuStateWithRepository(app) {
  const repo = await makeMenuRepo();
  await app.openRepo(repo);
  let snapshot = await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.path === repo,
    { timeoutMs: 15_000 },
  );
  assert(snapshot.menu_availability.fetch === true, "remote repo enables Fetch");
  assert(snapshot.menu_availability.pull === true, "named branch enables Pull");
  assert(snapshot.menu_availability.push === true, "named branch enables Push");
  assert(snapshot.menu_availability.view_repository_on_github === false, "non-GitHub remote disables GitHub menu actions");

  await exec("git", ["checkout", "--detach"], { cwd: repo });
  await app.command({ command: "refresh_repo" });
  snapshot = await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.current_branch === "detached HEAD",
    { timeoutMs: 10_000 },
  );
  assert(snapshot.menu_availability.fetch === true, "detached HEAD still allows Fetch");
  assert(snapshot.menu_availability.pull === false, "detached HEAD disables Pull");
  assert(snapshot.menu_availability.push === false, "detached HEAD disables Push");
  assert(snapshot.menu_availability.modify_current_branch === false, "detached HEAD disables branch mutation actions");

  await exec("git", ["switch", "main"], { cwd: repo });
  await exec("git", ["merge", "conflict"], { cwd: repo }).catch(() => {});
  await app.command({ command: "refresh_repo" });
  snapshot = await app.waitForSnapshot(
    (snapshot) =>
      snapshot.operation?.kind === "merge" &&
      snapshot.menu_availability.change_worktree === false,
    { timeoutMs: 10_000 },
  );
  assert(snapshot.menu_availability.fetch === false, "active conflict disables Fetch");
  assert(snapshot.menu_availability.modify_current_branch === false, "active conflict disables branch actions");

  await exec("git", ["merge", "--abort"], { cwd: repo }).catch(() => {});
  await app.command({ command: "refresh_repo" });
  await app.command({ command: "open_in_terminal" });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Opened repository in Terminal." &&
      snapshot.error_message === "",
    { timeoutMs: 10_000 },
  );
}

async function makeMenuRepo() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-menu-e2e-"));
  const remote = path.join(root, "origin.git");
  const repo = path.join(root, "repo");
  await exec("git", ["init", "--bare", remote]);
  await exec("git", ["init", "-b", "main", repo]);
  await gitConfig(repo);
  await fs.writeFile(path.join(repo, "conflict.txt"), "base\n");
  await exec("git", ["add", "--all"], { cwd: repo });
  await exec("git", ["commit", "-m", "initial"], { cwd: repo });
  await exec("git", ["switch", "-c", "conflict"], { cwd: repo });
  await fs.writeFile(path.join(repo, "conflict.txt"), "branch\n");
  await exec("git", ["add", "--all"], { cwd: repo });
  await exec("git", ["commit", "-m", "branch conflict"], { cwd: repo });
  await exec("git", ["switch", "main"], { cwd: repo });
  await fs.writeFile(path.join(repo, "conflict.txt"), "main\n");
  await exec("git", ["add", "--all"], { cwd: repo });
  await exec("git", ["commit", "-m", "main conflict"], { cwd: repo });
  await exec("git", ["remote", "add", "origin", remote], { cwd: repo });
  await exec("git", ["push", "-u", "origin", "main"], { cwd: repo });
  return await fs.realpath(repo);
}

async function gitConfig(repo) {
  await exec("git", ["config", "user.name", "GitSpark E2E"], { cwd: repo });
  await exec("git", ["config", "user.email", "e2e@gitspark.local"], {
    cwd: repo,
  });
}
