//! The `latest.json` contract served per channel.
//!
//! GitHub Releases stays the binary store; this manifest is the stable,
//! signed index that points at those assets. Splitting the two means the app
//! never has to interpret GitHub's release semantics, and the metadata host
//! can move without changing where downloads come from.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::channel::UpdateChannel;

/// The signed manifest for one channel.
///
/// Anything read from this is only trustworthy after
/// [`super::verify::verify_ed25519_signature`] has passed over the raw bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateManifest {
    pub channel: UpdateChannel,
    /// Semver, without the leading `v` the git tag carries.
    pub version: String,
    /// ISO-8601 publication timestamp.
    pub pub_date: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes_url: Option<String>,
    /// Assets keyed by platform, e.g. `macos-universal`, `windows-x64`.
    pub assets: HashMap<String, AssetInfo>,
}

/// One downloadable artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetInfo {
    pub url: String,
    /// Hex SHA-256, checked before the artifact is unpacked.
    pub sha256: String,
    #[serde(default)]
    pub size: u64,
}

impl UpdateManifest {
    pub fn semver(&self) -> Result<semver::Version, semver::Error> {
        semver::Version::parse(self.version.trim().trim_start_matches('v'))
    }

    /// The asset for this build, trying the most specific key first.
    pub fn asset_for_current_platform(&self) -> Option<&AssetInfo> {
        platform_keys()
            .into_iter()
            .find_map(|key| self.assets.get(&key))
    }
}

/// Platform keys for this build, most specific first.
///
/// macOS ships a universal binary today, but an arch-specific key is tried
/// first so a future split build works without a client change. Anything not
/// macOS or Windows is treated as Linux, matching the release workflow's
/// three artifacts.
pub fn platform_keys() -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        let arch = if cfg!(target_arch = "aarch64") {
            "macos-arm64"
        } else {
            "macos-x86_64"
        };
        vec![arch.to_string(), "macos-universal".to_string()]
    }
    #[cfg(target_os = "windows")]
    {
        vec!["windows-x64".to_string()]
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        vec!["linux-x64".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shaped exactly like the contract in the auto-update issue.
    const SAMPLE: &str = r#"{
        "channel": "beta",
        "version": "0.6.0-beta.1",
        "pub_date": "2026-08-21T12:00:00Z",
        "notes": "Preview build",
        "notes_url": "https://github.com/JacobSamro/GitSpark/releases/tag/v0.6.0-beta.1",
        "assets": {
            "macos-universal": {
                "url": "https://github.com/JacobSamro/GitSpark/releases/download/v0.6.0-beta.1/gitspark-v0.6.0-beta.1-universal2-apple-darwin.dmg",
                "sha256": "AABB",
                "size": 16777216
            },
            "windows-x64": {
                "url": "https://example.invalid/win.zip",
                "sha256": "ccdd",
                "size": 9437184
            },
            "linux-x64": {
                "url": "https://example.invalid/linux.tar.gz",
                "sha256": "eeff",
                "size": 12582912
            }
        }
    }"#;

    #[test]
    fn parses_the_documented_manifest_shape() {
        let manifest: UpdateManifest = serde_json::from_str(SAMPLE).expect("parses");
        assert_eq!(manifest.channel, UpdateChannel::Beta);
        assert_eq!(manifest.semver().unwrap().to_string(), "0.6.0-beta.1");
        assert_eq!(manifest.assets.len(), 3);
    }

    #[test]
    fn tolerates_a_leading_v_on_the_version() {
        // The tag is `v0.6.0`; a generator that forgets to strip it should not
        // break every client.
        let json = SAMPLE.replace("\"0.6.0-beta.1\"", "\"v0.6.0-beta.1\"");
        let manifest: UpdateManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(manifest.semver().unwrap().to_string(), "0.6.0-beta.1");
    }

    #[test]
    fn finds_an_asset_for_whatever_platform_the_tests_run_on() {
        let manifest: UpdateManifest = serde_json::from_str(SAMPLE).unwrap();
        assert!(
            manifest.asset_for_current_platform().is_some(),
            "no asset matched any of {:?}",
            platform_keys()
        );
    }

    #[test]
    fn reports_no_asset_when_this_platform_is_absent() {
        // A partial release — one platform's build failed — must read as "no
        // update for me", not as an update pointing at someone else's binary.
        let manifest = UpdateManifest {
            channel: UpdateChannel::Release,
            version: "9.9.9".into(),
            pub_date: "2026-08-21T12:00:00Z".into(),
            notes: String::new(),
            notes_url: None,
            assets: HashMap::from([(
                "some-other-platform".to_string(),
                AssetInfo {
                    url: "https://example.invalid/x".into(),
                    sha256: "00".into(),
                    size: 1,
                },
            )]),
        };
        assert!(manifest.asset_for_current_platform().is_none());
    }

    #[test]
    fn notes_are_optional() {
        let minimal = r#"{
            "channel": "release",
            "version": "1.0.0",
            "pub_date": "2026-08-21T12:00:00Z",
            "assets": {}
        }"#;
        let manifest: UpdateManifest = serde_json::from_str(minimal).expect("parses");
        assert!(manifest.notes.is_empty());
        assert!(manifest.notes_url.is_none());
    }
}
