import { promises as fs } from "node:fs";

export async function readOpenUrlLog(logPath) {
  try {
    const text = await fs.readFile(logPath, "utf8");
    return text.split("\n").filter(Boolean);
  } catch (error) {
    if (error.code === "ENOENT") {
      return [];
    }
    throw error;
  }
}

export async function waitForOpenUrl(logPath, expectedUrl, message) {
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) {
    const openedUrls = await readOpenUrlLog(logPath);
    if (openedUrls.includes(expectedUrl)) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(message);
}
