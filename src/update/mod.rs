//! Auto-update, following the Zed-style model used by the sibling Chitti Meet
//! desktop client.
//!
//! ## Trust model
//!
//! GitHub Releases stays the binary store. A signed `latest.json` per channel
//! is published separately and is the only thing the client interprets, so the
//! app never has to reason about GitHub's release semantics — notably that
//! GitHub's "latest release" endpoint returns only the newest NON-prerelease,
//! which is the wrong answer for anyone on beta.
//!
//! Two gates protect the user, in this order:
//!
//! 1. The manifest carries a detached Ed25519 signature verified against a key
//!    compiled into the binary. Whoever serves the metadata cannot substitute
//!    their own manifest.
//! 2. The artifact's SHA-256 is checked against the digest in that verified
//!    manifest before anything is unpacked.
//!
//! **This build fails closed.** With no key baked in, `check_for_update`
//! refuses rather than accepting an unsigned manifest — accepting one would
//! hand whoever hosts the metadata the ability to choose what code runs.
//!
//! ## The cycle
//!
//! [`driver::run`] performs check → download → verify on a worker thread and
//! reports each transition as an [`UpdateState`]. When it ends on
//! [`UpdateState::ReadyToInstall`], [`apply::install`] replaces the installed
//! application and [`apply::relaunch`] starts the new one. Nothing is applied
//! without the user asking: the download is silent, the restart is not.
//!
//! Applying differs per platform for reasons that are not stylistic — a
//! running executable cannot replace itself on Windows, and a macOS
//! application is a directory inside a disk image. Those live in [`apply`].

pub mod apply;
pub mod channel;
pub mod check;
pub mod download;
pub mod driver;
pub mod manifest;
pub mod verify;

use std::path::PathBuf;

use anyhow::{Context, Result};

/// This build's version, parsed.
///
/// Fallible rather than panicking: a malformed `CARGO_PKG_VERSION` should
/// disable updates, not take the app down on launch.
pub fn current_version() -> Result<semver::Version> {
    semver::Version::parse(env!("CARGO_PKG_VERSION")).with_context(|| {
        format!(
            "this build has an unparseable version: {}",
            env!("CARGO_PKG_VERSION")
        )
    })
}

/// What the UI shows, and the only update state the app holds.
///
/// Deliberately linear: check, download, ready. There is no "paused" or
/// "retry later" state because an update that failed is simply reported and
/// the user can ask again — a half-resumable state machine would be more
/// surface area than the feature is worth.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum UpdateState {
    /// Nothing has been checked yet, or the app is up to date. Shows nothing.
    #[default]
    Idle,
    /// A check is in flight.
    Checking,
    /// Newer version found; the download has not finished.
    Downloading { version: String, percent: u8 },
    /// Downloaded and verified. This is the "Restart to Update" state.
    ReadyToInstall { version: String, artifact: PathBuf },
    /// Something went wrong. Held so the user is told rather than left with a
    /// silently dead indicator.
    Failed { message: String },
}

impl UpdateState {
    /// Text for the title-bar indicator, or `None` to show nothing.
    ///
    /// `Idle` renders nothing at all: an always-present "up to date" badge is
    /// noise in a title bar the user looks at all day.
    pub fn indicator_label(&self) -> Option<String> {
        match self {
            UpdateState::Idle => None,
            UpdateState::Checking => Some("Checking for updates\u{2026}".to_string()),
            UpdateState::Downloading { percent, .. } => {
                Some(format!("Downloading update\u{2026} {percent}%"))
            }
            UpdateState::ReadyToInstall { version, .. } => {
                Some(format!("Restart to update to {version}"))
            }
            UpdateState::Failed { .. } => Some("Update failed".to_string()),
        }
    }

    /// Whether clicking the indicator should do something.
    pub fn is_actionable(&self) -> bool {
        matches!(
            self,
            UpdateState::ReadyToInstall { .. } | UpdateState::Failed { .. }
        )
    }
}
