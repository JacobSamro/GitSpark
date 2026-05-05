import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { assert } from "../support/assertions.mjs";
import { gitOptionalOutput, gitOutputWithEnv } from "../support/fixtures.mjs";
import { nodeById } from "../support/tree.mjs";

const exec = promisify(execFile);

export async function testSettingsScopeRegressions(app, fixture) {
  const repo = await makeSettingsRepo();
  await app.openRepo(repo);
  await app.waitForSnapshot(
    (snapshot) => snapshot.repo?.path === repo,
    { timeoutMs: 15_000 },
  );

  await app.command({ command: "save_settings", section: "remote" });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.error_message === "This repository does not have a remote." &&
      snapshot.repo?.path === repo,
    { timeoutMs: 10_000 },
  );

  await exec("git", ["remote", "add", "origin", "https://github.com/gitspark/settings.git"], {
    cwd: repo,
  });
  await app.command({ command: "refresh_repo" });
  await app.command({ command: "show_repository_settings", show: true });
  await app.getByTestId("settings-tab-remote").click();
  await app.getByTestId("settings-remote-url").fill("");
  await app.getByTestId("settings-save-remote").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.show_settings === true &&
      snapshot.error_message === "Remote URL cannot be empty.",
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("settings-remote-url").fill("https://github.com/gitspark/settings.git");
  await app.getByTestId("settings-save-remote").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.show_settings === false &&
      snapshot.status_message === "Remote settings saved.",
    { timeoutMs: 10_000 },
  );

  await app.command({ command: "show_repository_settings", show: true });
  await app.getByTestId("settings-tab-ignored-files").click();
  await app.getByTestId("settings-ignored-files-text").fill("");
  await app.getByTestId("settings-save-ignored-files").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.show_settings === false &&
      snapshot.status_message === "Ignored files saved.",
    { timeoutMs: 10_000 },
  );
  assert(
    !(await exists(path.join(repo, ".gitignore"))),
    "saving empty ignored files removes root .gitignore",
  );

  await app.command({ command: "show_repository_settings", show: true });
  await app.getByTestId("settings-tab-git").click();
  await app.getByTestId("settings-git-scope-local").click();
  await app.getByTestId("settings-git-user-name").fill("Local Only");
  await app.getByTestId("settings-git-user-email").fill("");
  await app.getByTestId("settings-save-git").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.show_settings === false &&
      snapshot.status_message === "Git config saved." &&
      snapshot.git_user_name === "Local Only" &&
      snapshot.git_user_email === "",
    { timeoutMs: 10_000 },
  );
  assert(
    (await gitOptionalOutput(repo, ["config", "--local", "--get", "user.email"])) ===
      null,
    "incomplete local identity leaves local user.email unset",
  );

  await app.command({ command: "show_repository_settings", show: true });
  await app.getByTestId("settings-tab-git").click();
  await app.getByTestId("settings-pull-rebase").click();
  await app.getByTestId("settings-save-git").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.show_settings === false &&
      snapshot.status_message === "Git config saved." &&
      snapshot.git_pull_rebase === true,
    { timeoutMs: 10_000 },
  );
  assert(
    (await exec("git", ["config", "--get", "pull.rebase"], { cwd: repo })).stdout.trim() ===
      "true",
    "pull.rebase true persists to repository config",
  );

  await app.command({ command: "show_repository_settings", show: true });
  await app.getByTestId("settings-tab-git").click();
  await app.getByTestId("settings-pull-rebase").click();
  await app.getByTestId("settings-save-git").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.show_settings === false &&
      snapshot.status_message === "Git config saved." &&
      snapshot.git_pull_rebase === false,
    { timeoutMs: 10_000 },
  );
  assert(
    (await exec("git", ["config", "--get", "pull.rebase"], { cwd: repo })).stdout.trim() ===
      "false",
    "pull.rebase false persists to repository config",
  );

  await exec("git", ["config", "--unset", "pull.rebase"], { cwd: repo });
  await app.command({ command: "refresh_repo" });
  await app.waitForSnapshot(
    (snapshot) => snapshot.git_pull_rebase === null,
    { timeoutMs: 10_000 },
  );

  await app.command({ command: "show_global_settings", show: true });
  await app.getByTestId("settings-tab-git").click();
  await app.getByTestId("settings-git-user-name").fill("Global Fallback");
  await app.getByTestId("settings-git-user-email").fill("global-fallback@gitspark.local");
  await app.getByTestId("settings-save-git").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.show_settings === false &&
      snapshot.status_message === "Git config saved.",
    { timeoutMs: 10_000 },
  );

  await app.command({ command: "show_repository_settings", show: true });
  await app.getByTestId("settings-tab-git").click();
  await app.getByTestId("settings-git-scope-global").click();
  await app.getByTestId("settings-save-git").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.show_settings === false &&
      snapshot.status_message === "Git config saved." &&
      snapshot.git_user_name === "Global Fallback" &&
      snapshot.git_user_email === "global-fallback@gitspark.local",
    { timeoutMs: 10_000 },
  );
  assert(
    (await gitOptionalOutput(repo, ["config", "--local", "--get", "user.name"])) === null,
    "clearing local identity removes local user.name",
  );
  assert(
    (await gitOutputWithEnv(
      repo,
      ["config", "--global", "--get", "user.email"],
      { GIT_CONFIG_GLOBAL: fixture.globalGitConfig },
    )) === "global-fallback@gitspark.local",
    "global identity is stored in isolated global config",
  );
}

async function makeSettingsRepo() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-settings-e2e-"));
  const repo = path.join(root, "repo");
  await exec("git", ["init", "-b", "main", repo]);
  await fs.writeFile(path.join(repo, ".gitignore"), "target/\n");
  await fs.writeFile(path.join(repo, "README.md"), "settings\n");
  await exec("git", ["add", "--all"], { cwd: repo });
  await exec("git", ["-c", "user.name=GitSpark E2E", "-c", "user.email=e2e@gitspark.local", "commit", "-m", "initial"], {
    cwd: repo,
  });
  return await fs.realpath(repo);
}

async function exists(filePath) {
  try {
    await fs.access(filePath);
    return true;
  } catch {
    return false;
  }
}
