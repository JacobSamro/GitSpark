import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { nodeById } from "../support/tree.mjs";

const exec = promisify(execFile);

export async function testImageDiffPreview(app) {
  const repo = await makeImageDiffRepo();
  await app.openRepo(repo);
  await app.command({ command: "select_change", path: "pixel.png" });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.selected_change === "pixel.png" &&
      snapshot.selected_diff_is_image === true &&
      nodeById(snapshot.test_tree, "diff-image-preview")?.visible === true &&
      nodeById(snapshot.test_tree, "diff-image-reveal")?.visible === true &&
      nodeById(snapshot.test_tree, "diff-image-reveal")?.enabled === true &&
      nodeById(snapshot.test_tree, "diff-image-open-default")?.visible === true &&
      nodeById(snapshot.test_tree, "diff-image-open-default")?.enabled === true &&
      nodeById(snapshot.test_tree, "diff-binary-open-default")?.visible ===
        false &&
      nodeById(snapshot.test_tree, "diff-option-side-by-side")?.visible ===
        false,
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("diff-image-reveal").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Revealed 'pixel.png' in Finder." &&
      snapshot.error_message === "",
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("diff-image-open-default").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message ===
        "Opened 'pixel.png' with the default program." &&
      snapshot.error_message === "",
    { timeoutMs: 10_000 },
  );
}

async function makeImageDiffRepo() {
  const repo = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-image-diff-"));
  await exec("git", ["init", "-b", "main"], { cwd: repo });
  await exec("git", ["config", "user.name", "GitSpark E2E"], { cwd: repo });
  await exec("git", ["config", "user.email", "e2e@gitspark.local"], {
    cwd: repo,
  });
  const baseline = await fs.readFile(path.resolve("assets/gitspark.png"));
  await fs.writeFile(path.join(repo, "pixel.png"), baseline);
  await exec("git", ["add", "pixel.png"], { cwd: repo });
  await exec("git", ["commit", "-m", "baseline image"], { cwd: repo });
  await fs.writeFile(
    path.join(repo, "pixel.png"),
    Buffer.concat([baseline, Buffer.from("\nchanged")]),
  );
  return await fs.realpath(repo);
}
