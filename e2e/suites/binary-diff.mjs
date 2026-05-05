import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { nodeById } from "../support/tree.mjs";

const exec = promisify(execFile);

export async function testBinaryDiffFallback(app) {
  const repo = await makeBinaryDiffRepo();
  await app.openRepo(repo);
  await app.command({ command: "select_change", path: "asset.bin" });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.selected_change === "asset.bin" &&
      nodeById(snapshot.test_tree, "diff-binary-reveal")?.visible === true &&
      nodeById(snapshot.test_tree, "diff-binary-reveal")?.enabled === true &&
      nodeById(snapshot.test_tree, "diff-binary-open-default")?.visible ===
        true &&
      nodeById(snapshot.test_tree, "diff-binary-open-default")?.enabled ===
        true,
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("diff-binary-reveal").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Revealed 'asset.bin' in Finder." &&
      snapshot.error_message === "",
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("diff-binary-open-default").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message ===
        "Opened 'asset.bin' with the default program." &&
      snapshot.error_message === "",
    { timeoutMs: 10_000 },
  );
}

async function makeBinaryDiffRepo() {
  const repo = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-binary-diff-"));
  await exec("git", ["init", "-b", "main"], { cwd: repo });
  await exec("git", ["config", "user.name", "GitSpark E2E"], { cwd: repo });
  await exec("git", ["config", "user.email", "e2e@gitspark.local"], {
    cwd: repo,
  });
  await fs.writeFile(path.join(repo, "asset.bin"), Buffer.from([0, 1, 2, 3]));
  await exec("git", ["add", "asset.bin"], { cwd: repo });
  await exec("git", ["commit", "-m", "baseline"], { cwd: repo });
  await fs.writeFile(path.join(repo, "asset.bin"), Buffer.from([3, 2, 1, 0]));
  return await fs.realpath(repo);
}
