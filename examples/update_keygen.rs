//! Generate the Ed25519 keypair that signs update manifests.
//!
//! ```sh
//! cargo run --release --example update_keygen -- /secure/path/private.key
//! ```
//!
//! The PUBLIC key is printed. The PRIVATE key is written only to the given
//! path, mode 600, and never to stdout — so it cannot end up in a terminal
//! transcript, a CI log, or a scrollback buffer.
//!
//! The private key belongs in CI secrets and nowhere else. Losing it means
//! rotating the public key in a release, which strands anyone who has not
//! updated yet; leaking it means an attacker can sign manifests your users
//! will trust and install.

use std::fs;
use std::io::Write;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::SigningKey;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(out_path) = std::env::args().nth(1) else {
        eprintln!("usage: update_keygen <private-key-output-path>");
        std::process::exit(2);
    };

    // 32 bytes straight from the OS CSPRNG, which is exactly what
    // SigningKey::generate does with an OsRng — read directly so the shipped
    // binary does not gain a rand feature for the sake of a build-time tool.
    let seed = read_urandom()?;

    let signing = SigningKey::from_bytes(&seed);
    let public = BASE64.encode(signing.verifying_key().as_bytes());
    let private = BASE64.encode(signing.to_bytes());

    write_private(&out_path, &private)?;

    // Only the public half is ever printed.
    println!("{public}");
    eprintln!("private key written to {out_path} (mode 600)");
    Ok(())
}

fn read_urandom() -> Result<[u8; 32], Box<dyn std::error::Error>> {
    use std::io::Read;
    let mut file = fs::File::open("/dev/urandom")?;
    let mut seed = [0u8; 32];
    file.read_exact(&mut seed)?;
    Ok(seed)
}

/// Write the private key with owner-only permissions, created that way rather
/// than chmod'ed afterwards so it is never briefly world-readable.
fn write_private(path: &str, contents: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(contents.as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}
