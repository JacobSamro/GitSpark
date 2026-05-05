import { GitSparkAutomation } from "./gitspark.mjs";
import { testConflictFlows } from "./suites/conflict-flows.mjs";
import {
  testChangeFileActions,
  testCommitFlow,
  testCreateBranchDialog,
  testDiscardConfirmationDialog,
  testGithubOpenActions,
  testHistoryAndBranchFlows,
  testNetworkFlows,
  testStashAndSwitchDialog,
  testStashFlows,
  testUndoLastCommit,
} from "./suites/git-workflows.mjs";
import {
  testAutomationBasics,
  testLocalChangeAutoSync,
  testRepositoryFlows,
  testShellControls,
} from "./suites/smoke.mjs";
import {
  testAiSuccess,
  testAiValidation,
  testIdentityWarningOpensMissingEmail,
  testSettingsPersistence,
} from "./suites/settings-ai.mjs";
import { makeFreshSampleRepo } from "./support/fixtures.mjs";
import { startMockAiServer } from "./support/mock-ai.mjs";

const fixture = await makeFreshSampleRepo();
const aiServer = await startMockAiServer();
const app = await GitSparkAutomation.launch({
  env: {
    GITSPARK_AI_ENDPOINT: aiServer.url,
    GITSPARK_CONFIG_DIR: fixture.configDir,
    GIT_CONFIG_GLOBAL: fixture.globalGitConfig,
    GITSPARK_OPEN_COMMAND: "/usr/bin/true",
    GITSPARK_OPEN_URL_COMMAND: fixture.openUrlCommand,
    GITSPARK_REVEAL_COMMAND: "/usr/bin/true",
  },
});

try {
  await testAutomationBasics(app);
  await testShellControls(app);
  await testRepositoryFlows(app, fixture);
  await testLocalChangeAutoSync(app, fixture);
  await testAiValidation(app);
  await testSettingsPersistence(app, fixture);
  await testIdentityWarningOpensMissingEmail(app, fixture);
  await testAiSuccess(app, aiServer);
  await testCommitFlow(app);
  await testCreateBranchDialog(app);
  await testHistoryAndBranchFlows(app, fixture);
  await testNetworkFlows(app, fixture);
  await testStashFlows(app, fixture);
  await testStashAndSwitchDialog(app, fixture);
  await testGithubOpenActions(app, fixture);
  await testDiscardConfirmationDialog(app, fixture);
  await testChangeFileActions(app, fixture);
  await testUndoLastCommit(app, fixture);
  await testConflictFlows(app, fixture);
} finally {
  await app.close();
  await aiServer.close();
}
