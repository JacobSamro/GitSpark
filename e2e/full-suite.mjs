import { GitSparkAutomation } from "./gitspark.mjs";
import { testAutomationContracts } from "./suites/automation-contracts.mjs";
import { testCompareEdgeCases } from "./suites/compare-edge-cases.mjs";
import { testConflictFlows } from "./suites/conflict-flows.mjs";
import { testFileOperationEdgeCases } from "./suites/file-edge-cases.mjs";
import { testGithubEnterpriseUrlBehavior } from "./suites/github-url-behavior.mjs";
import { testKeyboardFocusPaths } from "./suites/keyboard-focus.mjs";
import {
  testMenuStateWithRepository,
  testMenuStateWithoutRepository,
} from "./suites/menu-state.mjs";
import { testPerformanceScaleSmoke } from "./suites/performance-scale.mjs";
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
import { testCreateCloneRepositoryWorkflows } from "./suites/repository-create-clone.mjs";
import {
  testAiSuccess,
  testAiValidation,
  testIdentityWarningOpensMissingEmail,
  testSettingsPersistence,
} from "./suites/settings-ai.mjs";
import { testSettingsScopeRegressions } from "./suites/settings-scope-regressions.mjs";
import { testStashEdgeCases } from "./suites/stash-edge-cases.mjs";
import { testVisualContracts } from "./suites/visual-contracts.mjs";
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
  await testMenuStateWithoutRepository(app);
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
  await testCompareEdgeCases(app);
  await testCreateCloneRepositoryWorkflows(app);
  await testGithubEnterpriseUrlBehavior(app, fixture);
  await testSettingsScopeRegressions(app, fixture);
  await testFileOperationEdgeCases(app);
  await testStashEdgeCases(app);
  await testKeyboardFocusPaths(app, fixture);
  await testVisualContracts(app);
  await testAutomationContracts(app);
  await testPerformanceScaleSmoke(app);
  await testMenuStateWithRepository(app);
} finally {
  await app.close();
  await aiServer.close();
}
