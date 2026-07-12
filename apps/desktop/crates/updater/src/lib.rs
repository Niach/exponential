//! Self-update engine (EXP-22) — the gpui-free half of the in-app updater.
//!
//! [`crate::ui`]'s update module owns the check, the state machine, and the
//! banner; this crate owns everything that touches the filesystem and the
//! network payloads: capability probing (can this install self-update at
//! all?), the streaming download, SHA256SUMS verification, and the per-OS
//! swap. Everything here is blocking — callers run it on the gpui background
//! executor, exactly like the existing release check.
//!
//! Per-platform swap strategies (masterplan §11.2 follow-up):
//! - **Linux**: atomic rename over `$APPIMAGE`. The running app is unaffected
//!   (the old inode keeps backing the mounted squashfs) and the self-registered
//!   `.desktop`/`mimeapps.list` keep pointing at the same path.
//! - **Windows**: `self-replace` (the rustup/uv locked-exe rename technique).
//! - **macOS**: mount the DMG, `rsync -a --delete` the bundled `.app` over the
//!   running bundle, detach (Zed's `auto_update` flow at our pinned gpui rev).
//!
//! The updater never decides *whether* to update — the ui layer gates on
//! channel (staging never updates), on `is_newer`, and on the release actually
//! carrying the asset this platform needs (which is also the macOS signing
//! gate: unsigned releases ship a `.zip`, not the `.dmg`, so macOS stays
//! banner-only until the Developer-ID secrets land).

use std::{
    fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{bail, Context as _, Result};
use sha2::Digest as _;

/// GitHub rejects API/asset calls without a User-Agent.
const USER_AGENT: &str = "exp-desktop-updater";
/// The checksums asset every desktop release carries (build-desktop.yml).
pub const SUMS_ASSET: &str = "SHA256SUMS.txt";

/// How a self-update is applied on this platform. All variants exist on every
/// platform so the ui state machine never needs `cfg` — [`install`] rejects a
/// strategy that doesn't match the compiled OS.
#[derive(Clone, Debug)]
pub enum Strategy {
    /// Linux: atomic rename over the running AppImage.
    AppImage { appimage: PathBuf },
    /// Windows: swap the running exe via `self-replace`.
    ReplaceExe,
    /// macOS: mount the DMG and sync the `.app` over the running bundle.
    ReplaceBundle { bundle: PathBuf },
}

/// What this install can do when a newer release shows up.
#[derive(Clone, Debug)]
pub enum Capability {
    /// Download + swap + relaunch in-app.
    SelfUpdate(Strategy),
    /// Only link the releases page (dev builds, unwritable installs, and
    /// platforms whose asset the release doesn't carry).
    BannerOnly,
}

/// Probe what the running install supports. Pure environment inspection — no
/// network. `BannerOnly` is the universal safe fallback, so every uncertainty
/// degrades to today's behavior.
pub fn capability() -> Capability {
    #[cfg(target_os = "linux")]
    {
        // Only AppImage runs self-update: `$APPIMAGE` is set by the AppImage
        // runtime to the path being executed. Raw dev binaries stay banner-only.
        appimage_capability(std::env::var_os("APPIMAGE").map(PathBuf::from))
    }
    #[cfg(target_os = "windows")]
    {
        match std::env::current_exe() {
            Ok(exe) if exe.parent().is_some_and(dir_writable) => {
                Capability::SelfUpdate(Strategy::ReplaceExe)
            }
            _ => Capability::BannerOnly,
        }
    }
    #[cfg(target_os = "macos")]
    {
        // current_exe = <bundle>.app/Contents/MacOS/exp-desktop — anything else
        // (raw dev binary) can't be swapped as a bundle.
        let bundle = std::env::current_exe().ok().and_then(|exe| {
            let bundle = exe.parent()?.parent()?.parent()?;
            (bundle.extension()? == "app").then(|| bundle.to_path_buf())
        });
        match bundle {
            Some(bundle) if bundle.parent().is_some_and(dir_writable) => {
                Capability::SelfUpdate(Strategy::ReplaceBundle { bundle })
            }
            _ => Capability::BannerOnly,
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        Capability::BannerOnly
    }
}

/// Linux capability from a candidate `$APPIMAGE` value (split out for tests).
fn appimage_capability(appimage: Option<PathBuf>) -> Capability {
    match appimage {
        Some(path)
            if path.is_file() && path.parent().is_some_and(dir_writable) =>
        {
            Capability::SelfUpdate(Strategy::AppImage { appimage: path })
        }
        _ => Capability::BannerOnly,
    }
}

/// Can we create files in `dir`? Probed by actually creating one — metadata
/// permission bits lie (ACLs, read-only mounts, network shares).
fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".exp-update-probe-{}", std::process::id()));
    match fs::File::create(&probe) {
        Ok(_) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// The release asset this strategy installs from. Names must match the
/// packaging scripts / build-desktop.yml exactly; a mismatch (e.g. an arch we
/// don't build) simply never finds its asset and degrades to banner-only.
/// Only the production channel ever reaches the updater, so the channel
/// segment is a constant.
pub fn expected_asset_name(strategy: &Strategy) -> String {
    let arch = std::env::consts::ARCH;
    match strategy {
        Strategy::AppImage { .. } => format!("Exponential-production-{arch}.AppImage"),
        Strategy::ReplaceExe => format!("Exponential-production-{arch}-windows.exe"),
        Strategy::ReplaceBundle { .. } => "Exponential-production.dmg".to_string(),
    }
}

/// Where downloads are staged: `<data_local>/exponential/updates` (the app's
/// per-user data dir, incl. the macOS casing).
pub fn staging_dir() -> Option<PathBuf> {
    let dir = dirs::data_local_dir()?.join(if cfg!(target_os = "macos") {
        "Exponential"
    } else {
        "exponential"
    });
    Some(dir.join("updates"))
}

/// Best-effort wipe of the staging dir — leftovers only exist after a crash
/// mid-pipeline (a successful install consumes or deletes its download).
/// Called once at startup.
pub fn cleanup_staging() {
    if let Some(dir) = staging_dir() {
        if dir.exists() {
            if let Err(err) = fs::remove_dir_all(&dir) {
                log::warn!("[updater] staging cleanup failed: {err}");
            }
        }
    }
}

fn agent() -> ureq::Agent {
    // Connect/read timeouts, NOT an overall `.timeout()` — a whole-request
    // deadline would kill a large download on a slow link.
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(30))
        .build()
}

/// Fetch a small text asset (SHA256SUMS.txt) into memory.
pub fn fetch_text(url: &str) -> Result<String> {
    agent()
        .get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("GET {url}"))?
        .into_string()
        .context("read body")
}

/// Stream `url` to `dest`, reporting `(received, total)` after each chunk.
/// Writes `<dest>.part` and renames on success so `dest` is never partial.
pub fn download(url: &str, dest: &Path, mut progress: impl FnMut(u64, Option<u64>)) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).context("create staging dir")?;
    }
    let part = dest.with_extension("part");

    let response = agent()
        .get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("GET {url}"))?;
    let total = response
        .header("Content-Length")
        .and_then(|len| len.parse::<u64>().ok());

    let mut reader = response.into_reader();
    let result = (|| -> Result<()> {
        let mut file = fs::File::create(&part).context("create download file")?;
        let mut received = 0u64;
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = reader.read(&mut buf).context("read download stream")?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).context("write download file")?;
            received += n as u64;
            progress(received, total);
        }
        if let Some(total) = total {
            if received != total {
                bail!("download truncated: {received} of {total} bytes");
            }
        }
        file.sync_all().ok();
        Ok(())
    })();

    match result {
        Ok(()) => {
            fs::rename(&part, dest).context("finalize download")?;
            Ok(())
        }
        Err(err) => {
            let _ = fs::remove_file(&part);
            Err(err)
        }
    }
}

/// Verify `file` against its `SHA256SUMS.txt` entry (`<hex>  <name>` lines,
/// `sha256sum` format; a leading `*` on the name marks binary mode).
pub fn verify_sha256(file: &Path, sums_text: &str, asset_name: &str) -> Result<()> {
    let expected = sums_text
        .lines()
        .filter_map(|line| {
            let (hex, name) = line.trim().split_once(char::is_whitespace)?;
            Some((hex.to_ascii_lowercase(), name.trim_start().trim_start_matches('*')))
        })
        .find(|(_, name)| *name == asset_name)
        .map(|(hex, _)| hex)
        .with_context(|| format!("{asset_name} not listed in {SUMS_ASSET}"))?;

    let mut hasher = sha2::Sha256::new();
    let mut reader = fs::File::open(file).context("open downloaded file")?;
    std::io::copy(&mut reader, &mut hasher).context("hash downloaded file")?;
    let actual = hex_encode(&hasher.finalize());

    if actual != expected {
        bail!("checksum mismatch for {asset_name}: expected {expected}, got {actual}");
    }
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Apply the verified download. Returns the path to hand to gpui's
/// `set_restart_path` (`None` = relaunch the current exe path, which is
/// correct after a Windows in-place swap). The staged file is consumed
/// (Linux) or deleted (Windows/macOS) on success.
pub fn install(strategy: &Strategy, staged: &Path) -> Result<Option<PathBuf>> {
    match strategy {
        Strategy::AppImage { appimage } => {
            install_appimage(appimage, staged).map(|()| Some(appimage.clone()))
        }
        Strategy::ReplaceExe => {
            install_exe(staged)?;
            let _ = fs::remove_file(staged);
            Ok(None)
        }
        Strategy::ReplaceBundle { bundle } => {
            install_bundle(bundle, staged)?;
            let _ = fs::remove_file(staged);
            Ok(Some(bundle.clone()))
        }
    }
}

/// Linux: make the staged AppImage executable, then atomically rename it over
/// the running one. The staging dir may sit on a different filesystem than
/// `$APPIMAGE`; a cross-device rename fails, so fall back to copying to a
/// sibling `.new` file (same fs) and renaming that.
fn install_appimage(appimage: &Path, staged: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(staged, fs::Permissions::from_mode(0o755))
            .context("chmod staged AppImage")?;

        match fs::rename(staged, appimage) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::CrossesDevices => {
                let sibling = appimage.with_extension("AppImage.new");
                fs::copy(staged, &sibling).context("copy AppImage next to target")?;
                fs::set_permissions(&sibling, fs::Permissions::from_mode(0o755))
                    .context("chmod copied AppImage")?;
                fs::rename(&sibling, appimage)
                    .context("rename AppImage into place")
                    .inspect_err(|_| {
                        let _ = fs::remove_file(&sibling);
                    })?;
                let _ = fs::remove_file(staged);
                Ok(())
            }
            Err(err) => Err(err).context("rename AppImage into place"),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (appimage, staged);
        bail!("AppImage install is Linux-only");
    }
}

/// Windows: swap the running exe with the staged one.
fn install_exe(staged: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        self_replace::self_replace(staged).context("self-replace exe")
    }
    #[cfg(not(windows))]
    {
        let _ = staged;
        bail!("exe self-replace is Windows-only");
    }
}

/// macOS: mount the staged DMG under the staging dir, rsync the contained
/// `.app` over the running bundle (Zed's shipping flow), detach.
fn install_bundle(bundle: &Path, staged: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        let mount_root = staged
            .parent()
            .context("staged DMG has no parent dir")?
            .join("mount");
        fs::create_dir_all(&mount_root).context("create mount root")?;

        let output = Command::new("hdiutil")
            .args(["attach", "-nobrowse", "-readonly", "-mountroot"])
            .arg(&mount_root)
            .arg(staged)
            .output()
            .context("run hdiutil attach")?;
        if !output.status.success() {
            bail!("hdiutil attach failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        // Detach whatever mounted under our root when we're done, success or not.
        struct Unmounter(PathBuf);
        impl Drop for Unmounter {
            fn drop(&mut self) {
                let _ = Command::new("hdiutil")
                    .args(["detach", "-force"])
                    .arg(&self.0)
                    .output();
            }
        }

        // The volume mounts at <mount_root>/<volname>; don't assume the
        // volname — scan for the entry that contains an .app.
        let (mount_point, mounted_app) = fs::read_dir(&mount_root)
            .context("read mount root")?
            .filter_map(|entry| entry.ok())
            .find_map(|entry| {
                let app = fs::read_dir(entry.path())
                    .ok()?
                    .filter_map(|e| e.ok())
                    .find(|e| e.path().extension().is_some_and(|ext| ext == "app"))?;
                Some((entry.path(), app.path()))
            })
            .context("no .app found in mounted DMG")?;
        let _unmounter = Unmounter(mount_point);

        // Trailing slash: sync the bundle's CONTENTS over the running bundle
        // in place (Zed's auto_update does exactly this).
        let output = Command::new("rsync")
            .args(["-a", "--delete"])
            .arg(format!("{}/", mounted_app.display()))
            .arg(bundle)
            .output()
            .context("run rsync")?;
        if !output.status.success() {
            bail!("rsync failed: {}", String::from_utf8_lossy(&output.stderr));
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (bundle, staged);
        bail!("bundle install is macOS-only");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha256_hex(data: &[u8]) -> String {
        let mut hasher = sha2::Sha256::new();
        hasher.update(data);
        hex_encode(&hasher.finalize())
    }

    #[test]
    fn verify_accepts_matching_checksum() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Exponential-production-x86_64.AppImage");
        fs::write(&file, b"payload").unwrap();
        let sums = format!(
            "{}  Exponential-production-x86_64.AppImage\nabc123  other.zip\n",
            sha256_hex(b"payload")
        );
        verify_sha256(&file, &sums, "Exponential-production-x86_64.AppImage").unwrap();
    }

    #[test]
    fn verify_rejects_mismatch_and_missing_entry() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("asset.bin");
        fs::write(&file, b"payload").unwrap();

        let wrong = format!("{}  asset.bin\n", sha256_hex(b"tampered"));
        assert!(verify_sha256(&file, &wrong, "asset.bin").is_err());
        assert!(verify_sha256(&file, "", "asset.bin").is_err());
    }

    #[test]
    fn verify_handles_binary_marker_and_case() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("asset.bin");
        fs::write(&file, b"payload").unwrap();
        let sums = format!("{}  *asset.bin\n", sha256_hex(b"payload").to_uppercase());
        verify_sha256(&file, &sums, "asset.bin").unwrap();
    }

    #[test]
    fn asset_names_match_release_pipeline() {
        assert_eq!(
            expected_asset_name(&Strategy::AppImage { appimage: PathBuf::from("/x") }),
            format!("Exponential-production-{}.AppImage", std::env::consts::ARCH)
        );
        assert_eq!(
            expected_asset_name(&Strategy::ReplaceExe),
            format!("Exponential-production-{}-windows.exe", std::env::consts::ARCH)
        );
        assert_eq!(
            expected_asset_name(&Strategy::ReplaceBundle { bundle: PathBuf::from("/x") }),
            "Exponential-production.dmg"
        );
    }

    #[test]
    fn appimage_capability_requires_existing_writable_file() {
        assert!(matches!(appimage_capability(None), Capability::BannerOnly));
        assert!(matches!(
            appimage_capability(Some(PathBuf::from("/nonexistent/app.AppImage"))),
            Capability::BannerOnly
        ));

        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Exponential.AppImage");
        fs::write(&file, b"elf").unwrap();
        assert!(matches!(
            appimage_capability(Some(file)),
            Capability::SelfUpdate(Strategy::AppImage { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn appimage_install_swaps_and_marks_executable() {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("Exponential.AppImage");
        let staged = dir.path().join("staged.AppImage");
        fs::write(&target, b"old").unwrap();
        fs::write(&staged, b"new").unwrap();

        install_appimage(&target, &staged).unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert!(!staged.exists());
        assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o755);
    }

    /// The full Linux pipeline end-to-end against a local fixture: stream the
    /// asset, verify it against a SHA256SUMS body, swap it over the "running"
    /// AppImage — the exact chain ui's `run_pipeline` composes.
    #[cfg(unix)]
    #[test]
    fn pipeline_chain_downloads_verifies_and_swaps() {
        use std::io::Write as _;
        use std::net::TcpListener;

        let payload = vec![42u8; 100_000];
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let payload_for_server = payload.clone();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = std::io::Read::read(&mut stream, &mut buf);
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                payload_for_server.len()
            );
            stream.write_all(header.as_bytes()).unwrap();
            stream.write_all(&payload_for_server).unwrap();
        });

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("Exponential.AppImage");
        fs::write(&target, b"old-version").unwrap();
        let strategy = Strategy::AppImage { appimage: target.clone() };
        let asset_name = expected_asset_name(&strategy);
        let sums = format!("{}  {asset_name}\n", sha256_hex(&payload));

        let staged = dir.path().join("staging").join(&asset_name);
        download(&format!("http://{addr}/{asset_name}"), &staged, |_, _| {}).unwrap();
        server.join().unwrap();
        verify_sha256(&staged, &sums, &asset_name).unwrap();
        let restart_path = install(&strategy, &staged).unwrap();

        assert_eq!(restart_path, Some(target.clone()));
        assert_eq!(fs::read(&target).unwrap(), payload);
        assert!(!staged.exists());
    }

    #[test]
    fn download_streams_and_verifies_length() {
        // Serve one response from an ephemeral local listener — same fixture
        // trick as ui's image_paste tests, without new dev-deps.
        use std::io::Write as _;
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let body = vec![7u8; 200_000];
        let body_for_server = body.clone();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = std::io::Read::read(&mut stream, &mut buf);
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body_for_server.len()
            );
            stream.write_all(header.as_bytes()).unwrap();
            stream.write_all(&body_for_server).unwrap();
        });

        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("nested").join("asset.bin");
        let mut last = (0u64, None);
        download(&format!("http://{addr}/asset"), &dest, |received, total| {
            last = (received, total);
        })
        .unwrap();
        server.join().unwrap();

        assert_eq!(fs::read(&dest).unwrap(), body);
        assert_eq!(last, (body.len() as u64, Some(body.len() as u64)));
        assert!(!dest.with_extension("part").exists());
    }
}
