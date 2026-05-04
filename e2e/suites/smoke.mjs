import { promises as fs } from "node:fs";
import path from "node:path";

import { expect } from "../gitspark.mjs";
import { assert } from "../support/assertions.mjs";

export async function testAutomationBasics(app) {
  const ping = await app.ping();
  assert(ping.pong === true, "ping returns pong");

  const tree = await app.testTree();
  assert(tree.test_id === "gitspark-root", "test tree exposes root node");
  await expect(app.getByTestId("gitspark-root")).toBeVisible();
}

export async function testShellControls(app) {
  await app.getByTestId("button-repo-selector").click();
  await expect(app.getByTestId("input-repo-filter")).toBeVisible();
  await app.getByTestId("input-repo-filter").fill("sample");
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.show_repo_selector === true &&
      snapshot.repo_filter_text === "sample",
  );

  await app.getByTestId("button-settings").click();
  await app.waitForSnapshot((snapshot) => snapshot.show_settings === true);
  await app.getByTestId("button-settings").click();
  await app.waitForSnapshot((snapshot) => snapshot.show_settings === false);
}

export async function testRepositoryFlows(app, fixture) {
  await app.openRepo(fixture.workRepo);
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.path === fixture.workRepo &&
      snapshot.status_message === "Repository loaded.",
    { timeoutMs: 15_000 },
  );

  await expect(app.getByTestId("change-src-main-rs")).toBeVisible({
    timeoutMs: 10_000,
  });
  await app.getByTestId("change-src-main-rs").click();
  await app.waitForSnapshot(
    (snapshot) => snapshot.selected_change === "src/main.rs",
  );

  await app.getByTestId("tab-history").click();
  await app.waitForSnapshot((snapshot) => snapshot.sidebar_tab === "history");
  await app.getByTestId("tab-changes").click();
  await app.waitForSnapshot((snapshot) => snapshot.sidebar_tab === "changes");

  await app.command({ command: "refresh_repo" });
  await app.waitForSnapshot(
    (snapshot) => snapshot.status_message === "Repository refreshed.",
    { timeoutMs: 10_000 },
  );
}

export async function testLocalChangeAutoSync(app, fixture) {
  await app.getByTestId("tab-changes").click();
  await new Promise((resolve) => setTimeout(resolve, 3_500));

  const watchedPath = path.join(fixture.workRepo, "watch-sync.txt");
  await fs.writeFile(watchedPath, "watch sync coverage\n");

  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.changes.some((change) => change.path === "watch-sync.txt"),
    { timeoutMs: 10_000 },
  );
  await expect(app.getByTestId("change-watch-sync-txt")).toBeVisible({
    timeoutMs: 10_000,
  });

  await app.getByTestId("change-watch-sync-txt-discard").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Discarded changes for 'watch-sync.txt'." &&
      !snapshot.repo?.changes.some((change) => change.path === "watch-sync.txt"),
    { timeoutMs: 10_000 },
  );
}
