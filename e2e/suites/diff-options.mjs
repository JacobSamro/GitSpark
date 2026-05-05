import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { assert } from "../support/assertions.mjs";
import { nodeById } from "../support/tree.mjs";

const exec = promisify(execFile);

export async function testDiffOptions(app) {
  const repo = await makeDiffOptionsRepo();
  await app.openRepo(repo);
  await app.command({ command: "select_change", path: "code.txt" });
  const before = await app.waitForSnapshot(
    (snapshot) =>
      snapshot.selected_change === "code.txt" &&
      snapshot.selected_diff_visible_line_count > 0 &&
      nodeById(snapshot.test_tree, "diff-option-side-by-side")?.selected ===
        false &&
      nodeById(snapshot.test_tree, "diff-option-hide-whitespace")?.selected === false,
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("diff-option-side-by-side").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.diff_show_side_by_side === true &&
      nodeById(snapshot.test_tree, "diff-option-side-by-side")?.selected ===
        true &&
      snapshot.selected_diff_visible_line_count ===
        before.selected_diff_visible_line_count,
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("diff-option-hide-whitespace").click();
  const hidden = await app.waitForSnapshot(
    (snapshot) =>
      snapshot.diff_hide_whitespace_changes === true &&
      snapshot.diff_show_side_by_side === true &&
      nodeById(snapshot.test_tree, "diff-option-hide-whitespace")?.selected === true &&
      snapshot.selected_diff_visible_line_count <
        before.selected_diff_visible_line_count,
    { timeoutMs: 10_000 },
  );

  assert(
    hidden.selected_diff_visible_line_count > 0,
    "non-whitespace diff lines remain visible when whitespace changes are hidden",
  );

  await app.getByTestId("diff-option-hide-whitespace").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.diff_hide_whitespace_changes === false &&
      snapshot.selected_diff_visible_line_count ===
        before.selected_diff_visible_line_count,
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("diff-option-side-by-side").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.diff_show_side_by_side === false &&
      nodeById(snapshot.test_tree, "diff-option-side-by-side")?.selected ===
        false,
    { timeoutMs: 10_000 },
  );
}

async function makeDiffOptionsRepo() {
  const repo = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-diff-options-"));
  await exec("git", ["init", "-b", "main"], { cwd: repo });
  await exec("git", ["config", "user.name", "GitSpark E2E"], { cwd: repo });
  await exec("git", ["config", "user.email", "e2e@gitspark.local"], {
    cwd: repo,
  });
  await fs.writeFile(
    path.join(repo, "code.txt"),
    [
      '    println!("hello");',
      "line 02",
      "line 03",
      "line 04",
      "line 05",
      "line 06",
      "line 07",
      "line 08",
      "line 09",
      "line 10",
      "line 11",
      "line 12",
    ].join("\n") + "\n",
  );
  await exec("git", ["add", "code.txt"], { cwd: repo });
  await exec("git", ["commit", "-m", "baseline"], { cwd: repo });
  await fs.writeFile(
    path.join(repo, "code.txt"),
    [
      '        println!("hello");',
      "line 02",
      "line 03",
      "line 04",
      "line 05",
      "line 06",
      "line 07",
      "line 08",
      "line 09",
      "line 10",
      "line 11",
      "line 12 changed",
    ].join("\n") + "\n",
  );
  return await fs.realpath(repo);
}
