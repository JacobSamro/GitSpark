import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { nodeById } from "../support/tree.mjs";

const exec = promisify(execFile);

export async function testSubmoduleDiff(app) {
  const { parent, child } = await makeSubmoduleDiffRepo();
  await app.openRepo(parent);
  await app.command({ command: "select_change", path: "deps/child" });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.selected_change === "deps/child" &&
      snapshot.selected_diff_is_submodule === true &&
      nodeById(snapshot.test_tree, "diff-submodule-open")?.visible === true &&
      nodeById(snapshot.test_tree, "diff-submodule-open")?.enabled === true &&
      nodeById(snapshot.test_tree, "diff-submodule-reveal")?.visible === true &&
      nodeById(snapshot.test_tree, "diff-submodule-reveal")?.enabled === true &&
      nodeById(snapshot.test_tree, "diff-option-side-by-side")?.visible ===
        false,
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("diff-submodule-reveal").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Revealed 'deps/child' in Finder." &&
      snapshot.error_message === "",
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("diff-submodule-open").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.path === child &&
      snapshot.status_message === "Repository loaded." &&
      snapshot.error_message === "",
    { timeoutMs: 10_000 },
  );
}

async function makeSubmoduleDiffRepo() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-submodule-diff-"));
  const child = path.join(root, "child");
  const parent = path.join(root, "parent");

  await fs.mkdir(child);
  await exec("git", ["init", "-b", "main"], { cwd: child });
  await configureGitUser(child);
  await fs.writeFile(path.join(child, "README.md"), "child baseline\n");
  await exec("git", ["add", "README.md"], { cwd: child });
  await exec("git", ["commit", "-m", "child baseline"], { cwd: child });

  await fs.mkdir(parent);
  await exec("git", ["init", "-b", "main"], { cwd: parent });
  await configureGitUser(parent);
  await exec(
    "git",
    ["-c", "protocol.file.allow=always", "submodule", "add", child, "deps/child"],
    { cwd: parent },
  );
  await exec("git", ["commit", "-am", "add submodule"], { cwd: parent });

  await fs.writeFile(path.join(child, "README.md"), "child changed\n");
  await exec("git", ["commit", "-am", "child changed"], { cwd: child });
  const { stdout: childOid } = await exec("git", ["rev-parse", "HEAD"], {
    cwd: child,
  });

  const submodulePath = path.join(parent, "deps", "child");
  await exec("git", ["-c", "protocol.file.allow=always", "fetch", "origin", "main"], {
    cwd: submodulePath,
  });
  await exec("git", ["checkout", childOid.trim()], { cwd: submodulePath });

  return {
    parent: await fs.realpath(parent),
    child: await fs.realpath(submodulePath),
  };
}

async function configureGitUser(repo) {
  await exec("git", ["config", "user.name", "GitSpark E2E"], { cwd: repo });
  await exec("git", ["config", "user.email", "e2e@gitspark.local"], {
    cwd: repo,
  });
}
