//! Deciding whether an update applies.
//!
//! Split deliberately in two: [`decide`] is pure and holds every rule worth
//! arguing about, while [`check_for_update`] only does I/O. The rules are
//! where the bugs live — offering a release user a beta, or re-offering the
//! version already running — so they are testable without a network.

use anyhow::{Context, Result, bail};

use super::channel::UpdateChannel;
use super::manifest::{AssetInfo, UpdateManifest};
use super::verify::verify_ed25519_signature;

/// Where signed channel metadata lives.
///
/// Overridable at build time so a fork or a staging host does not require a
/// code change.
pub const DEFAULT_UPDATE_BASE_URL: &str = match option_env!("GITSPARK_UPDATE_BASE_URL") {
    Some(url) => url,
    None => "https://jacobsamro.github.io/GitSpark",
};

/// Ed25519 public key that signs every manifest, base64.
///
/// The matching private key lives only in CI secrets. While this is empty the
/// updater refuses to act at all — see [`update_public_key`]. Failing closed
/// is the only safe default: an unsigned-but-accepted manifest would let
/// whoever serves the metadata choose what code users run.
pub const UPDATE_PUBLIC_KEY: &str = match option_env!("GITSPARK_UPDATE_PUBLIC_KEY") {
    Some(key) => key,
    None => "",
};

/// The configured key, or `None` if the build has no key baked in.
pub fn update_public_key() -> Option<&'static str> {
    let key = UPDATE_PUBLIC_KEY.trim();
    (!key.is_empty()).then_some(key)
}

/// What a check concluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateDecision {
    /// Nothing newer on this channel.
    UpToDate,
    /// The manifest is newer but has no artifact for this platform. Distinct
    /// from `UpToDate` because it is a broken release, not a quiet one, and
    /// the caller may want to say so rather than claim we are current.
    NoAssetForPlatform { version: String },
    /// A newer version with a usable artifact.
    Available {
        version: String,
        notes: String,
        notes_url: Option<String>,
        asset: AssetInfo,
    },
}

/// Apply the update rules to an already-verified manifest.
pub fn decide(
    manifest: &UpdateManifest,
    channel: UpdateChannel,
    current: &semver::Version,
) -> Result<UpdateDecision> {
    let candidate = manifest
        .semver()
        .with_context(|| format!("manifest version '{}' is not semver", manifest.version))?;

    // A release user must never be moved onto a pre-release, even if the
    // channel's own manifest somehow advertises one.
    if !channel.accepts(&candidate) {
        return Ok(UpdateDecision::UpToDate);
    }

    // Strictly greater: equal means we are running it, and older means a
    // rollback we were not asked to perform.
    if candidate <= *current {
        return Ok(UpdateDecision::UpToDate);
    }

    let Some(asset) = manifest.asset_for_current_platform() else {
        return Ok(UpdateDecision::NoAssetForPlatform {
            version: candidate.to_string(),
        });
    };

    Ok(UpdateDecision::Available {
        version: candidate.to_string(),
        notes: manifest.notes.clone(),
        notes_url: manifest.notes_url.clone(),
        asset: asset.clone(),
    })
}

/// Fetch the channel manifest, verify its signature, and decide.
///
/// Blocking, to be called from a worker thread — the app has no async runtime
/// and this must never touch the UI thread.
pub fn check_for_update(
    channel: UpdateChannel,
    current: &semver::Version,
) -> Result<UpdateDecision> {
    let Some(public_key) = update_public_key() else {
        bail!("this build has no update signing key, so updates are disabled");
    };

    let base = DEFAULT_UPDATE_BASE_URL.trim_end_matches('/');
    let manifest_url = format!("{base}/updates/{}/latest.json", channel.path());
    let signature_url = format!("{manifest_url}.sig");

    let agent = crate::ai::http_agent();
    let manifest_bytes = fetch(&agent, &manifest_url)?;
    let signature = String::from_utf8(fetch(&agent, &signature_url)?)
        .context("update signature file is not valid UTF-8")?;

    // Verify BEFORE parsing. Deserializing attacker-controlled JSON first
    // would mean acting on unverified structure.
    verify_ed25519_signature(public_key, &signature, &manifest_bytes)?;

    let manifest: UpdateManifest =
        serde_json::from_slice(&manifest_bytes).context("update manifest is not valid JSON")?;

    if manifest.channel != channel {
        // A manifest served under the wrong channel path is a publishing
        // mistake at best; refusing keeps beta content off the stable track.
        bail!(
            "manifest at the {} URL declares channel {}",
            channel,
            manifest.channel
        );
    }

    decide(&manifest, channel, current)
}

fn fetch(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>> {
    let mut response = agent
        .get(url)
        .call()
        .with_context(|| format!("failed to fetch {url}"))?;
    if response.status().as_u16() >= 400 {
        bail!("{url} returned HTTP {}", response.status().as_u16());
    }
    let mut body = Vec::new();
    std::io::Read::read_to_end(&mut response.body_mut().as_reader(), &mut body)
        .with_context(|| format!("failed to read {url}"))?;
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update::manifest::AssetInfo;
    use std::collections::HashMap;

    fn v(text: &str) -> semver::Version {
        semver::Version::parse(text).unwrap()
    }

    fn manifest_with(channel: UpdateChannel, version: &str, platforms: &[&str]) -> UpdateManifest {
        UpdateManifest {
            channel,
            version: version.to_string(),
            pub_date: "2026-08-21T12:00:00Z".into(),
            notes: "notes".into(),
            notes_url: None,
            assets: platforms
                .iter()
                .map(|key| {
                    (
                        (*key).to_string(),
                        AssetInfo {
                            url: "https://example.invalid/a".into(),
                            sha256: "00".into(),
                            size: 1,
                        },
                    )
                })
                .collect::<HashMap<_, _>>(),
        }
    }

    fn here() -> Vec<&'static str> {
        // Leak is fine in tests; keeps the helper signature simple.
        super::super::manifest::platform_keys()
            .into_iter()
            .map(|key| Box::leak(key.into_boxed_str()) as &'static str)
            .collect()
    }

    #[test]
    fn offers_a_newer_release() {
        let manifest = manifest_with(UpdateChannel::Release, "0.6.0", &here());
        let decision = decide(&manifest, UpdateChannel::Release, &v("0.5.0")).unwrap();
        assert!(matches!(decision, UpdateDecision::Available { .. }));
    }

    #[test]
    fn does_not_offer_the_version_already_running() {
        let manifest = manifest_with(UpdateChannel::Release, "0.5.0", &here());
        assert_eq!(
            decide(&manifest, UpdateChannel::Release, &v("0.5.0")).unwrap(),
            UpdateDecision::UpToDate
        );
    }

    #[test]
    fn never_downgrades() {
        // A rolled-back manifest must not drag a newer install backwards.
        let manifest = manifest_with(UpdateChannel::Release, "0.4.0", &here());
        assert_eq!(
            decide(&manifest, UpdateChannel::Release, &v("0.5.0")).unwrap(),
            UpdateDecision::UpToDate
        );
    }

    #[test]
    fn release_users_are_never_offered_a_prerelease() {
        // Even if the release manifest wrongly advertises one.
        let manifest = manifest_with(UpdateChannel::Release, "0.6.0-beta.1", &here());
        assert_eq!(
            decide(&manifest, UpdateChannel::Release, &v("0.5.0")).unwrap(),
            UpdateDecision::UpToDate
        );
    }

    #[test]
    fn beta_users_are_offered_prereleases_and_the_stable_that_follows() {
        let beta = manifest_with(UpdateChannel::Beta, "0.6.0-beta.2", &here());
        assert!(matches!(
            decide(&beta, UpdateChannel::Beta, &v("0.6.0-beta.1")).unwrap(),
            UpdateDecision::Available { .. }
        ));

        // 0.6.0 supersedes 0.6.0-beta.2 by semver precedence.
        let stable = manifest_with(UpdateChannel::Beta, "0.6.0", &here());
        assert!(matches!(
            decide(&stable, UpdateChannel::Beta, &v("0.6.0-beta.2")).unwrap(),
            UpdateDecision::Available { .. }
        ));
    }

    #[test]
    fn a_partial_release_is_reported_not_offered() {
        // Newer version, but this platform's build failed to publish.
        let manifest = manifest_with(UpdateChannel::Release, "0.6.0", &["not-our-platform"]);
        assert_eq!(
            decide(&manifest, UpdateChannel::Release, &v("0.5.0")).unwrap(),
            UpdateDecision::NoAssetForPlatform {
                version: "0.6.0".into()
            }
        );
    }

    #[test]
    fn a_nonsense_version_is_an_error_not_an_update() {
        let manifest = manifest_with(UpdateChannel::Release, "not-a-version", &here());
        assert!(decide(&manifest, UpdateChannel::Release, &v("0.5.0")).is_err());
    }

    /// The full chain a real check performs, minus the HTTP hop: sign a
    /// manifest, verify it against a pinned key, parse it, decide.
    ///
    /// Each piece is unit-tested above; this proves they compose, and that a
    /// byte changed anywhere in the manifest is caught before `decide` ever
    /// sees it.
    #[test]
    fn signed_manifest_round_trips_through_verify_parse_and_decide() {
        use base64::Engine as _;
        use base64::engine::general_purpose::STANDARD as BASE64;
        use ed25519_dalek::{Signer, SigningKey};

        let signing = SigningKey::from_bytes(&[11u8; 32]);
        let public = BASE64.encode(signing.verifying_key().as_bytes());

        let manifest = manifest_with(UpdateChannel::Release, "0.6.0", &here());
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let signature = BASE64.encode(signing.sign(&bytes).to_bytes());

        // Happy path.
        verify_ed25519_signature(&public, &signature, &bytes).expect("signature verifies");
        let parsed: UpdateManifest = serde_json::from_slice(&bytes).expect("parses");
        assert!(matches!(
            decide(&parsed, UpdateChannel::Release, &v("0.5.0")).unwrap(),
            UpdateDecision::Available { .. }
        ));

        // Tampered: bump the advertised version, keep the old signature. This
        // is the attack the pinned key exists to stop.
        let tampered = String::from_utf8(bytes.clone())
            .unwrap()
            .replace("0.6.0", "9.9.9");
        assert!(
            verify_ed25519_signature(&public, &signature, tampered.as_bytes()).is_err(),
            "a modified manifest passed verification"
        );
    }

    /// Guards the wiring, not the logic.
    ///
    /// The key is supplied by `.cargo/config.toml` via `option_env!`, which
    /// is silent when absent — a typo in that file would leave the updater
    /// permanently disabled with no error anywhere.
    #[test]
    fn the_configured_public_key_is_present_and_well_formed() {
        use base64::Engine as _;
        use base64::engine::general_purpose::STANDARD as BASE64;

        let key = update_public_key().expect(
            "no update public key compiled in — check GITSPARK_UPDATE_PUBLIC_KEY \
             in .cargo/config.toml",
        );
        let bytes = BASE64.decode(key).expect("public key is not valid base64");
        assert_eq!(bytes.len(), 32, "Ed25519 public keys are 32 bytes");

        // It must also be a usable curve point, not just 32 arbitrary bytes.
        let array: [u8; 32] = bytes.try_into().unwrap();
        ed25519_dalek::VerifyingKey::from_bytes(&array)
            .expect("compiled-in key is not a valid Ed25519 public key");
    }

    #[test]
    fn the_configured_base_url_is_absolute_and_https() {
        // A relative or http URL would silently defeat transport security and
        // is not something a signature check compensates for.
        assert!(
            DEFAULT_UPDATE_BASE_URL.starts_with("https://"),
            "update base URL must be https, got {DEFAULT_UPDATE_BASE_URL}"
        );
    }

    #[test]
    fn a_build_without_a_signing_key_refuses_to_check() {
        // Fail closed: with no pinned key there is nothing to verify against,
        // and accepting an unsigned manifest would let the metadata host pick
        // what code users run.
        if update_public_key().is_none() {
            let result = check_for_update(UpdateChannel::Release, &v("0.5.0"));
            assert!(result.is_err(), "unsigned build must not check for updates");
        }
    }
}
