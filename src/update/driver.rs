//! Driving one full update cycle: check, download, report.
//!
//! Deliberately free of any GPUI types. Everything here is blocking and runs
//! on a worker thread, and the only way it talks to the UI is by handing
//! finished [`UpdateState`] values to a `report` callback. That keeps the
//! interesting part — what the user is told, and in what order — testable
//! without a window, and keeps the app's copy of this logic down to spawning
//! a thread and draining a channel.

use std::sync::Arc;

use anyhow::Result;

use super::channel::UpdateChannel;
use super::check::{UpdateDecision, check_for_update};
use super::download::{download_artifact, prune_staging};
use super::{UpdateState, current_version};

/// Where a run sends its state transitions.
///
/// Shared rather than borrowed because the download hands its progress
/// callback off as a `'static` box, which cannot hold a reference to a
/// caller's stack.
pub type Reporter = Arc<dyn Fn(UpdateState) + Send + Sync>;

/// Run a check and, if something is available, download and verify it.
///
/// `report` is called on every state transition, ending on exactly one of
/// `Idle` (nothing to do), `ReadyToInstall`, or `Failed`. Callers can rely on
/// that: the indicator must never be left showing a spinner.
pub fn run(channel: UpdateChannel, report: Reporter) {
    report(UpdateState::Checking);
    match run_inner(channel, &report) {
        Ok(state) => report(state),
        // Anything that went wrong — offline, bad signature, checksum
        // mismatch — surfaces as one failure message. The user cannot act on
        // the distinction, and the detail is already in the error text.
        Err(error) => report(UpdateState::Failed {
            message: format!("{error:#}"),
        }),
    }
}

fn run_inner(channel: UpdateChannel, report: &Reporter) -> Result<UpdateState> {
    let current = current_version()?;

    let decision = check_for_update(channel, &current)?;
    let (version, asset) = match decision {
        // A release with no artifact for this platform is not an update we can
        // offer, so it is reported the same as being current rather than
        // dangling a version the user can never install.
        UpdateDecision::UpToDate
        | UpdateDecision::NoManifestPublished
        | UpdateDecision::NoAssetForPlatform { .. } => {
            return Ok(UpdateState::Idle);
        }
        UpdateDecision::Available { version, asset, .. } => (version, asset),
    };

    report(UpdateState::Downloading {
        version: version.clone(),
        percent: 0,
    });

    let progress_version = version.clone();
    let progress_report = Arc::clone(report);
    let artifact = download_artifact(
        &asset,
        &version,
        Some(Box::new(move |downloaded, total| {
            // A zero total means the server sent no Content-Length; hold at 0
            // rather than inventing a percentage, and never let rounding push
            // the label past 100%.
            let percent = if total > 0 {
                ((downloaded * 100) / total).min(100) as u8
            } else {
                0
            };
            progress_report(UpdateState::Downloading {
                version: progress_version.clone(),
                percent,
            });
        })),
    )?;

    // Older staged artifacts are dead weight once a newer one verifies; a
    // failure to tidy up is not a reason to refuse the update.
    let _ = prune_staging(Some(&version));

    Ok(UpdateState::ReadyToInstall { version, artifact })
}

/// The channel this build belongs to, from its own version string.
///
/// A prerelease build follows beta; everything else follows stable. Deriving
/// it rather than storing a preference means a user who installs a beta is on
/// the beta channel by construction, with no setting to get out of sync.
pub fn channel_for_this_build() -> UpdateChannel {
    current_version()
        .map(|version| UpdateChannel::from_version(&version))
        .unwrap_or(UpdateChannel::Release)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn a_run_always_reaches_a_terminal_state() {
        // The contract the indicator depends on: a run always ends on a state
        // that is not `Checking`, whatever the network did. If this regresses
        // the title bar spins forever.
        let seen: Arc<Mutex<Vec<UpdateState>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        run(
            UpdateChannel::Release,
            Arc::new(move |state| sink.lock().unwrap().push(state)),
        );

        let seen = seen.lock().unwrap();
        assert_eq!(
            seen.first(),
            Some(&UpdateState::Checking),
            "a run must announce that it started"
        );
        let last = seen.last().expect("at least one state");
        assert!(
            !matches!(
                last,
                UpdateState::Checking | UpdateState::Downloading { .. }
            ),
            "run ended on a non-terminal state: {last:?}"
        );
    }

    #[test]
    fn this_build_resolves_to_a_channel() {
        // Whatever the crate version is, it must map to a channel — the
        // updater has no "no channel" mode to fall back on.
        let channel = channel_for_this_build();
        assert!(matches!(
            channel,
            UpdateChannel::Release | UpdateChannel::Beta
        ));
    }
}
