import { randomUUID } from "node:crypto";

const DEFAULT_BASE_URL = process.env.GITSPARK_GITEA_URL || "http://localhost:3050";
const TOKEN_NAME = "e2e-token";

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

// Gitea's `/api/v1/version` answers as soon as the web server is up, well
// before the container is otherwise "ready" in any docker-compose sense —
// polling it directly is simpler and faster than waiting on container
// health checks.
export async function waitForGiteaReady(
  baseUrl = DEFAULT_BASE_URL,
  { timeoutMs = 60_000, intervalMs = 500 } = {},
) {
  const deadline = Date.now() + timeoutMs;
  let lastError;

  while (Date.now() <= deadline) {
    try {
      const response = await fetch(`${baseUrl}/api/v1/version`);
      if (response.ok) {
        return;
      }
    } catch (error) {
      lastError = error;
    }
    await delay(intervalMs);
  }

  throw new Error(
    `Gitea at ${baseUrl} did not become ready within ${timeoutMs}ms` +
      (lastError ? `: ${lastError.message}` : ""),
  );
}

function extractCookies(response) {
  const setCookies =
    typeof response.headers.getSetCookie === "function"
      ? response.headers.getSetCookie()
      : [];
  return setCookies.map((cookie) => cookie.split(";")[0]).join("; ");
}

// A fresh Gitea instance has no admin, so a personal access token cannot be
// minted through the API directly. Registering through the public sign-up
// form (a regular, non-admin user is all that's needed to create and push
// to a repo of one's own) is the only credential-free path in — and that
// form is CSRF-protected, so a session cookie has to be carried from the
// GET into the POST.
async function registerUser(baseUrl, { username, email, password }) {
  const signUpUrl = `${baseUrl}/user/sign_up`;
  const getResponse = await fetch(signUpUrl);
  const cookies = extractCookies(getResponse);
  const html = await getResponse.text();
  const csrfMatch = html.match(/name="_csrf" value="([^"]*)"/);
  if (!csrfMatch) {
    throw new Error("could not find a CSRF token on Gitea's sign-up form");
  }

  const body = new URLSearchParams({
    _csrf: csrfMatch[1],
    user_name: username,
    email,
    password,
    retype: password,
  });

  const postResponse = await fetch(signUpUrl, {
    method: "POST",
    headers: {
      "Content-Type": "application/x-www-form-urlencoded",
      Cookie: cookies,
    },
    body: body.toString(),
    redirect: "manual",
  });

  if (postResponse.status !== 303 && postResponse.status !== 302) {
    const text = await postResponse.text().catch(() => "");
    throw new Error(
      `Gitea sign-up failed with status ${postResponse.status}: ${text.slice(0, 300)}`,
    );
  }
}

async function createToken(baseUrl, { username, password }) {
  const auth = Buffer.from(`${username}:${password}`).toString("base64");
  const response = await fetch(
    `${baseUrl}/api/v1/users/${encodeURIComponent(username)}/tokens`,
    {
      method: "POST",
      headers: {
        Authorization: `Basic ${auth}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        name: TOKEN_NAME,
        scopes: ["write:repository", "write:user"],
      }),
    },
  );

  if (!response.ok) {
    const text = await response.text().catch(() => "");
    throw new Error(
      `Gitea token creation failed with status ${response.status}: ${text.slice(0, 300)}`,
    );
  }

  const data = await response.json();
  return data.sha1;
}

async function createRepo(baseUrl, { token, repoName }) {
  const response = await fetch(`${baseUrl}/api/v1/user/repos`, {
    method: "POST",
    headers: {
      Authorization: `token ${token}`,
      "Content-Type": "application/json",
    },
    // auto_init: false so the first push through GitSpark exercises the
    // real no-upstream/--set-upstream branch of push_origin, the same as
    // publishing a brand-new local repo would.
    body: JSON.stringify({ name: repoName, private: true, auto_init: false }),
  });

  if (!response.ok) {
    const text = await response.text().catch(() => "");
    throw new Error(
      `Gitea repo creation failed with status ${response.status}: ${text.slice(0, 300)}`,
    );
  }

  return response.json();
}

// Provisions a throwaway Gitea user, token, and empty private repo, and
// returns everything a caller needs to point a plain `git` remote (or
// GitSpark itself) at it over HTTP with the token embedded in the URL.
export async function provisionGiteaRepo({
  baseUrl = DEFAULT_BASE_URL,
  repoName = "gitspark-e2e-repo",
} = {}) {
  await waitForGiteaReady(baseUrl);

  const suffix = randomUUID().slice(0, 8);
  const username = `e2e-${suffix}`;
  const password = `E2e-${suffix}-Pw1!`;
  const email = `${username}@gitspark.local`;

  await registerUser(baseUrl, { username, email, password });
  const token = await createToken(baseUrl, { username, password });
  await createRepo(baseUrl, { token, repoName });

  const ownerRepoPath = `${username}/${repoName}`;
  const remoteUrl = baseUrl.replace(
    /^(https?:\/\/)/,
    `$1${username}:${token}@`,
  ) + `/${ownerRepoPath}.git`;

  return {
    baseUrl,
    apiBase: `${baseUrl}/api/v1`,
    username,
    password,
    token,
    repoName,
    ownerRepoPath,
    remoteUrl,
  };
}

// A push that just completed (git process exited 0) can be visible to a
// plain `git` client polling refs before Gitea's own API has caught up on
// its side — a real, observed race against a live server, not a local bare
// repo. A short retry absorbs that lag instead of the test being flaky.
export async function getRemoteBranchSha(
  { apiBase, token, ownerRepoPath },
  branch,
  { retries = 5, retryDelayMs = 500 } = {},
) {
  let lastStatus;

  for (let attempt = 0; attempt <= retries; attempt += 1) {
    const response = await fetch(
      `${apiBase}/repos/${ownerRepoPath}/branches/${encodeURIComponent(branch)}`,
      { headers: { Authorization: `token ${token}` } },
    );

    if (response.ok) {
      const data = await response.json();
      return data.commit.id;
    }

    lastStatus = response.status;
    if (attempt < retries) {
      await delay(retryDelayMs);
    }
  }

  throw new Error(
    `Gitea branch lookup for '${branch}' failed with status ${lastStatus} after ${retries + 1} attempts`,
  );
}
