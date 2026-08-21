//! Read-path git operations backed by [gitoxide](https://github.com/GitoxideLabs/gitoxide).
//!
//! Every shell-out costs ~10ms in process spawn alone, regardless of how
//! trivial the command is. These functions do the same work in-process. The
//! benchmark behind the choice is `examples/git_backends.rs`; on a 9600-file
//! tree gix reads status in ~24ms against git's ~40ms and libgit2's ~63ms.
//!
//! **Reads only, and fallible on purpose.** Every function returns `Option`,
//! and `None` means "gix could not answer" — the caller then runs the git
//! binary as it always did. That keeps a young dependency from turning a
//! missing API or an unusual repository into a user-visible failure.
//!
//! Writes are deliberately absent. `commit`, `merge`, `rebase`, `stash`,
//! `push`/`pull` depend on hooks, config precedence, credential helpers and
//! conflict handling where matching git's exact behaviour is worth far more
//! than the milliseconds — shelling out *is* the correct implementation there.

use std::path::{Path, PathBuf};

use crate::models::{BranchInfo, CommitInfo};

/// Log when a gix read falls back to the git binary.
///
/// Shares `GITSPARK_TRACE_GIT` with the shell tracer, so one flag shows both
/// what gix served and what it could not.
fn trace() -> bool {
    static TRACE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *TRACE.get_or_init(|| std::env::var("GITSPARK_TRACE_GIT").is_ok())
}

fn fell_back(op: &str, reason: &dyn std::fmt::Display) {
    if trace() {
        eprintln!("[gix] fallback: {op}: {reason}");
    }
}

/// Open a repository, discovering upwards from `path`.
fn open(path: &Path) -> Option<gix::Repository> {
    match gix::discover(path) {
        Ok(repo) => Some(repo),
        Err(error) => {
            fell_back("discover", &error);
            None
        }
    }
}

/// The working directory root — `git rev-parse --show-toplevel`.
///
/// Returns `None` for a bare repository, which has no worktree; the caller's
/// shell path reports that as the error it is.
pub fn repo_root(path: &Path) -> Option<PathBuf> {
    let repo = open(path)?;
    match repo.workdir() {
        Some(dir) => Some(dir.to_path_buf()),
        None => {
            fell_back("repo_root", &"bare repository has no worktree");
            None
        }
    }
}

/// The remote to treat as primary.
///
/// Mirrors the shell implementation's precedence exactly: the current
/// branch's upstream remote first, then `origin`, then any other remote in
/// name order. `Some(None)` means gix answered and there are no remotes.
pub fn primary_remote(path: &Path) -> Option<Option<String>> {
    let repo = open(path)?;

    // Upstream of the current branch, when there is one.
    if let Some(name) = repo
        .head_ref()
        .ok()
        .flatten()
        .and_then(|head| {
            let short = head.name().shorten().to_string();
            repo.branch_remote_name(short.as_str(), gix::remote::Direction::Fetch)
        })
        .map(|remote| remote.as_bstr().to_string())
        .filter(|name| !name.is_empty())
    {
        return Some(Some(name));
    }

    let mut names: Vec<String> = repo
        .remote_names()
        .into_iter()
        .map(|name| name.to_string())
        .filter(|name| !name.is_empty())
        .collect();

    if names.is_empty() {
        return Some(None);
    }
    names.sort_by_key(|name| if name == "origin" { 0 } else { 1 });
    Some(names.into_iter().next())
}

/// A remote's fetch URL — `git remote get-url <name>`.
pub fn remote_url(path: &Path, remote: &str) -> Option<String> {
    let repo = open(path)?;
    let remote = repo.find_remote(remote).ok()?;
    let url = remote.url(gix::remote::Direction::Fetch)?;
    Some(url.to_bstring().to_string())
}

/// Render a timestamp the way git's `%(committerdate:relative)` and `%ar` do.
///
/// This is a direct port of `show_date_relative` in git's `date.c`, rounding
/// included. It matters that it is a port and not an approximation: these
/// strings sit next to each other in the branch and history lists, and a
/// backend that says "2 months ago" where git says "8 weeks ago" is a visible
/// regression even though both are true.
pub fn relative_date(then_secs: i64, now_secs: i64) -> String {
    if then_secs > now_secs {
        return "in the future".to_string();
    }
    let mut diff = (now_secs - then_secs) as u64;

    if diff < 90 {
        return plural(diff, "second");
    }
    // Round to the nearest minute, then hour, then day, as git does.
    diff = (diff + 30) / 60;
    if diff < 90 {
        return plural(diff, "minute");
    }
    diff = (diff + 30) / 60;
    if diff < 36 {
        return plural(diff, "hour");
    }
    diff = (diff + 12) / 24;
    if diff < 14 {
        return plural(diff, "day");
    }
    if diff < 70 {
        return plural((diff + 3) / 7, "week");
    }
    if diff < 365 {
        return plural((diff + 15) / 30, "month");
    }
    if diff < 1825 {
        // Under five years git also names the leftover months.
        let total_months = (diff * 12 * 2 / 365 + 1) / 2;
        let years = total_months / 12;
        let months = total_months % 12;
        if months == 0 {
            return plural(years, "year");
        }
        let y = plural_bare(years, "year");
        let m = plural_bare(months, "month");
        return format!("{y}, {m} ago");
    }
    plural((diff + 183) / 365, "year")
}

fn plural(n: u64, unit: &str) -> String {
    format!("{} ago", plural_bare(n, unit))
}

fn plural_bare(n: u64, unit: &str) -> String {
    if n == 1 {
        format!("1 {unit}")
    } else {
        format!("{n} {unit}s")
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Local and remote branches, ordered exactly as the shell path orders them:
/// locals before remotes, current first, then case-insensitive by name.
///
/// `updated` is filled with [`relative_date`], which is a port of git's own
/// wording, so the strings are indistinguishable from the shell path's.
pub fn branches(path: &Path) -> Option<Vec<BranchInfo>> {
    let repo = open(path)?;
    let head_name = repo
        .head_ref()
        .ok()
        .flatten()
        .map(|head| head.name().shorten().to_string());

    let platform = match repo.references() {
        Ok(platform) => platform,
        Err(error) => {
            fell_back("branches", &error);
            return None;
        }
    };
    let iter = match platform.all() {
        Ok(iter) => iter,
        Err(error) => {
            fell_back("branches", &error);
            return None;
        }
    };

    let now = now_secs();
    let mut branches = Vec::new();
    for reference in iter {
        let Ok(reference) = reference else { continue };
        let full = reference.name().as_bstr().to_string();
        let is_remote = full.starts_with("refs/remotes/");
        if !is_remote && !full.starts_with("refs/heads/") {
            continue;
        }
        let name = reference.name().shorten().to_string();
        // `refs/remotes/<remote>/HEAD` is a symbolic pointer, not a branch.
        if name.is_empty() || name.ends_with("/HEAD") {
            continue;
        }
        // Peel to the commit for its date. A ref that will not peel (an
        // annotated tag object, a broken ref) simply has no date rather than
        // dropping the branch from the list.
        let updated = reference
            .clone()
            .peel_to_commit()
            .ok()
            .and_then(|commit| commit.time().ok())
            .map(|time| relative_date(time.seconds, now));

        branches.push(BranchInfo {
            is_current: !is_remote && head_name.as_deref() == Some(name.as_str()),
            name,
            is_remote,
            updated,
        });
    }

    branches.sort_by(|left, right| {
        left.is_remote
            .cmp(&right.is_remote)
            .then(right.is_current.cmp(&left.is_current))
            .then(left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Some(branches)
}

/// The first `limit` commits reachable from HEAD.
///
/// `date` uses [`relative_date`]; `tags` are resolved by peeling every ref
/// under `refs/tags` to its commit once, up front, so the walk stays O(commits)
/// rather than re-scanning refs per commit.
pub fn history(path: &Path, limit: usize) -> Option<Vec<CommitInfo>> {
    let repo = open(path)?;
    let head_id = match repo.head_id() {
        Ok(id) => id,
        Err(error) => {
            // An unborn branch is normal in a fresh repository, not a failure.
            fell_back("history", &error);
            return None;
        }
    };
    let head_oid = head_id.to_string();

    let walk = match head_id.ancestors().all() {
        Ok(walk) => walk,
        Err(error) => {
            fell_back("history", &error);
            return None;
        }
    };

    // Peel every tag once into commit -> names, rather than per commit.
    let mut tags_by_commit: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    if let Ok(platform) = repo.references()
        && let Ok(tags) = platform.tags()
    {
        for reference in tags.flatten() {
            let name = reference.name().shorten().to_string();
            if let Ok(commit) = reference.clone().peel_to_commit() {
                tags_by_commit
                    .entry(commit.id().to_string())
                    .or_default()
                    .push(name);
            }
        }
    }

    let now = now_secs();
    let mut commits = Vec::with_capacity(limit.min(256));
    for info in walk.take(limit) {
        let Ok(info) = info else { continue };
        let Ok(commit) = repo.find_commit(info.id) else {
            continue;
        };
        let oid = info.id.to_string();
        let (summary, body) = match commit.message() {
            Ok(message) => (
                message.summary().to_string(),
                message
                    .body()
                    .map(|body| body.to_string())
                    .unwrap_or_default(),
            ),
            Err(_) => (String::new(), String::new()),
        };
        let (author_name, author_email) = match commit.author() {
            Ok(author) => (author.name.to_string(), author.email.to_string()),
            Err(_) => (String::new(), String::new()),
        };

        let date = commit
            .time()
            .ok()
            .map(|time| relative_date(time.seconds, now))
            .unwrap_or_default();
        let tags = tags_by_commit.get(&oid).cloned().unwrap_or_default();

        commits.push(CommitInfo {
            short_oid: oid.chars().take(7).collect(),
            is_head: oid == head_oid,
            oid,
            summary,
            body,
            author_name,
            author_email,
            date,
            tags,
        });
    }
    Some(commits)
}

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, path::Path, path::PathBuf};

    use super::{branches, history, primary_remote, relative_date, remote_url, repo_root};

    /// Expected values captured from real `git log --pretty=%ar` output, not
    /// read off the algorithm — the two disagreed once (an hour is "60
    /// minutes ago", not "1 hour ago") and git won.
    #[test]
    fn matches_git_relative_date_wording() {
        let now = 1_000_000_000i64;
        let ago = |secs: i64| relative_date(now - secs, now);

        assert_eq!(ago(0), "0 seconds ago");
        assert_eq!(ago(1), "1 second ago");
        assert_eq!(ago(89), "89 seconds ago");
        // 90s rounds to 2 minutes, not "1 minute".
        assert_eq!(ago(90), "2 minutes ago");
        // Verified against real git: an hour is "60 minutes ago", because
        // the minutes branch runs while the count is under 90.
        assert_eq!(ago(60 * 60), "60 minutes ago");
        assert_eq!(ago(60 * 60 * 35), "35 hours ago");
        // 36 hours crosses into days.
        assert_eq!(ago(60 * 60 * 36), "2 days ago");
        assert_eq!(ago(86400 * 13), "13 days ago");
        // 14 days becomes weeks, not "2 weeks" until rounding says so.
        assert_eq!(ago(86400 * 14), "2 weeks ago");
        assert_eq!(ago(86400 * 69), "10 weeks ago");
        // 70 days crosses into months.
        assert_eq!(ago(86400 * 70), "2 months ago");
        assert_eq!(ago(86400 * 364), "12 months ago");
        assert_eq!(ago(86400 * 365), "1 year ago");
    }

    #[test]
    fn names_leftover_months_under_five_years() {
        let now = 1_000_000_000i64;
        // git says "1 year, 5 months ago" rather than rounding to a year.
        let value = relative_date(now - 86400 * 520, now);
        assert_eq!(value, "1 year, 5 months ago");
    }

    #[test]
    fn collapses_to_years_past_five() {
        let now = 1_000_000_000i64;
        let value = relative_date(now - 86400 * 2000, now);
        assert_eq!(value, "5 years ago");
    }

    #[test]
    fn handles_clock_skew_without_panicking() {
        // A commit stamped in the future is not an error; git prints the
        // relative form regardless, and underflow here would panic on u64.
        assert_eq!(relative_date(2_000, 1_000), "in the future");
    }

    // ----------------------------------------------------------------------
    // Equivalence with the git binary
    //
    // The point of these is not that gix works — it is that gix and git agree.
    // A read backend that is fast and subtly different is worse than a slow
    // one, so each of these builds a real repository and compares field by
    // field against the shell output the app used to rely on.
    // ----------------------------------------------------------------------

    fn run(repo: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?} failed");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn scratch_repo(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("gitspark-gix-{name}-{unique}"));
        fs::create_dir_all(&path).unwrap();
        run(&path, &["init", "-b", "main", "."]);
        run(&path, &["config", "user.name", "GitSpark Test"]);
        run(&path, &["config", "user.email", "test@gitspark.local"]);
        fs::write(path.join("a.txt"), "a\n").unwrap();
        run(&path, &["add", "--all"]);
        run(&path, &["commit", "-m", "initial"]);
        path
    }

    #[test]
    fn repo_root_matches_rev_parse_show_toplevel() {
        let repo = scratch_repo("root");
        let shell = PathBuf::from(run(&repo, &["rev-parse", "--show-toplevel"]).trim());
        let gix = repo_root(&repo).expect("gix resolved the root");
        // Compare canonicalized: /tmp is a symlink to /private/tmp on macOS,
        // and the two backends report different sides of it.
        assert_eq!(
            fs::canonicalize(&gix).unwrap(),
            fs::canonicalize(&shell).unwrap()
        );
        fs::remove_dir_all(&repo).ok();
    }

    #[test]
    fn branches_match_for_each_ref() {
        let repo = scratch_repo("branches");
        run(&repo, &["branch", "feature/one"]);
        run(&repo, &["branch", "Feature-Two"]);
        run(&repo, &["branch", "zzz"]);

        let shell: Vec<(String, bool)> = run(
            &repo,
            &[
                "for-each-ref",
                "--format=%(refname:short)\t%(HEAD)",
                "refs/heads",
                "refs/remotes",
            ],
        )
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let name = parts.next()?.trim().to_string();
            let head = parts.next().unwrap_or("").trim() == "*";
            (!name.is_empty() && !name.ends_with("/HEAD")).then_some((name, head))
        })
        .collect();

        let gix: Vec<(String, bool)> = branches(&repo)
            .expect("gix listed branches")
            .into_iter()
            .map(|b| (b.name, b.is_current))
            .collect();

        let mut shell_sorted = shell.clone();
        shell_sorted.sort();
        let mut gix_sorted = gix.clone();
        gix_sorted.sort();
        assert_eq!(
            gix_sorted, shell_sorted,
            "branch set or current flag differs"
        );

        // The current branch must sort first, as the shell path guaranteed.
        assert!(gix[0].1, "current branch is not first");
        fs::remove_dir_all(&repo).ok();
    }

    #[test]
    fn branch_dates_match_git_wording() {
        let repo = scratch_repo("branch-dates");
        let shell = run(
            &repo,
            &[
                "for-each-ref",
                "--format=%(committerdate:relative)",
                "refs/heads/main",
            ],
        )
        .trim()
        .to_string();
        let gix = branches(&repo)
            .expect("gix listed branches")
            .into_iter()
            .find(|b| b.name == "main")
            .and_then(|b| b.updated)
            .expect("gix produced a date");
        assert_eq!(gix, shell, "relative date wording differs from git");
        fs::remove_dir_all(&repo).ok();
    }

    #[test]
    fn history_matches_git_log_including_tags() {
        let repo = scratch_repo("history");
        for i in 0..4 {
            fs::write(repo.join(format!("f{i}.txt")), format!("{i}\n")).unwrap();
            run(&repo, &["add", "--all"]);
            run(&repo, &["commit", "-m", &format!("commit {i}")]);
        }
        run(&repo, &["tag", "v1.0.0"]);
        run(&repo, &["tag", "-a", "v1.1.0", "-m", "annotated"]);

        let shell: Vec<(String, String, String)> = run(
            &repo,
            &["log", "-n50", "HEAD", "--pretty=format:%H\x1f%s\x1f%ar"],
        )
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let mut p = line.split('\x1f');
            (
                p.next().unwrap_or("").to_string(),
                p.next().unwrap_or("").to_string(),
                p.next().unwrap_or("").to_string(),
            )
        })
        .collect();

        let gix = history(&repo, 50).expect("gix walked history");
        assert_eq!(gix.len(), shell.len(), "commit count differs");

        for (got, want) in gix.iter().zip(shell.iter()) {
            assert_eq!(got.oid, want.0, "oid or order differs");
            assert_eq!(got.summary, want.1, "summary differs for {}", want.0);
            assert_eq!(got.date, want.2, "relative date differs for {}", want.0);
        }
        assert!(gix[0].is_head, "first commit should be HEAD");

        // Both a lightweight and an annotated tag must resolve to HEAD.
        let head_tags = &gix[0].tags;
        assert!(
            head_tags.contains(&"v1.0.0".to_string()),
            "lightweight tag missing: {head_tags:?}"
        );
        assert!(
            head_tags.contains(&"v1.1.0".to_string()),
            "annotated tag missing: {head_tags:?}"
        );
        fs::remove_dir_all(&repo).ok();
    }

    #[test]
    fn remote_lookup_matches_git() {
        let repo = scratch_repo("remote");
        assert_eq!(primary_remote(&repo), Some(None), "no remotes yet");

        run(
            &repo,
            &["remote", "add", "upstream", "https://example.com/u.git"],
        );
        run(
            &repo,
            &["remote", "add", "origin", "https://example.com/o.git"],
        );

        // origin wins over an alphabetically earlier remote, as git's caller did.
        assert_eq!(primary_remote(&repo), Some(Some("origin".to_string())));
        assert_eq!(
            remote_url(&repo, "origin").as_deref(),
            Some(run(&repo, &["remote", "get-url", "origin"]).trim())
        );
        fs::remove_dir_all(&repo).ok();
    }
}
