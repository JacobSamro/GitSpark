# GitSpark

GitSpark is a Git GUI written in Rust with egui. No Electron, no web views. It calls `git` directly as a subprocess, so it needs Git on your machine and nothing else.

## Features

**Primary & diff view** - See staged and unstaged changes, inspect diffs per file. Detects binary files so you don't get a wall of nonsense.

**Merge** - Pick a branch, merge it in.

**Switch repos** - Open a repo with a file dialog or grab one from your recent list (stores up to 12).

**Switch branch** - Lists local branches, lets you jump between them.

**Git config** - Edit your Git name, email, and other config values from inside the app instead of the terminal.

**AI commit messages** - Connects to any OpenAI-compatible API to generate conventional commit messages from your diff. Works with whatever model you want as long as it speaks the same API format. You provide the key.

## License

MIT
