//! Signature and checksum verification for update artifacts.
//!
//! Two independent gates, and both matter:
//!
//! 1. The manifest carries a detached Ed25519 signature, checked against a
//!    public key compiled into this binary. Whoever serves the metadata
//!    cannot substitute their own — a compromised Pages host or a MITM can
//!    withhold an update but cannot point the updater at a hostile artifact.
//! 2. The artifact's SHA-256 is checked against the digest in that
//!    now-trusted manifest, before anything is unpacked or executed.
//!
//! This is the whole trust model of the updater. Everything downstream —
//! staging, replacing the installed app, restarting — assumes these passed.

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

/// Verify a detached Ed25519 signature over `data`.
///
/// `public_key_b64` is the base64 32-byte verifying key compiled into the
/// binary; `signature_b64` is the base64 64-byte signature fetched alongside
/// the manifest.
pub fn verify_ed25519_signature(
    public_key_b64: &str,
    signature_b64: &str,
    data: &[u8],
) -> Result<()> {
    let key_bytes = BASE64
        .decode(public_key_b64.trim())
        .context("update public key is not valid base64")?;
    let key: [u8; 32] = key_bytes
        .try_into()
        .map_err(|bytes: Vec<u8>| anyhow!("public key must be 32 bytes, got {}", bytes.len()))?;
    let verifying_key = VerifyingKey::from_bytes(&key).context("invalid Ed25519 public key")?;

    let sig_bytes = BASE64
        .decode(signature_b64.trim())
        .context("update signature is not valid base64")?;
    let sig: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|bytes: Vec<u8>| anyhow!("signature must be 64 bytes, got {}", bytes.len()))?;

    verifying_key
        .verify(data, &Signature::from_bytes(&sig))
        .context("update manifest signature is invalid — refusing to trust it")
}

/// Hex-encoded SHA-256 of `data`.
pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// Check `data` against an expected hex SHA-256 from a verified manifest.
///
/// The comparison is case-insensitive on the expected side only, because
/// `hex::encode` always produces lowercase.
pub fn verify_sha256(data: &[u8], expected_hex: &str) -> Result<()> {
    let actual = sha256_hex(data);
    if actual != expected_hex.trim().to_lowercase() {
        bail!("artifact checksum mismatch: expected {expected_hex}, got {actual}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    /// A throwaway keypair. The production private key lives in CI secrets and
    /// never appears here; these tests only prove the verification logic.
    fn test_keypair() -> (String, SigningKey) {
        // Deterministic seed keeps failures reproducible.
        let signing = SigningKey::from_bytes(&[7u8; 32]);
        let public = BASE64.encode(signing.verifying_key().as_bytes());
        (public, signing)
    }

    fn sign(key: &SigningKey, data: &[u8]) -> String {
        BASE64.encode(key.sign(data).to_bytes())
    }

    #[test]
    fn accepts_a_genuine_signature() {
        let (public, signing) = test_keypair();
        let data = b"{\"version\":\"1.0.0\"}";
        assert!(verify_ed25519_signature(&public, &sign(&signing, data), data).is_ok());
    }

    #[test]
    fn rejects_a_tampered_manifest() {
        // The attack this defends against: the signature is genuine, but the
        // bytes it covers were altered in transit.
        let (public, signing) = test_keypair();
        let signature = sign(&signing, b"{\"version\":\"1.0.0\"}");
        assert!(
            verify_ed25519_signature(&public, &signature, b"{\"version\":\"9.9.9\"}").is_err(),
            "altered manifest was accepted"
        );
    }

    #[test]
    fn rejects_a_signature_from_the_wrong_key() {
        // A host that serves its own manifest AND its own signature still
        // fails, because the verifying key is compiled in.
        let (public, _) = test_keypair();
        let attacker = SigningKey::from_bytes(&[9u8; 32]);
        let data = b"malicious manifest";
        assert!(verify_ed25519_signature(&public, &sign(&attacker, data), data).is_err());
    }

    #[test]
    fn rejects_malformed_keys_and_signatures() {
        let (public, signing) = test_keypair();
        let data = b"payload";
        let good = sign(&signing, data);

        assert!(verify_ed25519_signature("not base64!!", &good, data).is_err());
        assert!(verify_ed25519_signature(&public, "not base64!!", data).is_err());
        // Right encoding, wrong length — must not panic on the array convert.
        assert!(verify_ed25519_signature(&BASE64.encode([0u8; 16]), &good, data).is_err());
        assert!(verify_ed25519_signature(&public, &BASE64.encode([0u8; 8]), data).is_err());
    }

    #[test]
    fn rejects_an_all_zero_signature() {
        let (public, _) = test_keypair();
        assert!(verify_ed25519_signature(&public, &BASE64.encode([0u8; 64]), b"x").is_err());
    }

    #[test]
    fn sha256_matches_the_known_digest_of_an_empty_input() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_accepts_a_match_in_either_case() {
        let data = b"gitspark";
        let expected = sha256_hex(data);
        assert!(verify_sha256(data, &expected).is_ok());
        assert!(verify_sha256(data, &expected.to_uppercase()).is_ok());
    }

    #[test]
    fn sha256_rejects_a_mismatch() {
        assert!(verify_sha256(b"gitspark", &"0".repeat(64)).is_err());
    }
}
