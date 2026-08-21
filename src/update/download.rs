//! Downloading and verifying an update artifact.
//!
//! Nothing here trusts the network. The URL came from a manifest whose
//! signature was already checked, and the bytes that arrive are hashed against
//! that manifest's digest before anything else touches them.

use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use super::manifest::AssetInfo;
use super::verify::verify_sha256;

/// Reported as bytes downloaded and total, so the UI can show progress
/// without this module knowing anything about the UI.
pub type ProgressFn = dyn Fn(u64, u64) + Send + 'static;

/// Refuse anything larger than this. The manifest states a size, but the
/// manifest is only as current as the release — this is a backstop against a
/// truncated or malicious response filling the disk.
const MAX_ARTIFACT_BYTES: u64 = 512 * 1024 * 1024;

/// Where staged updates live, under the OS cache directory so an abandoned
/// download is not left in the user's home.
pub fn staging_dir() -> Result<PathBuf> {
    let base = dirs::cache_dir().context("no cache directory for this platform")?;
    Ok(base.join("GitSpark").join("updates"))
}

/// Download `asset`, verify its checksum, and return the file path.
///
/// The download goes to a `.part` file and is renamed only after the checksum
/// passes, so a crash or a failed verification can never leave something that
/// looks like a finished, trusted artifact.
pub fn download_artifact(
    asset: &AssetInfo,
    version: &str,
    progress: Option<Box<ProgressFn>>,
) -> Result<PathBuf> {
    let dir = staging_dir()?.join(version);
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create staging directory {}", dir.display()))?;

    let file_name = artifact_file_name(&asset.url)?;
    let final_path = dir.join(&file_name);
    let part_path = dir.join(format!("{file_name}.part"));

    // A previous run may have finished this exact artifact. Re-verify rather
    // than trusting the file name: a stale or tampered file must not be used.
    if final_path.is_file()
        && let Ok(existing) = fs::read(&final_path)
        && verify_sha256(&existing, &asset.sha256).is_ok()
    {
        return Ok(final_path);
    }

    let agent = crate::ai::http_agent();
    let mut response = agent
        .get(&asset.url)
        .call()
        .with_context(|| format!("failed to download {}", asset.url))?;
    if response.status().as_u16() >= 400 {
        bail!("{} returned HTTP {}", asset.url, response.status().as_u16());
    }

    let expected_total = if asset.size > 0 { asset.size } else { 0 };
    let mut reader = response.body_mut().as_reader();
    let mut file = fs::File::create(&part_path)
        .with_context(|| format!("failed to create {}", part_path.display()))?;

    let mut buffer = vec![0u8; 64 * 1024];
    let mut downloaded: u64 = 0;
    loop {
        let read = reader.read(&mut buffer).context("download interrupted")?;
        if read == 0 {
            break;
        }
        downloaded += read as u64;
        if downloaded > MAX_ARTIFACT_BYTES {
            let _ = fs::remove_file(&part_path);
            bail!("update artifact exceeded {MAX_ARTIFACT_BYTES} bytes");
        }
        file.write_all(&buffer[..read])
            .context("failed to write update artifact")?;
        if let Some(report) = progress.as_ref() {
            report(downloaded, expected_total);
        }
    }
    file.flush().context("failed to flush update artifact")?;
    drop(file);

    let bytes = fs::read(&part_path).context("failed to read downloaded artifact")?;
    if let Err(error) = verify_sha256(&bytes, &asset.sha256) {
        // Remove it. A file that failed verification must not survive to be
        // picked up by the resume path above on the next attempt.
        let _ = fs::remove_file(&part_path);
        return Err(error);
    }

    fs::rename(&part_path, &final_path).with_context(|| {
        format!(
            "failed to move verified artifact into {}",
            final_path.display()
        )
    })?;
    Ok(final_path)
}

/// The file name to save an artifact under.
///
/// Taken from the URL's last path segment, then stripped of anything that
/// could escape the staging directory — a manifest is signed, but a signed
/// manifest with a `../` in a URL should still not be able to write outside
/// the directory we chose.
fn artifact_file_name(url: &str) -> Result<String> {
    let tail = url
        .rsplit('/')
        .next()
        .filter(|segment| !segment.is_empty())
        .context("artifact URL has no file name")?;
    let cleaned: String = tail
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_'))
        .collect();
    let cleaned = cleaned.trim_matches('.').to_string();
    if cleaned.is_empty() {
        bail!("artifact URL file name is not usable: {tail}");
    }
    Ok(cleaned)
}

/// Delete staged downloads other than `keep_version`.
pub fn prune_staging(keep_version: Option<&str>) -> Result<()> {
    let dir = staging_dir()?;
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(&dir)?.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if Some(name.as_ref()) == keep_version {
            continue;
        }
        let _ = fs::remove_dir_all(entry.path());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_a_file_name_from_the_url() {
        assert_eq!(
            artifact_file_name("https://example.com/a/gitspark-v1.0.0.dmg").unwrap(),
            "gitspark-v1.0.0.dmg"
        );
    }

    #[test]
    fn strips_path_traversal_from_the_file_name() {
        // Even a signed manifest must not be able to write outside staging.
        let name = artifact_file_name("https://example.com/x/..%2f..%2fetc%2fpasswd").unwrap();
        assert!(!name.contains('/'), "separator survived: {name}");
        assert!(!name.starts_with('.'), "leading dot survived: {name}");
    }

    #[test]
    fn rejects_a_url_with_no_usable_file_name() {
        assert!(artifact_file_name("https://example.com/").is_err());
        assert!(artifact_file_name("https://example.com/...").is_err());
    }

    /// Serve one fixed body on a throwaway port, then shut down.
    fn serve_once(body: Vec<u8>) -> u16 {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut socket, _)) = listener.accept() {
                let mut scratch = [0u8; 2048];
                let _ = std::io::Read::read(&mut socket, &mut scratch);
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = socket.write_all(header.as_bytes());
                let _ = socket.write_all(&body);
            }
        });
        port
    }

    #[test]
    fn downloads_and_verifies_a_real_response() {
        let body = b"gitspark update payload".to_vec();
        let port = serve_once(body.clone());
        let asset = AssetInfo {
            url: format!("http://127.0.0.1:{port}/gitspark-test.bin"),
            sha256: super::super::verify::sha256_hex(&body),
            size: body.len() as u64,
        };

        let path = download_artifact(&asset, "test-ok", None).expect("download succeeds");
        assert_eq!(fs::read(&path).unwrap(), body);
        assert!(
            path.extension().is_some_and(|ext| ext != "part"),
            "artifact was left as a .part file"
        );
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn rejects_a_corrupted_artifact_and_keeps_nothing() {
        // The bytes on the wire do not match the signed manifest's digest —
        // a tampered mirror, or a truncated transfer.
        let body = b"not what the manifest promised".to_vec();
        let port = serve_once(body);
        let asset = AssetInfo {
            url: format!("http://127.0.0.1:{port}/gitspark-bad.bin"),
            sha256: "0".repeat(64),
            size: 0,
        };

        let result = download_artifact(&asset, "test-bad", None);
        assert!(result.is_err(), "a checksum mismatch was accepted");

        // Nothing may survive: a leftover file would be picked up by the
        // resume path on the next attempt and treated as already downloaded.
        let dir = staging_dir().unwrap().join("test-bad");
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .map(|entries| entries.flatten().map(|e| e.file_name()).collect())
            .unwrap_or_default();
        assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reports_progress_while_downloading() {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::Arc;

        let body = vec![7u8; 200_000];
        let port = serve_once(body.clone());
        let asset = AssetInfo {
            url: format!("http://127.0.0.1:{port}/gitspark-progress.bin"),
            sha256: super::super::verify::sha256_hex(&body),
            size: body.len() as u64,
        };

        let seen = Arc::new(AtomicU64::new(0));
        let counter = Arc::clone(&seen);
        let path = download_artifact(
            &asset,
            "test-progress",
            Some(Box::new(move |done, _total| {
                counter.fetch_max(done, Ordering::Relaxed);
            })),
        )
        .expect("download succeeds");

        assert_eq!(seen.load(Ordering::Relaxed), body.len() as u64);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn staging_lives_under_the_cache_directory() {
        let dir = staging_dir().expect("cache dir resolves");
        assert!(dir.ends_with("GitSpark/updates"), "unexpected: {dir:?}");
    }
}
