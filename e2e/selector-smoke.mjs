import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { GitSparkAutomation, expect } from "./gitspark.mjs";

const exec = promisify(execFile);

const repo = await makeFixtureRepo();
const app = await GitSparkAutomation.launch();

try {
  await app.openRepo(repo);
  await expect(app.getByTestId("change-src-main-rs")).toBeVisible({
    timeoutMs: 10_000,
  });

  await app.getByTestId("change-src-main-rs").click();
  await app.getByTestId("input-commit-summary").fill("test: selector smoke");
  await app.getByTestId("button-commit-all").click();
  await expect(app.getByText("Commit created.")).toBeVisible({
    timeoutMs: 10_000,
  });
} finally {
  await app.close();
}

async function makeFixtureRepo() {
  const repo = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-e2e-"));
  await exec("git", ["init"], { cwd: repo });
  await exec("git", ["config", "user.name", "GitSpark E2E"], { cwd: repo });
  await exec("git", ["config", "user.email", "e2e@gitspark.local"], {
    cwd: repo,
  });

  await fs.mkdir(path.join(repo, "src"));
  await fs.writeFile(path.join(repo, "src", "main.rs"), "fn main() {}\n");
  await exec("git", ["add", "--all"], { cwd: repo });
  await exec("git", ["commit", "-m", "initial"], { cwd: repo });

  await fs.writeFile(
    path.join(repo, "src", "main.rs"),
    "fn main() {\n    println!(\"selector smoke\");\n}\n",
  );

  return repo;
}
