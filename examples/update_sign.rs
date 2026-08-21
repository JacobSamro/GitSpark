//! Sign an update manifest with the release private key.
//!
//! ```sh
//! GITSPARK_UPDATE_PRIVATE_KEY="$SECRET" \
//!   cargo run --release --example update_sign -- updates/release/latest.json
//! ```
//!
//! Writes `<manifest>.sig` containing the base64 Ed25519 signature over the
//! manifest's exact bytes. Run by the release workflow; the private key comes
//! from the environment and is never written to disk or printed.
//!
//! The signature covers the file byte for byte, so anything that rewrites the
//! JSON afterwards — reformatting, a trailing newline added by a later step —
//! invalidates it. Sign last.

use std::fs;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::{Signer, SigningKey};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(manifest_path) = std::env::args().nth(1) else {
        eprintln!("usage: update_sign <manifest-path>");
        std::process::exit(2);
    };

    let key_b64 = std::env::var("GITSPARK_UPDATE_PRIVATE_KEY")
        .map_err(|_| "GITSPARK_UPDATE_PRIVATE_KEY is not set")?;
    let key_bytes = BASE64.decode(key_b64.trim())?;
    let key: [u8; 32] = key_bytes
        .try_into()
        .map_err(|bytes: Vec<u8>| format!("private key must be 32 bytes, got {}", bytes.len()))?;
    let signing = SigningKey::from_bytes(&key);

    let manifest = fs::read(&manifest_path)?;
    let signature = BASE64.encode(signing.sign(&manifest).to_bytes());

    let sig_path = format!("{manifest_path}.sig");
    fs::write(&sig_path, format!("{signature}\n"))?;

    // Verify what was just written, so a broken release fails here rather than
    // on every user's machine.
    let written = fs::read_to_string(&sig_path)?;
    let check = BASE64.decode(written.trim())?;
    let sig: [u8; 64] = check
        .try_into()
        .map_err(|_| "written signature is not 64 bytes")?;
    ed25519_dalek::Verifier::verify(
        &signing.verifying_key(),
        &manifest,
        &ed25519_dalek::Signature::from_bytes(&sig),
    )?;

    // Public key only — a CI log is not a secret store.
    eprintln!("signed {manifest_path}");
    eprintln!("public key: {}", BASE64.encode(signing.verifying_key().as_bytes()));
    Ok(())
}
