use std::collections::HashMap;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime};
use std::{env, fs, io, process};

use anyhow::{Context, Result, anyhow, bail};

use crate::models::{
    BranchComparison, BranchInfo, ChangeEntry, CommitInfo, CreateRepositoryOptions, DiffEntry,
    GitIdentity, GitOperationKind, GitOperationState, RepoSnapshot, RepoSummary, WorktreeInfo,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const GITSPARK_STASH_MESSAGE_PREFIX: &str = "GitSpark stash for ";
/// How much of an untracked file to render as a synthetic diff.
///
/// The old limit was 400 lines, chosen when the diff view built an element per
/// line on every frame. That cost is gone — the view is virtualized — so this
/// is now only a guard against pathological generated files, and can be
/// generous enough that real source files are never cut.
const UNTRACKED_DIFF_MAX_LINES: usize = 5_000;

#[derive(Default)]
pub struct GitClient;

struct StashEntry {
    ref_name: String,
    sha: String,
    subject: String,
}

impl StashEntry {
    fn is_gitspark_stash_for(&self, branch_name: &str) -> bool {
        self.subject
            .ends_with(&format!("{GITSPARK_STASH_MESSAGE_PREFIX}{branch_name}"))
    }
}

impl GitClient {
    pub fn new() -> Self {
        Self
    }

    #[allow(dead_code)]
    pub fn create_repository(
        &self,
        parent_path: &Path,
        name: &str,
        description: &str,
    ) -> Result<RepoSnapshot> {
        self.create_repository_with_options(
            parent_path,
            CreateRepositoryOptions {
                name: name.to_string(),
                description: description.to_string(),
                branch_name: String::new(),
                initialize_readme: true,
                gitignore_template: String::new(),
                license_template: String::new(),
                initial_commit: false,
            },
        )
    }

    pub fn create_repository_with_options(
        &self,
        parent_path: &Path,
        options: CreateRepositoryOptions,
    ) -> Result<RepoSnapshot> {
        let parent_path = parent_path.to_path_buf();
        if !parent_path.exists() {
            fs::create_dir_all(&parent_path).with_context(|| {
                format!(
                    "failed to create parent directory '{}'",
                    parent_path.display()
                )
            })?;
        }
        if !parent_path.is_dir() {
            bail!("'{}' is not a directory", parent_path.display());
        }

        let name = options.name.trim();
        if name.is_empty() {
            bail!("repository name is required");
        }
        let directory_name = safe_repository_directory_name(name);
        if directory_name.is_empty() {
            bail!("repository name is not valid");
        }
        let repo_path = parent_path.join(directory_name);
        if repo_path.exists() {
            let mut entries = fs::read_dir(&repo_path)
                .with_context(|| format!("failed to inspect '{}'", repo_path.display()))?;
            if entries.next().is_some() {
                bail!("'{}' already exists and is not empty", repo_path.display());
            }
        } else {
            fs::create_dir_all(&repo_path)
                .with_context(|| format!("failed to create '{}'", repo_path.display()))?;
        }

        let branch_name = safe_branch_name(options.branch_name.trim());
        if branch_name.is_empty() {
            self.run_git(&repo_path, &["init"])
                .with_context(|| format!("failed to initialize '{}'", repo_path.display()))?;
        } else {
            self.run_git(&repo_path, &["init", "-b", &branch_name])
                .with_context(|| format!("failed to initialize '{}'", repo_path.display()))?;
        }

        if options.initialize_readme {
            let readme_path = repo_path.join("README.md");
            fs::write(&readme_path, format!("# {name}\n")).with_context(|| {
                format!("failed to write README at '{}'", readme_path.display())
            })?;
        }

        if let Some(contents) = gitignore_template_contents(&options.gitignore_template) {
            fs::write(repo_path.join(".gitignore"), contents).with_context(|| {
                format!(
                    "failed to write gitignore at '{}'",
                    repo_path.join(".gitignore").display()
                )
            })?;
        }

        if let Some(contents) = license_template_contents(&options.license_template, name) {
            fs::write(repo_path.join("LICENSE"), contents).with_context(|| {
                format!(
                    "failed to write license at '{}'",
                    repo_path.join("LICENSE").display()
                )
            })?;
        }

        let description = options.description.trim();
        if !description.is_empty() {
            let git_description_path = repo_path.join(".git").join("description");
            fs::write(&git_description_path, format!("{description}\n")).with_context(|| {
                format!(
                    "failed to write Git description at '{}'",
                    git_description_path.display()
                )
            })?;
        }

        if options.initial_commit {
            let changed = self.run_git(&repo_path, &["status", "--porcelain"])?;
            if !changed.trim().is_empty() {
                self.run_git(&repo_path, &["add", "--all"])
                    .context("failed to stage initial repository files")?;
                self.run_git(&repo_path, &["commit", "-m", "Initial commit"])
                    .context("failed to create initial commit")?;
            }
        }

        self.snapshot(&repo_path)
    }

    #[allow(dead_code)]
    pub fn clone_repository(&self, url: &str, destination_path: &Path) -> Result<RepoSnapshot> {
        let url = url.trim();
        if url.is_empty() {
            bail!("repository URL is required");
        }
        let destination_path = destination_path.to_path_buf();
        if destination_path.exists() {
            if !destination_path.is_dir() {
                bail!("'{}' is not a directory", destination_path.display());
            }
            let mut entries = fs::read_dir(&destination_path)
                .with_context(|| format!("failed to inspect '{}'", destination_path.display()))?;
            if entries.next().is_some() {
                bail!(
                    "'{}' already exists and is not empty",
                    destination_path.display()
                );
            }
        } else if let Some(parent) = destination_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create '{}'", parent.display()))?;
        }

        let parent = destination_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let destination = destination_path.to_string_lossy().to_string();
        run_git_command(
            parent,
            &["clone", "--recursive", "--", url, destination.as_str()],
        )
        .with_context(|| format!("failed to clone '{url}'"))?;

        self.snapshot(&destination_path)
    }

    pub fn clone_repository_into(
        &self,
        url: &str,
        parent_path: &Path,
        local_name: &str,
    ) -> Result<RepoSnapshot> {
        let url = url.trim();
        if url.is_empty() {
            bail!("repository URL is required");
        }
        let local_name = safe_repository_directory_name(local_name);
        if local_name.is_empty() {
            bail!("local repository name is required");
        }
        let parent_path = parent_path.to_path_buf();
        if parent_path.exists() {
            if !parent_path.is_dir() {
                bail!("'{}' is not a directory", parent_path.display());
            }
        } else {
            fs::create_dir_all(&parent_path)
                .with_context(|| format!("failed to create '{}'", parent_path.display()))?;
        }

        let destination_path = parent_path.join(local_name);
        if destination_path.exists() {
            if !destination_path.is_dir() {
                bail!("'{}' is not a directory", destination_path.display());
            }
            let mut entries = fs::read_dir(&destination_path)
                .with_context(|| format!("failed to inspect '{}'", destination_path.display()))?;
            if entries.next().is_some() {
                bail!(
                    "'{}' already exists and is not empty",
                    destination_path.display()
                );
            }
        }

        let destination = destination_path.to_string_lossy().to_string();
        run_git_command(
            &parent_path,
            &["clone", "--recursive", "--", url, destination.as_str()],
        )
        .with_context(|| format!("failed to clone '{url}'"))?;

        self.snapshot(&destination_path)
    }

    pub fn get_commit_diff(&self, repo_path: &Path, oid: &str) -> Result<Vec<DiffEntry>> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let oid = self.verify_commit_oid(&repo_path, oid)?;

        // Use diff-tree to get raw list of changed files
        let output = self.run_git(
            &repo_path,
            &[
                "diff-tree",
                "--root",
                "--no-commit-id",
                "--name-only",
                "-r",
                &oid,
            ],
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

            let submodule = submodule_diff_metadata(&diff_output);
            let is_image = path_is_supported_image(&file);
            let is_binary = !submodule.is_submodule && looks_binary_diff(&diff_output);

            diffs.push(DiffEntry {
                path: file,
                diff: diff_output,
                is_binary,
                is_image,
                is_submodule: submodule.is_submodule,
                submodule_old_oid: submodule.old_oid,
                submodule_new_oid: submodule.new_oid,
                ..Default::default()
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

    pub fn read_watch_fingerprint(&self, path: &Path) -> Result<String> {
        let repo_path = self.resolve_repo_root(path)?;
        let status = self.run_git(
            &repo_path,
            &[
                "status",
                "--porcelain=v2",
                "-b",
                "--ignore-submodules=dirty",
            ],
        )?;
        let stash_count = self.stash_count(&repo_path).unwrap_or(0);
        Ok(format!("{status}\n__stash_count={stash_count}"))
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
            self.run_git(&repo_path, &["push", "--follow-tags", &remote_name])
                .with_context(|| format!("failed to push to '{remote_name}'"))?;
        } else {
            self.run_git(
                &repo_path,
                &[
                    "push",
                    "--follow-tags",
                    "--set-upstream",
                    &remote_name,
                    "HEAD",
                ],
            )
            .with_context(|| format!("failed to publish branch to '{remote_name}'"))?;
        }

        self.snapshot(&repo_path)
    }

    pub fn publish_repository(
        &self,
        repo_path: &Path,
        name: &str,
        description: &str,
        private: bool,
    ) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        if name.trim().is_empty() {
            bail!("repository name is required");
        }
        if self.read_primary_remote(&repo_path)?.is_some() {
            bail!("repository already has a remote configured");
        }

        let mut command = Command::new("gh");
        command
            .arg("repo")
            .arg("create")
            .arg(name.trim())
            .arg("--source")
            .arg(&repo_path)
            .arg("--remote")
            .arg("origin")
            .arg("--push")
            .arg(if private { "--private" } else { "--public" })
            .current_dir(&repo_path);
        if !description.trim().is_empty() {
            command.arg("--description").arg(description.trim());
        }

        #[cfg(windows)]
        {
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let output = command.output().with_context(|| {
            format!(
                "failed to launch gh while publishing '{}'",
                repo_path.display()
            )
        })?;
        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let message = if !stderr.is_empty() {
                stderr
            } else if !stdout.is_empty() {
                stdout
            } else {
                format!("gh exited with status {}", output.status)
            };
            bail!("gh repo create failed: {message}");
        }

        self.snapshot(&repo_path)
    }

    pub fn stash_all(&self, repo_path: &Path) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let branch_name = self.current_stash_branch_name(&repo_path)?;
        let previous_stash_shas = self
            .list_stashes(&repo_path)?
            .into_iter()
            .filter(|stash| stash.is_gitspark_stash_for(&branch_name))
            .map(|stash| stash.sha)
            .collect::<Vec<_>>();
        let message = format!("{GITSPARK_STASH_MESSAGE_PREFIX}{branch_name}");
        self.run_git(&repo_path, &["stash", "push", "-u", "-m", message.as_str()])
            .context("failed to stash changes")?;

        for sha in previous_stash_shas {
            self.drop_stash_by_sha(&repo_path, &sha)?;
        }

        self.snapshot(&repo_path)
    }

    pub fn stash_pop(&self, repo_path: &Path) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let stash_ref = self.latest_gitspark_stash_ref_for_current_branch(&repo_path)?;
        self.run_git(&repo_path, &["stash", "pop", stash_ref.as_str()])
            .context("failed to pop stash")?;
        self.snapshot(&repo_path)
    }

    pub fn stash_drop(&self, repo_path: &Path) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let stash_ref = self.latest_gitspark_stash_ref_for_current_branch(&repo_path)?;
        self.run_git(&repo_path, &["stash", "drop", stash_ref.as_str()])
            .context("failed to drop stash")?;
        self.snapshot(&repo_path)
    }

    // ------------------------------------------------------------------
    // Worktrees
    // ------------------------------------------------------------------

    /// List every worktree attached to this repository.
    ///
    /// `is_current` is resolved against the *canonicalized* path: git prints
    /// real paths, while the app may hold one that traverses a symlink — on
    /// macOS `/tmp` is a symlink to `/private/tmp`, so comparing the raw
    /// strings marks nothing current.
    pub fn list_worktrees(&self, repo_path: &Path) -> Result<Vec<WorktreeInfo>> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let output = self
            .run_git(&repo_path, &["worktree", "list", "--porcelain"])
            .context("failed to list worktrees")?;
        let current = std::fs::canonicalize(&repo_path).unwrap_or(repo_path);
        Ok(parse_worktree_list(&output, &current))
    }

    /// Create a worktree at `path`, letting git name the branch.
    ///
    /// `git worktree add <path>` with no commit-ish and no `-b` creates a new
    /// branch named after the directory, based on HEAD — that is git's own
    /// documented convenience, not a guess we are making, which is why the UI
    /// can offer "pick a folder" as the whole interaction.
    pub fn add_worktree_at(&self, repo_path: &Path, path: &Path) -> Result<Vec<WorktreeInfo>> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let path_arg = path.to_string_lossy().to_string();
        self.run_git(&repo_path, &["worktree", "add", path_arg.as_str()])
            .context("failed to add worktree")?;
        self.list_worktrees(&repo_path)
    }

    /// Create a worktree at `path`.
    ///
    /// `create_branch` maps to `-b`. Git refuses to check out a branch that is
    /// already checked out in another worktree, so an existing branch can only
    /// be added once — the error is surfaced, not swallowed.
    #[allow(dead_code)]
    pub fn add_worktree(
        &self,
        repo_path: &Path,
        path: &Path,
        branch: &str,
        create_branch: bool,
    ) -> Result<Vec<WorktreeInfo>> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let path_arg = path.to_string_lossy().to_string();
        let mut args = vec!["worktree", "add"];
        if create_branch {
            args.extend_from_slice(&["-b", branch, path_arg.as_str()]);
        } else {
            args.extend_from_slice(&[path_arg.as_str(), branch]);
        }
        self.run_git(&repo_path, &args)
            .context("failed to add worktree")?;
        self.list_worktrees(&repo_path)
    }

    /// Remove a worktree. `force` discards uncommitted changes inside it.
    #[allow(dead_code)]
    pub fn remove_worktree(
        &self,
        repo_path: &Path,
        path: &Path,
        force: bool,
    ) -> Result<Vec<WorktreeInfo>> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let path_arg = path.to_string_lossy().to_string();
        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        args.push(path_arg.as_str());
        self.run_git(&repo_path, &args)
            .context("failed to remove worktree")?;
        self.list_worktrees(&repo_path)
    }

    /// Drop administrative entries for worktrees whose directory is gone.
    pub fn prune_worktrees(&self, repo_path: &Path) -> Result<Vec<WorktreeInfo>> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        self.run_git(&repo_path, &["worktree", "prune"])
            .context("failed to prune worktrees")?;
        self.list_worktrees(&repo_path)
    }

    pub fn latest_stash_files(&self, repo_path: &Path) -> Result<Vec<ChangeEntry>> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let stash_ref = self.latest_gitspark_stash_ref_for_current_branch(&repo_path)?;
        let output = self.run_git(
            &repo_path,
            &[
                "stash",
                "show",
                "--name-status",
                "--include-untracked",
                "--format=",
                stash_ref.as_str(),
            ],
        )?;

        Ok(output.lines().filter_map(parse_name_status_line).collect())
    }

    pub fn undo_last_commit(&self, repo_path: &Path) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        self.run_git(&repo_path, &["reset", "--soft", "HEAD~1"])
            .context("failed to undo last commit")?;
        self.snapshot(&repo_path)
    }

    pub fn checkout_commit(&self, repo_path: &Path, oid: &str) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let oid = self.verify_commit_oid(&repo_path, oid)?;

        self.run_git(&repo_path, &["switch", "--detach", &oid])
            .with_context(|| format!("failed to check out commit '{oid}'"))?;

        self.snapshot(&repo_path)
    }

    pub fn compare_current_branch_with(
        &self,
        repo_path: &Path,
        target_branch: &str,
    ) -> Result<BranchComparison> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let target_branch = target_branch.trim();
        if target_branch.is_empty() {
            bail!("branch name cannot be empty");
        }

        let current_branch = self.current_stash_branch_name(&repo_path)?;
        if current_branch == target_branch {
            bail!("cannot compare a branch with itself");
        }

        self.run_git(
            &repo_path,
            &[
                "rev-parse",
                "--verify",
                &format!("refs/heads/{target_branch}^{{commit}}"),
            ],
        )
        .with_context(|| format!("branch '{target_branch}' does not exist"))?;

        let range = format!("{target_branch}...HEAD");
        let counts = self
            .run_git(&repo_path, &["rev-list", "--left-right", "--count", &range])
            .with_context(|| format!("failed to compare with branch '{target_branch}'"))?;
        let mut parts = counts.split_whitespace();
        let behind = parts
            .next()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        let ahead = parts
            .next()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        let detail_revision = if behind > 0 {
            format!("HEAD..{target_branch}")
        } else {
            format!("{target_branch}..HEAD")
        };
        let commits = self.fetch_history_for_revision(&repo_path, &detail_revision, 100)?;
        let detail_range = if behind > 0 {
            format!("HEAD...{target_branch}")
        } else {
            range
        };
        let status_output = self
            .run_git(&repo_path, &["diff", "--name-status", &detail_range])
            .with_context(|| format!("failed to list files changed against '{target_branch}'"))?;
        let files = status_output
            .lines()
            .filter_map(parse_name_status_line)
            .collect::<Vec<_>>();
        let diffs = self.build_compare_diffs(&repo_path, &detail_range, &files)?;

        Ok(BranchComparison {
            current_branch,
            target_branch: target_branch.to_string(),
            ahead,
            behind,
            commits,
            diffs,
        })
    }

    pub fn delete_branch(&self, repo_path: &Path, branch_name: &str) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let branch_name = branch_name.trim();
        if branch_name.is_empty() {
            bail!("branch name cannot be empty");
        }
        self.run_git(&repo_path, &["branch", "-d", branch_name])
            .with_context(|| format!("failed to delete branch '{branch_name}'"))?;
        self.snapshot(&repo_path)
    }

    pub fn delete_branch_from_current_worktree(
        &self,
        repo_path: &Path,
        branch_name: &str,
    ) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let branch_name = branch_name.trim();
        if branch_name.is_empty() {
            bail!("branch name cannot be empty");
        }

        let current = self
            .run_git(&repo_path, &["branch", "--show-current"])
            .context("failed to read current branch")?;
        if current.trim() == branch_name {
            let fallback = self
                .list_branches(&repo_path)?
                .into_iter()
                .find(|branch| !branch.is_remote && branch.name != branch_name)
                .map(|branch| branch.name)
                .with_context(|| format!("cannot delete the only local branch '{branch_name}'"))?;
            self.run_git(&repo_path, &["switch", &fallback])
                .with_context(|| {
                    format!("failed to switch to '{fallback}' before deleting '{branch_name}'")
                })?;
        }

        self.delete_branch(&repo_path, branch_name)
    }

    pub fn rename_branch(
        &self,
        repo_path: &Path,
        old_name: &str,
        new_name: &str,
    ) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let old_name = old_name.trim();
        let new_name = new_name.trim();
        if old_name.is_empty() {
            bail!("branch name cannot be empty");
        }
        if new_name.is_empty() {
            bail!("new branch name cannot be empty");
        }
        if old_name == new_name {
            return self.snapshot(&repo_path);
        }

        self.run_git(&repo_path, &["branch", "-m", old_name, new_name])
            .with_context(|| format!("failed to rename branch '{old_name}' to '{new_name}'"))?;
        self.snapshot(&repo_path)
    }

    pub fn create_tag(&self, repo_path: &Path, oid: &str, tag_name: &str) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let oid = self.verify_commit_oid(&repo_path, oid)?;
        let tag_name = tag_name.trim();
        if tag_name.is_empty() {
            bail!("tag name cannot be empty");
        }

        self.run_git(&repo_path, &["tag", "-a", "-m", "", tag_name, &oid])
            .with_context(|| format!("failed to create tag '{tag_name}'"))?;
        self.snapshot(&repo_path)
    }

    pub fn delete_tag(&self, repo_path: &Path, tag_name: &str) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let tag_name = tag_name.trim();
        if tag_name.is_empty() {
            bail!("tag name cannot be empty");
        }

        self.run_git(&repo_path, &["tag", "-d", tag_name])
            .with_context(|| format!("failed to delete tag '{tag_name}'"))?;
        self.snapshot(&repo_path)
    }

    pub fn create_branch(&self, repo_path: &Path, branch_name: &str) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let branch_name = branch_name.trim();
        if branch_name.is_empty() {
            bail!("branch name cannot be empty");
        }
        self.run_git(&repo_path, &["switch", "-c", branch_name])
            .with_context(|| format!("failed to create branch '{branch_name}'"))?;
        self.snapshot(&repo_path)
    }

    pub fn create_branch_from_commit(
        &self,
        repo_path: &Path,
        branch_name: &str,
        oid: &str,
    ) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let branch_name = branch_name.trim();
        if branch_name.is_empty() {
            bail!("branch name cannot be empty");
        }
        let oid = self.verify_commit_oid(&repo_path, oid)?;
        self.run_git(&repo_path, &["switch", "-c", branch_name, &oid])
            .with_context(|| {
                format!("failed to create branch '{branch_name}' from commit '{oid}'")
            })?;
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

    pub fn revert_commit(&self, repo_path: &Path, oid: &str) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let oid = self.verify_commit_oid(&repo_path, oid)?;

        self.run_git(&repo_path, &["revert", "--no-edit", &oid])
            .with_context(|| format!("failed to revert commit '{oid}'"))?;

        self.snapshot(&repo_path)
    }

    pub fn reset_to_commit(&self, repo_path: &Path, oid: &str) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let oid = self.verify_commit_oid(&repo_path, oid)?;

        self.run_git(&repo_path, &["reset", &oid])
            .with_context(|| format!("failed to reset to commit '{oid}'"))?;

        self.snapshot(&repo_path)
    }

    #[allow(dead_code)]
    pub fn cherry_pick_commit(&self, repo_path: &Path, oid: &str) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let oid = self.verify_commit_oid(&repo_path, oid)?;

        self.run_git(&repo_path, &["cherry-pick", &oid])
            .with_context(|| format!("failed to cherry-pick commit '{oid}'"))?;

        self.snapshot(&repo_path)
    }

    pub fn cherry_pick_commit_onto_branch(
        &self,
        repo_path: &Path,
        oid: &str,
        branch_name: &str,
    ) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let oid = self.verify_commit_oid(&repo_path, oid)?;
        let branch_name = branch_name.trim();
        if branch_name.is_empty() {
            bail!("cherry-pick target branch cannot be empty");
        }

        self.switch_branch(&repo_path, branch_name)?;
        self.run_git(&repo_path, &["cherry-pick", &oid])
            .with_context(|| format!("failed to cherry-pick commit '{oid}'"))?;

        self.snapshot(&repo_path)
    }

    pub fn merge_branch(&self, repo_path: &Path, branch_name: &str) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let branch_name = branch_name.trim();
        if branch_name.is_empty() {
            bail!("merge target cannot be empty");
        }
        self.ensure_merge_preflight_clean(&repo_path, branch_name, "merge")?;

        self.run_git(&repo_path, &["merge", "--no-ff", branch_name])
            .with_context(|| format!("failed to merge branch '{branch_name}'"))?;

        self.snapshot(&repo_path)
    }

    pub fn continue_merge(&self, repo_path: &Path) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let Some(operation) = self.operation_state(&repo_path)? else {
            bail!("no merge is in progress");
        };
        if operation.kind != GitOperationKind::Merge {
            bail!("no merge is in progress");
        }
        if !operation.conflicted_files.is_empty() {
            bail!("resolve all conflicted files before continuing the merge");
        }

        self.run_git(&repo_path, &["commit", "--no-edit"])
            .context("failed to complete merge commit")?;

        self.snapshot(&repo_path)
    }

    pub fn abort_merge(&self, repo_path: &Path) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        self.run_git(&repo_path, &["merge", "--abort"])
            .context("failed to abort merge")?;

        self.snapshot(&repo_path)
    }

    pub fn update_current_branch_from(
        &self,
        repo_path: &Path,
        default_branch: &str,
    ) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let default_branch = default_branch.trim();
        if default_branch.is_empty() {
            bail!("default branch cannot be empty");
        }

        let current_branch = self.current_stash_branch_name(&repo_path)?;
        if current_branch == default_branch {
            bail!("current branch is already '{default_branch}'");
        }

        self.run_git(
            &repo_path,
            &[
                "rev-parse",
                "--verify",
                &format!("refs/heads/{default_branch}^{{commit}}"),
            ],
        )
        .with_context(|| format!("default branch '{default_branch}' does not exist"))?;
        self.ensure_merge_preflight_clean(&repo_path, default_branch, "update")?;

        self.run_git(&repo_path, &["merge", "--no-ff", default_branch])
            .with_context(|| format!("failed to update from '{default_branch}'"))?;

        self.snapshot(&repo_path)
    }

    pub fn rebase_current_branch_onto(
        &self,
        repo_path: &Path,
        target_branch: &str,
    ) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let target_branch = target_branch.trim();
        if target_branch.is_empty() {
            bail!("rebase target cannot be empty");
        }

        let current_branch = self.current_stash_branch_name(&repo_path)?;
        if current_branch == target_branch {
            bail!("cannot rebase a branch onto itself");
        }

        self.run_git(
            &repo_path,
            &[
                "rev-parse",
                "--verify",
                &format!("refs/heads/{target_branch}^{{commit}}"),
            ],
        )
        .with_context(|| format!("branch '{target_branch}' does not exist"))?;
        self.ensure_rebase_preflight_clean(&repo_path, target_branch)?;

        self.run_git(&repo_path, &["rebase", target_branch])
            .with_context(|| format!("failed to rebase onto '{target_branch}'"))?;

        self.snapshot(&repo_path)
    }

    pub fn continue_rebase(&self, repo_path: &Path) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        self.run_git(
            &repo_path,
            &["-c", "core.editor=true", "rebase", "--continue"],
        )
        .context("failed to continue rebase")?;

        self.snapshot(&repo_path)
    }

    pub fn skip_rebase(&self, repo_path: &Path) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        self.run_git(&repo_path, &["rebase", "--skip"])
            .context("failed to skip rebase commit")?;

        self.snapshot(&repo_path)
    }

    pub fn abort_rebase(&self, repo_path: &Path) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        self.run_git(&repo_path, &["rebase", "--abort"])
            .context("failed to abort rebase")?;

        self.snapshot(&repo_path)
    }

    pub fn mark_conflict_resolved(
        &self,
        repo_path: &Path,
        relative_path: &str,
    ) -> Result<Option<GitOperationState>> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let relative_path = relative_path.trim();
        if relative_path.is_empty() {
            bail!("conflict path cannot be empty");
        }
        if self.operation_state(&repo_path)?.is_none() {
            bail!("no merge or rebase is in progress");
        }

        self.run_git(&repo_path, &["add", "--", relative_path])
            .with_context(|| format!("failed to mark '{relative_path}' resolved"))?;
        self.operation_state(&repo_path)
    }

    pub fn operation_state(&self, repo_path: &Path) -> Result<Option<GitOperationState>> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let git_dir = self.git_dir(&repo_path)?;
        let current_branch = self
            .current_stash_branch_name(&repo_path)
            .unwrap_or_default();
        let conflicted_files = self.conflicted_files(&repo_path)?;
        let can_continue = conflicted_files.is_empty();

        if git_dir.join("MERGE_HEAD").exists() {
            let target_branch = self.merge_target_branch(&repo_path).ok().flatten();
            let message = if can_continue {
                "All conflicts are resolved. Continue the merge to create the merge commit."
                    .to_string()
            } else {
                format!(
                    "Resolve {} conflicted file{} before continuing.",
                    conflicted_files.len(),
                    if conflicted_files.len() == 1 { "" } else { "s" }
                )
            };
            return Ok(Some(GitOperationState {
                kind: GitOperationKind::Merge,
                current_branch,
                target_branch,
                conflicted_files,
                can_continue,
                message,
            }));
        }

        let rebase_dir = if git_dir.join("rebase-merge").exists() {
            Some(git_dir.join("rebase-merge"))
        } else if git_dir.join("rebase-apply").exists() {
            Some(git_dir.join("rebase-apply"))
        } else {
            None
        };

        if let Some(rebase_dir) = rebase_dir {
            let target_branch = self
                .read_git_state_file(&rebase_dir.join("onto_name"))
                .map(clean_git_ref_name);
            let current_branch = self
                .read_git_state_file(&rebase_dir.join("head-name"))
                .map(clean_git_ref_name)
                .filter(|name| !name.is_empty())
                .unwrap_or(current_branch);
            let message = if can_continue {
                "All conflicts are resolved. Continue, skip this commit, or abort the rebase."
                    .to_string()
            } else {
                format!(
                    "Resolve {} conflicted file{} before continuing or skipping.",
                    conflicted_files.len(),
                    if conflicted_files.len() == 1 { "" } else { "s" }
                )
            };
            return Ok(Some(GitOperationState {
                kind: GitOperationKind::Rebase,
                current_branch,
                target_branch,
                conflicted_files,
                can_continue,
                message,
            }));
        }

        Ok(None)
    }

    pub fn github_commit_url(&self, repo_path: &Path, oid: &str) -> Result<Option<String>> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let oid = self.verify_commit_oid(&repo_path, oid)?;
        let Some(remote_name) = self.read_primary_remote(&repo_path)? else {
            return Ok(None);
        };

        let remote_url = self
            .run_git_remote_url(&repo_path, &remote_name)
            .with_context(|| format!("failed to read remote URL for '{remote_name}'"))?;

        Ok(normalize_github_remote_url(remote_url.trim())
            .map(|base| format!("{base}/commit/{oid}")))
    }

    pub fn github_branch_url(&self, repo_path: &Path, branch_name: &str) -> Result<Option<String>> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let branch_name = branch_name.trim();
        if branch_name.is_empty() {
            bail!("branch name cannot be empty");
        }
        let Some(remote_name) = self.read_primary_remote(&repo_path)? else {
            return Ok(None);
        };

        let remote_url = self
            .run_git_remote_url(&repo_path, &remote_name)
            .with_context(|| format!("failed to read remote URL for '{remote_name}'"))?;

        Ok(normalize_github_remote_url(remote_url.trim())
            .map(|base| format!("{base}/tree/{}", encode_github_url_component(branch_name))))
    }

    pub fn github_compare_branch_url(
        &self,
        repo_path: &Path,
        branch_name: &str,
    ) -> Result<Option<String>> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let branch_name = branch_name.trim();
        if branch_name.is_empty() {
            bail!("branch name cannot be empty");
        }
        let Some(remote_name) = self.read_primary_remote(&repo_path)? else {
            return Ok(None);
        };

        let remote_url = self
            .run_git_remote_url(&repo_path, &remote_name)
            .with_context(|| format!("failed to read remote URL for '{remote_name}'"))?;

        Ok(normalize_github_remote_url(remote_url.trim()).map(|base| {
            format!(
                "{base}/compare/{}",
                encode_github_url_component(branch_name)
            )
        }))
    }

    pub fn github_repository_url(&self, repo_path: &Path) -> Result<Option<String>> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let Some(remote_name) = self.read_primary_remote(&repo_path)? else {
            return Ok(None);
        };

        let remote_url = self
            .run_git_remote_url(&repo_path, &remote_name)
            .with_context(|| format!("failed to read remote URL for '{remote_name}'"))?;

        Ok(normalize_github_remote_url(remote_url.trim()))
    }

    pub fn primary_remote(&self, repo_path: &Path) -> Result<Option<(String, String)>> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let Some(remote_name) = self.read_primary_remote(&repo_path)? else {
            return Ok(None);
        };

        let remote_url = self
            .run_git_remote_url(&repo_path, &remote_name)
            .with_context(|| format!("failed to read remote URL for '{remote_name}'"))?;

        Ok(Some((remote_name, remote_url.trim().to_string())))
    }

    pub fn set_remote_url(
        &self,
        repo_path: &Path,
        remote_name: &str,
        remote_url: &str,
    ) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let remote_name = remote_name.trim();
        let remote_url = remote_url.trim();
        if remote_name.is_empty() {
            bail!("remote name cannot be empty");
        }
        if remote_url.is_empty() {
            bail!("remote URL cannot be empty");
        }

        self.run_git(&repo_path, &["remote", "set-url", remote_name, remote_url])
            .with_context(|| format!("failed to set URL for remote '{remote_name}'"))?;

        self.snapshot(&repo_path)
    }

    pub fn github_file_url(&self, repo_path: &Path, relative_path: &str) -> Result<Option<String>> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let relative_path = relative_path.trim();
        if relative_path.is_empty() {
            bail!("file path cannot be empty");
        }

        let Some(remote_name) = self.read_primary_remote(&repo_path)? else {
            return Ok(None);
        };

        let remote_url = self
            .run_git_remote_url(&repo_path, &remote_name)
            .with_context(|| format!("failed to read remote URL for '{remote_name}'"))?;
        let branch = self
            .run_git(&repo_path, &["branch", "--show-current"])
            .context("failed to read current branch")?;
        let branch = branch.trim();
        if branch.is_empty() {
            bail!("cannot build file URL while HEAD is detached");
        }

        Ok(normalize_github_remote_url(remote_url.trim()).map(|base| {
            format!(
                "{base}/blob/{}/{}",
                encode_github_url_component(branch),
                encode_github_path(relative_path)
            )
        }))
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

    pub fn commit_paths(
        &self,
        repo_path: &Path,
        paths: &[String],
        message: &str,
    ) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let message = message.trim();
        if message.is_empty() {
            bail!("commit message cannot be empty");
        }
        if paths.is_empty() {
            bail!("no files selected for commit");
        }

        self.run_git(&repo_path, &["reset", "--mixed", "--quiet", "HEAD"])
            .context("failed to reset staged changes before committing selected files")?;

        let mut add_args = vec!["add", "--all", "--"];
        add_args.extend(paths.iter().map(String::as_str));
        self.run_git(&repo_path, &add_args)
            .context("failed to stage selected files")?;
        self.run_git(&repo_path, &["commit", "-m", message])
            .context("failed to create commit")?;

        self.snapshot(&repo_path)
    }

    pub fn commit_paths_with_path_content(
        &self,
        repo_path: &Path,
        paths: Option<&[String]>,
        relative_path: &str,
        content: &str,
        message: &str,
    ) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let relative_path = relative_path.trim();
        let message = message.trim();
        if relative_path.is_empty() {
            bail!("file path cannot be empty");
        }
        if message.is_empty() {
            bail!("commit message cannot be empty");
        }

        self.run_git(&repo_path, &["reset", "--mixed", "--quiet", "HEAD"])
            .context("failed to reset staged changes before committing included lines")?;

        if let Some(paths) = paths {
            let paths: Vec<&str> = paths
                .iter()
                .map(String::as_str)
                .filter(|path| *path != relative_path)
                .collect();
            if !paths.is_empty() {
                let mut add_args = vec!["add", "--all", "--"];
                add_args.extend(paths);
                self.run_git(&repo_path, &add_args)
                    .context("failed to stage selected files")?;
            }
        } else {
            self.run_git(&repo_path, &["add", "--all"])
                .context("failed to stage repository changes")?;
        }

        self.stage_path_content(&repo_path, relative_path, content)?;
        self.run_git(&repo_path, &["commit", "-m", message])
            .context("failed to create commit")?;

        self.snapshot(&repo_path)
    }

    fn stage_path_content(
        &self,
        repo_path: &Path,
        relative_path: &str,
        content: &str,
    ) -> Result<()> {
        let mode = self.index_mode_for_path(repo_path, relative_path)?;
        let temp_path = partial_blob_temp_path(repo_path, relative_path);
        fs::write(&temp_path, content)
            .with_context(|| format!("failed to write temporary blob '{}'", temp_path.display()))?;
        let blob = self
            .run_git(
                repo_path,
                &[
                    "hash-object",
                    "-w",
                    "--path",
                    relative_path,
                    temp_path.to_string_lossy().as_ref(),
                ],
            )
            .map(|blob| blob.trim().to_string());
        let _ = fs::remove_file(&temp_path);
        let blob = blob.context("failed to write included lines to the git object database")?;

        self.run_git(
            repo_path,
            &["update-index", "--cacheinfo", &mode, &blob, relative_path],
        )
        .with_context(|| format!("failed to stage included lines for '{relative_path}'"))?;

        Ok(())
    }

    pub fn head_file_text(&self, repo_path: &Path, relative_path: &str) -> Result<String> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let relative_path = relative_path.trim();
        if relative_path.is_empty() {
            bail!("file path cannot be empty");
        }
        self.run_git(&repo_path, &["show", &format!("HEAD:{relative_path}")])
            .with_context(|| format!("failed to read '{relative_path}' from HEAD"))
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

    pub fn read_gitignore(&self, repo_path: &Path) -> Result<String> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let gitignore_path = repo_path.join(".gitignore");
        if !gitignore_path.exists() {
            return Ok(String::new());
        }

        fs::read_to_string(&gitignore_path)
            .with_context(|| format!("failed to read '{}'", gitignore_path.display()))
    }

    pub fn write_gitignore(&self, repo_path: &Path, text: &str) -> Result<RepoSnapshot> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let gitignore_path = repo_path.join(".gitignore");

        if text.is_empty() {
            match fs::remove_file(&gitignore_path) {
                Ok(()) => {}
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => {
                    return Err(err).with_context(|| {
                        format!("failed to remove '{}'", gitignore_path.display())
                    });
                }
            }
        } else {
            fs::write(&gitignore_path, text)
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

        let identity = GitIdentity {
            user_name: self.read_optional_config(&repo_path, "user.name")?,
            user_email: self.read_optional_config(&repo_path, "user.email")?,
            pull_rebase: self.read_optional_bool_config(&repo_path, "pull.rebase")?,
            default_branch: non_empty(self.read_optional_config(&repo_path, "init.defaultBranch")?),
        };

        Ok(identity)
    }

    pub fn read_local_identity(&self, repo_path: &Path) -> Result<GitIdentity> {
        let repo_path = self.resolve_repo_root(repo_path)?;

        Ok(GitIdentity {
            user_name: self.read_optional_local_config(&repo_path, "user.name")?,
            user_email: self.read_optional_local_config(&repo_path, "user.email")?,
            pull_rebase: self.read_optional_local_bool_config(&repo_path, "pull.rebase")?,
            default_branch: non_empty(
                self.read_optional_local_config(&repo_path, "init.defaultBranch")?,
            ),
        })
    }

    pub fn read_global_identity(&self) -> Result<GitIdentity> {
        Ok(GitIdentity {
            user_name: self.read_optional_global_config("user.name")?,
            user_email: self.read_optional_global_config("user.email")?,
            pull_rebase: self.read_optional_global_bool_config("pull.rebase")?,
            default_branch: non_empty(self.read_optional_global_config("init.defaultBranch")?),
        })
    }

    pub fn write_global_identity(&self, identity: &GitIdentity) -> Result<()> {
        self.write_global_string_config("user.name", &identity.user_name)?;
        self.write_global_string_config("user.email", &identity.user_email)?;
        self.write_global_default_branch(identity.default_branch.as_deref())?;

        Ok(())
    }

    pub fn write_global_default_branch(&self, branch: Option<&str>) -> Result<()> {
        self.write_optional_global_string_config("init.defaultBranch", branch)
    }

    pub fn write_identity(&self, repo_path: &Path, identity: &GitIdentity) -> Result<()> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        self.write_optional_string_config(
            &repo_path,
            "user.name",
            non_empty(identity.user_name.clone()).as_deref(),
        )?;
        self.write_optional_string_config(
            &repo_path,
            "user.email",
            non_empty(identity.user_email.clone()).as_deref(),
        )?;
        self.write_optional_string_config(
            &repo_path,
            "init.defaultBranch",
            identity.default_branch.as_deref(),
        )?;

        Ok(())
    }

    pub fn clear_local_author_identity(&self, repo_path: &Path) -> Result<()> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        self.write_optional_string_config(&repo_path, "user.name", None)?;
        self.write_optional_string_config(&repo_path, "user.email", None)?;
        Ok(())
    }

    pub fn write_pull_rebase(&self, repo_path: &Path, value: Option<bool>) -> Result<()> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        self.write_bool_config(&repo_path, "pull.rebase", value)
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
        let tags = self.list_tags(repo_path).unwrap_or_default();
        let stash_count = self.stash_count(repo_path).unwrap_or(0);
        let remote_name = self.read_primary_remote(repo_path).unwrap_or(None);
        let has_github_remote = remote_name
            .as_deref()
            .and_then(|remote| {
                self.run_git_remote_url(repo_path, remote)
                    .ok()
                    .and_then(|url| normalize_github_remote_url(url.trim()))
            })
            .is_some();
        let last_fetched = self.read_last_fetched(repo_path);
        // Listed here, on the worker thread, so the UI never blocks for it.
        let worktrees = self.list_worktrees(repo_path).unwrap_or_default();

        Ok(RepoSnapshot {
            worktrees,
            repo: RepoSummary {
                path: repo_path.to_path_buf(),
                name: repo_name,
                current_branch: status.current_branch,
                head_oid: status.head_oid,
                remote_name,
                has_github_remote,
                ahead: status.ahead,
                behind: status.behind,
                last_fetched,
            },
            changes: status.changes,
            diffs,
            branches,
            history,
            tags,
            stash_count,
        })
    }

    fn list_tags(&self, repo_path: &Path) -> Result<Vec<String>> {
        let output = self.run_git(repo_path, &["tag", "--list"])?;
        Ok(output
            .lines()
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(ToOwned::to_owned)
            .collect())
    }

    fn fetch_history(&self, repo_path: &Path, limit: usize) -> Result<Vec<CommitInfo>> {
        // Only HEAD goes through gix; `fetch_history_for_revision` takes an
        // arbitrary revspec, which gix would have to parse identically to git
        // for no benefit — that path is not hot.
        if let Some(history) = crate::gitoxide::history(repo_path, limit) {
            return Ok(history);
        }
        self.fetch_history_for_revision(repo_path, "HEAD", limit)
    }

    fn fetch_history_for_revision(
        &self,
        repo_path: &Path,
        revision: &str,
        limit: usize,
    ) -> Result<Vec<CommitInfo>> {
        let output = self.run_git_bytes(
            repo_path,
            &[
                "log",
                &format!("-n{limit}"),
                revision,
                "--pretty=format:%x1e%H%x1f%h%x1f%s%x1f%b%x1f%an%x1f%ae%x1f%ar%x1f%D",
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
            if chunk.len() < 7 {
                continue;
            }

            // Parse tags from %D (ref decorations like "HEAD -> main, tag: v0.3.0, origin/main")
            let tags = chunk
                .get(7)
                .map_or_else(Vec::new, |refs| parse_ref_tags(refs));

            commits.push(CommitInfo {
                oid: chunk[0].trim().to_string(),
                short_oid: chunk[1].trim().to_string(),
                summary: chunk[2].trim().to_string(),
                body: chunk[3].trim_end().to_string(),
                author_name: chunk[4].trim().to_string(),
                author_email: chunk[5].trim().to_string(),
                date: chunk[6].trim().to_string(),
                is_head: false,
                tags,
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
        let branch_name = self.current_stash_branch_name(repo_path)?;
        Ok(self
            .list_stashes(repo_path)?
            .into_iter()
            .filter(|stash| stash.is_gitspark_stash_for(&branch_name))
            .count())
    }

    fn latest_gitspark_stash_ref_for_current_branch(&self, repo_path: &Path) -> Result<String> {
        let branch_name = self.current_stash_branch_name(repo_path)?;
        self.list_stashes(repo_path)?
            .into_iter()
            .find(|stash| stash.is_gitspark_stash_for(&branch_name))
            .map(|stash| stash.ref_name)
            .with_context(|| format!("no GitSpark stash found for branch '{branch_name}'"))
    }

    fn current_stash_branch_name(&self, repo_path: &Path) -> Result<String> {
        let branch_name = self.read_status(repo_path)?.current_branch;
        Ok(if branch_name.trim().is_empty() {
            "HEAD".to_string()
        } else {
            branch_name
        })
    }

    fn list_stashes(&self, repo_path: &Path) -> Result<Vec<StashEntry>> {
        let output = self.run_git(repo_path, &["stash", "list", "--format=%gd%x1f%H%x1f%s"])?;
        Ok(output
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(3, '\u{1f}');
                let ref_name = parts.next()?.trim();
                let sha = parts.next()?.trim();
                let subject = parts.next()?.trim();
                let ref_name = ref_name.trim();
                if ref_name.is_empty() || sha.is_empty() {
                    return None;
                }
                Some(StashEntry {
                    ref_name: ref_name.to_string(),
                    sha: sha.to_string(),
                    subject: subject.to_string(),
                })
            })
            .collect())
    }

    fn drop_stash_by_sha(&self, repo_path: &Path, sha: &str) -> Result<()> {
        let Some(stash) = self
            .list_stashes(repo_path)?
            .into_iter()
            .find(|stash| stash.sha == sha)
        else {
            return Ok(());
        };

        self.run_git(repo_path, &["stash", "drop", stash.ref_name.as_str()])
            .map(|_| ())
            .with_context(|| format!("failed to drop previous stash '{}'", stash.ref_name))
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

        // Nearly every operation on this client resolves the root before it
        // does anything else, so this shell-out was ~10ms of pure overhead
        // added to each one. gix answers in ~0.15ms.
        if let Some(root) = crate::gitoxide::repo_root(candidate) {
            return Ok(root);
        }

        let output = self
            .run_git(candidate, &["rev-parse", "--show-toplevel"])
            .with_context(|| format!("'{}' is not a Git repository", candidate.display()))?;

        Ok(PathBuf::from(output.trim()))
    }

    fn has_upstream(&self, repo_path: &Path) -> bool {
        self.run_git(
            repo_path,
            &[
                "rev-parse",
                "--abbrev-ref",
                "--symbolic-full-name",
                "@{upstream}",
            ],
        )
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
    }

    /// A remote's fetch URL, preferring gix and falling back to the binary.
    ///
    /// Returns the raw string with a trailing newline the way `run_git` does,
    /// because every caller trims it and changing that would be a silent
    /// behaviour change in six places.
    fn run_git_remote_url(&self, repo_path: &Path, remote: &str) -> Result<String> {
        if let Some(url) = crate::gitoxide::remote_url(repo_path, remote) {
            return Ok(url);
        }
        self.run_git(repo_path, &["remote", "get-url", remote])
    }

    fn read_primary_remote(&self, repo_path: &Path) -> Result<Option<String>> {
        if let Some(remote) = crate::gitoxide::primary_remote(repo_path) {
            return Ok(remote);
        }

        if let Ok(upstream) = self.run_git(
            repo_path,
            &[
                "rev-parse",
                "--abbrev-ref",
                "--symbolic-full-name",
                "@{upstream}",
            ],
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

    fn git_dir(&self, repo_path: &Path) -> Result<PathBuf> {
        let output = self.run_git(repo_path, &["rev-parse", "--git-dir"])?;
        let git_dir = PathBuf::from(output.trim());
        if git_dir.is_absolute() {
            Ok(git_dir)
        } else {
            Ok(repo_path.join(git_dir))
        }
    }

    fn read_git_state_file(&self, path: &Path) -> Option<String> {
        fs::read_to_string(path)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }

    fn conflicted_files(&self, repo_path: &Path) -> Result<Vec<ChangeEntry>> {
        let output = self.run_git(
            repo_path,
            &["diff", "--name-status", "--diff-filter=U", "--"],
        )?;
        Ok(output.lines().filter_map(parse_name_status_line).collect())
    }

    fn merge_target_branch(&self, repo_path: &Path) -> Result<Option<String>> {
        let git_dir = self.git_dir(repo_path)?;
        let Some(merge_head) = self.read_git_state_file(&git_dir.join("MERGE_HEAD")) else {
            return Ok(None);
        };
        let output = self.run_git(
            repo_path,
            &["name-rev", "--name-only", "--exclude=tags/*", &merge_head],
        )?;
        let name = output.trim();
        if name.is_empty() || name == "undefined" {
            Ok(None)
        } else {
            Ok(Some(clean_git_ref_name(name.to_string())))
        }
    }

    fn ensure_merge_preflight_clean(
        &self,
        repo_path: &Path,
        branch_name: &str,
        action: &str,
    ) -> Result<()> {
        let conflicts = self.merge_tree_conflicted_files(repo_path, "HEAD", branch_name)?;
        if conflicts.is_empty() {
            return Ok(());
        }

        bail!(
            "{} would conflict in {}. Resolve or merge manually before continuing.",
            action,
            summarize_paths(&conflicts)
        )
    }

    fn ensure_rebase_preflight_clean(&self, repo_path: &Path, target_branch: &str) -> Result<()> {
        let commits = self.run_git(
            repo_path,
            &["rev-list", "--reverse", &format!("{target_branch}..HEAD")],
        )?;
        let mut conflicts = Vec::new();
        for commit in commits.lines().filter(|line| !line.trim().is_empty()) {
            conflicts.extend(self.merge_tree_conflicted_files(repo_path, target_branch, commit)?);
        }
        conflicts.sort();
        conflicts.dedup();
        if conflicts.is_empty() {
            return Ok(());
        }

        bail!(
            "rebase would conflict in {}. Resolve or rebase manually before continuing.",
            summarize_paths(&conflicts)
        )
    }

    fn merge_tree_conflicted_files(
        &self,
        repo_path: &Path,
        left: &str,
        right: &str,
    ) -> Result<Vec<String>> {
        let output = Command::new("git")
            .args(["merge-tree", "--write-tree", "--name-only", left, right])
            .current_dir(repo_path)
            .output()
            .with_context(|| {
                format!(
                    "failed to launch git merge-tree in '{}'",
                    repo_path.display()
                )
            })?;

        if output.status.success() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut conflicts = Vec::new();
        for (ix, line) in stdout.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                break;
            }
            if ix == 0 && line.chars().all(|ch| ch.is_ascii_hexdigit()) && line.len() >= 40 {
                continue;
            }
            conflicts.push(line.to_string());
        }

        if conflicts.is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let message = if stderr.is_empty() {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            } else {
                stderr
            };
            bail!("git merge-tree failed: {message}");
        }

        Ok(conflicts)
    }

    fn list_branches(&self, repo_path: &Path) -> Result<Vec<BranchInfo>> {
        if let Some(branches) = crate::gitoxide::branches(repo_path) {
            return Ok(branches);
        }

        let output = self.run_git(
            repo_path,
            &[
                "for-each-ref",
                "--format=%(refname:short)\t%(HEAD)\t%(refname)\t%(committerdate:relative)",
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
                let updated = parts
                    .next()
                    .map(str::trim)
                    .filter(|value| !value.is_empty());

                if name.is_empty() || name.ends_with("/HEAD") {
                    return None;
                }

                Some(BranchInfo {
                    name: name.to_string(),
                    is_current: head == "*",
                    is_remote: full_ref.starts_with("refs/remotes/"),
                    updated: updated.map(str::to_string),
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

    /// Build a diff for a single file path (used for on-click refresh).
    pub fn get_file_diff(&self, repo_path: &Path, file_path: &str) -> Result<DiffEntry> {
        let repo_path = self.resolve_repo_root(repo_path)?;
        let change = ChangeEntry {
            path: file_path.to_string(),
            status: String::new(),
        };
        self.build_diff_entry(&repo_path, &change)
    }

    fn build_diffs(&self, repo_path: &Path, changes: &[ChangeEntry]) -> Result<Vec<DiffEntry>> {
        // One `git diff` for every path instead of two per path. A twenty-file
        // change set used to spawn forty subprocesses at roughly 10ms each,
        // just in process startup, before any diffing happened.
        let paths: Vec<&str> = changes.iter().map(|change| change.path.as_str()).collect();
        let staged = self.batch_diff(repo_path, &paths, true).unwrap_or_default();
        let unstaged = self
            .batch_diff(repo_path, &paths, false)
            .unwrap_or_default();

        changes
            .iter()
            .map(|change| {
                self.build_diff_entry_from(
                    repo_path,
                    change,
                    staged.get(change.path.as_str()).cloned(),
                    unstaged.get(change.path.as_str()).cloned(),
                )
            })
            .collect()
    }

    /// Run one `git diff` across `paths` and split the result per file.
    ///
    /// Returns `None` if the command failed, so the caller falls back to
    /// per-file calls rather than silently reporting empty diffs.
    fn batch_diff(
        &self,
        repo_path: &Path,
        paths: &[&str],
        cached: bool,
    ) -> Option<HashMap<String, String>> {
        if paths.is_empty() {
            return Some(HashMap::new());
        }

        // `core.quotepath=off` keeps non-ASCII paths literal in the `diff --git`
        // header, which is what the splitter matches against.
        let mut args: Vec<&str> = vec![
            "-c",
            "core.quotepath=off",
            "diff",
            "--no-ext-diff",
            "--no-color",
        ];
        if cached {
            args.push("--cached");
        }
        args.push("--");
        args.extend_from_slice(paths);

        let output = self.run_git(repo_path, &args).ok()?;
        let (mut sections, unattributed) = split_combined_diff(&output, paths);
        if unattributed > 0 {
            // A header we could not tie to a requested path means the output
            // is not safely splittable — bail to per-file rather than risk
            // showing one file's diff under another's name.
            return None;
        }
        // Every requested path is now accounted for: a missing section means
        // there genuinely is no diff of this kind for that file. Recording it
        // as empty is what stops the caller re-running a per-file `git diff`
        // for each of them, which was the whole point of batching.
        for path in paths {
            sections.entry((*path).to_string()).or_default();
        }
        Some(sections)
    }

    fn build_compare_diffs(
        &self,
        repo_path: &Path,
        range: &str,
        changes: &[ChangeEntry],
    ) -> Result<Vec<DiffEntry>> {
        changes
            .iter()
            .map(|change| {
                let diff = self.run_git(
                    repo_path,
                    &[
                        "diff",
                        "--no-ext-diff",
                        "--no-color",
                        range,
                        "--",
                        &change.path,
                    ],
                )?;
                let is_image = path_is_supported_image(&change.path);
                let submodule = submodule_diff_metadata(&diff);
                let is_binary = !submodule.is_submodule && looks_binary_diff(&diff);
                Ok(DiffEntry {
                    path: change.path.clone(),
                    diff: if diff.trim().is_empty() {
                        "No textual diff available".to_string()
                    } else {
                        diff
                    },
                    is_binary,
                    is_image,
                    is_submodule: submodule.is_submodule,
                    submodule_old_oid: submodule.old_oid,
                    submodule_new_oid: submodule.new_oid,
                    original_diff: None,
                    file_contents: None,
                })
            })
            .collect()
    }

    fn build_diff_entry(&self, repo_path: &Path, change: &ChangeEntry) -> Result<DiffEntry> {
        self.build_diff_entry_from(repo_path, change, None, None)
    }

    /// Build a diff entry, reusing already-fetched patch text when the caller
    /// has it.
    ///
    /// `None` means "not fetched", not "empty" — the per-file `git diff` runs
    /// in that case. That distinction is what lets the batched path fall back
    /// safely when the splitter cannot attribute a section to a path.
    fn build_diff_entry_from(
        &self,
        repo_path: &Path,
        change: &ChangeEntry,
        staged: Option<String>,
        unstaged: Option<String>,
    ) -> Result<DiffEntry> {
        let staged = match staged {
            Some(text) => text,
            None => self.run_git(
                repo_path,
                &[
                    "diff",
                    "--no-ext-diff",
                    "--no-color",
                    "--cached",
                    "--",
                    &change.path,
                ],
            )?,
        };
        let unstaged = match unstaged {
            Some(text) => text,
            None => self.run_git(
                repo_path,
                &["diff", "--no-ext-diff", "--no-color", "--", &change.path],
            )?,
        };

        let mut sections = Vec::new();
        if !staged.trim().is_empty() {
            sections.push(("Staged", staged));
        }
        if !unstaged.trim().is_empty() {
            sections.push(("Working tree", unstaged));
        }

        if sections.is_empty() {
            // No staged or unstaged diff — file might be untracked or entirely new
            let is_untracked = change.status == "??"
                || !self
                    .path_is_tracked(repo_path, &change.path)
                    .unwrap_or(true);
            if is_untracked {
                return self.build_untracked_diff(repo_path, &change.path);
            }
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

        let mut submodule = submodule_diff_metadata(&combined);
        if !submodule.is_submodule {
            submodule.is_submodule = self
                .path_is_submodule(repo_path, &change.path)
                .unwrap_or(false);
        }

        let is_image = path_is_supported_image(&change.path);
        let is_binary = !submodule.is_submodule
            && (looks_binary_diff(&combined)
                || self
                    .path_is_binary(repo_path, &change.path)
                    .unwrap_or(false));

        let diff_text = if combined.trim().is_empty() {
            "No textual diff available".to_string()
        } else if is_binary && looks_binary_diff(&combined) {
            "Binary file changed".to_string()
        } else {
            combined
        };

        // Load file contents for in-memory hunk expansion
        let file_contents = if !is_binary {
            let full_path = repo_path.join(&change.path);
            fs::read_to_string(&full_path)
                .ok()
                .map(|c| c.lines().map(String::from).collect())
        } else {
            None
        };

        Ok(DiffEntry {
            path: change.path.clone(),
            diff: diff_text,
            is_binary,
            is_image,
            is_submodule: submodule.is_submodule,
            submodule_old_oid: submodule.old_oid,
            submodule_new_oid: submodule.new_oid,
            original_diff: None,
            file_contents,
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
                is_image: path_is_supported_image(relative_path),
                ..Default::default()
            });
        }

        let contents = String::from_utf8(bytes).context("failed to decode file contents")?;

        // The hunk header must describe the body we actually emit. It used to
        // report the file's FULL line count while the body was truncated,
        // which produces a malformed patch — and the app's own parser reads
        // `new_count` from that header to decide how far the file extends, so
        // an inflated count also made it offer expand controls for lines that
        // were never in the diff.
        let emitted: Vec<&str> = contents.lines().take(UNTRACKED_DIFF_MAX_LINES).collect();
        let line_count = emitted.len().max(1);
        let body = emitted
            .iter()
            .map(|line| format!("+{line}"))
            .collect::<Vec<_>>()
            .join("\n");

        let diff =
            format!("--- /dev/null\n+++ b/{relative_path}\n@@ -0,0 +1,{line_count} @@\n{body}");

        Ok(DiffEntry {
            path: relative_path.to_string(),
            diff,
            is_binary: false,
            is_image: path_is_supported_image(relative_path),
            ..Default::default()
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

    fn path_is_submodule(&self, repo_path: &Path, relative_path: &str) -> Result<bool> {
        let output = self.run_git(repo_path, &["ls-files", "--stage", "--", relative_path])?;
        Ok(output
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .any(|mode| mode == "160000"))
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
        // Read from all sources (local > global > system) so user.name/email from
        // global config are shown when not overridden locally.
        match self.run_git(repo_path, &["config", "--get", key]) {
            Ok(value) => Ok(value.trim().to_string()),
            Err(error) if is_config_missing(&error) => Ok(String::new()),
            Err(error) => Err(error).with_context(|| format!("failed reading config '{key}'")),
        }
    }

    fn read_optional_local_config(&self, repo_path: &Path, key: &str) -> Result<String> {
        match self.run_git(repo_path, &["config", "--local", "--get", key]) {
            Ok(value) => Ok(value.trim().to_string()),
            Err(error) if is_config_missing(&error) => Ok(String::new()),
            Err(error) => {
                Err(error).with_context(|| format!("failed reading local config '{key}'"))
            }
        }
    }

    fn read_optional_global_config(&self, key: &str) -> Result<String> {
        match run_git_global(&["config", "--global", "--get", key]) {
            Ok(value) => Ok(value.trim().to_string()),
            Err(error) if is_config_missing(&error) => Ok(String::new()),
            Err(error) => {
                Err(error).with_context(|| format!("failed reading global config '{key}'"))
            }
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

    fn read_optional_local_bool_config(&self, repo_path: &Path, key: &str) -> Result<Option<bool>> {
        let value = self.read_optional_local_config(repo_path, key)?;
        if value.is_empty() {
            return Ok(None);
        }

        parse_git_bool(&value)
            .map(Some)
            .with_context(|| format!("invalid local boolean value for '{key}': '{value}'"))
    }

    fn read_optional_global_bool_config(&self, key: &str) -> Result<Option<bool>> {
        let value = self.read_optional_global_config(key)?;
        if value.is_empty() {
            return Ok(None);
        }

        parse_git_bool(&value)
            .map(Some)
            .with_context(|| format!("invalid global boolean value for '{key}': '{value}'"))
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
            None => {
                if self
                    .run_git(repo_path, &["config", "--local", "--get", key])
                    .is_err()
                {
                    return Ok(());
                }

                self.run_git(repo_path, &["config", "--local", "--unset", key])
                    .map(|_| ())
                    .with_context(|| format!("failed clearing config '{key}'"))
            }
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

    fn write_global_string_config(&self, key: &str, value: &str) -> Result<()> {
        self.write_optional_global_string_config(key, non_empty(value.to_string()).as_deref())
    }

    fn write_optional_global_string_config(&self, key: &str, value: Option<&str>) -> Result<()> {
        match value {
            Some(value) => run_git_global(&["config", "--global", key, value])
                .map(|_| ())
                .with_context(|| format!("failed writing global config '{key}'")),
            None => {
                if run_git_global(&["config", "--global", "--get", key]).is_err() {
                    return Ok(());
                }

                run_git_global(&["config", "--global", "--unset", key])
                    .map(|_| ())
                    .with_context(|| format!("failed clearing global config '{key}'"))
            }
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

    fn index_mode_for_path(&self, repo_path: &Path, relative_path: &str) -> Result<String> {
        let output = self.run_git(repo_path, &["ls-files", "--stage", "--", relative_path])?;
        let mode = output
            .lines()
            .find_map(|line| line.split_whitespace().next())
            .filter(|mode| !mode.is_empty())
            .ok_or_else(|| anyhow!("'{relative_path}' is not tracked"))?;
        Ok(mode.to_string())
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

/// Whether to log every git invocation. Read once — this is on the hot path.
///
/// `GITSPARK_TRACE_GIT=1` prints duration, thread, and args for every shell-out.
/// `MAIN` in the output means it blocked the UI thread, which is the single
/// most useful thing this can tell you: git subprocesses cost ~10ms each, so
/// anything recurring on MAIN is a dropped frame.
fn trace_git() -> bool {
    static TRACE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *TRACE.get_or_init(|| std::env::var("GITSPARK_TRACE_GIT").is_ok())
}

fn run_git_command(repo_path: &Path, args: &[&str]) -> Result<Output> {
    let started = trace_git().then(std::time::Instant::now);
    let mut command = Command::new("git");
    // Every read (status, log, diff, branch listing...) otherwise opportunistically
    // refreshes and rewrites the on-disk index, which briefly takes index.lock. The
    // watcher runs one of these within ~400ms of almost any change in the repo,
    // including a commit the user just ran themselves in a terminal — the two would
    // race for the lock, and whichever lost failed with "Unable to create
    // '.git/index.lock': File exists". This flag skips that optional refresh; the
    // writes GitSpark itself performs (commit, add, checkout...) still take the
    // locks they actually need, unaffected.
    command.arg("--no-optional-locks");
    command.args(args).current_dir(repo_path);

    #[cfg(windows)]
    {
        // Release builds are GUI subsystem apps on Windows, so console git children
        // would otherwise create their own terminal windows.
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = command.output().with_context(|| {
        format!(
            "failed to launch git in '{}' with args {:?}",
            repo_path.display(),
            args
        )
    })?;

    if let Some(started) = started {
        let thread = std::thread::current();
        eprintln!(
            "[git] {:>7.1}ms {} {}",
            started.elapsed().as_secs_f64() * 1000.0,
            if thread.name() == Some("main") {
                "MAIN"
            } else {
                "bg  "
            },
            args.join(" ")
        );
    }
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

fn partial_blob_temp_path(repo_path: &Path, relative_path: &str) -> PathBuf {
    let slug = relative_path
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>();
    env::temp_dir().join(format!(
        "gitspark-partial-{}-{}-{}",
        process::id(),
        slug,
        repo_path
            .to_string_lossy()
            .chars()
            .map(|ch| ch as u64)
            .fold(0u64, |acc, value| acc.wrapping_mul(31).wrapping_add(value))
    ))
}

fn run_git_global(args: &[&str]) -> Result<String> {
    let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let output = run_git_command(&current_dir, args)?;
    String::from_utf8(output.stdout).context("git output was not valid UTF-8")
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

fn normalize_github_remote_url(remote_url: &str) -> Option<String> {
    let remote_url = remote_url.trim();
    if remote_url.is_empty() {
        return None;
    }

    let (host, repository) = remote_url
        .strip_prefix("https://")
        .or_else(|| remote_url.strip_prefix("http://"))
        .and_then(split_remote_host_and_path)
        .or_else(|| {
            remote_url
                .strip_prefix("git://")
                .and_then(split_remote_host_and_path)
        })
        .or_else(|| {
            remote_url
                .strip_prefix("ssh://")
                .and_then(split_remote_host_and_path)
                .map(|(host, repository)| (strip_remote_port(host), repository))
        })
        .or_else(|| {
            let (host, repository) = remote_url.split_once(':')?;
            if host.contains('/') {
                return None;
            }
            Some((strip_remote_user(host), repository))
        })?;

    let host = strip_remote_user(host).trim();
    let repository = repository
        .split(['?', '#'])
        .next()
        .unwrap_or(repository)
        .trim_end_matches(".git")
        .trim_matches('/');
    if host.is_empty() || repository.is_empty() || !is_github_remote_host(host) {
        None
    } else {
        Some(format!("https://{host}/{repository}"))
    }
}

fn is_github_remote_host(host: &str) -> bool {
    let host = strip_remote_port(host).to_ascii_lowercase();
    host == "github.com" || host.starts_with("github.") || host.contains(".github.")
}

fn split_remote_host_and_path(remote_url: &str) -> Option<(&str, &str)> {
    let (host, path) = remote_url.split_once('/')?;
    Some((strip_remote_user(host), path))
}

fn strip_remote_user(host: &str) -> &str {
    host.rsplit_once('@').map(|(_, host)| host).unwrap_or(host)
}

fn strip_remote_port(host: &str) -> &str {
    host.split_once(':').map(|(host, _)| host).unwrap_or(host)
}

fn encode_github_path(path: &str) -> String {
    path.split('/')
        .map(encode_github_url_component)
        .collect::<Vec<_>>()
        .join("/")
}

fn encode_github_url_component(component: &str) -> String {
    let mut encoded = String::new();
    for byte in component.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn parse_git_bool(value: &str) -> Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Ok(true),
        "false" | "no" | "off" | "0" => Ok(false),
        other => bail!("unsupported git boolean '{other}'"),
    }
}

/// Parse `git worktree list --porcelain`.
///
/// Blank-line-separated records of `key value`, or a bare key for flags:
///
/// ```text
/// worktree /Users/x/proj
/// HEAD 4b49bed…
/// branch refs/heads/dev
///
/// worktree /Users/x/.wt/proj-master
/// HEAD 0eb22df…
/// detached
/// ```
///
/// The first record with a working directory is the primary worktree. A bare
/// repository emits a record with none; those are skipped, because there is
/// nothing there for the app to open.
/// Split a multi-file `git diff` into one patch per path.
///
/// Sections start at `diff --git a/<old> b/<new>`. Rather than parse the path
/// out of that header — which has to cope with spaces, `b/` appearing inside a
/// filename, and renames where the two halves differ — this matches the header
/// against the paths we ASKED for. Returns the sections plus a count of
/// headers it could NOT attribute — the caller treats a non-zero count as
/// "this output is not trustworthy" and falls back to per-file diffs, so an
/// odd filename costs subprocesses rather than a missing diff.
///
/// A requested path with no section is NOT a failure: a file changed only in
/// the working tree has no staged diff, which is the common case.
fn split_combined_diff(output: &str, paths: &[&str]) -> (HashMap<String, String>, usize) {
    let mut sections: HashMap<String, String> = HashMap::new();
    let mut current: Option<(String, Vec<&str>)> = None;
    let mut unattributed = 0usize;

    // Longest first: with both `src/a.rs` and `a.rs` requested, a header
    // ending in `b/src/a.rs` must not be attributed to `a.rs`.
    let mut candidates: Vec<&&str> = paths.iter().collect();
    candidates.sort_by_key(|path| std::cmp::Reverse(path.len()));

    let flush = |sections: &mut HashMap<String, String>, current: Option<(String, Vec<&str>)>| {
        if let Some((path, lines)) = current {
            // Trailing newline included: git's own per-file output ends with
            // one, and the batched result has to be byte-identical or the two
            // paths produce subtly different diff text for the same file.
            let mut text = lines.join("\n");
            text.push('\n');
            sections.insert(path, text);
        }
    };

    for line in output.lines() {
        if line.starts_with("diff --git ") {
            flush(&mut sections, current.take());
            // A rename writes a different b-path; matching on the suffix finds
            // the destination, which is the path the change set names.
            let matched = candidates
                .iter()
                .find(|path| line.ends_with(&format!(" b/{path}")))
                .map(|path| (**path).to_string());
            match matched {
                Some(path) => current = Some((path, vec![line])),
                None => {
                    unattributed += 1;
                    current = None;
                }
            }
            continue;
        }
        if let Some((_, lines)) = current.as_mut() {
            lines.push(line);
        }
    }
    flush(&mut sections, current.take());
    (sections, unattributed)
}

fn parse_worktree_list(output: &str, current: &Path) -> Vec<WorktreeInfo> {
    let mut worktrees: Vec<WorktreeInfo> = Vec::new();
    let mut entry: Option<WorktreeInfo> = None;
    let mut is_bare = false;

    fn flush(
        worktrees: &mut Vec<WorktreeInfo>,
        entry: &mut Option<WorktreeInfo>,
        is_bare: &mut bool,
        current: &Path,
    ) {
        if let Some(mut worktree) = entry.take() {
            if !*is_bare {
                worktree.is_main = worktrees.is_empty();
                worktree.is_current = std::fs::canonicalize(&worktree.path)
                    .as_deref()
                    .unwrap_or(worktree.path.as_path())
                    == current;
                worktrees.push(worktree);
            }
        }
        *is_bare = false;
    }

    for line in output.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            flush(&mut worktrees, &mut entry, &mut is_bare, current);
            continue;
        }
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        match key {
            "worktree" => {
                flush(&mut worktrees, &mut entry, &mut is_bare, current);
                let path = PathBuf::from(value);
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| value.to_string());
                entry = Some(WorktreeInfo {
                    path,
                    name,
                    ..WorktreeInfo::default()
                });
            }
            "bare" => is_bare = true,
            "branch" => {
                if let Some(worktree) = entry.as_mut() {
                    worktree.branch = Some(
                        value
                            .strip_prefix("refs/heads/")
                            .unwrap_or(value)
                            .to_string(),
                    );
                }
            }
            "detached" => {
                if let Some(worktree) = entry.as_mut() {
                    worktree.is_detached = true;
                }
            }
            // `locked` may carry a reason after the key; only the flag matters.
            "locked" => {
                if let Some(worktree) = entry.as_mut() {
                    worktree.is_locked = true;
                }
            }
            _ => {}
        }
    }
    flush(&mut worktrees, &mut entry, &mut is_bare, current);
    worktrees
}

fn parse_ref_tags(refs: &str) -> Vec<String> {
    refs.split(", ")
        .filter_map(|ref_name| ref_name.strip_prefix("tag: "))
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub(crate) fn safe_repository_directory_name(name: &str) -> String {
    name.trim()
        .chars()
        .map(|ch| {
            if ch <= '\u{1f}' || matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            {
                '-'
            } else {
                ch
            }
        })
        .collect::<String>()
        .trim_matches(|ch| matches!(ch, ' ' | '.'))
        .to_string()
}

pub(crate) fn inferred_clone_directory_name(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/').trim_end_matches('\\');
    let last = trimmed
        .rsplit(['/', '\\', ':'])
        .next()
        .unwrap_or(trimmed)
        .trim_end_matches(".git");
    safe_repository_directory_name(last)
}

fn safe_branch_name(name: &str) -> String {
    name.trim()
        .chars()
        .map(|ch| if ch.is_whitespace() { '-' } else { ch })
        .filter(|ch| !matches!(ch, '~' | '^' | ':' | '?' | '*' | '[' | '\\'))
        .collect::<String>()
        .trim_matches(|ch| matches!(ch, '/' | '.'))
        .to_string()
}

fn summarize_paths(paths: &[String]) -> String {
    match paths {
        [] => "unknown files".to_string(),
        [path] => format!("'{path}'"),
        [first, second] => format!("'{first}' and '{second}'"),
        [first, second, rest @ ..] => {
            format!("'{first}', '{second}', and {} more files", rest.len())
        }
    }
}

fn gitignore_template_contents(template: &str) -> Option<&'static str> {
    match template {
        "Rust" => Some("/target/\nCargo.lock\n"),
        "Node" => Some("node_modules/\nnpm-debug.log*\nyarn-debug.log*\nyarn-error.log*\n.env\n"),
        "Python" => Some("__pycache__/\n*.py[cod]\n.venv/\n.env\n"),
        _ => None,
    }
}

fn license_template_contents(template: &str, project_name: &str) -> Option<String> {
    match template {
        "MIT" => Some(format!(
            "MIT License\n\nCopyright (c) {project_name}\n\nPermission is hereby granted, free of charge, to any person obtaining a copy\nof this software and associated documentation files (the \"Software\"), to deal\nin the Software without restriction, including without limitation the rights\nto use, copy, modify, merge, publish, distribute, sublicense, and/or sell\ncopies of the Software, and to permit persons to whom the Software is\nfurnished to do so, subject to the following conditions:\n\nThe above copyright notice and this permission notice shall be included in all\ncopies or substantial portions of the Software.\n\nTHE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR\nIMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,\nFITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE\nAUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER\nLIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,\nOUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE\nSOFTWARE.\n"
        )),
        "Apache-2.0" => Some(format!(
            "Copyright {project_name}\n\nLicensed under the Apache License, Version 2.0 (the \"License\");\nyou may not use this file except in compliance with the License.\nYou may obtain a copy of the License at\n\n    http://www.apache.org/licenses/LICENSE-2.0\n\nUnless required by applicable law or agreed to in writing, software\ndistributed under the License is distributed on an \"AS IS\" BASIS,\nWITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.\nSee the License for the specific language governing permissions and\nlimitations under the License.\n"
        )),
        "GPL-3.0" => Some(format!(
            "{project_name}\n\nCopyright (C) {project_name}\n\nThis program is free software: you can redistribute it and/or modify it under\nthe terms of the GNU General Public License as published by the Free Software\nFoundation, either version 3 of the License, or (at your option) any later\nversion.\n\nThis program is distributed in the hope that it will be useful, but WITHOUT\nANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS\nFOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.\n"
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, path::Path};

    use crate::models::{CreateRepositoryOptions, GitIdentity};

    use super::{
        GITSPARK_STASH_MESSAGE_PREFIX, GitClient, UNTRACKED_DIFF_MAX_LINES, encode_github_path,
        fill_missing_author_identity, inferred_clone_directory_name, normalize_github_remote_url,
        parse_author_ident, parse_ref_tags, parse_worktree_list, safe_repository_directory_name,
        split_combined_diff,
    };

    #[test]
    fn normalizes_github_remotes_with_owner_and_repo() {
        let cases = [
            (
                "https://github.com/JacobSamro/GitSpark.git",
                "https://github.com/JacobSamro/GitSpark",
            ),
            (
                "git@github.com:JacobSamro/GitSpark.git",
                "https://github.com/JacobSamro/GitSpark",
            ),
            (
                "ssh://git@github.com/JacobSamro/GitSpark.git",
                "https://github.com/JacobSamro/GitSpark",
            ),
            (
                "https://github.enterprise.local/JacobSamro/GitSpark.git",
                "https://github.enterprise.local/JacobSamro/GitSpark",
            ),
            (
                "https://github.enterprise.local:8443/JacobSamro/GitSpark.git",
                "https://github.enterprise.local:8443/JacobSamro/GitSpark",
            ),
            (
                "git@github.enterprise.local:JacobSamro/GitSpark.git",
                "https://github.enterprise.local/JacobSamro/GitSpark",
            ),
            (
                "ssh://git@github.enterprise.local:2222/JacobSamro/GitSpark.git",
                "https://github.enterprise.local/JacobSamro/GitSpark",
            ),
        ];

        for (input, expected) in cases {
            assert_eq!(
                normalize_github_remote_url(input).as_deref(),
                Some(expected)
            );
        }
    }

    #[test]
    fn rejects_non_github_remote_hosts_for_github_urls() {
        for input in [
            "https://gitlab.com/owner/repo.git",
            "git@gitlab.com:owner/repo.git",
            "https://bitbucket.org/owner/repo.git",
            "ssh://git@git.internal.local/owner/repo.git",
        ] {
            assert_eq!(normalize_github_remote_url(input), None);
        }
    }

    #[test]
    fn encodes_github_blob_paths_per_segment() {
        assert_eq!(
            encode_github_path("dashboards/platform/page one.rs"),
            "dashboards/platform/page%20one.rs"
        );
    }

    #[test]
    fn builds_github_branch_urls_with_encoded_branch_names() {
        let remote = temp_repo("github-branch-url-remote");
        run_git(&remote, &["init", "--bare"]);

        let repo = temp_repo("github-branch-url");
        fs::write(repo.join("README.md"), "one\n").unwrap();
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "GitSpark Test"]);
        run_git(&repo, &["config", "user.email", "test@gitspark.local"]);
        run_git(
            &repo,
            &["remote", "add", "origin", "git@github.com:owner/repo.git"],
        );
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "initial"]);

        let git = GitClient::new();
        assert_eq!(
            git.github_branch_url(&repo, "feature/test branch")
                .unwrap()
                .as_deref(),
            Some("https://github.com/owner/repo/tree/feature%2Ftest%20branch")
        );
        assert_eq!(
            git.github_compare_branch_url(&repo, "feature/test branch")
                .unwrap()
                .as_deref(),
            Some("https://github.com/owner/repo/compare/feature%2Ftest%20branch")
        );

        let _ = fs::remove_dir_all(repo);
        let _ = fs::remove_dir_all(remote);
    }

    #[test]
    fn builds_github_urls_against_enterprise_hosts() {
        let repo = temp_repo("github-enterprise-urls");
        fs::write(repo.join("README.md"), "one\n").unwrap();
        fs::create_dir_all(repo.join("src")).unwrap();
        fs::write(repo.join("src/file name.rs"), "fn main() {}\n").unwrap();
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "GitSpark Test"]);
        run_git(&repo, &["config", "user.email", "test@gitspark.local"]);
        run_git(
            &repo,
            &[
                "remote",
                "add",
                "origin",
                "git@github.enterprise.local:owner/repo.git",
            ],
        );
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "initial"]);
        let oid = run_git(&repo, &["rev-parse", "HEAD"]).trim().to_string();

        let git = GitClient::new();
        assert_eq!(
            git.github_repository_url(&repo).unwrap().as_deref(),
            Some("https://github.enterprise.local/owner/repo")
        );
        assert_eq!(
            git.github_commit_url(&repo, &oid).unwrap().as_deref(),
            Some(format!("https://github.enterprise.local/owner/repo/commit/{oid}").as_str())
        );
        assert_eq!(
            git.github_branch_url(&repo, "feature/test branch")
                .unwrap()
                .as_deref(),
            Some("https://github.enterprise.local/owner/repo/tree/feature%2Ftest%20branch")
        );
        assert_eq!(
            git.github_compare_branch_url(&repo, "feature/test branch")
                .unwrap()
                .as_deref(),
            Some("https://github.enterprise.local/owner/repo/compare/feature%2Ftest%20branch")
        );
        assert_eq!(
            git.github_file_url(&repo, "src/file name.rs")
                .unwrap()
                .as_deref(),
            Some("https://github.enterprise.local/owner/repo/blob/main/src/file%20name.rs")
        );

        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn sanitizes_repository_directory_names() {
        assert_eq!(safe_repository_directory_name("My Repo"), "My Repo");
        assert_eq!(safe_repository_directory_name(" bad/repo? "), "bad-repo-");
        assert_eq!(safe_repository_directory_name("..."), "");
        assert_eq!(
            inferred_clone_directory_name("git@github.com:owner/project.git"),
            "project"
        );
    }

    #[test]
    fn creates_local_repository_with_readme_and_description() {
        let parent = temp_repo("create-parent");
        let snapshot = GitClient::new()
            .create_repository(&parent, "New Repo", "Local test repository")
            .unwrap();
        let repo = parent.join("New Repo");

        assert_eq!(snapshot.repo.path, repo);
        assert_eq!(snapshot.repo.name, "New Repo");
        assert!(repo.join(".git").is_dir());
        assert_eq!(
            fs::read_to_string(repo.join("README.md")).unwrap(),
            "# New Repo\n"
        );
        assert_eq!(
            fs::read_to_string(repo.join(".git").join("description")).unwrap(),
            "Local test repository\n"
        );

        let err = GitClient::new()
            .create_repository(&parent, "New Repo", "")
            .unwrap_err()
            .to_string();
        assert!(err.contains("already exists and is not empty"));

        let _ = fs::remove_dir_all(parent);
    }

    #[test]
    fn creates_repository_with_templates_branch_and_initial_commit() {
        let parent = temp_repo("create-options-parent");
        let snapshot = GitClient::new()
            .create_repository_with_options(
                &parent,
                CreateRepositoryOptions {
                    name: "Templated Repo".to_string(),
                    description: "Repository options".to_string(),
                    branch_name: "trunk".to_string(),
                    initialize_readme: true,
                    gitignore_template: "Rust".to_string(),
                    license_template: "MIT".to_string(),
                    initial_commit: true,
                },
            )
            .unwrap();
        let repo = parent.join("Templated Repo");

        assert_eq!(snapshot.repo.current_branch, "trunk");
        assert!(snapshot.repo.head_oid.is_some());
        assert_eq!(
            fs::read_to_string(repo.join(".gitignore")).unwrap(),
            "/target/\nCargo.lock\n"
        );
        assert!(
            fs::read_to_string(repo.join("LICENSE"))
                .unwrap()
                .contains("MIT License")
        );
        let committed_files = run_git(&repo, &["show", "--format=", "--name-only", "HEAD"]);
        assert!(committed_files.contains("README.md"));
        assert!(committed_files.contains(".gitignore"));
        assert!(committed_files.contains("LICENSE"));

        let _ = fs::remove_dir_all(parent);
    }

    #[test]
    fn clones_local_repository_into_empty_destination() {
        let source = temp_repo("clone-source");
        fs::write(source.join("README.md"), "source\n").unwrap();
        run_git(&source, &["init", "-b", "main"]);
        run_git(&source, &["config", "user.name", "GitSpark Test"]);
        run_git(&source, &["config", "user.email", "test@gitspark.local"]);
        run_git(&source, &["add", "--all"]);
        run_git(&source, &["commit", "-m", "initial"]);

        let parent = temp_repo("clone-parent");
        let destination = parent.join("cloned");
        let snapshot = GitClient::new()
            .clone_repository(&source.to_string_lossy(), &destination)
            .unwrap();

        assert_eq!(snapshot.repo.path, destination);
        assert_eq!(snapshot.repo.name, "cloned");
        assert_eq!(
            fs::read_to_string(destination.join("README.md")).unwrap(),
            "source\n"
        );
        assert_eq!(snapshot.history.len(), 1);

        let err = GitClient::new()
            .clone_repository(&source.to_string_lossy(), &destination)
            .unwrap_err()
            .to_string();
        assert!(err.contains("already exists and is not empty"));

        let _ = fs::remove_dir_all(source);
        let _ = fs::remove_dir_all(parent);
    }

    #[test]
    fn clones_local_repository_into_named_child_of_parent_folder() {
        let source = temp_repo("clone-parent-source");
        fs::write(source.join("README.md"), "source\n").unwrap();
        run_git(&source, &["init", "-b", "main"]);
        run_git(&source, &["config", "user.name", "GitSpark Test"]);
        run_git(&source, &["config", "user.email", "test@gitspark.local"]);
        run_git(&source, &["add", "--all"]);
        run_git(&source, &["commit", "-m", "initial"]);

        let parent = temp_repo("clone-parent-folder");
        fs::write(parent.join("keep.txt"), "parent can contain files\n").unwrap();
        let snapshot = GitClient::new()
            .clone_repository_into(&source.to_string_lossy(), &parent, "local-copy")
            .unwrap();
        let destination = parent.join("local-copy");

        assert_eq!(snapshot.repo.path, destination);
        assert_eq!(snapshot.repo.name, "local-copy");
        assert_eq!(
            fs::read_to_string(destination.join("README.md")).unwrap(),
            "source\n"
        );

        let _ = fs::remove_dir_all(source);
        let _ = fs::remove_dir_all(parent);
    }

    #[test]
    fn parses_tag_decorations_with_commas() {
        assert_eq!(
            parse_ref_tags("HEAD -> main, tag: release,one, tag: v1.0.0, origin/main"),
            vec!["release,one".to_string(), "v1.0.0".to_string()]
        );
    }

    #[test]
    fn reads_history_tags_with_commas() {
        let repo = temp_repo("history-tag-commas");
        fs::write(repo.join("README.md"), "one\n").unwrap();
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "GitSpark Test"]);
        run_git(&repo, &["config", "user.email", "test@gitspark.local"]);
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "initial"]);
        run_git(&repo, &["tag", "release,one"]);

        let snapshot = GitClient::new().open_repo(&repo).unwrap();
        assert_eq!(snapshot.history[0].tags, vec!["release,one".to_string()]);

        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn deletes_local_tag() {
        let repo = temp_repo("delete-tag");
        fs::write(repo.join("README.md"), "one\n").unwrap();
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "GitSpark Test"]);
        run_git(&repo, &["config", "user.email", "test@gitspark.local"]);
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "initial"]);
        let oid = run_git(&repo, &["rev-parse", "HEAD"]).trim().to_string();

        let git = GitClient::new();
        git.create_tag(&repo, &oid, "v1.0.0").unwrap();
        let snapshot = git.delete_tag(&repo, "v1.0.0").unwrap();

        assert!(!snapshot.tags.iter().any(|tag| tag == "v1.0.0"));
        assert!(run_git(&repo, &["tag", "--list"]).trim().is_empty());

        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn reads_all_tags_outside_visible_history_limit() {
        let repo = temp_repo("all-tags");
        fs::write(repo.join("README.md"), "0\n").unwrap();
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "GitSpark Test"]);
        run_git(&repo, &["config", "user.email", "test@gitspark.local"]);
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "commit 0"]);
        run_git(&repo, &["tag", "older-than-visible-history"]);

        for ix in 1..=105 {
            fs::write(repo.join("README.md"), format!("{ix}\n")).unwrap();
            run_git(&repo, &["add", "--all"]);
            run_git(&repo, &["commit", "-m", &format!("commit {ix}")]);
        }

        let snapshot = GitClient::new().open_repo(&repo).unwrap();
        assert!(!snapshot.history.iter().any(|commit| {
            commit
                .tags
                .iter()
                .any(|tag| tag == "older-than-visible-history")
        }));
        assert!(
            snapshot
                .tags
                .iter()
                .any(|tag| tag == "older-than-visible-history")
        );

        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn creates_annotated_tags_like_github_desktop() {
        let repo = temp_repo("annotated-tags");
        fs::write(repo.join("README.md"), "one\n").unwrap();
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "GitSpark Test"]);
        run_git(&repo, &["config", "user.email", "test@gitspark.local"]);
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "initial"]);
        let oid = run_git(&repo, &["rev-parse", "HEAD"]).trim().to_string();

        GitClient::new().create_tag(&repo, &oid, "v1.0.0").unwrap();

        let tag_type = run_git(&repo, &["cat-file", "-t", "v1.0.0"]);
        assert_eq!(tag_type.trim(), "tag");

        let peeled_target = run_git(&repo, &["rev-parse", "v1.0.0^{}"]);
        assert_eq!(peeled_target.trim(), oid);

        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn restores_gitspark_branch_stash_when_user_stash_is_newer() {
        let repo = temp_repo("branch-aware-stash");
        fs::write(repo.join("README.md"), "one\n").unwrap();
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "GitSpark Test"]);
        run_git(&repo, &["config", "user.email", "test@gitspark.local"]);
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "initial"]);

        fs::write(repo.join("README.md"), "gitspark change\n").unwrap();
        let snapshot = GitClient::new().stash_all(&repo).unwrap();
        assert_eq!(snapshot.stash_count, 1);

        fs::write(repo.join("README.md"), "user stash\n").unwrap();
        run_git(&repo, &["stash", "push", "-m", "User stash"]);
        let snapshot = GitClient::new().open_repo(&repo).unwrap();
        assert_eq!(snapshot.stash_count, 1);

        GitClient::new().stash_pop(&repo).unwrap();

        let readme = fs::read_to_string(repo.join("README.md")).unwrap();
        assert_eq!(readme, "gitspark change\n");
        let stash_list = run_git(&repo, &["stash", "list", "--format=%s"]);
        assert!(stash_list.contains("User stash"));
        assert!(!stash_list.contains(GITSPARK_STASH_MESSAGE_PREFIX));
        let snapshot = GitClient::new().open_repo(&repo).unwrap();
        assert_eq!(snapshot.stash_count, 0);

        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn replaces_existing_gitspark_branch_stash_when_stashing_again() {
        let repo = temp_repo("replace-branch-stash");
        fs::write(repo.join("README.md"), "one\n").unwrap();
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "GitSpark Test"]);
        run_git(&repo, &["config", "user.email", "test@gitspark.local"]);
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "initial"]);

        fs::write(repo.join("README.md"), "first stash\n").unwrap();
        let snapshot = GitClient::new().stash_all(&repo).unwrap();
        assert_eq!(snapshot.stash_count, 1);

        fs::write(repo.join("README.md"), "second stash\n").unwrap();
        let snapshot = GitClient::new().stash_all(&repo).unwrap();
        assert_eq!(snapshot.stash_count, 1);

        let stash_list = run_git(&repo, &["stash", "list", "--format=%s"]);
        assert_eq!(stash_list.matches(GITSPARK_STASH_MESSAGE_PREFIX).count(), 1);

        GitClient::new().stash_pop(&repo).unwrap();
        let readme = fs::read_to_string(repo.join("README.md")).unwrap();
        assert_eq!(readme, "second stash\n");

        let snapshot = GitClient::new().open_repo(&repo).unwrap();
        assert_eq!(snapshot.stash_count, 0);

        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn compares_current_branch_with_target_branch() {
        let repo = temp_repo("compare-branch");
        fs::write(repo.join("README.md"), "one\n").unwrap();
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "GitSpark Test"]);
        run_git(&repo, &["config", "user.email", "test@gitspark.local"]);
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "initial"]);

        run_git(&repo, &["switch", "-c", "feature"]);
        fs::write(repo.join("feature.txt"), "feature\n").unwrap();
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "feature"]);

        run_git(&repo, &["switch", "main"]);
        fs::write(repo.join("main.txt"), "main\n").unwrap();
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "main"]);

        run_git(&repo, &["switch", "feature"]);
        let comparison = GitClient::new()
            .compare_current_branch_with(&repo, "main")
            .unwrap();

        assert_eq!(comparison.current_branch, "feature");
        assert_eq!(comparison.target_branch, "main");
        assert_eq!(comparison.ahead, 1);
        assert_eq!(comparison.behind, 1);

        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn updates_current_branch_from_default_branch() {
        let repo = temp_repo("update-from-default");
        fs::write(repo.join("README.md"), "one\n").unwrap();
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "GitSpark Test"]);
        run_git(&repo, &["config", "user.email", "test@gitspark.local"]);
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "initial"]);

        run_git(&repo, &["switch", "-c", "feature"]);
        fs::write(repo.join("feature.txt"), "feature\n").unwrap();
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "feature"]);

        run_git(&repo, &["switch", "main"]);
        fs::write(repo.join("main.txt"), "main\n").unwrap();
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "main"]);

        run_git(&repo, &["switch", "feature"]);
        let snapshot = GitClient::new()
            .update_current_branch_from(&repo, "main")
            .unwrap();

        assert_eq!(snapshot.repo.current_branch, "feature");
        assert_eq!(
            snapshot.history[0].summary,
            "Merge branch 'main' into feature"
        );
        assert!(repo.join("main.txt").exists());

        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn rebases_current_branch_onto_target_branch() {
        let repo = temp_repo("rebase-branch");
        fs::write(repo.join("README.md"), "one\n").unwrap();
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "GitSpark Test"]);
        run_git(&repo, &["config", "user.email", "test@gitspark.local"]);
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "initial"]);

        run_git(&repo, &["switch", "-c", "feature"]);
        fs::write(repo.join("feature.txt"), "feature\n").unwrap();
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "feature"]);

        run_git(&repo, &["switch", "main"]);
        fs::write(repo.join("main.txt"), "main\n").unwrap();
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "main"]);

        run_git(&repo, &["switch", "feature"]);
        let snapshot = GitClient::new()
            .rebase_current_branch_onto(&repo, "main")
            .unwrap();

        assert_eq!(snapshot.repo.current_branch, "feature");
        assert_eq!(snapshot.history[0].summary, "feature");
        assert!(repo.join("main.txt").exists());

        let comparison = GitClient::new()
            .compare_current_branch_with(&repo, "main")
            .unwrap();
        assert_eq!(comparison.ahead, 1);
        assert_eq!(comparison.behind, 0);

        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn pushes_annotated_tags_with_branch() {
        let remote = temp_repo("push-tags-remote");
        run_git(&remote, &["init", "--bare"]);

        let repo = temp_repo("push-tags-work");
        fs::write(repo.join("README.md"), "one\n").unwrap();
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "GitSpark Test"]);
        run_git(&repo, &["config", "user.email", "test@gitspark.local"]);
        run_git(
            &repo,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "initial"]);
        run_git(&repo, &["push", "--set-upstream", "origin", "main"]);
        let oid = run_git(&repo, &["rev-parse", "HEAD"]).trim().to_string();

        GitClient::new().create_tag(&repo, &oid, "v1.0.0").unwrap();
        GitClient::new().push_origin(&repo).unwrap();

        let remote_tag_type = run_git(&remote, &["cat-file", "-t", "refs/tags/v1.0.0"]);
        assert_eq!(remote_tag_type.trim(), "tag");

        let remote_tag_target = run_git(&remote, &["rev-parse", "refs/tags/v1.0.0^{}"]);
        assert_eq!(remote_tag_target.trim(), oid);

        let _ = fs::remove_dir_all(repo);
        let _ = fs::remove_dir_all(remote);
    }

    /// A second working copy of `remote`, for tests that need "someone
    /// else" to move the remote out from under the repo under test — a
    /// plain filesystem path is a fully functional git remote, so this
    /// needs no server, network, or mock of any kind.
    fn clone_of(remote: &Path, name: &str) -> std::path::PathBuf {
        let other = temp_repo(name);
        run_git(&other, &["clone", remote.to_str().unwrap(), "."]);
        run_git(&other, &["config", "user.name", "GitSpark Test"]);
        run_git(&other, &["config", "user.email", "test@gitspark.local"]);
        other
    }

    #[test]
    fn pulls_a_fast_forward_from_origin() {
        let remote = temp_repo("pull-ff-remote");
        // Pin the bare remote's default branch explicitly: `clone_of` below
        // checks out whatever HEAD points to, and that symref falls back to
        // the host's `init.defaultBranch` (still "master" on a factory-default
        // git) whenever it isn't set here — confirmed as the reason this test
        // passed on macOS (global `init.defaultBranch = main`) but failed on a
        // clean Linux CI runner.
        run_git(&remote, &["init", "--bare", "-b", "main"]);

        let repo = temp_repo("pull-ff-repo");
        fs::write(repo.join("a.txt"), "one\n").unwrap();
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "GitSpark Test"]);
        run_git(&repo, &["config", "user.email", "test@gitspark.local"]);
        run_git(
            &repo,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "initial"]);
        run_git(&repo, &["push", "--set-upstream", "origin", "main"]);

        // Someone else pushes a commit `repo` has never seen.
        let other = clone_of(&remote, "pull-ff-other");
        fs::write(other.join("b.txt"), "two\n").unwrap();
        run_git(&other, &["add", "--all"]);
        run_git(&other, &["commit", "-m", "someone else's commit"]);
        run_git(&other, &["push", "origin", "main"]);

        GitClient::new().pull_origin(&repo).unwrap();

        assert!(
            repo.join("b.txt").exists(),
            "the fast-forward should have brought the new file down"
        );

        let _ = fs::remove_dir_all(repo);
        let _ = fs::remove_dir_all(remote);
        let _ = fs::remove_dir_all(other);
    }

    #[test]
    fn pull_reports_the_real_git_reason_when_history_has_diverged() {
        let remote = temp_repo("pull-diverged-remote");
        // See the identical comment in `pulls_a_fast_forward_from_origin`.
        run_git(&remote, &["init", "--bare", "-b", "main"]);

        let repo = temp_repo("pull-diverged-repo");
        fs::write(repo.join("a.txt"), "one\n").unwrap();
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "GitSpark Test"]);
        run_git(&repo, &["config", "user.email", "test@gitspark.local"]);
        run_git(
            &repo,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "initial"]);
        run_git(&repo, &["push", "--set-upstream", "origin", "main"]);

        // Someone else pushes to origin...
        let other = clone_of(&remote, "pull-diverged-other");
        fs::write(other.join("b.txt"), "two\n").unwrap();
        run_git(&other, &["add", "--all"]);
        run_git(&other, &["commit", "-m", "remote-side commit"]);
        run_git(&other, &["push", "origin", "main"]);

        // ...while `repo` also commits locally, without pulling first. Now
        // neither side is an ancestor of the other — the exact shape of the
        // "Pull origin failed: failed to pull from 'origin'" report, which
        // carried no more information than that until `{err}` became
        // `{err:#}` at the call site that turns this into a status message.
        fs::write(repo.join("c.txt"), "three\n").unwrap();
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "local-only commit"]);

        let err = GitClient::new()
            .pull_origin(&repo)
            .expect_err("diverged history cannot fast-forward");
        let full_message = format!("{err:#}");

        assert!(
            full_message.contains("failed to pull from 'origin'"),
            "lost the context: {full_message}"
        );
        assert!(
            full_message.contains("Not possible to fast-forward"),
            "lost git's own reason, which is the actual point: {full_message}"
        );

        let _ = fs::remove_dir_all(repo);
        let _ = fs::remove_dir_all(remote);
        let _ = fs::remove_dir_all(other);
    }

    #[test]
    fn push_reports_the_real_git_reason_when_rejected() {
        let remote = temp_repo("push-rejected-remote");
        // See the identical comment in `pulls_a_fast_forward_from_origin`.
        run_git(&remote, &["init", "--bare", "-b", "main"]);

        let repo = temp_repo("push-rejected-repo");
        fs::write(repo.join("a.txt"), "one\n").unwrap();
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "GitSpark Test"]);
        run_git(&repo, &["config", "user.email", "test@gitspark.local"]);
        run_git(
            &repo,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "initial"]);
        run_git(&repo, &["push", "--set-upstream", "origin", "main"]);

        // Someone else pushes to origin ahead of `repo`...
        let other = clone_of(&remote, "push-rejected-other");
        fs::write(other.join("b.txt"), "two\n").unwrap();
        run_git(&other, &["add", "--all"]);
        run_git(&other, &["commit", "-m", "remote-side commit"]);
        run_git(&other, &["push", "origin", "main"]);

        // ...and `repo` tries to push its own commit without ever fetching
        // that. A real, everyday way to hit "Push origin failed" — someone
        // else on the branch beat you to it.
        fs::write(repo.join("c.txt"), "three\n").unwrap();
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "local-only commit"]);

        let err = GitClient::new()
            .push_origin(&repo)
            .expect_err("a push behind the remote must be rejected, not silently force-pushed");
        let full_message = format!("{err:#}");

        assert!(
            full_message.contains("failed to push to 'origin'"),
            "lost the context: {full_message}"
        );
        assert!(
            full_message.contains("[rejected]")
                || full_message.contains("failed to push some refs"),
            "lost git's own reason, which is the actual point: {full_message}"
        );

        let _ = fs::remove_dir_all(repo);
        let _ = fs::remove_dir_all(remote);
        let _ = fs::remove_dir_all(other);
    }

    #[test]
    fn commits_only_selected_paths() {
        let repo = temp_repo("commit-selected-paths");
        fs::write(repo.join("included.txt"), "one\n").unwrap();
        fs::write(repo.join("excluded.txt"), "one\n").unwrap();
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "GitSpark Test"]);
        run_git(&repo, &["config", "user.email", "test@gitspark.local"]);
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "initial"]);

        fs::write(repo.join("included.txt"), "two\n").unwrap();
        fs::write(repo.join("excluded.txt"), "two\n").unwrap();

        GitClient::new()
            .commit_paths(&repo, &[String::from("included.txt")], "update included")
            .unwrap();

        let committed_files = run_git(&repo, &["show", "--format=", "--name-only", "HEAD"]);
        assert_eq!(committed_files.trim(), "included.txt");

        let status = run_git(&repo, &["status", "--short"]);
        assert!(status.contains(" M excluded.txt"), "{status}");
        assert!(!status.contains("included.txt"), "{status}");

        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn deletes_current_branch_after_switching_to_fallback() {
        let repo = temp_repo("delete-current-branch");
        fs::write(repo.join("README.md"), "one\n").unwrap();
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "GitSpark Test"]);
        run_git(&repo, &["config", "user.email", "test@gitspark.local"]);
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "initial"]);
        run_git(&repo, &["switch", "-c", "feature/delete-current"]);

        let snapshot = GitClient::new()
            .delete_branch_from_current_worktree(&repo, "feature/delete-current")
            .unwrap();

        assert_eq!(snapshot.repo.current_branch, "main");
        assert!(
            !snapshot
                .branches
                .iter()
                .any(|branch| branch.name == "feature/delete-current"),
            "deleted branch should be absent from snapshot"
        );

        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn reads_and_clears_local_author_identity_without_touching_other_config() {
        let repo = temp_repo("local-identity");
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "Local Name"]);
        run_git(&repo, &["config", "user.email", "local@example.test"]);
        run_git(&repo, &["config", "init.defaultBranch", "trunk"]);

        let git = GitClient::new();
        let local = git.read_local_identity(&repo).unwrap();
        assert_eq!(local.user_name, "Local Name");
        assert_eq!(local.user_email, "local@example.test");

        git.clear_local_author_identity(&repo).unwrap();
        let cleared = git.read_local_identity(&repo).unwrap();
        assert_eq!(cleared.user_name, "");
        assert_eq!(cleared.user_email, "");
        assert_eq!(cleared.default_branch.as_deref(), Some("trunk"));

        git.write_identity(
            &repo,
            &GitIdentity {
                user_name: "Repo Name".to_string(),
                user_email: "repo@example.test".to_string(),
                pull_rebase: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();

        let local = git.read_local_identity(&repo).unwrap();
        assert_eq!(local.user_name, "Repo Name");
        assert_eq!(local.user_email, "repo@example.test");
        assert_eq!(local.default_branch.as_deref(), Some("main"));

        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn parses_effective_git_author_identity() {
        let (name, email) =
            parse_author_ident("Jane Q. Developer <jane@example.com> 1710000000 +0530").unwrap();
        assert_eq!(name, "Jane Q. Developer");
        assert_eq!(email, "jane@example.com");
    }

    #[test]
    fn effective_author_identity_only_fills_missing_config_values() {
        let mut identity = GitIdentity {
            user_name: "Configured Name".to_string(),
            user_email: String::new(),
            pull_rebase: None,
            default_branch: None,
        };

        fill_missing_author_identity(
            &mut identity,
            "Effective Name".to_string(),
            "effective@example.test".to_string(),
        );

        assert_eq!(identity.user_name, "Configured Name");
        assert_eq!(identity.user_email, "effective@example.test");
    }

    #[test]
    fn read_identity_treats_incomplete_effective_author_as_missing_config() {
        let repo = temp_repo("incomplete-author-ident");
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "Visual Identity"]);
        run_git(&repo, &["config", "user.email", ""]);

        let identity = GitClient::new().read_identity(&repo).unwrap();
        assert_eq!(identity.user_name, "Visual Identity");
        assert_eq!(identity.user_email, "");

        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn reads_and_updates_primary_remote_url() {
        let repo = temp_repo("remote-settings");
        run_git(&repo, &["init", "-b", "main"]);
        run_git(
            &repo,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/example/old.git",
            ],
        );

        let git = GitClient::new();
        assert_eq!(
            git.primary_remote(&repo).unwrap(),
            Some((
                "origin".to_string(),
                "https://github.com/example/old.git".to_string(),
            ))
        );

        let snapshot = git
            .set_remote_url(&repo, "origin", "https://github.com/example/new.git")
            .unwrap();

        assert_eq!(snapshot.repo.remote_name.as_deref(), Some("origin"));
        assert_eq!(
            git.primary_remote(&repo).unwrap(),
            Some((
                "origin".to_string(),
                "https://github.com/example/new.git".to_string(),
            ))
        );
        assert_eq!(
            run_git(&repo, &["remote", "get-url", "origin"]).trim(),
            "https://github.com/example/new.git"
        );

        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn reads_writes_and_removes_root_gitignore() {
        let repo = temp_repo("gitignore-settings");
        run_git(&repo, &["init", "-b", "main"]);

        let git = GitClient::new();
        assert_eq!(git.read_gitignore(&repo).unwrap(), "");

        let snapshot = git.write_gitignore(&repo, "target/\n*.log\n").unwrap();
        assert_eq!(snapshot.repo.remote_name, None);
        assert_eq!(
            fs::read_to_string(repo.join(".gitignore")).unwrap(),
            "target/\n*.log\n"
        );
        assert_eq!(git.read_gitignore(&repo).unwrap(), "target/\n*.log\n");

        git.write_gitignore(&repo, "").unwrap();
        assert!(!repo.join(".gitignore").exists());
        assert_eq!(git.read_gitignore(&repo).unwrap(), "");

        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn detects_and_aborts_conflicted_merge_operation() {
        let repo = temp_repo("merge-conflict-operation");
        setup_conflict_repo(&repo);
        run_git(&repo, &["switch", "feature"]);
        run_git_expect(&repo, &["merge", "--no-ff", "main"], false);

        let git = GitClient::new();
        let operation = git.operation_state(&repo).unwrap().unwrap();
        assert_eq!(operation.kind, crate::models::GitOperationKind::Merge);
        assert_eq!(operation.current_branch, "feature");
        assert_eq!(operation.target_branch.as_deref(), Some("main"));
        assert!(!operation.can_continue);
        assert_eq!(operation.conflicted_files[0].path, "conflict.txt");

        let snapshot = git.abort_merge(&repo).unwrap();
        assert_eq!(snapshot.repo.current_branch, "feature");
        assert!(git.operation_state(&repo).unwrap().is_none());

        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn detects_and_aborts_conflicted_rebase_operation() {
        let repo = temp_repo("rebase-conflict-operation");
        setup_conflict_repo(&repo);
        run_git(&repo, &["switch", "feature"]);
        run_git_expect(&repo, &["rebase", "main"], false);

        let git = GitClient::new();
        let operation = git.operation_state(&repo).unwrap().unwrap();
        assert_eq!(operation.kind, crate::models::GitOperationKind::Rebase);
        assert_eq!(operation.current_branch, "feature");
        assert!(!operation.can_continue);
        assert_eq!(operation.conflicted_files[0].path, "conflict.txt");

        let snapshot = git.abort_rebase(&repo).unwrap();
        assert_eq!(snapshot.repo.current_branch, "feature");
        assert!(git.operation_state(&repo).unwrap().is_none());

        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn merge_preflight_blocks_predictable_conflicts_without_starting_merge() {
        let repo = temp_repo("merge-preflight-conflict");
        setup_conflict_repo(&repo);
        run_git(&repo, &["switch", "feature"]);

        let git = GitClient::new();
        let result = git.merge_branch(&repo, "main").unwrap_err().to_string();

        assert!(result.contains("merge would conflict in 'conflict.txt'"));
        assert!(git.operation_state(&repo).unwrap().is_none());
        assert!(
            !fs::read_to_string(repo.join("conflict.txt"))
                .unwrap()
                .contains("<<<<<<<")
        );

        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn rebase_preflight_blocks_predictable_conflicts_without_starting_rebase() {
        let repo = temp_repo("rebase-preflight-conflict");
        setup_conflict_repo(&repo);
        run_git(&repo, &["switch", "feature"]);

        let git = GitClient::new();
        let result = git
            .rebase_current_branch_onto(&repo, "main")
            .unwrap_err()
            .to_string();

        assert!(result.contains("rebase would conflict in 'conflict.txt'"));
        assert!(git.operation_state(&repo).unwrap().is_none());
        assert!(
            !fs::read_to_string(repo.join("conflict.txt"))
                .unwrap()
                .contains("<<<<<<<")
        );

        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn marks_conflicted_file_resolved_after_external_edit() {
        let repo = temp_repo("mark-conflict-resolved");
        setup_conflict_repo(&repo);
        run_git(&repo, &["switch", "feature"]);
        run_git_expect(&repo, &["merge", "--no-ff", "main"], false);

        fs::write(repo.join("conflict.txt"), "resolved\n").unwrap();
        let operation = GitClient::new()
            .mark_conflict_resolved(&repo, "conflict.txt")
            .unwrap()
            .unwrap();

        assert!(operation.can_continue);
        assert!(operation.conflicted_files.is_empty());
        run_git(&repo, &["merge", "--abort"]);

        let _ = fs::remove_dir_all(repo);
    }

    fn temp_repo(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("gitspark-{name}-{unique}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn setup_conflict_repo(repo: &Path) {
        fs::write(repo.join("conflict.txt"), "base\n").unwrap();
        run_git(repo, &["init", "-b", "main"]);
        run_git(repo, &["config", "user.name", "GitSpark Test"]);
        run_git(repo, &["config", "user.email", "test@gitspark.local"]);
        run_git(repo, &["add", "--all"]);
        run_git(repo, &["commit", "-m", "initial"]);
        run_git(repo, &["switch", "-c", "feature"]);
        fs::write(repo.join("conflict.txt"), "feature\n").unwrap();
        run_git(repo, &["commit", "-am", "feature change"]);
        run_git(repo, &["switch", "main"]);
        fs::write(repo.join("conflict.txt"), "main\n").unwrap();
        run_git(repo, &["commit", "-am", "main change"]);
    }

    fn run_git(repo: &Path, args: &[&str]) -> String {
        run_git_expect(repo, args, true)
    }

    fn run_git_expect(repo: &Path, args: &[&str], expect_success: bool) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(
            output.status.success() == expect_success,
            "git {:?} returned unexpected status: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    // ------------------------------------------------------------------
    // Worktrees
    // ------------------------------------------------------------------

    #[test]
    fn parses_worktree_porcelain_records() {
        let output = concat!(
            "worktree /Users/x/proj\n",
            "HEAD 4b49bed1\n",
            "branch refs/heads/dev\n",
            "\n",
            "worktree /Users/x/.wt/proj-master\n",
            "HEAD 0eb22df2\n",
            "branch refs/heads/master\n",
            "\n",
            "worktree /Users/x/.wt/proj-detached\n",
            "HEAD 146b9893\n",
            "detached\n",
        );
        let worktrees = parse_worktree_list(output, Path::new("/nowhere"));
        assert_eq!(worktrees.len(), 3);

        assert_eq!(worktrees[0].name, "proj");
        assert_eq!(worktrees[0].branch.as_deref(), Some("dev"));
        assert!(
            worktrees[0].is_main,
            "the first record is the primary worktree"
        );

        assert_eq!(worktrees[1].name, "proj-master");
        assert_eq!(worktrees[1].branch.as_deref(), Some("master"));
        assert!(!worktrees[1].is_main);

        assert!(
            worktrees[2].is_detached,
            "detached checkouts carry no branch"
        );
        assert_eq!(worktrees[2].branch, None);
    }

    #[test]
    fn skips_bare_repositories_and_promotes_the_first_real_tree() {
        // A bare repo emits a record with no working directory. Nothing can be
        // opened there, so the NEXT record must become the primary worktree.
        let output = concat!(
            "worktree /Users/x/proj.git\n",
            "bare\n",
            "\n",
            "worktree /Users/x/proj\n",
            "HEAD 4b49bed1\n",
            "branch refs/heads/main\n",
        );
        let worktrees = parse_worktree_list(output, Path::new("/nowhere"));
        assert_eq!(worktrees.len(), 1);
        assert_eq!(worktrees[0].name, "proj");
        assert!(worktrees[0].is_main);
    }

    #[test]
    fn records_the_locked_flag_with_or_without_a_reason() {
        let output = concat!(
            "worktree /Users/x/proj\n",
            "HEAD 4b49bed1\n",
            "branch refs/heads/main\n",
            "\n",
            "worktree /Users/x/.wt/a\n",
            "HEAD 0eb22df2\n",
            "branch refs/heads/a\n",
            "locked\n",
            "\n",
            "worktree /Users/x/.wt/b\n",
            "HEAD 146b9893\n",
            "branch refs/heads/b\n",
            "locked on a removable drive\n",
        );
        let worktrees = parse_worktree_list(output, Path::new("/nowhere"));
        assert!(!worktrees[0].is_locked);
        assert!(worktrees[1].is_locked, "a bare `locked` key sets the flag");
        assert!(
            worktrees[2].is_locked,
            "`locked <reason>` also sets the flag"
        );
    }

    #[test]
    fn lists_adds_and_removes_worktrees_against_real_git() {
        let repo = temp_repo("worktree-lifecycle");
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "GitSpark Test"]);
        run_git(&repo, &["config", "user.email", "test@gitspark.local"]);
        fs::write(repo.join("a.txt"), "a\n").unwrap();
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "initial"]);

        let client = GitClient::new();

        let worktrees = client.list_worktrees(&repo).unwrap();
        assert_eq!(worktrees.len(), 1);
        assert!(worktrees[0].is_main);
        assert!(
            worktrees[0].is_current,
            "the repo we asked about must resolve as current even through a /tmp symlink"
        );
        assert_eq!(worktrees[0].branch.as_deref(), Some("main"));

        let extra = repo.parent().unwrap().join(format!(
            "{}-feature",
            repo.file_name().unwrap().to_string_lossy()
        ));
        let worktrees = client.add_worktree(&repo, &extra, "feature", true).unwrap();
        assert_eq!(worktrees.len(), 2);
        let added = worktrees.iter().find(|w| !w.is_main).unwrap();
        assert_eq!(added.branch.as_deref(), Some("feature"));
        assert!(!added.is_current, "adding a worktree does not switch to it");

        // Listing FROM the new worktree must mark that one current instead.
        let from_extra = client.list_worktrees(&extra).unwrap();
        let current = from_extra.iter().find(|w| w.is_current).unwrap();
        assert_eq!(current.branch.as_deref(), Some("feature"));

        let worktrees = client.remove_worktree(&repo, &extra, false).unwrap();
        assert_eq!(worktrees.len(), 1);
        assert!(worktrees[0].is_main);

        fs::remove_dir_all(&repo).ok();
        fs::remove_dir_all(&extra).ok();
    }

    #[test]
    fn refuses_to_check_out_a_branch_already_checked_out_elsewhere() {
        let repo = temp_repo("worktree-duplicate-branch");
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "GitSpark Test"]);
        run_git(&repo, &["config", "user.email", "test@gitspark.local"]);
        fs::write(repo.join("a.txt"), "a\n").unwrap();
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "initial"]);

        let client = GitClient::new();
        let extra = repo.parent().unwrap().join(format!(
            "{}-dup",
            repo.file_name().unwrap().to_string_lossy()
        ));

        // `main` is checked out in the primary worktree, so git must reject
        // this. It is the constraint the picker has to surface, not hide.
        assert!(
            client.add_worktree(&repo, &extra, "main", false).is_err(),
            "git allowed the same branch in two worktrees"
        );

        fs::remove_dir_all(&repo).ok();
        fs::remove_dir_all(&extra).ok();
    }

    // ----------------------------------------------------------------------
    // Untracked-file synthetic diffs
    // ----------------------------------------------------------------------

    /// Read the `+count` out of `@@ -0,0 +1,<count> @@`.
    fn hunk_new_count(diff: &str) -> usize {
        let header = diff
            .lines()
            .find(|line| line.starts_with("@@ "))
            .expect("diff has a hunk header");
        header
            .split("+1,")
            .nth(1)
            .and_then(|rest| rest.split(' ').next())
            .and_then(|n| n.parse().ok())
            .expect("hunk header carries a new-line count")
    }

    fn added_line_count(diff: &str) -> usize {
        // `+++ b/<path>` is the file header, not an added line.
        diff.lines()
            .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
            .count()
    }

    #[test]
    fn untracked_diff_header_matches_body_for_a_small_file() {
        let repo = temp_repo("untracked-small");
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "GitSpark Test"]);
        run_git(&repo, &["config", "user.email", "test@gitspark.local"]);
        fs::write(repo.join("new.txt"), "a\nb\nc\n").unwrap();

        let client = GitClient::new();
        let entry = client.get_file_diff(&repo, "new.txt").unwrap();

        assert_eq!(hunk_new_count(&entry.diff), 3);
        assert_eq!(added_line_count(&entry.diff), 3);
        fs::remove_dir_all(&repo).ok();
    }

    #[test]
    fn untracked_diff_header_matches_body_when_truncated() {
        let repo = temp_repo("untracked-truncated");
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "GitSpark Test"]);
        run_git(&repo, &["config", "user.email", "test@gitspark.local"]);

        // Comfortably past the cap, so truncation definitely happens.
        let big: String = (0..UNTRACKED_DIFF_MAX_LINES + 500)
            .map(|i| format!("line {i}\n"))
            .collect();
        fs::write(repo.join("big.txt"), big).unwrap();

        let client = GitClient::new();
        let entry = client.get_file_diff(&repo, "big.txt").unwrap();

        let header = hunk_new_count(&entry.diff);
        let body = added_line_count(&entry.diff);
        assert_eq!(
            header, body,
            "hunk header claims {header} lines but the body has {body} — the patch is malformed"
        );
        assert_eq!(
            body, UNTRACKED_DIFF_MAX_LINES,
            "body should stop at the cap"
        );
        fs::remove_dir_all(&repo).ok();
    }

    // ----------------------------------------------------------------------
    // Batched diff splitting
    // ----------------------------------------------------------------------

    #[test]
    fn splits_a_multi_file_diff_by_path() {
        let output = concat!(
            "diff --git a/src/a.rs b/src/a.rs\n",
            "index 111..222 100644\n",
            "--- a/src/a.rs\n",
            "+++ b/src/a.rs\n",
            "@@ -1 +1 @@\n",
            "-old a\n",
            "+new a\n",
            "diff --git a/src/b.rs b/src/b.rs\n",
            "index 333..444 100644\n",
            "--- a/src/b.rs\n",
            "+++ b/src/b.rs\n",
            "@@ -1 +1 @@\n",
            "-old b\n",
            "+new b\n",
        );
        let (sections, _) = split_combined_diff(output, &["src/a.rs", "src/b.rs"]);
        assert_eq!(sections.len(), 2);
        assert!(sections["src/a.rs"].contains("+new a"));
        assert!(
            !sections["src/a.rs"].contains("new b"),
            "sections bled together"
        );
        assert!(sections["src/b.rs"].contains("+new b"));
        assert!(sections["src/b.rs"].starts_with("diff --git a/src/b.rs"));
    }

    #[test]
    fn handles_paths_containing_spaces() {
        let output = concat!(
            "diff --git a/my file.txt b/my file.txt\n",
            "@@ -1 +1 @@\n",
            "+hello\n",
        );
        let (sections, _) = split_combined_diff(output, &["my file.txt"]);
        assert!(
            sections.contains_key("my file.txt"),
            "a path with a space was not attributed"
        );
    }

    #[test]
    fn prefers_the_longest_matching_path() {
        // Both end in `a.rs`; the header must attribute to the full path, not
        // the shorter one that also suffix-matches.
        let output = concat!(
            "diff --git a/src/a.rs b/src/a.rs\n",
            "@@ -1 +1 @@\n",
            "+nested\n",
        );
        let (sections, _) = split_combined_diff(output, &["a.rs", "src/a.rs"]);
        assert!(sections.contains_key("src/a.rs"));
        assert!(
            !sections.contains_key("a.rs"),
            "attributed a nested file to the shorter path"
        );
    }

    #[test]
    fn reports_sections_it_cannot_attribute() {
        // An unmatched header must never be guessed at. Reporting it lets the
        // caller fall back to per-file diffs instead of risking one file's
        // patch appearing under another file's name.
        let output = concat!(
            "diff --git a/unexpected.txt b/unexpected.txt\n",
            "@@ -1 +1 @@\n",
            "+surprise\n",
        );
        let (sections, unattributed) = split_combined_diff(output, &["asked-for.txt"]);
        assert!(sections.is_empty());
        assert_eq!(unattributed, 1);
    }

    #[test]
    fn a_path_with_no_section_is_not_a_failure() {
        // Files changed only in the working tree have no staged diff. That is
        // the common case, not an error, and must not trigger a fallback.
        let output = concat!(
            "diff --git a/a.rs b/a.rs\n",
            "@@ -1 +1 @@\n",
            "+only a changed\n",
        );
        let (sections, unattributed) = split_combined_diff(output, &["a.rs", "b.rs"]);
        assert_eq!(
            unattributed, 0,
            "b.rs having no diff is not an attribution failure"
        );
        assert!(sections.contains_key("a.rs"));
        assert!(!sections.contains_key("b.rs"));
    }

    #[test]
    fn batched_and_per_file_diffs_agree_against_real_git() {
        let repo = temp_repo("batch-diff");
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "GitSpark Test"]);
        run_git(&repo, &["config", "user.email", "test@gitspark.local"]);
        for name in ["a.rs", "b.rs", "with space.txt"] {
            fs::write(repo.join(name), "one\ntwo\n").unwrap();
        }
        run_git(&repo, &["add", "--all"]);
        run_git(&repo, &["commit", "-m", "initial"]);

        // A staged change, an unstaged change, and one of each on a third file.
        fs::write(repo.join("a.rs"), "one\nCHANGED\n").unwrap();
        run_git(&repo, &["add", "a.rs"]);
        fs::write(repo.join("b.rs"), "one\nWORKING\n").unwrap();
        fs::write(repo.join("with space.txt"), "one\nSPACED\n").unwrap();

        let client = GitClient::new();
        let snapshot = client.snapshot(&repo).unwrap();
        assert!(!snapshot.changes.is_empty());

        // The batched result must equal what a per-file call produces.
        for entry in &snapshot.diffs {
            let single = client.get_file_diff(&repo, &entry.path).unwrap();
            assert_eq!(
                entry.diff, single.diff,
                "batched diff differs from per-file diff for {}",
                entry.path
            );
        }
        fs::remove_dir_all(&repo).ok();
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

#[cfg(test)]
fn fill_missing_author_identity(identity: &mut GitIdentity, name: String, email: String) {
    if identity.user_name.trim().is_empty() {
        identity.user_name = name;
    }
    if identity.user_email.trim().is_empty() {
        identity.user_email = email;
    }
}

#[cfg(test)]
fn parse_author_ident(value: &str) -> Result<(String, String)> {
    let Some(email_end) = value.rfind('>') else {
        bail!("git author identity did not contain an email address");
    };
    let before_email = &value[..email_end];
    let Some(email_start) = before_email.rfind(" <") else {
        bail!("git author identity did not contain a name/email separator");
    };

    let name = before_email[..email_start].trim();
    let email = before_email[email_start + 2..].trim();
    if name.is_empty() || email.is_empty() {
        bail!("git author identity was missing a name or email");
    }

    Ok((name.to_string(), email.to_string()))
}

fn parse_name_status_line(line: &str) -> Option<ChangeEntry> {
    let mut parts = line.split('\t');
    let status = parts.next()?.trim();
    if status.is_empty() {
        return None;
    }

    let path = parts.last()?.trim();
    if path.is_empty() {
        return None;
    }

    Some(ChangeEntry {
        path: path.to_string(),
        status: status.to_string(),
    })
}

fn clean_git_ref_name(name: String) -> String {
    name.trim()
        .strip_prefix("refs/heads/")
        .or_else(|| name.trim().strip_prefix("remotes/"))
        .unwrap_or(name.trim())
        .trim_end_matches("^0")
        .to_string()
}

fn looks_binary_diff(diff: &str) -> bool {
    diff.contains("Binary files") || diff.contains("GIT binary patch")
}

fn path_is_supported_image(path: &str) -> bool {
    let Some(extension) = std::path::Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
    else {
        return false;
    };

    matches!(
        extension.to_ascii_lowercase().as_str(),
        "avif"
            | "jpg"
            | "jpeg"
            | "png"
            | "gif"
            | "webp"
            | "tif"
            | "tiff"
            | "tga"
            | "dds"
            | "bmp"
            | "ico"
            | "hdr"
            | "exr"
            | "pbm"
            | "pam"
            | "ppm"
            | "pgm"
            | "ff"
            | "farbfeld"
            | "qoi"
            | "svg"
    )
}

#[derive(Default)]
struct SubmoduleDiffMetadata {
    is_submodule: bool,
    old_oid: Option<String>,
    new_oid: Option<String>,
}

fn submodule_diff_metadata(diff: &str) -> SubmoduleDiffMetadata {
    let mut metadata = SubmoduleDiffMetadata::default();

    for line in diff.lines() {
        if line.contains("Subproject commit ") {
            metadata.is_submodule = true;
        }

        if let Some(oid) = line.strip_prefix("-Subproject commit ") {
            metadata.old_oid = Some(oid.trim().to_string());
        } else if let Some(oid) = line.strip_prefix("+Subproject commit ") {
            metadata.new_oid = Some(oid.trim().to_string());
        }
    }

    metadata
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
    let message = error
        .chain()
        .map(|error| error.to_string())
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase();
    message.contains("exit status: 1")
        || message.contains("returned non-zero exit status: 1")
        || message.contains("unable to read config")
        || message.contains("no such section")
        || message.contains("no such key")
        || message.contains("key not found")
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
