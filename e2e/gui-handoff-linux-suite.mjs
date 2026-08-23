// Standalone e2e entry point that drives GitSpark's OS handoffs (external
// editor, reveal in file manager, open with default program, open URL,
// open in terminal) against a REAL Linux desktop — a real xdg-open, a real
// MIME database, and a window manager — instead of the env-var stubs the
// rest of the e2e suite uses. Meant to run inside the
// docker/e2e-linux-desktop image, which registers the stub app as the
// default handler at build time; see that Dockerfile for what's set up
// before this ever runs.
//
// Deliberately not part of full-suite.mjs: that suite must keep working
// with no GUI/desktop dependency for anyone without this Docker image.

import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { GitSparkAutomation } from "./gitspark.mjs";
import { testGuiHandoffLinux } from "./suites/gui-handoff-linux.mjs";

const exec = promisify(execFile);

const root = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-gui-handoff-"));
const repo = path.join(root, "repo");
const configDir = path.join(root, "config");
const globalGitConfig = path.join(root, "global.gitconfig");
const githubBaseUrl = "https://github.com/gitspark/e2e-fixture";

await exec("git", ["init", "-q", "-b", "main", repo]);
await exec("git", ["config", "user.name", "GitSpark E2E"], { cwd: repo });
await exec("git", ["config", "user.email", "e2e@gitspark.local"], { cwd: repo });
// No GITSPARK_EDITOR_COMMAND is set for this suite — core.editor pointing
// straight at the stub is what exercises the real shell-command fallback
// chain (src/ui/app/operations.rs's resolve_editor_command).
await exec("git", ["config", "core.editor", "/usr/local/bin/gitspark-stub.sh"], {
  cwd: repo,
});
await exec("git", ["remote", "add", "origin", `${githubBaseUrl}.git`], { cwd: repo });

const notesPath = path.join(repo, "notes.gitsparktest");
await fs.writeFile(notesPath, "gui handoff fixture\n");
await fs.writeFile(path.join(repo, "README.md"), "# GitSpark GUI handoff fixture\n");
await exec("git", ["add", "README.md"], { cwd: repo });
await exec("git", ["commit", "-q", "-m", "initial commit"], { cwd: repo });
// notes.gitsparktest stays untracked/uncommitted — an ordinary Changes-list
// entry, same as every other e2e fixture file used for file actions.

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

  await testGuiHandoffLinux(app, {
    repoPath: repo,
    notesPath,
    githubBaseUrl,
  });

  console.log("GUI handoff (Linux) suite passed.");
} finally {
  await app.close();
  await fs.rm(root, { recursive: true, force: true });
}
