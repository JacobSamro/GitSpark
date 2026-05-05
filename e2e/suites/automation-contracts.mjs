import { assert } from "../support/assertions.mjs";
import { flattenNodes, nodeById } from "../support/tree.mjs";

export async function testAutomationContracts(app) {
  await app.getByTestId("tab-changes").click();
  let snapshot = await app.snapshot();
  assertVisibleButtonsHaveStableIds(snapshot.test_tree);
  assert(
    nodeById(snapshot.test_tree, "tab-changes")?.selected === true,
    "selected changes tab is reflected in the test tree",
  );

  if (snapshot.repo?.changes.length) {
    const selected = snapshot.selected_change ?? snapshot.repo.changes[0].path;
    await app.command({ command: "select_change", path: selected });
    snapshot = await app.snapshot();
    const selectedNode = flattenNodes(snapshot.test_tree).find(
      (node) => node.id?.startsWith("change-") && node.text === selected,
    );
    assert(
      selectedNode?.selected === true,
      "selected change matches the visual selected change node",
    );
  }

  await app.getByTestId("tab-history").click();
  snapshot = await app.waitForSnapshot(
    (snapshot) => snapshot.sidebar_tab === "history" && snapshot.repo?.history.length,
    { timeoutMs: 10_000 },
  );
  const commit = snapshot.repo.history[0];
  await app.command({ command: "select_commit", oid: commit.oid });
  snapshot = await app.snapshot();
  assert(
    nodeById(snapshot.test_tree, `commit-${commit.short_oid}`)?.selected === true,
    "selected commit matches the visual selected commit node",
  );

  await app.getByTestId("button-branch-selector").click();
  await app.getByTestId("input-branch-filter").fill("contract-dialog");
  await app.getByTestId("button-branch-new").click();
  snapshot = await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "create_branch" &&
      nodeById(snapshot.test_tree, "new-branch-name")?.visible === true,
    { timeoutMs: 10_000 },
  );
  assert(
    snapshot.active_dialog === "create_branch",
    "active dialog snapshot matches the visible create-branch dialog node",
  );
  await app.getByTestId("dialog-cancel").click();

  const target = (await app.snapshot()).repo?.branches.find(
    (branch) => !branch.is_current && !branch.is_remote,
  )?.name;
  if (target) {
    await app.command({ command: "compare_branch", name: target });
    snapshot = await app.waitForSnapshot(
      (snapshot) => snapshot.compare?.target_branch === target,
      { timeoutMs: 10_000 },
    );
    assert(
      nodeById(snapshot.test_tree, "compare-merge-button")?.enabled ===
        (snapshot.compare.behind > 0),
      "compare merge CTA uses the same behind > 0 rule in UI and automation",
    );
    await app.getByTestId("compare-exit-button").click();
  }

  snapshot = await app.snapshot();
  const disabledControls = flattenNodes(snapshot.test_tree).filter(
    (node) => node.visible !== false && node.enabled === false,
  );
  assert(
    disabledControls.every((node) => node.id || node.test_id),
    "disabled visible controls have stable automation IDs",
  );
}

function assertVisibleButtonsHaveStableIds(tree) {
  const missing = flattenNodes(tree).filter(
    (node) =>
      node.role === "button" &&
      node.visible !== false &&
      (!node.id || !node.test_id),
  );
  assert(
    missing.length === 0,
    `every visible button has a stable ID; missing ${missing
      .map((node) => node.text)
      .join(", ")}`,
  );
}
