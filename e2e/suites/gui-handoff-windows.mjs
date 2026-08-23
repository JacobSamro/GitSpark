import { promises as fs } from "node:fs";

import { assert } from "../support/assertions.mjs";
import { markerPath, waitForProcessWithCommandLine } from "../support/gui-handoff-windows.mjs";

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
// overrides set, on a real windows-2022/windows-latest GitHub-hosted
// runner — which (confirmed via research before building this, not
// assumed) already runs a real interactive desktop session, so GUI
// processes launch for real with no extra setup the way they would on a
// self-hosted/service-mode Windows runner.
export async function testGuiHandoffWindows(app, fixture) {
  await clearMarker();
  await app.command({
    command: "change_action",
    path: fixture.notesFileName,
    action: "open_in_editor",
  });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === `Opened '${fixture.notesFileName}' in external editor.` &&
      snapshot.error_message === "",
    { timeoutMs: 15_000 },
  );
  const editorMarker = await waitForMarkerContains(fixture.notesPath);
  assert(
    editorMarker.includes(fixture.notesPath),
    "external editor handoff (real core.editor, real sh.exe, real powershell.exe) reaches the stub with the right path",
  );

  await clearMarker();
  await app.command({
    command: "change_action",
    path: fixture.notesFileName,
    action: "reveal_in_finder",
  });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === `Revealed '${fixture.notesFileName}' in Finder.` &&
      snapshot.error_message === "",
    { timeoutMs: 15_000 },
  );
  const revealMarker = await waitForMarkerContains(fixture.notesPath);
  assert(
    revealMarker.includes(fixture.notesPath),
    "reveal-in-explorer handoff (real ShellExecute, real registry file association) reaches the stub with the right path",
  );

  await clearMarker();
  await app.command({
    command: "change_action",
    path: fixture.notesFileName,
    action: "open_with_default",
  });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === `Opened '${fixture.notesFileName}' with the default program.` &&
      snapshot.error_message === "",
    { timeoutMs: 15_000 },
  );
  const openDefaultMarker = await waitForMarkerContains(fixture.notesPath);
  assert(
    openDefaultMarker.includes(fixture.notesPath),
    "open-with-default handoff (real ShellExecute, real registry file association) reaches the stub with the right path",
  );

  // A URL protocol handler is subject to Windows' "UserChoice" protections
  // on default-browser selection, which resist being silently overwritten
  // outside Settings — so this doesn't try to hijack http/https at all.
  // Instead it confirms a real browser process actually launches with the
  // right URL, which is what "does the OS handoff really complete" means
  // here.
  const targetUrl = `${fixture.githubBaseUrl}/blob/main/${fixture.notesFileName}`;
  await app.command({
    command: "change_action",
    path: fixture.notesFileName,
    action: "view_on_github",
  });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === `Opened '${fixture.notesFileName}' on GitHub.` &&
      snapshot.error_message === "",
    { timeoutMs: 15_000 },
  );
  const browserCommandLine = await waitForProcessWithCommandLine(targetUrl);
  assert(
    browserCommandLine.includes(targetUrl),
    `open-URL handoff reaches a real browser process with the right URL: ${browserCommandLine}`,
  );

  // "Open in Terminal" has no per-file equivalent to drive through
  // change_action — it's the menu-bar/keybinding-only action, exposed to
  // automation as its own command (src/ui/automation.rs OpenInTerminal).
  // This does NOT assert a terminal specifically opened — same finding as
  // the Linux suite: GitSpark's non-macOS fallback hands the repo
  // *directory* to the generic OS-open mechanism, which is a
  // file-manager/Explorer handoff, not a terminal one. Recorded via
  // process inspection rather than the registry stub, since overriding the
  // shared Directory shell handler would affect the whole CI job, not just
  // this test.
  const beforeTerminal = Date.now();
  await app.command({ command: "open_in_terminal" });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Opened repository in Terminal." && snapshot.error_message === "",
    { timeoutMs: 15_000 },
  );
  const terminalCommandLine = await waitForProcessWithCommandLine(fixture.repoPath);
  assert(
    terminalCommandLine.includes(fixture.repoPath),
    "\"Open in Terminal\" hands the repo path to *some* real OS process — " +
      `recorded command line: ${terminalCommandLine} (${Date.now() - beforeTerminal}ms)`,
  );
}
