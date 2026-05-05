import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { assert } from "../support/assertions.mjs";
import { nodeById } from "../support/tree.mjs";
import { waitForOpenUrl } from "../support/url-log.mjs";

const exec = promisify(execFile);

export async function testGithubEnterpriseUrlBehavior(app, fixture) {
  const repo = await makeUrlRepo();
  await app.openRepo(repo);
  let snapshot = await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.path === repo && snapshot.repo.history.length > 0,
    { timeoutMs: 15_000 },
  );
  const commit = snapshot.repo.history[0];

  await assertGithubRemote(app, fixture, repo, {
    remote: "https://github.com/gitspark/url-fixture.git",
    expectedBase: "https://github.com/gitspark/url-fixture",
    commit,
  });
  await assertGithubRemote(app, fixture, repo, {
    remote: "https://github.enterprise.local/octo/url-fixture.git",
    expectedBase: "https://github.enterprise.local/octo/url-fixture",
    commit,
  });
  await assertGithubRemote(app, fixture, repo, {
    remote: "https://github.enterprise.local:8443/octo/url-fixture.git",
    expectedBase: "https://github.enterprise.local:8443/octo/url-fixture",
    commit,
  });
  await assertGithubRemote(app, fixture, repo, {
    remote: "ssh://git@github.enterprise.local:2222/octo/url-fixture.git",
    expectedBase: "https://github.enterprise.local/octo/url-fixture",
    commit,
  });

  await exec("git", ["remote", "set-url", "origin", "https://gitlab.com/octo/url-fixture.git"], {
    cwd: repo,
  });
  await app.command({ command: "refresh_repo" });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.has_github_remote === false &&
      !nodeById(snapshot.test_tree, `commit-${commit.short_oid}-view-on-github`) &&
      !nodeById(snapshot.test_tree, "compare-on-github"),
    { timeoutMs: 10_000 },
  );

  for (const remote of [
    "https://bitbucket.org/octo/url-fixture.git",
    "ssh://git@code.internal.local/octo/url-fixture.git",
  ]) {
    await exec("git", ["remote", "set-url", "origin", remote], { cwd: repo });
    await app.command({ command: "refresh_repo" });
    await app.waitForSnapshot(
      (snapshot) =>
        snapshot.repo?.has_github_remote === false &&
        !snapshot.test_tree.children.some((node) =>
          node.id?.includes("view-on-github"),
        ),
      { timeoutMs: 10_000 },
    );
  }
}

async function assertGithubRemote(app, fixture, repo, { remote, expectedBase, commit }) {
  await exec("git", ["remote", "set-url", "origin", remote], { cwd: repo });
  await app.command({ command: "refresh_repo" });
  await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.has_github_remote === true,
    { timeoutMs: 10_000 },
  );

  await app.command({
    command: "history_action",
    oid: commit.oid,
    action: "view_on_github",
  });
  await waitForOpenUrl(
    fixture.openUrlLog,
    `${expectedBase}/commit/${commit.oid}`,
    `${remote} opens commit URL`,
  );

  await app.command({
    command: "branch_action",
    name: "feature/encoded-path",
    action: "view_on_github",
  });
  await waitForOpenUrl(
    fixture.openUrlLog,
    `${expectedBase}/tree/feature%2Fencoded-path`,
    `${remote} encodes branch names`,
  );

  await app.getByTestId("tab-changes").click();
  await app.getByTestId("change-dir-file-with-space-txt-view-on-github").click();
  await waitForOpenUrl(
    fixture.openUrlLog,
    `${expectedBase}/blob/main/dir/file%20with%20space.txt`,
    `${remote} encodes file paths`,
  );
}

async function makeUrlRepo() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-url-e2e-"));
  const repo = path.join(root, "repo");
  await exec("git", ["init", "-b", "main", repo]);
  await exec("git", ["config", "user.name", "GitSpark E2E"], { cwd: repo });
  await exec("git", ["config", "user.email", "e2e@gitspark.local"], {
    cwd: repo,
  });
  await fs.mkdir(path.join(repo, "dir"), { recursive: true });
  await fs.writeFile(path.join(repo, "README.md"), "url base\n");
  await fs.writeFile(path.join(repo, "dir", "file with space.txt"), "tracked\n");
  await exec("git", ["add", "--all"], { cwd: repo });
  await exec("git", ["commit", "-m", "initial"], { cwd: repo });
  await exec("git", ["branch", "feature/encoded-path"], { cwd: repo });
  await exec("git", ["remote", "add", "origin", "https://github.com/gitspark/url-fixture.git"], {
    cwd: repo,
  });
  await fs.writeFile(path.join(repo, "dir", "file with space.txt"), "tracked\nchanged\n");
  return await fs.realpath(repo);
}
