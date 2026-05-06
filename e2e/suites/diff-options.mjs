import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { assert } from "../support/assertions.mjs";
import { flattenNodes, nodeById } from "../support/tree.mjs";

const exec = promisify(execFile);

export async function testDiffOptions(app) {
  const repo = await makeDiffOptionsRepo();
  await app.openRepo(repo);
  await app.command({ command: "select_change", path: "code.txt" });
  const before = await app.waitForSnapshot(
    (snapshot) =>
      snapshot.selected_change === "code.txt" &&
      snapshot.selected_diff_visible_line_count > 0 &&
      snapshot.selected_diff_selectable_line_count === 4 &&
      nodeById(snapshot.test_tree, "diff-options-menu")?.visible === true,
    { timeoutMs: 10_000 },
  );

  const selectableLine = flattenNodes(before.test_tree).find((node) =>
    node.id?.startsWith("diff-line-code-txt-added-"),
  );
  assert(selectableLine, "changed diff lines expose stable selectable nodes");

  await app.getByTestId(selectableLine.test_id).click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.selected_diff_selected_line_count === 1 &&
      nodeById(snapshot.test_tree, selectableLine.id)?.selected === true,
    { timeoutMs: 10_000 },
  );

  await app.getByTestId(selectableLine.test_id).click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.selected_diff_selected_line_count === 0 &&
      nodeById(snapshot.test_tree, selectableLine.id)?.selected === false,
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("diff-options-menu").click();
  await app.getByTestId("diff-option-side-by-side").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.diff_show_side_by_side === true &&
      snapshot.selected_diff_visible_line_count ===
        before.selected_diff_visible_line_count,
    { timeoutMs: 10_000 },
  );
  await app.getByTestId(selectableLine.test_id).click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.diff_show_side_by_side === true &&
      snapshot.selected_diff_selected_line_count === 1 &&
      nodeById(snapshot.test_tree, selectableLine.id)?.selected === true,
    { timeoutMs: 10_000 },
  );
  await app.getByTestId(selectableLine.test_id).click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.diff_show_side_by_side === true &&
      snapshot.selected_diff_selected_line_count === 0 &&
      nodeById(snapshot.test_tree, selectableLine.id)?.selected === false,
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("diff-options-menu").click();
  await app.getByTestId("diff-option-hide-whitespace").click();
  const hidden = await app.waitForSnapshot(
    (snapshot) =>
      snapshot.diff_hide_whitespace_changes === true &&
      snapshot.diff_show_side_by_side === true &&
      snapshot.selected_diff_visible_line_count <
        before.selected_diff_visible_line_count &&
      snapshot.selected_diff_selectable_line_count === 0 &&
      snapshot.selected_diff_selected_line_count === 0,
    { timeoutMs: 10_000 },
  );

  assert(
    hidden.selected_diff_visible_line_count > 0,
    "non-whitespace diff lines remain visible when whitespace changes are hidden",
  );

  await app.getByTestId("diff-options-menu").click();
  await app.getByTestId("diff-option-hide-whitespace").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.diff_hide_whitespace_changes === false &&
      snapshot.selected_diff_visible_line_count ===
        before.selected_diff_visible_line_count,
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("diff-options-menu").click();
  await app.getByTestId("diff-option-unified").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.diff_show_side_by_side === false &&
      nodeById(snapshot.test_tree, "diff-options-menu")?.visible === true,
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("diff-line-code-txt-deleted-1").click();
  await app.getByTestId("diff-line-code-txt-added-1").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.selected_diff_selected_line_count === 2 &&
      nodeById(snapshot.test_tree, "diff-discard-selected-lines")?.visible === true,
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("input-commit-summary").fill("test: selected lines");
  await app.getByTestId("button-commit-all").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Commit created." &&
      snapshot.selected_diff_selected_line_count === 0 &&
      snapshot.selected_diff_selectable_line_count === 2 &&
      snapshot.repo?.changes.some((change) => change.path === "code.txt"),
    { timeoutMs: 15_000 },
  );
  const { stdout: committedText } = await exec("git", ["show", "HEAD:code.txt"], {
    cwd: repo,
  });
  assert(
    committedText.startsWith('        println!("hello");\n'),
    "line-level commit includes selected replacement",
  );
  assert(
    committedText.includes("line 12\n") &&
      !committedText.includes("line 12 changed\n"),
    "line-level commit leaves unselected replacement out of HEAD",
  );

  await app.getByTestId("diff-line-code-txt-deleted-12").click();
  await app.getByTestId("diff-line-code-txt-added-12").click();
  await app.getByTestId("diff-discard-selected-lines").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.selected_diff_selected_line_count === 0 &&
      snapshot.repo?.changes.length === 0 &&
      snapshot.status_message === "Discarded 2 selected lines from 'code.txt'." &&
      snapshot.error_message === "",
    { timeoutMs: 10_000 },
  );

  const finalText = await fs.readFile(path.join(repo, "code.txt"), "utf8");
  assert(
    finalText.startsWith('        println!("hello");\n'),
    "line-level commit keeps selected replacement in the working tree",
  );
  assert(
    finalText.includes("line 12\n") && !finalText.includes("line 12 changed\n"),
    "discard selected lines restores the remaining uncommitted replacement",
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
