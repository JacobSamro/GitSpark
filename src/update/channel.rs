//! Update channels.
//!
//! Two tracks, distinguished by the tag shape the release workflow produces:
//! `v0.5.0` is `release`, `v0.6.0-beta.1` is `beta`.
//!
//! Clients never resolve updates through GitHub's `latest release` endpoint.
//! That endpoint only ever returns the newest NON-prerelease, so a beta user
//! asking it "what is newest" gets the wrong answer by design. Each channel
//! instead has its own manifest at a stable URL.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateChannel {
    /// Stable tags: `vX.Y.Z`.
    #[default]
    Release,
    /// Pre-release tags: `vX.Y.Z-beta.N`.
    Beta,
}

impl UpdateChannel {
    /// Path segment under the metadata host.
    pub fn path(self) -> &'static str {
        match self {
            UpdateChannel::Release => "release",
            UpdateChannel::Beta => "beta",
        }
    }

    /// Which channel a version belongs to.
    ///
    /// Any pre-release identifier means beta. `0.6.0-beta.1` and
    /// `0.6.0-rc.1` are both previews of `0.6.0` and belong on the same
    /// track — treating them as separate channels would strand anyone on an
    /// `rc` build with nothing to update to.
    pub fn from_version(version: &semver::Version) -> Self {
        if version.pre.is_empty() {
            UpdateChannel::Release
        } else {
            UpdateChannel::Beta
        }
    }

    /// Whether a version is acceptable on this channel.
    ///
    /// Release users must never be offered a pre-release. Beta users accept
    /// both, because a beta track is a preview OF the next stable — once
    /// `0.6.0` ships it supersedes `0.6.0-beta.2`, and refusing it would
    /// leave beta users permanently behind.
    pub fn accepts(self, version: &semver::Version) -> bool {
        match self {
            UpdateChannel::Release => version.pre.is_empty(),
            UpdateChannel::Beta => true,
        }
    }
}

impl fmt::Display for UpdateChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.path())
    }
}

impl FromStr for UpdateChannel {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "release" | "stable" => Ok(UpdateChannel::Release),
            "beta" | "preview" => Ok(UpdateChannel::Beta),
            // An unknown value on disk is a settings file from a future or
            // corrupted build. Falling back to release is the safe read: it
            // can only ever offer fewer updates, never more.
            _ => Ok(UpdateChannel::Release),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::UpdateChannel;
    use semver::Version;

    fn v(text: &str) -> Version {
        Version::parse(text).expect("valid semver")
    }

    #[test]
    fn classifies_versions_by_prerelease_tag() {
        assert_eq!(UpdateChannel::from_version(&v("0.5.0")), UpdateChannel::Release);
        assert_eq!(
            UpdateChannel::from_version(&v("0.6.0-beta.1")),
            UpdateChannel::Beta
        );
        assert_eq!(
            UpdateChannel::from_version(&v("0.6.0-rc.1")),
            UpdateChannel::Beta,
            "rc builds belong on the same preview track as beta"
        );
    }

    #[test]
    fn release_never_accepts_a_prerelease() {
        assert!(UpdateChannel::Release.accepts(&v("0.6.0")));
        assert!(!UpdateChannel::Release.accepts(&v("0.6.0-beta.1")));
    }

    #[test]
    fn beta_accepts_the_stable_that_supersedes_it() {
        // Once 0.6.0 ships it is newer than 0.6.0-beta.2 by semver, and a
        // beta user must be able to move onto it.
        assert!(UpdateChannel::Beta.accepts(&v("0.6.0")));
        assert!(UpdateChannel::Beta.accepts(&v("0.6.0-beta.2")));
        assert!(v("0.6.0") > v("0.6.0-beta.2"));
    }

    #[test]
    fn parses_known_names_and_falls_back_safely() {
        assert_eq!("release".parse::<UpdateChannel>().unwrap(), UpdateChannel::Release);
        assert_eq!("  BETA ".parse::<UpdateChannel>().unwrap(), UpdateChannel::Beta);
        // A value from a future build must not fail the app open; release is
        // the conservative read.
        assert_eq!("nightly".parse::<UpdateChannel>().unwrap(), UpdateChannel::Release);
    }
}
