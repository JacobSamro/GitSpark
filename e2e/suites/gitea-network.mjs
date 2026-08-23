import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { expect } from "../gitspark.mjs";
import { assert } from "../support/assertions.mjs";
import { getRemoteBranchSha } from "../support/gitea.mjs";

const exec = promisify(execFile);

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

// GitSpark's primary network button only shows "Push" once the repo has an
// upstream tracking branch to compute ahead/behind against (see
// NetworkAction::from_snapshot in src/ui/domain_state.rs — with no upstream
// configured, ahead/behind are always 0/0 and the button reads "Fetch"
// instead). So the repo is published once via plain git first, exactly as
// a real clone of an already-tracked repo would look, then a second commit
// is made locally to put GitSpark's UI into the "ahead" state a real push
// click exercises.
async function makeLocalRepo(root, remoteUrl) {
  await exec("git", ["init", "-q", "-b", "main", root]);
  await gitConfig(root);
  await fs.writeFile(path.join(root, "README.md"), "# GitSpark Gitea E2E\n");
  await exec("git", ["add", "--all"], { cwd: root });
  await exec("git", ["commit", "-q", "-m", "initial commit"], { cwd: root });
  await exec("git", ["remote", "add", "origin", remoteUrl], { cwd: root });
  await exec("git", ["push", "-q", "-u", "origin", "main"], { cwd: root });

  await fs.writeFile(path.join(root, "local-second-commit.txt"), "second commit\n");
  await exec("git", ["add", "--all"], { cwd: root });
  await exec("git", ["commit", "-q", "-m", "second commit"], { cwd: root });
}

// Same shape as `createRemoteOnlyCommit` in support/fixtures.mjs, but
// against a real Gitea clone instead of a local bare repo — pushes a commit
// "someone else" made, over the same token-in-URL HTTP remote.
async function pushCommitFromClone(cloneDir, remoteUrl, fileName, content) {
  await exec("git", ["clone", "-q", remoteUrl, cloneDir]);
  await gitConfig(cloneDir);
  await fs.writeFile(path.join(cloneDir, fileName), content);
  await exec("git", ["add", "--all"], { cwd: cloneDir });
  await exec("git", ["commit", "-q", "-m", `remote: add ${fileName}`], {
    cwd: cloneDir,
  });
  await exec("git", ["push", "-q", "origin", "main"], { cwd: cloneDir });
}

export async function testGiteaNetworkFlows(app, gitea) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-gitea-e2e-"));
  const workRepo = path.join(root, "work-repo");
  await makeLocalRepo(workRepo, gitea.remoteUrl);

  await app.openRepo(workRepo);
  await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.path === workRepo,
    { timeoutMs: 10_000 },
  );

  // 1. Push the ahead-by-one local commit — exercises push_origin's
  // has-upstream branch over real HTTP auth.
  await app.waitForSnapshot((snapshot) => snapshot.repo?.ahead === 1, {
    timeoutMs: 10_000,
  });
  await app.getByTestId("button-network-push").click();
  await expect(app.getByText("Push origin complete.")).toBeVisible({
    timeoutMs: 20_000,
  });

  const localHead = await gitOutput(workRepo, ["rev-parse", "HEAD"]);
  const pushedSha = await getRemoteBranchSha(gitea, "main");
  assert(
    pushedSha === localHead,
    `push must land on the real Gitea server: local ${localHead}, remote ${pushedSha}`,
  );

  // 2. An upstream-only commit, fetched and fast-forward pulled — the
  // "someone else pushed" case, now over a live server instead of a local
  // bare repo.
  await pushCommitFromClone(
    path.join(root, "clone-upstream"),
    gitea.remoteUrl,
    "upstream.txt",
    "upstream change\n",
  );

  await app.getByTestId("button-network-fetch").click();
  await expect(app.getByText("Fetch origin complete.")).toBeVisible({
    timeoutMs: 20_000,
  });
  await app.waitForSnapshot((snapshot) => snapshot.repo?.behind === 1, {
    timeoutMs: 10_000,
  });

  await app.getByTestId("button-network-pull").click();
  await expect(app.getByText("Pull origin complete.")).toBeVisible({
    timeoutMs: 20_000,
  });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.behind === 0 &&
      snapshot.repo?.history.some(
        (commit) => commit.summary === "remote: add upstream.txt",
      ),
    { timeoutMs: 10_000 },
  );
  assert(
    await fs
      .access(path.join(workRepo, "upstream.txt"))
      .then(() => true)
      .catch(() => false),
    "fast-forward pull must actually update the working tree",
  );

  // 3. The rough edges: a real divergence. Someone else pushes again while
  // this clone independently commits its own change, without fetching
  // first — the everyday way to hit "Push origin failed" against a real
  // server, mirroring push_reports_the_real_git_reason_when_rejected in
  // src/git.rs, but this time driven through the actual UI.
  await pushCommitFromClone(
    path.join(root, "clone-diverge"),
    gitea.remoteUrl,
    "remote-only.txt",
    "diverging remote change\n",
  );
  await fs.writeFile(path.join(workRepo, "local-only.txt"), "diverging local change\n");
  await exec("git", ["add", "--all"], { cwd: workRepo });
  await exec("git", ["commit", "-q", "-m", "local: add local-only.txt"], {
    cwd: workRepo,
  });

  // The local-only commit was made outside the app (plain git), so this
  // waits for GitSpark's own filesystem watcher to notice it and refresh
  // ahead/behind before clicking — the same reason step 1 waits for
  // ahead === 1 before its push.
  await app.waitForSnapshot((snapshot) => snapshot.repo?.ahead === 1, {
    timeoutMs: 10_000,
  });
  await app.getByTestId("button-network-push").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.error_message.startsWith("Push origin failed:") &&
      (snapshot.error_message.includes("[rejected]") ||
        snapshot.error_message.includes("failed to push some refs")),
    { timeoutMs: 20_000 },
  );

  // Gitea's rejection surfaces the same way a local bare repo's would, so
  // there is nothing Gitea-specific about the wording — the value here is
  // in confirming that a REAL server's rejection round-trips through the
  // UI correctly rather than hanging, timing out, or getting swallowed.
  //
  // The primary network button still reads "Push" here (ahead > 0), so
  // fetching now means opening the network dropdown's secondary fetch
  // action rather than the primary button — a path the rest of the e2e
  // suite has never exercised either.
  await app.getByTestId("network-caret").click();
  await app.waitForSnapshot((snapshot) => snapshot.show_network_dropdown === true, {
    timeoutMs: 5_000,
  });
  await app.getByTestId("network-dropdown-fetch").click();
  await expect(app.getByText("Fetch origin complete.")).toBeVisible({
    timeoutMs: 20_000,
  });
  await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.ahead === 1 && snapshot.repo?.behind === 1,
    { timeoutMs: 10_000 },
  );

  // GitSpark's pull is always `--ff-only` (see pull_origin in src/git.rs) —
  // it never attempts a real merge, so a genuinely diverged branch fails
  // pull the same way whether or not the two sides would actually conflict
  // on file content. This is the concrete rough edge: GitSpark surfaces
  // "not possible to fast-forward", never a merge-conflict UI, because it
  // has none.
  await app.getByTestId("button-network-pull").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.error_message.startsWith("Pull origin failed:") &&
      snapshot.error_message.includes("Not possible to fast-forward"),
    { timeoutMs: 30_000 },
  );

  await fs.rm(root, { recursive: true, force: true });
}

// Exercises GitSpark's "Clone Repository" flow against the same live Gitea
// remote, proving clone (not just push/fetch/pull on an already-open repo)
// also works over a real authenticated HTTP transport.
export async function testGiteaCloneWorkflow(app, gitea) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-gitea-clone-e2e-"));
  const cloneParent = path.join(root, "clones");
  await fs.mkdir(cloneParent, { recursive: true });

  await app.command({ command: "show_clone_repository" });
  await app.getByTestId("clone-repository-url-input").fill(gitea.remoteUrl);
  await app.getByTestId("clone-repository-path-input").fill(cloneParent);
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "clone_repository" &&
      snapshot.test_tree?.children?.some((node) => node.id === "clone-repository-confirm"),
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("clone-repository-confirm").click();
  const clonedPath = path.join(cloneParent, gitea.repoName);
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.path === clonedPath &&
      snapshot.status_message === `Cloned repository from '${gitea.remoteUrl}'.`,
    { timeoutMs: 20_000 },
  );

  const clonedHead = await gitOutput(clonedPath, ["rev-parse", "HEAD"]);
  const remoteHead = await getRemoteBranchSha(gitea, "main");
  assert(
    clonedHead === remoteHead,
    `clone from a live Gitea remote must check out the real HEAD: cloned ${clonedHead}, remote ${remoteHead}`,
  );

  await fs.rm(root, { recursive: true, force: true });
}
