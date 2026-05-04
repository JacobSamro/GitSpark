import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

const exec = promisify(execFile);

export async function makeFreshSampleRepo() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-full-e2e-"));
  const remote = path.join(root, "origin.git");
  const workRepo = path.join(root, "sample-repo");
  const remoteClone = path.join(root, "remote-clone");
  const configDir = path.join(root, "config");
  const settingsPath = path.join(configDir, "settings.toml");
  const openUrlLog = path.join(root, "opened-urls.log");
  const openUrlScript = path.join(root, "open-url.sh");
  const githubBaseUrl = "https://github.com/gitspark/e2e-fixture";
  const githubRemote = `${githubBaseUrl}.git`;

  await fs.writeFile(
    openUrlScript,
    `#!/bin/sh\nprintf '%s\\n' "$1" >> ${shellQuote(openUrlLog)}\n`,
    { mode: 0o755 },
  );

  await exec("git", ["init", "--bare", remote]);
  await exec("git", ["init", "-b", "main", workRepo]);
  await gitConfig(workRepo);
  await exec("git", ["config", "core.editor", "/usr/bin/true"], {
    cwd: workRepo,
  });

  await fs.mkdir(path.join(workRepo, "src"));
  await fs.writeFile(path.join(workRepo, "src", "main.rs"), "fn main() {}\n");
  await fs.writeFile(
    path.join(workRepo, "README.md"),
    "# GitSpark E2E Sample\n",
  );
  await exec("git", ["add", "--all"], { cwd: workRepo });
  await exec("git", ["commit", "-m", "initial sample"], { cwd: workRepo });
  await exec("git", ["branch", "feature/update"], { cwd: workRepo });
  await exec("git", ["branch", "delete/me"], { cwd: workRepo });

  await exec("git", ["switch", "-c", "switch/conflict"], { cwd: workRepo });
  await fs.writeFile(
    path.join(workRepo, "README.md"),
    "# GitSpark E2E Sample\n\nconflict branch version\n",
  );
  await exec("git", ["add", "README.md"], { cwd: workRepo });
  await exec("git", ["commit", "-m", "branch: add conflicting readme"], {
    cwd: workRepo,
  });

  await exec("git", ["switch", "main"], { cwd: workRepo });
  await exec("git", ["switch", "-c", "cherry/source"], { cwd: workRepo });
  await fs.writeFile(path.join(workRepo, "cherry.txt"), "cherry-pick fixture\n");
  await exec("git", ["add", "cherry.txt"], { cwd: workRepo });
  await exec("git", ["commit", "-m", "feature: add cherry pick fixture"], {
    cwd: workRepo,
  });
  const cherryPickOid = await gitOutput(workRepo, ["rev-parse", "HEAD"]);

  await exec("git", ["switch", "main"], { cwd: workRepo });
  await exec("git", ["switch", "-c", "merge/source"], { cwd: workRepo });
  await fs.writeFile(path.join(workRepo, "merge.txt"), "merge fixture\n");
  await exec("git", ["add", "merge.txt"], { cwd: workRepo });
  await exec("git", ["commit", "-m", "feature: add merge fixture"], {
    cwd: workRepo,
  });

  await exec("git", ["switch", "main"], { cwd: workRepo });
  await exec("git", ["remote", "add", "origin", remote], { cwd: workRepo });
  await exec("git", ["push", "-u", "origin", "main"], { cwd: workRepo });
  await exec("git", ["push", "origin", "feature/update"], { cwd: workRepo });

  await fs.writeFile(
    path.join(workRepo, "src", "main.rs"),
    "fn main() {\n    println!(\"fresh sample repo\");\n}\n",
  );

  await exec("git", ["clone", remote, remoteClone]);
  await gitConfig(remoteClone);

  return {
    root: await fs.realpath(root),
    remote: await fs.realpath(remote),
    workRepo: await fs.realpath(workRepo),
    remoteClone: await fs.realpath(remoteClone),
    configDir,
    settingsPath,
    cherryPickOid,
    githubBaseUrl,
    githubRemote,
    openUrlCommand: shellQuote(openUrlScript),
    openUrlLog,
  };
}

export async function createRemoteOnlyCommit(remoteClone) {
  await exec("git", ["checkout", "main"], { cwd: remoteClone });
  await exec("git", ["fetch", "origin", "main"], { cwd: remoteClone });
  await exec("git", ["reset", "--hard", "origin/main"], { cwd: remoteClone });
  await fs.writeFile(
    path.join(remoteClone, "upstream.txt"),
    `upstream change ${Date.now()}\n`,
  );
  await exec("git", ["add", "--all"], { cwd: remoteClone });
  await exec("git", ["commit", "-m", "remote: add upstream change"], {
    cwd: remoteClone,
  });
  await exec("git", ["push", "origin", "main"], { cwd: remoteClone });
}

export async function gitOutput(repo, args) {
  const { stdout } = await exec("git", args, { cwd: repo });
  return stdout.trim();
}

function shellQuote(value) {
  return `'${value.replaceAll("'", "'\"'\"'")}'`;
}

async function gitConfig(repo) {
  await exec("git", ["config", "user.name", "GitSpark E2E"], { cwd: repo });
  await exec("git", ["config", "user.email", "e2e@gitspark.local"], {
    cwd: repo,
  });
}
