// Standalone e2e entry point that drives GitSpark's OS handoffs (external
// editor, reveal in Explorer, open with default program, open URL, open in
// terminal) against a real windows-2022/windows-latest GitHub-hosted
// runner's interactive desktop session — instead of the env-var stubs the
// rest of the e2e suite uses.
//
// Deliberately not part of full-suite.mjs: that suite must keep working
// with no OS-handoff/registry dependency for anyone running it locally on
// any platform.

import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { GitSparkAutomation } from "./gitspark.mjs";
import { testGuiHandoffWindows } from "./suites/gui-handoff-windows.mjs";
import {
  setupStub,
  stubEditorCommand,
  teardownStub,
  testFileName,
} from "./support/gui-handoff-windows.mjs";

const exec = promisify(execFile);

const root = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-gui-handoff-"));
const repo = path.join(root, "repo");
const configDir = path.join(root, "config");
const globalGitConfig = path.join(root, "global.gitconfig");
const githubBaseUrl = "https://github.com/gitspark/e2e-fixture";
const notesFileName = testFileName();

await fs.mkdir(repo, { recursive: true });
await exec("git", ["init", "-q", "-b", "main", repo]);
await exec("git", ["config", "user.name", "GitSpark E2E"], { cwd: repo });
await exec("git", ["config", "user.email", "e2e@gitspark.local"], { cwd: repo });
// No GITSPARK_EDITOR_COMMAND is set for this suite — core.editor pointing
// straight at the stub is what exercises the real fallback chain
// (src/ui/app/operations.rs's resolve_editor_command) end to end.
await exec("git", ["config", "core.editor", stubEditorCommand()], { cwd: repo });
await exec("git", ["remote", "add", "origin", `${githubBaseUrl}.git`], { cwd: repo });

const notesPath = path.join(repo, notesFileName);
await fs.writeFile(notesPath, "gui handoff fixture\n");
await fs.writeFile(path.join(repo, "README.md"), "# GitSpark GUI handoff fixture\n");
await exec("git", ["add", "README.md"], { cwd: repo });
await exec("git", ["commit", "-q", "-m", "initial commit"], { cwd: repo });
// notes.gitsparktest stays untracked/uncommitted — an ordinary Changes-list
// entry, same as every other e2e fixture file used for file actions.

await setupStub();

const app = await GitSparkAutomation.launch({
  env: {
    GITSPARK_CONFIG_DIR: configDir,
    GIT_CONFIG_GLOBAL: globalGitConfig,
  },
});

try {
  await app.openRepo(repo);
  await app.waitForSnapshot((snapshot) => snapshot.repo?.path === repo, {
    timeoutMs: 10_000,
  });

  await testGuiHandoffWindows(app, {
    repoPath: repo,
    notesPath,
    notesFileName,
    githubBaseUrl,
  });

  console.log("GUI handoff (Windows) suite passed.");
} finally {
  await app.close();
  await teardownStub();
  await fs.rm(root, { recursive: true, force: true }).catch(() => {});
}
