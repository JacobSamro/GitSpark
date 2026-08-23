import { promises as fs } from "node:fs";

import { assert } from "../support/assertions.mjs";
import { markerPath } from "../support/gui-handoff-linux.mjs";

async function clearMarker() {
  await fs.rm(markerPath(), { force: true });
}

async function waitForMarkerContains(needle, { timeoutMs = 15_000, intervalMs = 200 } = {}) {
  const deadline = Date.now() + timeoutMs;
  let lastContent = "";
  while (Date.now() <= deadline) {
    lastContent = await fs.readFile(markerPath(), "utf8").catch(() => "");
    if (lastContent.includes(needle)) {
      return lastContent;
    }
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
  throw new Error(
    `Timed out waiting for the stub app's marker file to record "${needle}". ` +
      `Last content:\n${lastContent || "(empty)"}`,
  );
}

// Drives every OS handoff GitSpark has, with none of the GITSPARK_*_COMMAND
// overrides set, against the real Linux desktop the Docker image
// (docker/e2e-linux-desktop) sets up: a real xdg-open, a real MIME
// database, and a window manager for it to resolve against. The Dockerfile
// registers the stub app as the default handler for the fixture's test
// extension, http(s) URLs, and plain directories.
export async function testGuiHandoffLinux(app, fixture) {
  await clearMarker();
  await app.command({
    command: "change_action",
    path: "notes.gitsparktest",
    action: "open_in_editor",
  });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Opened 'notes.gitsparktest' in external editor." &&
      snapshot.error_message === "",
    { timeoutMs: 15_000 },
  );
  const editorMarker = await waitForMarkerContains(fixture.notesPath);
  assert(
    editorMarker.includes(fixture.notesPath),
    "external editor handoff (real core.editor, real shell) reaches the stub with the right path",
  );

  await clearMarker();
  await app.command({
    command: "change_action",
    path: "notes.gitsparktest",
    action: "reveal_in_finder",
  });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Revealed 'notes.gitsparktest' in Finder." &&
      snapshot.error_message === "",
    { timeoutMs: 15_000 },
  );
  const revealMarker = await waitForMarkerContains(fixture.notesPath);
  assert(
    revealMarker.includes(fixture.notesPath),
    "reveal-in-finder handoff (real xdg-open, real MIME lookup) reaches the stub with the right path",
  );

  await clearMarker();
  await app.command({
    command: "change_action",
    path: "notes.gitsparktest",
    action: "open_with_default",
  });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Opened 'notes.gitsparktest' with the default program." &&
      snapshot.error_message === "",
    { timeoutMs: 15_000 },
  );
  const openDefaultMarker = await waitForMarkerContains(fixture.notesPath);
  assert(
    openDefaultMarker.includes(fixture.notesPath),
    "open-with-default handoff (real xdg-open, real MIME lookup) reaches the stub with the right path",
  );

  await clearMarker();
  await app.command({
    command: "change_action",
    path: "notes.gitsparktest",
    action: "view_on_github",
  });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Opened 'notes.gitsparktest' on GitHub." &&
      snapshot.error_message === "",
    { timeoutMs: 15_000 },
  );
  const urlMarker = await waitForMarkerContains(
    `${fixture.githubBaseUrl}/blob/main/notes.gitsparktest`,
  );
  assert(
    urlMarker.includes(fixture.githubBaseUrl),
    "open-URL handoff (real xdg-open, real x-scheme-handler resolution) reaches the stub with the right URL",
  );

  // "Open in Terminal" has no per-file equivalent to drive through
  // change_action — it's the menu-bar/keybinding-only action, exposed to
  // automation as its own command (src/ui/automation.rs OpenInTerminal).
  // Unlike the cases above, this does NOT assert a terminal specifically
  // opened — see the plan's "Open in Terminal" finding: GitSpark's
  // non-macOS fallback hands a *directory* to the same OS-open mechanism
  // as the file cases, which is a file-manager handoff, not a terminal
  // one. This records what actually happens rather than asserting a
  // behavior GitSpark doesn't implement yet.
  await clearMarker();
  await app.command({ command: "open_in_terminal" });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Opened repository in Terminal." &&
      snapshot.error_message === "",
    { timeoutMs: 15_000 },
  );
  const terminalMarker = await waitForMarkerContains(fixture.repoPath);
  assert(
    terminalMarker.includes(fixture.repoPath),
    "\"Open in Terminal\" hands the repo path to *some* real OS handoff — " +
      `recorded invocation: ${terminalMarker.trim()}`,
  );
}
