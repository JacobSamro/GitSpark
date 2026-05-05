import { promises as fs } from "node:fs";

import { expect } from "../gitspark.mjs";
import { assert } from "../support/assertions.mjs";
import { gitOptionalOutput, gitOutput } from "../support/fixtures.mjs";

export async function testAiValidation(app) {
  await app.command({ command: "generate_ai_commit" });
  await expect(
    app.getByText(
      "AI generation failed: AI API key is missing. Add one in settings before generating commit messages.",
    ),
  ).toBeVisible({ timeoutMs: 10_000 });
}

export async function testSettingsPersistence(app, fixture) {
  await app.getByTestId("button-settings").click();
  await app.getByTestId("settings-tab-git").click();
  await app.getByTestId("settings-git-user-name").fill(".");
  await app.getByTestId("settings-git-user-email").fill("precise@gitspark.local");
  await app.getByTestId("settings-save-git").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.error_message ===
      "Name is invalid, it consists only of disallowed characters.",
    { timeoutMs: 10_000 },
  );

  await app.getByTestId("settings-git-user-name").fill("GitSpark Precise");
  await app
    .getByTestId("settings-git-user-email")
    .fill("precise@gitspark.local");
  await app.getByTestId("settings-git-default-branch").fill("main");
  await app.getByTestId("settings-save-git").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Git config saved." &&
      snapshot.error_message === "",
    { timeoutMs: 10_000 },
  );

  assert(
    (await gitOutput(fixture.workRepo, ["config", "user.name"])) ===
      "GitSpark Precise",
    "git user.name persisted to repository config",
  );
  assert(
    (await gitOutput(fixture.workRepo, ["config", "user.email"])) ===
      "precise@gitspark.local",
    "git user.email persisted to repository config",
  );

  await app.getByTestId("settings-git-scope-global").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.test_tree?.children?.some(
        (node) => node.id === "settings-git-user-name" && node.enabled === false,
      ) &&
      snapshot.test_tree?.children?.some(
        (node) => node.id === "settings-git-user-email" && node.enabled === false,
      ),
    { timeoutMs: 10_000 },
  );
  await app.getByTestId("settings-save-git").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Git config saved." &&
      snapshot.error_message === "",
    { timeoutMs: 10_000 },
  );
  assert(
    (await gitOptionalOutput(fixture.workRepo, ["config", "--local", "--get", "user.name"])) ===
      null,
    "global scope clears repository user.name override",
  );
  assert(
    (await gitOptionalOutput(fixture.workRepo, ["config", "--local", "--get", "user.email"])) ===
      null,
    "global scope clears repository user.email override",
  );

  await app.getByTestId("settings-tab-ai").click();
  await app.getByTestId("settings-provider-openai-compatible").click();
  await app.getByTestId("settings-ai-model").fill("gpt-e2e-precise");
  await app
    .getByTestId("settings-ai-endpoint")
    .fill("http://127.0.0.1:9123/v1/chat/completions");
  await app.getByTestId("settings-ai-api-key").fill("sk-e2e-test-key");
  await app
    .getByTestId("settings-ai-system-prompt")
    .fill("Return a precise JSON commit suggestion.");
  await app.getByTestId("settings-save-ai").click();
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "AI settings saved." &&
      snapshot.error_message === "",
    { timeoutMs: 10_000 },
  );

  const settingsToml = await fs.readFile(fixture.settingsPath, "utf8");
  assert(
    settingsToml.includes('model = "gpt-e2e-precise"'),
    "AI model persisted to isolated settings file",
  );
  assert(
    settingsToml.includes(
      'endpoint = "http://127.0.0.1:9123/v1/chat/completions"',
    ),
    "AI endpoint persisted to isolated settings file",
  );
  assert(
    settingsToml.includes('api_key = "sk-e2e-test-key"'),
    "AI API key persisted to isolated settings file",
  );
  assert(
    settingsToml.includes('system_prompt = "Return a precise JSON commit suggestion."'),
    "AI system prompt persisted to isolated settings file",
  );

  await app.getByTestId("button-settings").click();
  await app.waitForSnapshot((snapshot) => snapshot.show_settings === false);
}

export async function testAiSuccess(app, aiServer) {
  await app.command({ command: "generate_ai_commit" });
  await app.waitForSnapshot(
    (snapshot) =>
      snapshot.status_message === "Generated commit suggestion." &&
      snapshot.error_message === "" &&
      snapshot.commit_summary === "test: mocked ai summary" &&
      snapshot.commit_body === "Mocked body from local e2e server.",
    { timeoutMs: 10_000 },
  );

  assert(aiServer.requests.length === 1, "mock AI server received one request");
  const request = aiServer.requests[0];
  assert(
    request.headers.authorization === "Bearer sk-e2e-test-key",
    "AI request sends configured bearer token",
  );
  assert(
    request.body.messages.some((message) =>
      message.content.includes("src/main.rs"),
    ),
    "AI request includes the working-tree diff",
  );
}
