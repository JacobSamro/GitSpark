use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, anyhow, bail};

use crate::models::{
    BranchInfo, ChangeEntry, CommitInfo, DiffEntry, GitIdentity, RepoSnapshot, RepoSummary,
};

#[derive(Default)]
pub struct GitClient;

impl GitClient {
    pub fn new() -> Self {
        Self
    }

    pub fn get_commit_diff(&self, repo_path: &Path, oid: &str) -> Result<Vec<DiffEntry>> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let oid = self.verify_commit_oid(&repo_path, oid)?;

        // Use diff-tree to get raw list of changed files
        let output = self.run_git(
            &repo_path,
            &["diff-tree", "--no-commit-id", "--name-only", "-r", &oid],
        )?;
        let files: Vec<String> = output
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect();

        let mut diffs = Vec::new();
        for file in files {
            // Fetch diff for this file in this commit.
            let diff_output = match self.run_git(
                &repo_path,
                &[
                    "show",
                    "--format=",
                    "--no-ext-diff",
                    "--no-color",
                    &oid,
                    "--",
                    &file,
                ],
            ) {
                Ok(content) => content,
                Err(_) => "Binary file or deleted".to_string(),
            };

            let is_binary = looks_binary_diff(&diff_output);

            diffs.push(DiffEntry {
                path: file,
                diff: diff_output,
                is_binary,
            });
        }

        Ok(diffs)
    }

    pub fn open_repo(&self, path: impl Into<PathBuf>) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(path.into().as_path())?;
        self.snapshot(&repo_path)
    }

    pub fn refresh_repo(&self, path: &Path) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(path)?;
        self.snapshot(&repo_path)
    }

    pub fn fetch_origin(&self, repo_path: &Path) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let remote_name = self
            .read_primary_remote(&repo_path)?
            .ok_or_else(|| anyhow!("no remote configured for this repository"))?;

        self.run_git(&repo_path, &["fetch", "--prune", &remote_name])
            .with_context(|| format!("failed to fetch from '{remote_name}'"))?;

        self.snapshot(&repo_path)
    }

    pub fn pull_origin(&self, repo_path: &Path) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let remote_name = self
            .read_primary_remote(&repo_path)?
            .ok_or_else(|| anyhow!("no remote configured for this repository"))?;

        if self.has_upstream(&repo_path) {
            self.run_git(&repo_path, &["pull", "--ff-only"])
                .with_context(|| format!("failed to pull from '{remote_name}'"))?;
        } else {
            let status = self.read_status(&repo_path)?;
            if status.current_branch == "HEAD" || status.current_branch == "detached HEAD" {
                bail!("cannot pull while HEAD is detached");
            }

            self.run_git(
                &repo_path,
                &["pull", "--ff-only", &remote_name, &status.current_branch],
            )
            .with_context(|| {
                format!(
                    "failed to pull '{}' from '{}'",
                    status.current_branch, remote_name
                )
            })?;
        }

        self.snapshot(&repo_path)
    }

    pub fn push_origin(&self, repo_path: &Path) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let remote_name = self
            .read_primary_remote(&repo_path)?
            .ok_or_else(|| anyhow!("no remote configured for this repository"))?;

        if self.has_upstream(&repo_path) {
            self.run_git(&repo_path, &["push", &remote_name])
                .with_context(|| format!("failed to push to '{remote_name}'"))?;
        } else {
            self.run_git(&repo_path, &["push", "--set-upstream", &remote_name, "HEAD"])
                .with_context(|| format!("failed to publish branch to '{remote_name}'"))?;
        }

        self.snapshot(&repo_path)
    }

    pub fn switch_branch(&self, repo_path: &Path, branch_name: &str) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let branch_name = branch_name.trim();
        if branch_name.is_empty() {
            bail!("branch name cannot be empty");
        }

        if self.local_branch_exists(&repo_path, branch_name)? {
            self.run_git(&repo_path, &["switch", branch_name])
                .with_context(|| format!("failed to switch to branch '{branch_name}'"))?;
        } else if self.remote_branch_exists(&repo_path, branch_name)? {
            let local_name = branch_name
                .split_once('/')
                .map(|(_, name)| name)
                .filter(|name| !name.is_empty())
                .unwrap_or(branch_name);

            self.run_git(
                &repo_path,
                &["switch", "--track", "-c", local_name, branch_name],
            )
            .with_context(|| format!("failed to create tracking branch from '{branch_name}'"))?;
        } else {
            self.run_git(&repo_path, &["switch", branch_name])
                .with_context(|| format!("failed to switch to branch '{branch_name}'"))?;
        }

        self.snapshot(&repo_path)
    }

    pub fn merge_branch(&self, repo_path: &Path, branch_name: &str) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let branch_name = branch_name.trim();
        if branch_name.is_empty() {
            bail!("merge target cannot be empty");
        }

        self.run_git(&repo_path, &["merge", "--no-ff", branch_name])
            .with_context(|| format!("failed to merge branch '{branch_name}'"))?;

        self.snapshot(&repo_path)
    }

    pub fn commit_all(&self, repo_path: &Path, message: &str) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let message = message.trim();
        if message.is_empty() {
            bail!("commit message cannot be empty");
        }

        self.run_git(&repo_path, &["add", "--all"])
            .context("failed to stage repository changes")?;
        self.run_git(&repo_path, &["commit", "-m", message])
            .context("failed to create commit")?;

        self.snapshot(&repo_path)
    }

    pub fn discard_change(&self, repo_path: &Path, relative_path: &str) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let relative_path = relative_path.trim();
        if relative_path.is_empty() {
            bail!("file path cannot be empty");
        }

        if self.path_is_tracked(&repo_path, relative_path)? {
            match self.run_git(
                &repo_path,
                &[
                    "restore",
                    "--source=HEAD",
                    "--staged",
                    "--worktree",
                    "--",
                    relative_path,
                ],
            ) {
                Ok(_) => {}
                Err(_) => {
                    self.run_git(&repo_path, &["checkout", "--", relative_path])
                        .with_context(|| {
                            format!("failed to discard tracked changes for '{relative_path}'")
                        })?;
                }
            }
        } else {
            let full_path = repo_path.join(relative_path);
            if full_path.is_dir() {
                fs::remove_dir_all(&full_path).with_context(|| {
                    format!(
                        "failed to remove untracked directory '{}'",
                        full_path.display()
                    )
                })?;
            } else if full_path.exists() {
                fs::remove_file(&full_path).with_context(|| {
                    format!("failed to remove untracked file '{}'", full_path.display())
                })?;
            }
        }

        self.snapshot(&repo_path)
    }

    pub fn append_gitignore_pattern(
        &self,
        repo_path: &Path,
        pattern: &str,
    ) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let pattern = pattern.trim();
        if pattern.is_empty() {
            bail!("ignore pattern cannot be empty");
        }

        let gitignore_path = repo_path.join(".gitignore");
        let mut content = if gitignore_path.exists() {
            fs::read_to_string(&gitignore_path)
                .with_context(|| format!("failed to read '{}'", gitignore_path.display()))?
        } else {
            String::new()
        };

        let already_present = content.lines().map(str::trim).any(|line| line == pattern);

        if !already_present {
            if !content.is_empty() && !content.ends_with('\n') {
                content.push('\n');
            }
            content.push_str(pattern);
            content.push('\n');
            fs::write(&gitignore_path, content)
                .with_context(|| format!("failed to write '{}'", gitignore_path.display()))?;
        }

        self.snapshot(&repo_path)
    }

    pub fn read_config_value(&self, repo_path: &Path, key: &str) -> Result<Option<String>> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let value = self.read_optional_config(&repo_path, key)?;
        Ok(non_empty(value))
    }

    pub fn read_identity(&self, repo_path: &Path) -> Result<GitIdentity> {
        let repo_path = self.resolve_repo_root(repo_path)?;

        Ok(GitIdentity {
            user_name: self.read_optional_config(&repo_path, "user.name")?,
            user_email: self.read_optional_config(&repo_path, "user.email")?,
            pull_rebase: self.read_optional_bool_config(&repo_path, "pull.rebase")?,
            default_branch: non_empty(self.read_optional_config(&repo_path, "init.defaultBranch")?),
        })
    }

    pub fn write_identity(&self, repo_path: &Path, identity: &GitIdentity) -> Result<()> {
        let repo_path = self.resolve_repo_root(repo_path)?;

        self.write_string_config(&repo_path, "user.name", &identity.user_name)?;
        self.write_string_config(&repo_path, "user.email", &identity.user_email)?;
        self.write_bool_config(&repo_path, "pull.rebase", identity.pull_rebase)?;
        self.write_optional_string_config(
            &repo_path,
            "init.defaultBranch",
            identity.default_branch.as_deref(),
        )?;

        Ok(())
    }

    fn snapshot(&self, repo_path: &Path) -> Result<RepoSnapshot> {
        let status = self.read_status(repo_path)?;
        let repo_name = repo_path
            .file_name()
            .and_then(|name| name.to_str())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| repo_path.display().to_string());

        let branches = self.list_branches(repo_path)?;
        let diffs = self.build_diffs(repo_path, &status.changes)?;
        let history = self.fetch_history(repo_path, 100).unwrap_or_default();
        let stash_count = self.stash_count(repo_path).unwrap_or(0);
        let remote_name = self.read_primary_remote(repo_path).unwrap_or(None);
        let last_fetched = self.read_last_fetched(repo_path);

        Ok(RepoSnapshot {
            repo: RepoSummary {
                path: repo_path.to_path_buf(),
                name: repo_name,
                current_branch: status.current_branch,
                head_oid: status.head_oid,
                remote_name,
                ahead: status.ahead,
                behind: status.behind,
                last_fetched,
            },
            changes: status.changes,
            diffs,
            branches,
            history,
            stash_count,
        })
    }

    fn fetch_history(&self, repo_path: &Path, limit: usize) -> Result<Vec<CommitInfo>> {
        let output = self.run_git_bytes(
            repo_path,
            &[
                "log",
                &format!("-n{limit}"),
                "--pretty=format:%x1e%H%x1f%h%x1f%s%x1f%b%x1f%an%x1f%ae%x1f%ar",
            ],
        )?;

        let raw = String::from_utf8(output).context("git log output was not valid UTF-8")?;
        if raw.is_empty() {
            return Ok(Vec::new());
        }

        let mut commits = Vec::new();
        for record in raw
            .split('\u{1e}')
            .filter(|record| !record.trim().is_empty())
        {
            let chunk: Vec<&str> = record.split('\u{1f}').collect();
            if chunk.len() != 7 {
                continue;
            }

            commits.push(CommitInfo {
                oid: chunk[0].trim().to_string(),
                short_oid: chunk[1].trim().to_string(),
                summary: chunk[2].trim().to_string(),
                body: chunk[3].trim_end().to_string(),
                author_name: chunk[4].trim().to_string(),
                author_email: chunk[5].trim().to_string(),
                date: chunk[6].trim().to_string(),
                is_head: false,
            });
        }

        if let Some(first) = commits.first_mut() {
            first.is_head = true;
        }

        Ok(commits)
    }

    fn verify_commit_oid(&self, repo_path: &Path, oid: &str) -> Result<String> {
        let candidate = oid.trim();
        if candidate.is_empty() {
            bail!("commit id is empty");
        }

        let resolved = self.run_git(
            repo_path,
            &["rev-parse", "--verify", &format!("{candidate}^{{commit}}")],
        )?;

        Ok(resolved.trim().to_string())
    }

    fn stash_count(&self, repo_path: &Path) -> Result<usize> {
        let output = self.run_git(repo_path, &["stash", "list", "--format=%gd"])?;
        Ok(output
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count())
    }

    fn resolve_repo_root(&self, path: &Path) -> Result<PathBuf> {
        if !path.exists() {
            bail!("repository path '{}' does not exist", path.display());
        }

        let candidate = if path.is_file() {
            path.parent()
                .ok_or_else(|| anyhow!("'{}' has no parent directory", path.display()))?
        } else {
            path
        };

        let output = self
            .run_git(candidate, &["rev-parse", "--show-toplevel"])
            .with_context(|| format!("'{}' is not a Git repository", candidate.display()))?;

        Ok(PathBuf::from(output.trim()))
    }

    fn has_upstream(&self, repo_path: &Path) -> bool {
        self.run_git(
            repo_path,
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"],
        )
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
    }

    fn read_primary_remote(&self, repo_path: &Path) -> Result<Option<String>> {
        if let Ok(upstream) = self.run_git(
            repo_path,
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"],
        ) {
            let upstream = upstream.trim();
            if let Some((remote, _)) = upstream.split_once('/') {
                if !remote.is_empty() {
                    return Ok(Some(remote.to_string()));
                }
            }
        }

        let remotes = self.run_git(repo_path, &["remote"])?;
        let mut names = remotes
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();

        if names.is_empty() {
            return Ok(None);
        }

        names.sort_by_key(|name| if name == "origin" { 0 } else { 1 });
        Ok(names.into_iter().next())
    }

    fn read_last_fetched(&self, repo_path: &Path) -> Option<String> {
        let git_dir_output = self.run_git(repo_path, &["rev-parse", "--git-dir"]).ok()?;
        let git_dir = git_dir_output.trim();
        if git_dir.is_empty() {
            return None;
        }

        let git_dir_path = {
            let path = PathBuf::from(git_dir);
            if path.is_absolute() {
                path
            } else {
                repo_path.join(path)
            }
        };

        let fetch_head = git_dir_path.join("FETCH_HEAD");
        let metadata = fs::metadata(fetch_head).ok()?;
        if metadata.len() == 0 {
            return None;
        }

        let modified = metadata.modified().ok()?;
        Some(format_relative_time(modified))
    }

    fn local_branch_exists(&self, repo_path: &Path, branch_name: &str) -> Result<bool> {
        self.run_git(
            repo_path,
            &[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch_name}"),
            ],
        )
        .map(|_| true)
        .or_else(|error| {
            if is_ref_missing(&error) {
                Ok(false)
            } else {
                Err(error)
            }
        })
    }

    fn remote_branch_exists(&self, repo_path: &Path, branch_name: &str) -> Result<bool> {
        self.run_git(
            repo_path,
            &[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/remotes/{branch_name}"),
            ],
        )
        .map(|_| true)
        .or_else(|error| {
            if is_ref_missing(&error) {
                Ok(false)
            } else {
                Err(error)
            }
        })
    }

    fn read_status(&self, repo_path: &Path) -> Result<StatusSnapshot> {
        let output = self.run_git_bytes(
            repo_path,
            &[
                "status",
                "--porcelain=v2",
                "--branch",
                "--untracked-files=all",
                "-z",
            ],
        )?;

        parse_status_porcelain_v2(&output)
    }

    fn list_branches(&self, repo_path: &Path) -> Result<Vec<BranchInfo>> {
        let output = self.run_git(
            repo_path,
            &[
                "for-each-ref",
                "--format=%(refname:short)\t%(HEAD)\t%(refname)",
                "refs/heads",
                "refs/remotes",
            ],
        )?;

        let mut branches = output
            .lines()
            .filter_map(|line| {
                let mut parts = line.split('\t');
                let name = parts.next()?.trim();
                let head = parts.next()?.trim();
                let full_ref = parts.next()?.trim();

                if name.is_empty() || name.ends_with("/HEAD") {
                    return None;
                }

                Some(BranchInfo {
                    name: name.to_string(),
                    is_current: head == "*",
                    is_remote: full_ref.starts_with("refs/remotes/"),
                })
            })
            .collect::<Vec<_>>();

        branches.sort_by(|left, right| {
            left.is_remote
                .cmp(&right.is_remote)
                .then(right.is_current.cmp(&left.is_current))
                .then(left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });

        Ok(branches)
    }

    fn build_diffs(&self, repo_path: &Path, changes: &[ChangeEntry]) -> Result<Vec<DiffEntry>> {
        changes
            .iter()
            .map(|change| self.build_diff_entry(repo_path, change))
            .collect()
    }

    fn build_diff_entry(&self, repo_path: &Path, change: &ChangeEntry) -> Result<DiffEntry> {
        let staged = self.run_git(
            repo_path,
            &[
                "diff",
                "--no-ext-diff",
                "--no-color",
                "--cached",
                "--",
                &change.path,
            ],
        )?;
        let unstaged = self.run_git(
            repo_path,
            &["diff", "--no-ext-diff", "--no-color", "--", &change.path],
        )?;

        let mut sections = Vec::new();
        if !staged.trim().is_empty() {
            sections.push(("Staged", staged));
        }
        if !unstaged.trim().is_empty() {
            sections.push(("Working tree", unstaged));
        }

        if sections.is_empty() && change.status == "??" {
            return self.build_untracked_diff(repo_path, &change.path);
        }

        let combined = if sections.len() <= 1 {
            sections
                .pop()
                .map(|(_, diff)| diff)
                .unwrap_or_else(|| "No textual diff available".to_string())
        } else {
            sections
                .into_iter()
                .map(|(label, diff)| format!("### {label}\n{diff}"))
                .collect::<Vec<_>>()
                .join("\n\n")
        };

        let is_binary = looks_binary_diff(&combined)
            || self
                .path_is_binary(repo_path, &change.path)
                .unwrap_or(false);

        Ok(DiffEntry {
            path: change.path.clone(),
            diff: if combined.trim().is_empty() {
                "No textual diff available".to_string()
            } else if is_binary && looks_binary_diff(&combined) {
                "Binary file changed".to_string()
            } else {
                combined
            },
            is_binary,
        })
    }

    fn build_untracked_diff(&self, repo_path: &Path, relative_path: &str) -> Result<DiffEntry> {
        let full_path = repo_path.join(relative_path);
        let bytes = fs::read(&full_path)
            .with_context(|| format!("failed to read file '{}'", full_path.display()))?;

        if std::str::from_utf8(&bytes).is_err() {
            return Ok(DiffEntry {
                path: relative_path.to_string(),
                diff: "Binary file added".to_string(),
                is_binary: true,
            });
        }

        let contents = String::from_utf8(bytes).context("failed to decode file contents")?;
        let line_count = contents.lines().count().max(1);
        let body = contents
            .lines()
            .take(400)
            .map(|line| format!("+{line}"))
            .collect::<Vec<_>>()
            .join("\n");

        let diff =
            format!("--- /dev/null\n+++ b/{relative_path}\n@@ -0,0 +1,{line_count} @@\n{body}");

        Ok(DiffEntry {
            path: relative_path.to_string(),
            diff,
            is_binary: false,
        })
    }

    fn path_is_tracked(&self, repo_path: &Path, relative_path: &str) -> Result<bool> {
        self.run_git(
            repo_path,
            &["ls-files", "--error-unmatch", "--", relative_path],
        )
        .map(|_| true)
        .or_else(|error| {
            if is_path_not_tracked(&error) {
                Ok(false)
            } else {
                Err(error)
            }
        })
    }

    fn path_is_binary(&self, repo_path: &Path, relative_path: &str) -> Result<bool> {
        let full_path = repo_path.join(relative_path);
        if !full_path.exists() {
            return Ok(false);
        }

        let bytes = fs::read(&full_path)
            .with_context(|| format!("failed to read file '{}'", full_path.display()))?;
        Ok(std::str::from_utf8(&bytes).is_err())
    }

    fn read_optional_config(&self, repo_path: &Path, key: &str) -> Result<String> {
        match self.run_git(repo_path, &["config", "--local", "--get", key]) {
            Ok(value) => Ok(value.trim().to_string()),
            Err(error) if is_config_missing(&error) => Ok(String::new()),
            Err(error) => Err(error).with_context(|| format!("failed reading config '{key}'")),
        }
    }

    fn read_optional_bool_config(&self, repo_path: &Path, key: &str) -> Result<Option<bool>> {
        let value = self.read_optional_config(repo_path, key)?;
        if value.is_empty() {
            return Ok(None);
        }

        parse_git_bool(&value)
            .map(Some)
            .with_context(|| format!("invalid boolean value for '{key}': '{value}'"))
    }

    fn write_string_config(&self, repo_path: &Path, key: &str, value: &str) -> Result<()> {
        self.write_optional_string_config(repo_path, key, non_empty(value.to_string()).as_deref())
    }

    fn write_optional_string_config(
        &self,
        repo_path: &Path,
        key: &str,
        value: Option<&str>,
    ) -> Result<()> {
        match value {
            Some(value) => self
                .run_git(repo_path, &["config", "--local", key, value])
                .map(|_| ())
                .with_context(|| format!("failed writing config '{key}'")),
            None => self
                .run_git(repo_path, &["config", "--local", "--unset", key])
                .map(|_| ())
                .or_else(|error| {
                    if is_config_missing(&error) {
                        Ok(())
                    } else {
                        Err(error)
                    }
                })
                .with_context(|| format!("failed clearing config '{key}'")),
        }
    }

    fn write_bool_config(&self, repo_path: &Path, key: &str, value: Option<bool>) -> Result<()> {
        match value {
            Some(value) => self
                .run_git(
                    repo_path,
                    &[
                        "config",
                        "--local",
                        key,
                        if value { "true" } else { "false" },
                    ],
                )
                .map(|_| ())
                .with_context(|| format!("failed writing config '{key}'")),
            None => self.write_optional_string_config(repo_path, key, None),
        }
    }

    fn run_git(&self, repo_path: &Path, args: &[&str]) -> Result<String> {
        let output = self.run_git_bytes(repo_path, args)?;
        String::from_utf8(output).context("git output was not valid UTF-8")
    }

    fn run_git_bytes(&self, repo_path: &Path, args: &[&str]) -> Result<Vec<u8>> {
        let output = run_git_command(repo_path, args)?;
        Ok(output.stdout)
    }
}

#[derive(Default)]
struct StatusSnapshot {
    current_branch: String,
    head_oid: Option<String>,
    ahead: usize,
    behind: usize,
    changes: Vec<ChangeEntry>,
}

fn run_git_command(repo_path: &Path, args: &[&str]) -> Result<Output> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .with_context(|| {
            format!(
                "failed to launch git in '{}' with args {:?}",
                repo_path.display(),
                args
            )
        })?;

    if output.status.success() {
        return Ok(output);
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let message = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("git exited with status {}", output.status)
    };

    Err(anyhow!(
        "git {:?} failed in '{}': {}",
        args,
        repo_path.display(),
        message
    ))
}

fn parse_status_porcelain_v2(bytes: &[u8]) -> Result<StatusSnapshot> {
    let tokens = bytes
        .split(|byte| *byte == 0)
        .filter(|token| !token.is_empty())
        .map(|token| String::from_utf8_lossy(token).into_owned())
        .collect::<Vec<_>>();

    let mut snapshot = StatusSnapshot::default();
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];

        if let Some(head) = token.strip_prefix("# branch.head ") {
            snapshot.current_branch = if head == "(detached)" {
                "detached HEAD".to_string()
            } else {
                head.to_string()
            };
        } else if let Some(oid) = token.strip_prefix("# branch.oid ") {
            if oid != "(initial)" {
                snapshot.head_oid = Some(oid.to_string());
            }
        } else if let Some(ab) = token.strip_prefix("# branch.ab ") {
            for part in ab.split_whitespace() {
                if let Some(ahead) = part.strip_prefix('+') {
                    snapshot.ahead = ahead.parse().unwrap_or(0);
                } else if let Some(behind) = part.strip_prefix('-') {
                    snapshot.behind = behind.parse().unwrap_or(0);
                }
            }
        } else if let Some(record) = token.strip_prefix("1 ") {
            let fields = record.splitn(8, ' ').collect::<Vec<_>>();
            if fields.len() == 8 {
                snapshot.changes.push(ChangeEntry {
                    path: fields[7].to_string(),
                    status: compact_status(fields[0]),
                });
            }
        } else if let Some(record) = token.strip_prefix("2 ") {
            let fields = record.splitn(9, ' ').collect::<Vec<_>>();
            if fields.len() == 9 {
                let original_path = tokens.get(index + 1).cloned().unwrap_or_default();
                snapshot.changes.push(ChangeEntry {
                    path: fields[8].to_string(),
                    status: format!("{} {}", compact_status(fields[0]), original_path),
                });
                index += 1;
            }
        } else if let Some(record) = token.strip_prefix("u ") {
            let fields = record.splitn(10, ' ').collect::<Vec<_>>();
            if fields.len() == 10 {
                snapshot.changes.push(ChangeEntry {
                    path: fields[9].to_string(),
                    status: compact_status(fields[0]),
                });
            }
        } else if let Some(path) = token.strip_prefix("? ") {
            snapshot.changes.push(ChangeEntry {
                path: path.to_string(),
                status: "??".to_string(),
            });
        }

        index += 1;
    }

    if snapshot.current_branch.is_empty() {
        snapshot.current_branch = "HEAD".to_string();
    }

    Ok(snapshot)
}

fn compact_status(xy: &str) -> String {
    let compact = xy.replace(' ', "");
    if compact.is_empty() {
        "??".to_string()
    } else {
        compact
    }
}

fn parse_git_bool(value: &str) -> Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Ok(true),
        "false" | "no" | "off" | "0" => Ok(false),
        other => bail!("unsupported git boolean '{other}'"),
    }
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn looks_binary_diff(diff: &str) -> bool {
    diff.contains("Binary files") || diff.contains("GIT binary patch")
}

fn format_relative_time(timestamp: SystemTime) -> String {
    let elapsed = SystemTime::now()
        .duration_since(timestamp)
        .unwrap_or(Duration::ZERO);

    let seconds = elapsed.as_secs();
    match seconds {
        0..=44 => "just now".to_string(),
        45..=89 => "1 minute ago".to_string(),
        90..=2_699 => format!("{} minutes ago", seconds / 60),
        2_700..=5_399 => "1 hour ago".to_string(),
        5_400..=86_399 => format!("{} hours ago", seconds / 3_600),
        86_400..=172_799 => "1 day ago".to_string(),
        _ => format!("{} days ago", seconds / 86_400),
    }
}

fn is_config_missing(error: &anyhow::Error) -> bool {
    let message = error.to_string();
    message.contains("exit status: 1")
        || message.contains("returned non-zero exit status: 1")
        || message.contains("unable to read config")
        || message.contains("key does not contain a section")
}

fn is_ref_missing(error: &anyhow::Error) -> bool {
    let message = error.to_string();
    message.contains("exit status: 1") || message.contains("returned non-zero exit status: 1")
}

fn is_path_not_tracked(error: &anyhow::Error) -> bool {
    let message = error.to_string();
    message.contains("did not match any file")
        || message.contains("pathspec")
        || message.contains("exit status: 1")
        || message.contains("returned non-zero exit status: 1")
}
