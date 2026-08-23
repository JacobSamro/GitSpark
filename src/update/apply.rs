//! Replacing the installed application with a verified artifact.
//!
//! This is the only part of the updater that writes outside its staging
//! directory, so it runs strictly after the manifest signature and the
//! artifact checksum have both passed. Nothing here re-checks that — by the
//! time we are replacing files, trust has already been established or the
//! caller has made a mistake.
//!
//! Each platform needs a different trick, for a reason:
//!
//! - **macOS** ships a `.dmg`. Mount it, `rsync` the `.app` over the installed
//!   bundle, unmount. The bundle is a directory, so a file copy will not do.
//! - **Linux** ships a tarball extracted to a directory; `rsync` it over the
//!   install directory.
//! - **Windows** cannot delete or overwrite a running `.exe`, but it *can*
//!   rename one. So the running binary is renamed aside and the new one moved
//!   into place, with the rename reversed if the copy fails.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// Install a verified artifact over the current installation.
///
/// Returns after the files are replaced; the caller decides when to restart.
pub fn install(artifact: &Path, staging: &Path) -> Result<()> {
    if !artifact.exists() {
        bail!("update artifact is missing: {}", artifact.display());
    }

    #[cfg(target_os = "macos")]
    return install_macos(artifact, staging);

    #[cfg(target_os = "linux")]
    return install_linux(artifact, staging);

    #[cfg(target_os = "windows")]
    return install_windows(artifact, staging);

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = (artifact, staging);
        bail!("automatic updates are not supported on this platform")
    }
}

// ---------------------------------------------------------------------------
// macOS
// ---------------------------------------------------------------------------

/// Walk up from `…/Foo.app/Contents/MacOS/binary` to `…/Foo.app`.
///
/// Kept separate from any filesystem access so it can be tested against
/// synthetic paths on any platform — this is the part that decides what gets
/// overwritten, and getting it wrong means writing over the wrong directory.
pub fn app_bundle_for(exe: &Path) -> Option<PathBuf> {
    let mut current = exe.parent()?;
    // Bounded rather than looping to the filesystem root: a bundle is always
    // exactly `Contents/MacOS` deep, and an unbounded walk on a non-bundle
    // path could match some unrelated `.app` far above.
    for _ in 0..3 {
        if current.extension().is_some_and(|ext| ext == "app") {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
    None
}

#[cfg(target_os = "macos")]
fn install_macos(dmg: &Path, staging: &Path) -> Result<()> {
    let exe = std::env::current_exe().context("cannot determine the running executable")?;
    let app = app_bundle_for(&exe)
        .with_context(|| format!("{} is not inside a .app bundle", exe.display()))?;
    install_macos_into(dmg, staging, &app)
}

/// The mount-and-replace half, with the destination injected.
///
/// Split out so it can be exercised against a throwaway bundle. Testing this
/// through `current_exe()` would mean overwriting the running binary, which is
/// not a test anyone should run twice.
#[cfg(target_os = "macos")]
pub(crate) fn install_macos_into(dmg: &Path, staging: &Path, app: &Path) -> Result<()> {
    let app_name = app
        .file_name()
        .context("installed bundle has no name")?
        .to_owned();

    // Start from a clean mount root. A previous crash can leave a stale volume
    // directory here, and the volume lookup below takes the first entry it
    // finds — which would then be the wrong one.
    let mount_root = staging.join("mount");
    let _ = std::fs::remove_dir_all(&mount_root);
    std::fs::create_dir_all(&mount_root)
        .with_context(|| format!("failed to create {}", mount_root.display()))?;

    let attach = Command::new("hdiutil")
        .args(["attach", "-nobrowse", "-quiet"])
        .arg(dmg)
        .arg("-mountroot")
        .arg(&mount_root)
        .output()
        .context("failed to run hdiutil attach")?;
    if !attach.status.success() {
        bail!(
            "hdiutil attach failed: {}",
            String::from_utf8_lossy(&attach.stderr).trim()
        );
    }

    let result = copy_bundle_from_mount(&mount_root, &app_name, app);

    // Always detach, even when the copy failed — a leaked mount survives the
    // app and the user has no obvious way to clear it.
    let volume = mounted_volume(&mount_root).ok();
    if let Some(volume) = volume {
        let _ = Command::new("hdiutil")
            .args(["detach", "-force", "-quiet"])
            .arg(&volume)
            .output();
    }
    let _ = std::fs::remove_dir_all(&mount_root);

    result?;

    // The bundle inherits com.apple.quarantine from the DMG. Left in place,
    // Gatekeeper blocks the app on its next launch and the update looks like
    // it broke the install.
    let _ = Command::new("xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(app)
        .output();

    Ok(())
}

#[cfg(target_os = "macos")]
fn copy_bundle_from_mount(
    mount_root: &Path,
    app_name: &std::ffi::OsStr,
    destination: &Path,
) -> Result<()> {
    let volume = mounted_volume(mount_root)?;
    let source = volume.join(app_name);
    if !source.is_dir() {
        bail!(
            "the disk image does not contain {}",
            app_name.to_string_lossy()
        );
    }

    // Trailing slash tells rsync to copy the CONTENTS of the bundle rather
    // than nesting it inside the destination.
    let mut source_arg = source.into_os_string();
    source_arg.push("/");

    let rsync = Command::new("rsync")
        // --delete so files removed in the new version do not linger; without
        // it an update leaves orphaned resources behind forever.
        //
        // --ignore-times because rsync's default quick check skips any file
        // whose size and mtime match the destination's, and that is not a safe
        // assumption for an app update: a rebuilt file can keep its size, and
        // mtimes only have one-second granularity. Skipping a changed file
        // here leaves a half-updated bundle that still reports the old
        // version, so pay the full copy and be certain.
        .args(["-a", "--delete", "--ignore-times", "--exclude", "Icon?"])
        .arg(&source_arg)
        .arg(destination)
        .output()
        .context("failed to run rsync")?;
    if !rsync.status.success() {
        bail!(
            "rsync failed: {}",
            String::from_utf8_lossy(&rsync.stderr).trim()
        );
    }
    Ok(())
}

/// The single directory `hdiutil` created under our mount root.
#[cfg(target_os = "macos")]
fn mounted_volume(mount_root: &Path) -> Result<PathBuf> {
    let mut volumes: Vec<PathBuf> = std::fs::read_dir(mount_root)
        .with_context(|| format!("failed to read {}", mount_root.display()))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    match volumes.len() {
        0 => bail!("the disk image mounted no volumes"),
        1 => Ok(volumes.pop().expect("checked length")),
        // Ambiguous rather than guessing: picking arbitrarily here would mean
        // copying from an unknown image.
        n => bail!("expected one mounted volume, found {n}"),
    }
}

// ---------------------------------------------------------------------------
// Linux
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn install_linux(artifact: &Path, _staging: &Path) -> Result<()> {
    let exe = std::env::current_exe().context("cannot determine the running executable")?;
    let install_dir = exe
        .parent()
        .context("the running executable has no parent directory")?;

    if !artifact.is_dir() {
        bail!(
            "expected an extracted directory for the Linux update, got {}",
            artifact.display()
        );
    }

    let mut source = artifact.to_path_buf().into_os_string();
    source.push("/");

    let rsync = Command::new("rsync")
        // --ignore-times for the same reason as the macOS path: rsync's
        // size-and-mtime quick check can skip a genuinely changed file.
        .args(["-a", "--delete", "--ignore-times"])
        .arg(&source)
        .arg(install_dir)
        .output()
        .context("rsync is required for automatic updates but could not be run")?;
    if !rsync.status.success() {
        bail!(
            "rsync failed: {}",
            String::from_utf8_lossy(&rsync.stderr).trim()
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn install_windows(artifact: &Path, staging: &Path) -> Result<()> {
    let exe = std::env::current_exe().context("cannot determine the running executable")?;

    // Windows refuses to delete or overwrite a running executable, but it
    // will rename one. Move ourselves aside, then put the new binary in the
    // path we just vacated.
    let backup_dir = staging.join("backup");
    std::fs::create_dir_all(&backup_dir)?;
    let backup = backup_dir.join(
        exe.file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("gitspark.exe")),
    );
    let _ = std::fs::remove_file(&backup);

    std::fs::rename(&exe, &backup).with_context(|| {
        format!(
            "failed to move the running executable aside: {}",
            exe.display()
        )
    })?;

    if let Err(error) = std::fs::copy(artifact, &exe) {
        // Put ourselves back. Failing here without rolling back would leave
        // no executable at all at the installed path.
        let _ = std::fs::rename(&backup, &exe);
        return Err(error).context("failed to install the update; rolled back");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::app_bundle_for;
    use std::path::{Path, PathBuf};

    #[test]
    fn finds_the_bundle_from_the_binary_inside_it() {
        let exe = Path::new("/Applications/GitSpark.app/Contents/MacOS/gitspark");
        assert_eq!(
            app_bundle_for(exe),
            Some(PathBuf::from("/Applications/GitSpark.app"))
        );
    }

    #[test]
    fn finds_a_bundle_in_a_nested_location() {
        let exe = Path::new("/Users/x/Downloads/build/GitSpark.app/Contents/MacOS/gitspark");
        assert_eq!(
            app_bundle_for(exe),
            Some(PathBuf::from("/Users/x/Downloads/build/GitSpark.app"))
        );
    }

    #[test]
    fn returns_none_for_a_loose_binary() {
        // A `cargo run` build is not in a bundle, and must not resolve to one
        // — replacing an unrelated .app would be catastrophic.
        assert_eq!(
            app_bundle_for(Path::new("/Users/x/project/target/release/gitspark")),
            None
        );
    }

    #[test]
    fn does_not_reach_past_the_expected_bundle_depth() {
        // An .app far above a loose binary must NOT be adopted. The walk is
        // bounded precisely so this cannot happen.
        let exe = Path::new("/Applications/Other.app/Contents/Resources/deep/nested/tool");
        assert_eq!(app_bundle_for(exe), None);
    }

    /// End-to-end on macOS: build a real DMG containing an updated bundle,
    /// apply it over a throwaway "installed" bundle, and check the contents.
    ///
    /// This is the only test that exercises hdiutil, the volume lookup, and
    /// rsync together — the parts that cannot be reasoned about from types.
    /// It targets a temporary directory, never the running application.
    #[cfg(target_os = "macos")]
    #[test]
    fn replaces_an_installed_bundle_from_a_real_dmg() {
        use std::fs;
        use std::process::Command;

        let root = std::env::temp_dir().join(format!("gitspark-apply-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let staging = root.join("staging");
        let installed = root.join("installed/GitSpark.app");
        let payload = root.join("payload");
        let source_app = payload.join("GitSpark.app");

        // The "installed" bundle, carrying an old marker and a file that the
        // new version drops — --delete must remove it.
        //
        // The two version markers are deliberately the same length and written
        // moments apart. That is the exact shape rsync's default quick check
        // mistakes for "unchanged", and this test exists to keep the copy
        // honest about it — do not "fix" the markers to differing sizes.
        fs::create_dir_all(installed.join("Contents/MacOS")).unwrap();
        fs::write(installed.join("Contents/version.txt"), "0.5.0").unwrap();
        fs::write(installed.join("Contents/stale.txt"), "should not survive").unwrap();

        // The updated bundle that ships inside the image.
        fs::create_dir_all(source_app.join("Contents/MacOS")).unwrap();
        fs::write(source_app.join("Contents/version.txt"), "0.6.0").unwrap();
        fs::write(source_app.join("Contents/MacOS/gitspark"), "new binary").unwrap();

        let dmg = root.join("update.dmg");
        let created = Command::new("hdiutil")
            .args(["create", "-quiet", "-srcfolder"])
            .arg(&payload)
            .args(["-volname", "GitSpark", "-ov", "-format", "UDZO"])
            .arg(&dmg)
            .output()
            .expect("hdiutil create runs");
        assert!(
            created.status.success(),
            "hdiutil create failed: {}",
            String::from_utf8_lossy(&created.stderr)
        );

        super::install_macos_into(&dmg, &staging, &installed).expect("install succeeds");

        assert_eq!(
            fs::read_to_string(installed.join("Contents/version.txt")).unwrap(),
            "0.6.0",
            "the installed bundle was not updated"
        );
        assert!(
            installed.join("Contents/MacOS/gitspark").is_file(),
            "the new binary is missing"
        );
        assert!(
            !installed.join("Contents/stale.txt").exists(),
            "a file removed in the new version survived the update"
        );
        // The mount must not be left behind.
        assert!(
            !staging.join("mount").exists(),
            "the disk image mount root was not cleaned up"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn handles_a_path_with_no_parent() {
        assert_eq!(app_bundle_for(Path::new("/")), None);
    }
}
