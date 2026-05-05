import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { assert } from "../support/assertions.mjs";
import { nodeById } from "../support/tree.mjs";

const exec = promisify(execFile);

export async function testFileOperationEdgeCases(app) {
  const repo = await makeFileEdgeRepo();
  await app.openRepo(repo);
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.path === repo &&
      hasChanges(snapshot, [
        ".gitignore",
        "binary.bin",
        "deleted.txt",
        "deep/path/with spaces/file name.txt",
        "renamed-new.txt",
        "unicode-é.txt",
      ]),
    { timeoutMs: 15_000 },
  );

  let snapshot = await app.snapshot();
  assert(
    nodeById(snapshot.test_tree, "change-gitignore-ignore-path")?.enabled === false,
    ".gitignore ignore action is disabled",
  );
  assert(
    snapshot.repo.changes.some(
      (change) => change.path === "ignored-folder/ignored.txt",
    ) === false,
    "ignored folders do not appear in the changes list",
  );

  await app.command({
    command: "change_action",
    path: "deep/path/with spaces/file name.txt",
    action: "copy_relative_path",
  });
  assert(
    (await app.clipboardText()).text === "deep/path/with spaces/file name.txt",
    "copy relative path handles deeply nested paths with spaces",
  );

  await app.command({
    command: "change_action",
    path: "unicode-é.txt",
    action: "copy_full_path",
  });
  assert(
    (await app.clipboardText()).text === path.join(repo, "unicode-é.txt"),
    "copy full path handles Unicode filenames",
  );

  await app.command({
    command: "change_action",
    path: "binary.bin",
    action: "open_in_editor",
  });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Opened 'binary.bin' in external editor." &&
      snapshot.error_message === "",
    { timeoutMs: 10_000 },
  );

  await app.command({
    command: "change_action",
    path: "deleted.txt",
    action: "discard",
  });
  await app.waitForSnapshot(
    (snapshot) =>
      !snapshot.repo?.changes.some((change) => change.path === "deleted.txt") &&
      snapshot.status_message === "Discarded changes for 'deleted.txt'.",
    { timeoutMs: 10_000 },
  );
  assert(
    (await fs.readFile(path.join(repo, "deleted.txt"), "utf8")) === "delete me\n",
    "discard deleted file restores tracked file",
  );

  await app.command({
    command: "change_action",
    path: "untracked file.txt",
    action: "discard",
  });
  await app.waitForSnapshot(
    (snapshot) =>
      !snapshot.repo?.changes.some((change) => change.path === "untracked file.txt") &&
      snapshot.status_message === "Discarded changes for 'untracked file.txt'.",
    { timeoutMs: 10_000 },
  );
  assert(
    !(await exists(path.join(repo, "untracked file.txt"))),
    "discard untracked file removes it from disk",
  );

  await app.command({
    command: "change_action",
    path: "renamed-new.txt",
    action: "copy_relative_path",
  });
  assert(
    (await app.clipboardText()).text === "renamed-new.txt",
    "renamed files expose the new path for file actions",
  );
}

function hasChanges(snapshot, paths) {
  return paths.every((path) =>
    snapshot.repo?.changes.some((change) => change.path === path),
  );
}

async function makeFileEdgeRepo() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-file-e2e-"));
  const repo = path.join(root, "repo");
  await exec("git", ["init", "-b", "main", repo]);
  await exec("git", ["config", "user.name", "GitSpark E2E"], { cwd: repo });
  await exec("git", ["config", "user.email", "e2e@gitspark.local"], {
    cwd: repo,
  });
  await fs.mkdir(path.join(repo, "ignored-folder"), { recursive: true });
  await fs.writeFile(path.join(repo, ".gitignore"), "ignored-folder/\n");
  await fs.writeFile(path.join(repo, "deleted.txt"), "delete me\n");
  await fs.writeFile(path.join(repo, "renamed-old.txt"), "rename me\n");
  await fs.writeFile(path.join(repo, "README.md"), "file edge\n");
  await exec("git", ["add", "--all"], { cwd: repo });
  await exec("git", ["commit", "-m", "initial"], { cwd: repo });

  await fs.writeFile(path.join(repo, ".gitignore"), "ignored-folder/\n*.cache\n");
  await fs.writeFile(path.join(repo, "binary.bin"), Buffer.from([0, 255, 1, 254]));
  await fs.rm(path.join(repo, "deleted.txt"));
  await fs.mkdir(path.join(repo, "deep", "path", "with spaces"), {
    recursive: true,
  });
  await fs.writeFile(
    path.join(repo, "deep", "path", "with spaces", "file name.txt"),
    "deep\n",
  );
  await exec("git", ["mv", "renamed-old.txt", "renamed-new.txt"], { cwd: repo });
  await fs.writeFile(path.join(repo, "unicode-é.txt"), "unicode\n");
  await fs.writeFile(path.join(repo, "untracked file.txt"), "untracked\n");
  await fs.writeFile(path.join(repo, "ignored-folder", "ignored.txt"), "ignored\n");
  return await fs.realpath(repo);
}

async function exists(filePath) {
  try {
    await fs.access(filePath);
    return true;
  } catch {
    return false;
  }
}
