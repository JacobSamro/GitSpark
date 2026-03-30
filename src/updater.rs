use std::cmp::Ordering;

use anyhow::{Context, Result, bail};

use crate::models::{UpdateChannel, UpdateManifest};

/// Base URL for the update metadata hosted on GitHub Pages.
/// Override by setting the `GITSPARK_UPDATE_URL` environment variable.
const DEFAULT_UPDATE_BASE_URL: &str = "https://jacobsamro.github.io/GitSpark";

/// Current version of the application, sourced from Cargo.toml at compile time.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Returns the base URL for update metadata.
fn update_base_url() -> String {
    std::env::var("GITSPARK_UPDATE_URL")
        .unwrap_or_else(|_| DEFAULT_UPDATE_BASE_URL.to_string())
}

/// Fetch the latest update manifest for the given channel from GitHub Pages.
pub fn fetch_manifest(channel: UpdateChannel) -> Result<UpdateManifest> {
    let url = format!("{}/updates/{}/latest.json", update_base_url(), channel.slug());
    let body: String = ureq::get(&url)
        .call()
        .with_context(|| format!("failed to fetch update manifest from {url}"))?
        .body_mut()
        .read_to_string()
        .with_context(|| "failed to read update manifest body")?;
    let manifest: UpdateManifest = serde_json::from_str(&body)
        .with_context(|| "failed to parse update manifest JSON")?;
    Ok(manifest)
}

/// Returns the platform key used in update manifests.
pub fn platform_key() -> &'static str {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    { "windows-x64" }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    { "linux-x64" }
    #[cfg(target_os = "macos")]
    { "macos-universal" }
    #[cfg(not(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
        target_os = "macos",
    )))]
    { "unknown" }
}

/// Compare two semver-style version strings.
///
/// Supports optional pre-release suffixes (`1.2.3-beta.1`).
/// Returns `Ordering::Less` if `current` is older than `latest`.
pub fn compare_versions(current: &str, latest: &str) -> Ordering {
    let parse = |v: &str| -> (Vec<u64>, Option<String>) {
        let v = v.strip_prefix('v').unwrap_or(v);
        if let Some((base, pre)) = v.split_once('-') {
            let nums: Vec<u64> = base.split('.').filter_map(|s| s.parse().ok()).collect();
            (nums, Some(pre.to_string()))
        } else {
            let nums: Vec<u64> = v.split('.').filter_map(|s| s.parse().ok()).collect();
            (nums, None)
        }
    };

    let (cur_nums, cur_pre) = parse(current);
    let (lat_nums, lat_pre) = parse(latest);

    // Compare numeric parts
    let max_len = cur_nums.len().max(lat_nums.len());
    for i in 0..max_len {
        let c = cur_nums.get(i).copied().unwrap_or(0);
        let l = lat_nums.get(i).copied().unwrap_or(0);
        match c.cmp(&l) {
            Ordering::Equal => continue,
            other => return other,
        }
    }

    // Same base version: a release (no pre-release) is newer than a pre-release
    match (&cur_pre, &lat_pre) {
        (None, None) => Ordering::Equal,
        (Some(_), None) => Ordering::Less,    // current is pre-release, latest is release
        (None, Some(_)) => Ordering::Greater,  // current is release, latest is pre-release
        (Some(a), Some(b)) => compare_pre_release(a, b),
    }
}

/// Compare pre-release identifiers per semver rules.
/// Split on `.`, compare each identifier numerically when possible,
/// then lexicographically.
fn compare_pre_release(a: &str, b: &str) -> Ordering {
    let parts_a: Vec<&str> = a.split('.').collect();
    let parts_b: Vec<&str> = b.split('.').collect();

    for (pa, pb) in parts_a.iter().zip(parts_b.iter()) {
        let ord = match (pa.parse::<u64>(), pb.parse::<u64>()) {
            (Ok(na), Ok(nb)) => na.cmp(&nb),
            _ => pa.cmp(pb),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }

    parts_a.len().cmp(&parts_b.len())
}

/// Check whether a newer version is available.
/// Returns `Ok(Some(version))` if an update exists, `Ok(None)` otherwise.
pub fn check_for_update(channel: UpdateChannel) -> Result<Option<String>> {
    let manifest = fetch_manifest(channel)?;
    if compare_versions(CURRENT_VERSION, &manifest.version) == Ordering::Less {
        Ok(Some(manifest.version))
    } else {
        Ok(None)
    }
}

/// Detect whether the current install layout supports self-update on Linux.
///
/// Returns `true` only when the executable resides inside the managed
/// install directory `~/.local/opt/gitspark/`.
#[cfg(target_os = "linux")]
pub fn is_managed_install() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    let managed = home.join(".local").join("opt").join("gitspark");
    exe.starts_with(&managed)
}

/// Detect whether the current install layout supports self-update on Windows.
///
/// Returns `true` only when the executable resides inside
/// `%LocalAppData%\GitSpark`.
#[cfg(target_os = "windows")]
pub fn is_managed_install() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let Some(local_app_data) = dirs::data_local_dir() else {
        return false;
    };
    let managed = local_app_data.join("GitSpark");
    exe.starts_with(&managed)
}

/// On macOS updates are handled by Sparkle; always return true.
#[cfg(target_os = "macos")]
pub fn is_managed_install() -> bool {
    true
}

/// Fallback for other platforms.
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub fn is_managed_install() -> bool {
    false
}

/// Build the URL to open the release page for a given version tag.
pub fn release_page_url(version: &str) -> String {
    let tag = if version.starts_with('v') {
        version.to_string()
    } else {
        format!("v{version}")
    };
    format!("https://github.com/JacobSamro/GitSpark/releases/tag/{tag}")
}

/// Verify a SHA-256 checksum of a file at `path` matches `expected`.
pub fn verify_sha256(path: &std::path::Path, expected: &str) -> Result<()> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)
        .with_context(|| format!("failed to open {} for verification", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize_hex();
    if digest != expected {
        bail!(
            "SHA-256 mismatch for {}: expected {expected}, got {digest}",
            path.display()
        );
    }
    Ok(())
}

/// Minimal SHA-256 implementation (FIPS 180-4).
///
/// Used exclusively for verifying update artifacts so that we avoid pulling in
/// a heavy crypto crate just for one hash.
struct Sha256 {
    state: [u32; 8],
    buffer: Vec<u8>,
    total_len: u64,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
                0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
            ],
            buffer: Vec::new(),
            total_len: 0,
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
        self.total_len += data.len() as u64;

        while self.buffer.len() >= 64 {
            let block: [u8; 64] = self.buffer[..64]
                .try_into()
                .expect("SHA-256 buffer always has >= 64 bytes here");
            self.buffer.drain(..64);
            self.compress(&block);
        }
    }

    fn compress(&mut self, block: &[u8; 64]) {
        #[rustfmt::skip]
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
            0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
            0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
            0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
            0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
            0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
            0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
            0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
            0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
            0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
            0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
            0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
            0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
            0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
        ];

        let mut w = [0u32; 64];
        for (i, chunk) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes(
                chunk.try_into().expect("chunks_exact(4) guarantees 4-byte slices"),
            );
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }

    fn finalize_hex(mut self) -> String {
        let bit_len = self.total_len * 8;
        self.buffer.push(0x80);
        while (self.buffer.len() % 64) != 56 {
            self.buffer.push(0);
        }
        self.buffer.extend_from_slice(&bit_len.to_be_bytes());

        for chunk_start in (0..self.buffer.len()).step_by(64) {
            let block: [u8; 64] = self.buffer[chunk_start..chunk_start + 64]
                .try_into()
                .expect("SHA-256 padding guarantees buffer length is a multiple of 64");
            self.compress(&block);
        }

        self.state
            .iter()
            .map(|word| format!("{word:08x}"))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- UpdateChannel::from_tag -------------------------------------------

    #[test]
    fn channel_from_stable_tag() {
        assert_eq!(UpdateChannel::from_tag("v0.3.0"), UpdateChannel::Release);
        assert_eq!(UpdateChannel::from_tag("v1.0.0"), UpdateChannel::Release);
        assert_eq!(UpdateChannel::from_tag("0.3.0"), UpdateChannel::Release);
    }

    #[test]
    fn channel_from_beta_tag() {
        assert_eq!(UpdateChannel::from_tag("v0.4.0-beta.1"), UpdateChannel::Beta);
        assert_eq!(UpdateChannel::from_tag("v1.0.0-rc.2"), UpdateChannel::Beta);
        assert_eq!(UpdateChannel::from_tag("0.5.0-alpha.3"), UpdateChannel::Beta);
    }

    // -- compare_versions -------------------------------------------------

    #[test]
    fn compare_equal_versions() {
        assert_eq!(compare_versions("0.3.3", "0.3.3"), Ordering::Equal);
        assert_eq!(compare_versions("v0.3.3", "0.3.3"), Ordering::Equal);
    }

    #[test]
    fn compare_newer_patch() {
        assert_eq!(compare_versions("0.3.3", "0.3.4"), Ordering::Less);
    }

    #[test]
    fn compare_newer_minor() {
        assert_eq!(compare_versions("0.3.3", "0.4.0"), Ordering::Less);
    }

    #[test]
    fn compare_newer_major() {
        assert_eq!(compare_versions("0.3.3", "1.0.0"), Ordering::Less);
    }

    #[test]
    fn compare_older_version() {
        assert_eq!(compare_versions("1.0.0", "0.3.3"), Ordering::Greater);
    }

    #[test]
    fn compare_pre_release_vs_release() {
        // A pre-release version is lower than the release with same base
        assert_eq!(compare_versions("0.4.0-beta.1", "0.4.0"), Ordering::Less);
    }

    #[test]
    fn compare_release_vs_pre_release() {
        // A release version is higher than a pre-release with same base
        assert_eq!(compare_versions("0.4.0", "0.4.0-beta.1"), Ordering::Greater);
    }

    #[test]
    fn compare_pre_release_ordering() {
        assert_eq!(compare_versions("0.4.0-beta.1", "0.4.0-beta.2"), Ordering::Less);
        assert_eq!(compare_versions("0.4.0-beta.2", "0.4.0-beta.1"), Ordering::Greater);
        assert_eq!(compare_versions("0.4.0-beta.1", "0.4.0-beta.1"), Ordering::Equal);
    }

    #[test]
    fn compare_different_base_with_pre_release() {
        // 0.3.3 < 0.4.0-beta.1 because base 0.3.3 < 0.4.0
        assert_eq!(compare_versions("0.3.3", "0.4.0-beta.1"), Ordering::Less);
    }

    // -- manifest parsing -------------------------------------------------

    #[test]
    fn parses_update_manifest() {
        let json = r#"{
            "channel": "beta",
            "version": "0.4.0-beta.1",
            "pub_date": "2026-03-12T12:00:00Z",
            "notes_url": "https://github.com/JacobSamro/GitSpark/releases/tag/v0.4.0-beta.1",
            "assets": {
                "windows-x64": {
                    "url": "https://example.com/gitspark-windows.zip",
                    "sha256": "abc123"
                },
                "linux-x64": {
                    "url": "https://example.com/gitspark-linux.tar.gz",
                    "sha256": "def456"
                }
            }
        }"#;

        let manifest: UpdateManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.channel, "beta");
        assert_eq!(manifest.version, "0.4.0-beta.1");
        assert_eq!(manifest.assets.len(), 2);
        assert!(manifest.assets.contains_key("windows-x64"));
        assert!(manifest.assets.contains_key("linux-x64"));
        assert_eq!(manifest.assets["linux-x64"].sha256, "def456");
    }

    // -- SHA-256 -----------------------------------------------------------

    #[test]
    fn sha256_empty_string() {
        let mut h = Sha256::new();
        h.update(b"");
        assert_eq!(
            h.finalize_hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_known_value() {
        let mut h = Sha256::new();
        h.update(b"hello world");
        assert_eq!(
            h.finalize_hex(),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn sha256_multipart() {
        let mut h = Sha256::new();
        h.update(b"hello ");
        h.update(b"world");
        assert_eq!(
            h.finalize_hex(),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    // -- release_page_url --------------------------------------------------

    #[test]
    fn release_page_url_with_prefix() {
        assert_eq!(
            release_page_url("v0.4.0"),
            "https://github.com/JacobSamro/GitSpark/releases/tag/v0.4.0"
        );
    }

    #[test]
    fn release_page_url_without_prefix() {
        assert_eq!(
            release_page_url("0.4.0"),
            "https://github.com/JacobSamro/GitSpark/releases/tag/v0.4.0"
        );
    }
}
