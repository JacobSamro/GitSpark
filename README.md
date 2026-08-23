<p align="center">
  <img src="assets/gitspark.png" width="128" alt="GitSpark icon" />
</p>

<h1 align="center">GitSpark</h1>

<p align="center">
A Git GUI written in Rust. No Electron, no web views.<br>
Runs on macOS, Windows, and Linux.
</p>

<p align="center">
  <a href="https://github.com/JacobSamro/GitSpark/releases/latest">Download</a>
</p>

---

GitSpark is a desktop Git client. It shells out to the `git` CLI on your machine, so there's nothing to configure and no embedded runtime sitting between you and your repos. If Git works in your terminal, it works here.

The rendering layer is [GPUI](https://gpui.rs), the GPU-accelerated UI framework from the Zed editor. The whole thing is a single native binary.

## What you get

Diffs with syntax highlighting and intra-line markers. Stage files, write a commit message, push. The usual stuff, but without a browser engine burning through your battery.

You can also have an AI write your commit messages. Bring your own API key (OpenAI, OpenRouter, or any compatible endpoint) and it'll generate a conventional-commit-style summary from your diff. Optional, obviously.

Branch switching, creation, and merging are in the toolbar. So are push/pull/fetch, ahead/behind counts, and tags.

The sidebar has two tabs: changes (your working tree) and history (the commit log). You can click into any past commit to see its diff, or right-click to copy the SHA and do other git operations.

Open repos through a file dialog or pick from your recent list. Settings for Git identity and AI provider are inside the app.

## Downloads

| | |
|---|---|
| macOS (Intel + Apple Silicon) | `.dmg` |
| Windows x64 | `Setup.exe` |
| Linux x64 | `.tar.gz` |

All on the [Releases](https://github.com/JacobSamro/GitSpark/releases/latest) page.

## You need

Git installed and on your PATH. [GitHub CLI](https://cli.github.com/) (`gh`), also on your PATH and authenticated, if you want to use "Publish repository" to create a new GitHub repo from GitSpark.

## License

MIT
