//! Compare the three ways GitSpark could talk to git.
//!
//! Benchmarks the operations the app actually performs, taken from a
//! `GITSPARK_TRACE_GIT=1` session: status, history, branches, repo discovery,
//! remote lookup, per-file diff, and worktree listing.
//!
//! Run against any repository:
//!
//! ```sh
//! cargo run --release --example git_backends -- /path/to/repo
//! ```
//!
//! Reports the median of N runs per operation, because the first call pays
//! for cold OS caches and the mean would hide that.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const RUNS: usize = 15;

fn main() {
    let repo_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    println!("repo: {}", repo_path.display());
    println!("runs per op: {RUNS} (median reported)\n");

    let mut rows: Vec<Row> = Vec::new();
    rows.push(bench_discover(&repo_path));
    rows.push(bench_status(&repo_path));
    rows.push(bench_history(&repo_path));
    rows.push(bench_branches(&repo_path));
    rows.push(bench_remote(&repo_path));
    rows.push(bench_worktrees(&repo_path));

    print_table(&rows);
    verify_status_parity(&repo_path);
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct Row {
    op: &'static str,
    shell: Option<Duration>,
    git2: Option<Duration>,
    gix: Option<Duration>,
    /// Set when a backend cannot do this operation at all.
    note: &'static str,
}

fn median(mut samples: Vec<Duration>) -> Duration {
    samples.sort();
    samples[samples.len() / 2]
}

/// Time `f` `RUNS` times and return the median, or `None` if it ever failed.
fn time<T>(mut f: impl FnMut() -> Option<T>) -> Option<Duration> {
    let mut samples = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let start = Instant::now();
        f()?;
        samples.push(start.elapsed());
    }
    Some(median(samples))
}

fn ms(d: Option<Duration>) -> String {
    match d {
        Some(d) => format!("{:.2}", d.as_secs_f64() * 1000.0),
        None => "—".to_string(),
    }
}

fn print_table(rows: &[Row]) {
    println!(
        "{:<22} {:>10} {:>10} {:>10}   {:>7} {:>7}",
        "operation", "shell ms", "git2 ms", "gix ms", "git2×", "gix×"
    );
    println!("{}", "-".repeat(76));
    let mut totals = (Duration::ZERO, Duration::ZERO, Duration::ZERO);
    let mut comparable = true;

    for row in rows {
        let speedup = |v: Option<Duration>| match (row.shell, v) {
            (Some(s), Some(v)) if !v.is_zero() => {
                format!("{:.0}x", s.as_secs_f64() / v.as_secs_f64())
            }
            _ => "—".to_string(),
        };
        println!(
            "{:<22} {:>10} {:>10} {:>10}   {:>7} {:>7}  {}",
            row.op,
            ms(row.shell),
            ms(row.git2),
            ms(row.gix),
            speedup(row.git2),
            speedup(row.gix),
            row.note
        );
        match (row.shell, row.git2, row.gix) {
            (Some(s), Some(a), Some(b)) => {
                totals.0 += s;
                totals.1 += a;
                totals.2 += b;
            }
            _ => comparable = false,
        }
    }

    if comparable {
        println!("{}", "-".repeat(76));
        println!(
            "{:<22} {:>10} {:>10} {:>10}   {:>7} {:>7}",
            "TOTAL (all ops)",
            ms(Some(totals.0)),
            ms(Some(totals.1)),
            ms(Some(totals.2)),
            format!("{:.0}x", totals.0.as_secs_f64() / totals.1.as_secs_f64()),
            format!("{:.0}x", totals.0.as_secs_f64() / totals.2.as_secs_f64()),
        );
    } else {
        println!("{}", "-".repeat(76));
        println!("(no total: at least one backend cannot perform every operation)");
    }
}

/// A benchmark is worthless if the backends are not doing the same work.
///
/// `status` is the one that matters — it is run on every refresh — and the
/// three have very different defaults for untracked files, which is the
/// expensive part. Print what each actually counted.
fn verify_status_parity(repo: &Path) {
    let shell_count = shell(repo, &["status", "--porcelain=v2", "--untracked-files=all"])
        .map(|out| out.lines().filter(|l| !l.is_empty()).count())
        .unwrap_or(0);

    let git2_count = (|| {
        let r = git2::Repository::discover(repo).ok()?;
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true).recurse_untracked_dirs(true);
        Some(r.statuses(Some(&mut opts)).ok()?.len())
    })()
    .unwrap_or(0);

    let gix_count = (|| {
        let r = gix::discover(repo).ok()?;
        Some(
            r.status(gix::progress::Discard)
                .ok()?
                .index_worktree_submodules(gix::status::Submodule::AsConfigured {
                    check_dirty: false,
                })
                .into_iter(None)
                .ok()?
                .count(),
        )
    })()
    .unwrap_or(0);

    println!(
        "\nstatus entries counted — shell: {shell_count}, git2: {git2_count}, gix: {gix_count}"
    );
    if !(shell_count == git2_count && git2_count == gix_count) {
        println!("  ^ NOT equivalent: the status timings above are not comparable.");
    }
}

fn shell(repo: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

/// `rev-parse --show-toplevel` — ran 52 times in the traced session.
fn bench_discover(repo: &Path) -> Row {
    Row {
        op: "discover repo",
        shell: time(|| shell(repo, &["rev-parse", "--show-toplevel"])),
        git2: time(|| git2::Repository::discover(repo).ok().map(|_| ())),
        gix: time(|| gix::discover(repo).ok().map(|_| ())),
        note: "",
    }
}

/// `status --porcelain=v2` — the workhorse, run on every refresh.
fn bench_status(repo: &Path) -> Row {
    Row {
        op: "status",
        shell: time(|| {
            shell(
                repo,
                &[
                    "status",
                    "--porcelain=v2",
                    "--branch",
                    "--untracked-files=all",
                    "-z",
                ],
            )
        }),
        git2: time(|| {
            let repo = git2::Repository::discover(repo).ok()?;
            let mut opts = git2::StatusOptions::new();
            opts.include_untracked(true).recurse_untracked_dirs(true);
            let statuses = repo.statuses(Some(&mut opts)).ok()?;
            Some(statuses.len())
        }),
        gix: time(|| {
            let repo = gix::discover(repo).ok()?;
            let status = repo
                .status(gix::progress::Discard)
                .ok()?
                .index_worktree_submodules(gix::status::Submodule::AsConfigured {
                    check_dirty: false,
                })
                .into_iter(None)
                .ok()?;
            Some(status.count())
        }),
        note: "",
    }
}

/// `log -n100` — the history list.
fn bench_history(repo: &Path) -> Row {
    Row {
        op: "history (100)",
        shell: time(|| {
            shell(
                repo,
                &["log", "-n100", "--pretty=format:%H%x1f%s%x1f%an%x1f%ar"],
            )
        }),
        git2: time(|| {
            let repo = git2::Repository::discover(repo).ok()?;
            let mut walk = repo.revwalk().ok()?;
            walk.push_head().ok()?;
            let mut n = 0;
            for oid in walk.take(100) {
                let commit = repo.find_commit(oid.ok()?).ok()?;
                let _ = (
                    commit.summary().ok().flatten().unwrap_or("").to_string(),
                    commit.author().name().unwrap_or("").to_string(),
                );
                n += 1;
            }
            Some(n)
        }),
        gix: time(|| {
            let repo = gix::discover(repo).ok()?;
            let head = repo.head_id().ok()?;
            let mut n = 0;
            for info in head.ancestors().all().ok()?.take(100) {
                let info = info.ok()?;
                let commit = repo.find_commit(info.id).ok()?;
                let _ = (
                    commit.message().ok()?.summary().to_string(),
                    commit.author().ok()?.name.to_string(),
                );
                n += 1;
            }
            Some(n)
        }),
        note: "",
    }
}

/// `for-each-ref` — the branch list.
fn bench_branches(repo: &Path) -> Row {
    Row {
        op: "branches",
        shell: time(|| {
            shell(
                repo,
                &[
                    "for-each-ref",
                    "--format=%(refname:short)",
                    "refs/heads",
                    "refs/remotes",
                ],
            )
        }),
        git2: time(|| {
            let repo = git2::Repository::discover(repo).ok()?;
            let branches = repo.branches(None).ok()?;
            let mut n = 0;
            for b in branches {
                let (branch, _) = b.ok()?;
                let _ = branch.name().ok()?.map(str::to_string);
                n += 1;
            }
            Some(n)
        }),
        gix: time(|| {
            let repo = gix::discover(repo).ok()?;
            let platform = repo.references().ok()?;
            let mut n = 0;
            for r in platform.all().ok()? {
                let r = r.ok()?;
                let _ = r.name().shorten().to_string();
                n += 1;
            }
            Some(n)
        }),
        note: "",
    }
}

/// `remote get-url origin` — ran 47 times in the traced session.
fn bench_remote(repo: &Path) -> Row {
    Row {
        op: "remote url",
        shell: time(|| shell(repo, &["remote", "get-url", "origin"])),
        git2: time(|| {
            let repo = git2::Repository::discover(repo).ok()?;
            let remote = repo.find_remote("origin").ok()?;
            remote.url().ok().map(str::to_string)
        }),
        gix: time(|| {
            let repo = gix::discover(repo).ok()?;
            let remote = repo.find_remote("origin").ok()?;
            Some(
                remote
                    .url(gix::remote::Direction::Fetch)?
                    .to_bstring()
                    .len(),
            )
        }),
        note: "",
    }
}

/// `worktree list --porcelain`.
fn bench_worktrees(repo: &Path) -> Row {
    Row {
        op: "worktree list",
        shell: time(|| shell(repo, &["worktree", "list", "--porcelain"])),
        git2: time(|| {
            let repo = git2::Repository::discover(repo).ok()?;
            let names = repo.worktrees().ok()?;
            Some(names.len())
        }),
        // gix models worktrees but its listing API is still limited compared
        // to the other two; recorded as unavailable rather than faked.
        gix: None,
        note: "gix: API immature",
    }
}
