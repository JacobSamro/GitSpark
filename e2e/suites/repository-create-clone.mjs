import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { assert } from "../support/assertions.mjs";
import { nodeById } from "../support/tree.mjs";

const exec = promisify(execFile);

export async function testCreateCloneRepositoryWorkflows(app) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "gitspark-repo-e2e-"));
  const parentPath = path.join(root, "repos");
  await fs.mkdir(parentPath, { recursive: true });
  const parent = await fs.realpath(parentPath);

  await app.command({ command: "show_create_repository" });
  await app.getByTestId("create-repository-name-input").fill(".");
  await app.getByTestId("create-repository-path-input").fill(parent);
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "create_repository" &&
      nodeById(snapshot.test_tree, "create-repository-confirm")?.enabled === false &&
      nodeById(snapshot.test_tree, "create-repository-validation-message")?.text ===
        ". is not a valid repository name.",
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("create-repository-cancel").click();

  await app.command({ command: "show_create_repository" });
  await app.getByTestId("create-repository-name-input").fill("No Readme");
  await app.getByTestId("create-repository-path-input").fill(parent);
  await app.getByTestId("create-repository-readme-checkbox").click();
  await app.getByTestId("create-repository-initial-commit-checkbox").click();
  await app.getByTestId("create-repository-confirm").click();
  const noReadmePath = path.join(parent, "No Readme");
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.path === noReadmePath &&
      snapshot.status_message === "Created repository 'No Readme'.",
    { timeoutMs: 15_000 },
  );
  assert(
    !(await exists(path.join(noReadmePath, "README.md"))),
    "create repo with README off does not write README.md",
  );
  assert(
    (await gitOptionalOutput(noReadmePath, ["rev-parse", "--verify", "HEAD"])) ===
      null,
    "create repo with initial commit off leaves unborn HEAD",
  );

  await createRepositoryWithTemplate(app, parent, {
    name: "Rust MIT",
    gitignoreId: "create-repository-gitignore-rust",
    licenseId: "create-repository-license-mit",
    gitignoreNeedle: "/target/",
    licenseNeedle: "MIT License",
  });
  await createRepositoryWithTemplate(app, parent, {
    name: "Node Apache",
    gitignoreId: "create-repository-gitignore-node",
    licenseId: "create-repository-license-apache-2-0",
    gitignoreNeedle: "node_modules/",
    licenseNeedle: "Apache License",
  });
  await createRepositoryWithTemplate(app, parent, {
    name: "Python GPL",
    gitignoreId: "create-repository-gitignore-python",
    licenseId: "create-repository-license-gpl-3-0",
    gitignoreNeedle: "__pycache__/",
    licenseNeedle: "GNU General Public License",
  });

  const source = await makeCloneSource(root);
  const cloneParent = path.join(root, "clone-parent");
  await fs.mkdir(cloneParent, { recursive: true });
  await fs.writeFile(path.join(cloneParent, "unrelated.txt"), "keep me\n");

  await app.command({ command: "show_clone_repository" });
  await app.getByTestId("clone-repository-url-input").fill(source);
  await app.getByTestId("clone-repository-path-input").fill(cloneParent);
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "clone_repository" &&
      nodeById(snapshot.test_tree, "clone-repository-name-input")?.text ===
        "source-repo" &&
      nodeById(snapshot.test_tree, "clone-repository-confirm")?.enabled === true,
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("clone-repository-confirm").click();
  const inferredClone = path.join(cloneParent, "source-repo");
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.path === inferredClone &&
      snapshot.status_message === `Cloned repository from '${source}'.`,
    { timeoutMs: 20_000 },
  );
  assert(
    (await fs.readFile(path.join(cloneParent, "unrelated.txt"), "utf8")) ===
      "keep me\n",
    "clone into parent folder preserves existing unrelated files",
  );

  await app.command({ command: "show_clone_repository" });
  await app.getByTestId("clone-repository-url-input").fill(source);
  await app.getByTestId("clone-repository-path-input").fill(cloneParent);
  await app.getByTestId("clone-repository-name-input").fill("source-repo");
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.active_dialog === "clone_repository" &&
      nodeById(snapshot.test_tree, "clone-repository-confirm")?.enabled === false &&
      nodeById(snapshot.test_tree, "clone-repository-validation-message")?.text.endsWith(
        "source-repo already exists and is not empty.",
      ),
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("clone-repository-cancel").click();

  const selectedBeforeFailure = (await app.snapshot()).repo?.path;
  await app.command({ command: "show_clone_repository" });
  await app.getByTestId("clone-repository-url-input").fill(path.join(root, "missing.git"));
  await app.getByTestId("clone-repository-path-input").fill(cloneParent);
  await app.getByTestId("clone-repository-name-input").fill("failed-clone");
  await app.getByTestId("clone-repository-confirm").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.path === selectedBeforeFailure &&
      snapshot.error_message.startsWith("Clone repository failed:") &&
      !(snapshot.repo?.path || "").endsWith("failed-clone"),
    { timeoutMs: 20_000 },
  );
}

async function createRepositoryWithTemplate(
  app,
  parent,
  { name, gitignoreId, licenseId, gitignoreNeedle, licenseNeedle },
) {
  await app.command({ command: "show_create_repository" });
  await app.getByTestId("create-repository-name-input").fill(name);
  await app.getByTestId("create-repository-path-input").fill(parent);
  await ensureSelected(app, "create-repository-readme-checkbox", true);
  await ensureSelected(app, "create-repository-initial-commit-checkbox", true);
  await app.getByTestId(gitignoreId).click();
  await app.getByTestId(licenseId).click();
  await app.getByTestId("create-repository-confirm").click();
  const repoPath = path.join(parent, name);
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.repo?.path === repoPath &&
      snapshot.status_message === `Created repository '${name}'.`,
    { timeoutMs: 15_000 },
  );
  assert(
    (await fs.readFile(path.join(repoPath, ".gitignore"), "utf8")).includes(
      gitignoreNeedle,
    ),
    `${name} writes selected gitignore template`,
  );
  assert(
    (await fs.readFile(path.join(repoPath, "LICENSE"), "utf8")).includes(
      licenseNeedle,
    ),
    `${name} writes selected license template`,
  );
  assert(
    (await gitOptionalOutput(repoPath, ["rev-parse", "--verify", "HEAD"])) !==
      null,
    `${name} creates the initial commit`,
  );
}

async function makeCloneSource(root) {
  const source = path.join(root, "source-repo");
  await exec("git", ["init", "-b", "main", source]);
  await exec("git", ["config", "user.name", "GitSpark E2E"], { cwd: source });
  await exec("git", ["config", "user.email", "e2e@gitspark.local"], {
    cwd: source,
  });
  await fs.writeFile(path.join(source, "README.md"), "clone source\n");
  await exec("git", ["add", "--all"], { cwd: source });
  await exec("git", ["commit", "-m", "initial"], { cwd: source });
  return await fs.realpath(source);
}

async function ensureSelected(app, id, selected) {
  const snapshot = await app.snapshot();
  if (nodeById(snapshot.test_tree, id)?.selected !== selected) {
    await app.getByTestId(id).click();
  }
}

async function gitOptionalOutput(repo, args) {
  try {
    const { stdout } = await exec("git", args, { cwd: repo });
    return stdout.trim();
  } catch {
    return null;
  }
}

async function exists(filePath) {
  try {
    await fs.access(filePath);
    return true;
  } catch {
    return false;
  }
}
