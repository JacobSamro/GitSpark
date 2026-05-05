import { spawn } from "node:child_process";
import { randomUUID } from "node:crypto";
import { promises as fs } from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";

const DEFAULT_ADDR = "127.0.0.1:7878";
const DEFAULT_LAUNCH_TIMEOUT_MS = Number(
  process.env.GITSPARK_E2E_LAUNCH_TIMEOUT_MS || 300_000,
);

export class GitSparkAutomation {
  constructor({ addr = DEFAULT_ADDR, child = null } = {}) {
    this.addr = addr;
    this.child = child;
  }

  static connect(addr = process.env.GITSPARK_AUTOMATION_ADDR || DEFAULT_ADDR) {
    return new GitSparkAutomation({ addr });
  }

  static async launch({
    cwd = process.cwd(),
    command = "cargo",
    args = ["run", "--locked"],
    env = {},
    timeoutMs = DEFAULT_LAUNCH_TIMEOUT_MS,
  } = {}) {
    const readyFile = path.join(
      os.tmpdir(),
      `gitspark-automation-${randomUUID()}.addr`,
    );
    const child = spawn(command, args, {
      cwd,
      env: {
        ...process.env,
        ...env,
        GITSPARK_AUTOMATION_ADDR: "127.0.0.1:0",
        GITSPARK_AUTOMATION_READY_FILE: readyFile,
      },
      stdio: "inherit",
    });

    try {
      const addr = await waitForReadyFile(readyFile, timeoutMs);
      return new GitSparkAutomation({ addr, child });
    } catch (error) {
      child.kill();
      throw error;
    }
  }

  async close() {
    if (!this.child || this.child.killed) {
      return;
    }

    this.child.kill();
    await new Promise((resolve) => {
      this.child.once("exit", resolve);
      setTimeout(resolve, 2_000);
    });
  }

  async command(command) {
    const response = await sendJsonLine(this.addr, command);
    if (!response.ok) {
      let debug = "";
      try {
        debug = `\n${formatSnapshotDebug(await this.snapshot())}`;
      } catch {
        debug = "";
      }
      throw new Error(
        `${response.error || "GitSpark automation command failed"}${debug}`,
      );
    }
    return response.result;
  }

  async openRepo(repoPath) {
    return this.command({ command: "open_repo", path: repoPath });
  }

  async ping() {
    return this.command({ command: "ping" });
  }

  async snapshot() {
    return this.command({ command: "snapshot" });
  }

  async testTree() {
    return this.command({ command: "test_tree" });
  }

  async clipboardText() {
    return this.command({ command: "clipboard_text" });
  }

  getByTestId(testId) {
    return new Locator(this, { by: "test_id", value: testId });
  }

  getByText(text) {
    return new Locator(this, { by: "text", value: text });
  }

  async waitForSnapshot(predicate, { timeoutMs = 10_000, intervalMs = 100 } = {}) {
    const deadline = Date.now() + timeoutMs;
    let lastSnapshot = null;

    while (Date.now() <= deadline) {
      lastSnapshot = await this.snapshot();
      if (await predicate(lastSnapshot)) {
        return lastSnapshot;
      }
      await delay(intervalMs);
    }

    throw new Error(
      `Timed out waiting for snapshot condition\n${formatSnapshotDebug(lastSnapshot)}`,
    );
  }
}

function formatSnapshotDebug(snapshot) {
  if (!snapshot) {
    return "No snapshot was available.";
  }

  const visibleEnabled = visibleEnabledNodes(snapshot.test_tree)
    .slice(0, 80)
    .map((node) => ({
      id: node.id,
      test_id: node.test_id,
      role: node.role,
      text: node.text,
    }));

  return JSON.stringify(
    {
      active_dialog: snapshot.active_dialog,
      sidebar_tab: snapshot.sidebar_tab,
      show_settings: snapshot.show_settings,
      show_repo_selector: snapshot.show_repo_selector,
      show_branch_selector: snapshot.show_branch_selector,
      show_network_dropdown: snapshot.show_network_dropdown,
      status_message: snapshot.status_message,
      error_message: snapshot.error_message,
      repo: snapshot.repo
        ? {
            name: snapshot.repo.name,
            current_branch: snapshot.repo.current_branch,
            ahead: snapshot.repo.ahead,
            behind: snapshot.repo.behind,
            has_github_remote: snapshot.repo.has_github_remote,
            change_count: snapshot.repo.changes.length,
            branch_count: snapshot.repo.branches.length,
          }
        : null,
      compare: snapshot.compare,
      visible_enabled_nodes: visibleEnabled,
      test_tree: compactNode(snapshot.test_tree),
    },
    null,
    2,
  );
}

function visibleEnabledNodes(node) {
  if (!node) {
    return [];
  }
  const own = node.visible !== false && node.enabled !== false ? [node] : [];
  return own.concat((node.children || []).flatMap(visibleEnabledNodes));
}

function compactNode(node, depth = 0) {
  if (!node || depth > 2) {
    return null;
  }
  const children = (node.children || [])
    .slice(0, 40)
    .map((child) => compactNode(child, depth + 1))
    .filter(Boolean);

  return {
    id: node.id,
    test_id: node.test_id,
    role: node.role,
    text: node.text,
    visible: node.visible,
    enabled: node.enabled,
    selected: node.selected,
    ...(children.length ? { children } : {}),
  };
}

export class Locator {
  constructor(app, selector) {
    this.app = app;
    this.selector = selector;
  }

  async all() {
    return this.app.command({ command: "query", selector: this.selector });
  }

  async click() {
    return this.app.command({ command: "click", selector: this.selector });
  }

  async fill(text) {
    return this.app.command({
      command: "fill",
      selector: this.selector,
      text,
    });
  }

  async typeText(text) {
    return this.app.command({
      command: "type_text",
      selector: this.selector,
      text,
    });
  }

  async press(...keys) {
    return this.app.command({
      command: "press_keys",
      selector: this.selector,
      keys,
    });
  }

  async isVisible() {
    const nodes = await this.all();
    return nodes.some((node) => node.visible);
  }

  async waitFor({ state = "visible", timeoutMs = 5_000 } = {}) {
    const deadline = Date.now() + timeoutMs;
    let lastNodes = [];

    while (Date.now() <= deadline) {
      lastNodes = await this.all();
      const visible = lastNodes.some((node) => node.visible);

      if (state === "visible" && visible) {
        return;
      }
      if (state === "hidden" && !visible) {
        return;
      }

      await delay(100);
    }

    throw new Error(
      `Timed out waiting for ${JSON.stringify(this.selector)} to be ${state}; last match count: ${lastNodes.length}`,
    );
  }
}

export function expect(locator) {
  return {
    async toBeVisible(options) {
      await locator.waitFor({ ...options, state: "visible" });
    },
    async toBeHidden(options) {
      await locator.waitFor({ ...options, state: "hidden" });
    },
  };
}

async function sendJsonLine(addr, command) {
  const [host, portText] = addr.split(":");
  const port = Number(portText);

  return new Promise((resolve, reject) => {
    const socket = net.createConnection({ host, port });
    let buffer = "";

    socket.setTimeout(5_000);
    socket.on("connect", () => {
      socket.write(`${JSON.stringify(command)}\n`);
    });
    socket.on("data", (chunk) => {
      buffer += chunk.toString("utf8");
      const newline = buffer.indexOf("\n");
      if (newline === -1) {
        return;
      }

      socket.end();
      try {
        resolve(JSON.parse(buffer.slice(0, newline)));
      } catch (error) {
        reject(error);
      }
    });
    socket.on("timeout", () => {
      socket.destroy(new Error("Timed out waiting for GitSpark automation"));
    });
    socket.on("error", reject);
  });
}

async function waitForReadyFile(readyFile, timeoutMs) {
  const deadline = Date.now() + timeoutMs;

  while (Date.now() <= deadline) {
    try {
      const addr = (await fs.readFile(readyFile, "utf8")).trim();
      if (addr) {
        return addr;
      }
    } catch (error) {
      if (error.code !== "ENOENT") {
        throw error;
      }
    }

    await delay(100);
  }

  throw new Error(`Timed out waiting for GitSpark automation ready file`);
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
