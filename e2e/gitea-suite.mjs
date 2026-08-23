// Standalone e2e entry point that drives GitSpark's push/pull/fetch/clone
// against a REAL Gitea server over HTTP, instead of the local bare repos the
// rest of the e2e suite uses. Requires a running Gitea instance — bring one
// up with `docker compose -f docker-compose.gitea.yml up -d` (or point
// GITSPARK_GITEA_URL at any other Gitea instance) before running this.
//
// Deliberately not part of full-suite.mjs: that suite must keep working
// with zero external dependencies for anyone without Docker running.

import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";

import { GitSparkAutomation } from "./gitspark.mjs";
import {
  testGiteaCloneWorkflow,
  testGiteaNetworkFlows,
} from "./suites/gitea-network.mjs";
import { provisionGiteaRepo, waitForGiteaReady } from "./support/gitea.mjs";

const root = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-gitea-suite-"));
const configDir = path.join(root, "config");
const globalGitConfig = path.join(root, "global.gitconfig");

await waitForGiteaReady();

const app = await GitSparkAutomation.launch({
  env: {
    GITSPARK_CONFIG_DIR: configDir,
    GIT_CONFIG_GLOBAL: globalGitConfig,
    GITSPARK_EDITOR_COMMAND: "/usr/bin/true",
    GITSPARK_OPEN_COMMAND: "/usr/bin/true",
    GITSPARK_OPEN_URL_COMMAND: "/usr/bin/true",
    GITSPARK_REVEAL_COMMAND: "/usr/bin/true",
  },
});

try {
  const networkFlowsRepo = await provisionGiteaRepo({
    repoName: "gitspark-e2e-network",
  });
  await testGiteaNetworkFlows(app, networkFlowsRepo);

  const cloneRepo = await provisionGiteaRepo({
    repoName: "gitspark-e2e-clone-source",
  });
  // The clone workflow needs at least one commit to clone — publish one
  // through plain git rather than through GitSpark, since what's under test
  // here is the clone path, not another push.
  await seedRemoteWithOneCommit(cloneRepo.remoteUrl);
  await testGiteaCloneWorkflow(app, cloneRepo);

  console.log("Gitea e2e suite passed.");
} finally {
  await app.close();
  await fs.rm(root, { recursive: true, force: true });
}

async function seedRemoteWithOneCommit(remoteUrl) {
  const { execFile } = await import("node:child_process");
  const { promisify } = await import("node:util");
  const exec = promisify(execFile);

  const seedDir = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-gitea-seed-"));
  await exec("git", ["init", "-q", "-b", "main", seedDir]);
  await exec("git", ["config", "user.name", "GitSpark E2E"], { cwd: seedDir });
  await exec("git", ["config", "user.email", "e2e@gitspark.local"], {
    cwd: seedDir,
  });
  await fs.writeFile(path.join(seedDir, "README.md"), "# Clone source\n");
  await exec("git", ["add", "--all"], { cwd: seedDir });
  await exec("git", ["commit", "-q", "-m", "initial commit"], { cwd: seedDir });
  await exec("git", ["remote", "add", "origin", remoteUrl], { cwd: seedDir });
  await exec("git", ["push", "-q", "-u", "origin", "main"], { cwd: seedDir });
  await fs.rm(seedDir, { recursive: true, force: true });
}
