// Sets up (and tears down) a real, deterministic OS handoff target on
// Windows for the GUI-handoff suite: a PowerShell "stub app" registered as
// the file-type handler for a fixture-only extension, plus a marker file it
// writes its invocation args to. PowerShell rather than a .cmd/.bat file
// for the stub itself — running it via `sh -lc` (which is how GitSpark's
// editor launch actually works, since Git for Windows ships a real sh.exe)
// means the command line has to survive MSYS2's argument translation, and
// PowerShell's `-File`/`-NoProfile`-style flags avoid the classic
// "/c" vs "//c" slash-mangling gotcha that a raw cmd.exe invocation would
// hit there.
import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import path from "node:path";
import { promisify } from "node:util";

const exec = promisify(execFile);

const ROOT_DIR = "C:\\gitspark-e2e";
const STUB_SCRIPT = path.join(ROOT_DIR, "stub.ps1");
const MARKER_FILE = path.join(ROOT_DIR, "marker.log");
const EXTENSION = ".gitsparktest";
const PROG_ID = "GitSparkTestFile";

export function markerPath() {
  return MARKER_FILE;
}

export function stubEditorCommand() {
  return `powershell -NoProfile -ExecutionPolicy Bypass -File "${STUB_SCRIPT}"`;
}

async function runPowerShell(script) {
  const { stdout } = await exec("powershell.exe", [
    "-NoProfile",
    "-ExecutionPolicy",
    "Bypass",
    "-Command",
    script,
  ]);
  return stdout;
}

// Registers the extension under HKEY_CURRENT_USER\Software\Classes, which
// merges into HKEY_CLASSES_ROOT the same way HKLM's copy does — this is
// what ShellExecute (what `open::that`/`open::that_detached` use under the
// hood) actually resolves against, and it needs no elevation to write.
export async function setupStub() {
  await fs.mkdir(ROOT_DIR, { recursive: true });
  await fs.writeFile(
    STUB_SCRIPT,
    "param([Parameter(ValueFromRemainingArguments=$true)][string[]]$StubArgs)\n" +
      `Add-Content -LiteralPath '${MARKER_FILE}' -Value ($StubArgs -join ' ')\n`,
  );

  const openCommand = `powershell.exe -NoProfile -ExecutionPolicy Bypass -File "${STUB_SCRIPT}" "%1"`;
  await runPowerShell(
    `
    New-Item -Path 'HKCU:\\Software\\Classes\\${EXTENSION}' -Force | Out-Null
    Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\${EXTENSION}' -Name '(Default)' -Value '${PROG_ID}'
    New-Item -Path 'HKCU:\\Software\\Classes\\${PROG_ID}\\shell\\open\\command' -Force | Out-Null
    Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\${PROG_ID}\\shell\\open\\command' -Name '(Default)' -Value '${openCommand.replace(/'/g, "''")}'
    `.trim(),
  );
}

export async function teardownStub() {
  await runPowerShell(
    `
    Remove-Item -Path 'HKCU:\\Software\\Classes\\${EXTENSION}' -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -Path 'HKCU:\\Software\\Classes\\${PROG_ID}' -Recurse -Force -ErrorAction SilentlyContinue
    `.trim(),
  ).catch(() => {});
}

export function testFileName() {
  return `notes${EXTENSION}`;
}

// Some handoffs (open URL, "Open in Terminal") don't have a clean
// registry-association story on Windows the way a custom file extension
// does — a URL protocol handler is subject to the "UserChoice" protections
// Windows puts on default-browser selection specifically, and overriding
// the shell handler for Directory would affect the whole CI job, not just
// this test. For those, verify via real process inspection instead: did a
// process whose command line contains `needle` show up.
export async function waitForProcessWithCommandLine(needle, { timeoutMs = 20_000, intervalMs = 500 } = {}) {
  const deadline = Date.now() + timeoutMs;
  const escaped = needle.replace(/'/g, "''");
  let lastOutput = "";
  while (Date.now() <= deadline) {
    lastOutput = await runPowerShell(
      `(Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -like '*${escaped}*' } | Select-Object -First 1 -ExpandProperty CommandLine)`,
    ).catch(() => "");
    if (lastOutput.trim()) {
      return lastOutput.trim();
    }
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
  throw new Error(
    `Timed out waiting for a process whose command line contains "${needle}". Last check returned nothing.`,
  );
}
