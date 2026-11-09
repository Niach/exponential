//! Self-update engine (EXP-22) — the gpui-free half of the in-app updater.
//!
//! [`crate::ui`]'s update module owns the check, the state machine, and the
//! banner; this crate owns everything that touches the filesystem and the
//! network payloads: capability probing (can this install self-update at
//! all?), the streaming download, the signed SHA256SUMS feed (EXP-1430), and
//! the per-OS swap. Everything here is blocking — callers run it on the gpui background
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

pub mod restart;

/// GitHub rejects API/asset calls without a User-Agent.
const USER_AGENT: &str = "exp-desktop-updater";
/// The checksums asset every desktop release carries (build-desktop.yml).
pub const SUMS_ASSET: &str = "SHA256SUMS.txt";
/// The minisign (ed25519) signature over [`SUMS_ASSET`]'s exact bytes, written
/// by the reusable `.github/actions/updater-feed` action.
pub const SIG_ASSET: &str = "SHA256SUMS.txt.minisig";

/// The feed's minisign public key (base64, the second line of the `.pub`
/// file), compiled in from `EXP_UPDATE_PUBKEY`. `Some` = every update MUST
/// carry a valid [`SIG_ASSET`]; `None` (local/dev builds) = checksums only.
pub fn pinned_pubkey() -> Option<&'static str> {
    option_env!("EXP_UPDATE_PUBKEY")
        .map(str::trim)
        .filter(|key| !key.is_empty())
}

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

/// Why [`capability`] degraded to banner-only, as one human-readable sentence
/// fragment — `None` when the install can self-update. EXP-316: the blocking
/// "Update required" view surfaces this; before, a Linux user staring at a
/// browser-link button had no way to tell WHICH gate failed ($APPIMAGE unset
/// vs a moved file vs an unwritable folder all rendered identically).
pub fn self_update_unavailable_reason() -> Option<String> {
    if matches!(capability(), Capability::SelfUpdate(_)) {
        return None;
    }
    #[cfg(target_os = "linux")]
    {
        let Some(appimage) = std::env::var_os("APPIMAGE").map(PathBuf::from) else {
            return Some(
                "this install isn't running as an AppImage ($APPIMAGE is unset)".to_string(),
            );
        };
        if !appimage.is_file() {
            // The classic AppImageLauncher failure: it moves the file after
            // launch, so the runtime's recorded path no longer exists.
            return Some(format!(
                "the AppImage this app was launched from no longer exists (moved or renamed?): {}",
                appimage.display()
            ));
        }
        Some(format!(
            "the AppImage's folder isn't writable: {}",
            appimage
                .parent()
                .map(|dir| dir.display().to_string())
                .unwrap_or_default()
        ))
    }
    #[cfg(target_os = "windows")]
    {
        Some("the folder containing the app isn't writable".to_string())
    }
    #[cfg(target_os = "macos")]
    {
        let bundle = std::env::current_exe().ok().and_then(|exe| {
            let bundle = exe.parent()?.parent()?.parent()?;
            (bundle.extension()? == "app").then(|| bundle.to_path_buf())
        });
        match bundle {
            None => Some("this install isn't running from a .app bundle".to_string()),
            Some(bundle) => Some(format!(
                "the folder containing the app isn't writable: {}",
                bundle
                    .parent()
                    .map(|dir| dir.display().to_string())
                    .unwrap_or_default()
            )),
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        Some("self-update isn't supported on this platform".to_string())
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

/// The `cli-v*` release asset for this machine (EXP-403): a bare binary
/// named `exponential-<target triple>`, matching build-cli.yml's matrix.
/// Unsupported OS/arch combinations still produce a name — it simply never
/// matches an asset and the update check degrades to "no update found".
pub fn cli_asset_name() -> String {
    format!("exponential-{}", cli_target_triple())
}

/// The build-cli.yml target triple for the running host.
pub fn cli_target_triple() -> String {
    let arch = std::env::consts::ARCH;
    match std::env::consts::OS {
        "linux" => format!("{arch}-unknown-linux-gnu"),
        "macos" => format!("{arch}-apple-darwin"),
        other => format!("{arch}-{other}"),
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

/// The updater's own `reqwest` client — same library as the rest of the app
/// (EXP-304: there is exactly one HTTP stack here), but deliberately NOT
/// `api::http::shared()`: that one carries a 30s whole-request deadline, which
/// would kill a hundred-megabyte release download on a slow link. No default
/// deadline here; callers set their own per request, and TCP keepalive probes
/// kill a silently-dead peer rather than leaving a transfer hanging.
///
/// Release assets are plain `github.com/<repo>/releases/download/<tag>/<name>`
/// URLs (never api.github.com, so no API rate limit applies) that 302 to
/// `release-assets.githubusercontent.com`; the redirect policy is pinned
/// explicitly so a default change can never strand the download on the 302.
///
/// Built per call — the updater makes a handful of requests in a session, so
/// there is nothing to pool.
fn client() -> Result<reqwest::blocking::Client> {
    exp_http::blocking()
        .connect_timeout(Duration::from_secs(10))
        .tcp_keepalive(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .context("build updater HTTP client")
}

/// Whole-request backstop for the release download. Generous — a large build
/// over a slow link is legitimate — but bounded, so a wedged transfer can't
/// hang the updater thread for the life of the process.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// Attempts per request. A laptop waking from sleep or a flapping Wi-Fi gives
/// "Could not resolve host" / "operation timed out" for a minute (the 0.14.91
/// failure: one connect timeout to github.com failed the whole update), so
/// transient failures retry with backoff instead of surfacing at once.
const ATTEMPTS: u32 = 4;

/// Backoff before retry `n` (1-based): 2s, 4s, 8s — ~14s in total, inside what
/// a person watching the banner tolerates. Tests shrink it to milliseconds.
fn backoff(retry: u32) -> Duration {
    let unit = if cfg!(test) { Duration::from_millis(5) } else { Duration::from_secs(1) };
    unit * 2u32.pow(retry.min(5))
}

/// One failed attempt: the error, and whether trying again can help.
struct Attempt {
    error: anyhow::Error,
    transient: bool,
}

/// Retry policy by HTTP status: rate limits and server errors pass, a 404 or
/// 403 (wrong tag, missing asset) never will.
fn transient_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status == reqwest::StatusCode::REQUEST_TIMEOUT
        || status.is_server_error()
}

/// The innermost cause of a transport error ("operation timed out", "dns
/// error: …") — reqwest's top-level message repeats the URL we already print.
fn transport_cause(err: &reqwest::Error) -> String {
    let mut cause: &dyn std::error::Error = err;
    while let Some(next) = cause.source() {
        cause = next;
    }
    let kind = if err.is_connect() {
        "connect failed"
    } else if err.is_timeout() {
        "timed out"
    } else {
        "request failed"
    };
    let inner = cause.to_string();
    if inner == err.to_string() { kind.to_string() } else { format!("{kind}: {inner}") }
}

/// Send one GET; a non-2xx answer becomes an error carrying the status AND
/// the start of the body (GitHub explains a 403/429 there), so the banner
/// shows the real reason instead of a bare step name.
fn get_once(url: &str, timeout: Duration) -> Result<reqwest::blocking::Response, Attempt> {
    let client = client().map_err(|error| Attempt { error, transient: false })?;
    let response = client
        .get(url)
        .timeout(timeout)
        .header("User-Agent", USER_AGENT)
        .send()
        .map_err(|err| Attempt {
            error: anyhow::anyhow!("GET {url}: {}", transport_cause(&err)),
            transient: true,
        })?;
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().unwrap_or_default();
    let body = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let snippet: String = body.chars().take(200).collect();
    let error = if snippet.is_empty() {
        anyhow::anyhow!("GET {url}: HTTP {status}")
    } else {
        anyhow::anyhow!("GET {url}: HTTP {status}: {snippet}")
    };
    Err(Attempt { error, transient: transient_status(status) })
}

/// Run `attempt` up to [`ATTEMPTS`] times, backing off between transient
/// failures. The last error says how many tries it took.
fn with_retries<T>(mut attempt: impl FnMut() -> Result<T, Attempt>) -> Result<T> {
    let mut tries = 0;
    loop {
        tries += 1;
        match attempt() {
            Ok(value) => return Ok(value),
            Err(Attempt { error, transient }) if transient && tries < ATTEMPTS => {
                log::warn!("[updater] attempt {tries}/{ATTEMPTS} failed, retrying: {error:#}");
                std::thread::sleep(backoff(tries));
            }
            Err(Attempt { error, transient }) => {
                return Err(if transient && tries > 1 {
                    error.context(format!("gave up after {tries} attempts"))
                } else {
                    error
                });
            }
        }
    }
}

/// Fetch a small asset as raw bytes (signatures cover bytes, not decoded
/// text), retrying transient failures.
pub fn fetch_bytes(url: &str) -> Result<Vec<u8>> {
    with_retries(|| {
        let response = get_once(url, Duration::from_secs(30))?;
        response
            .bytes()
            .map(|bytes| bytes.to_vec())
            .map_err(|err| Attempt {
                error: anyhow::anyhow!("GET {url}: read body: {}", transport_cause(&err)),
                transient: true,
            })
    })
}

/// Fetch a small text asset (the CLI's release list) into memory.
pub fn fetch_text(url: &str) -> Result<String> {
    String::from_utf8(fetch_bytes(url)?).with_context(|| format!("GET {url}: body is not UTF-8"))
}

/// Fetch `SHA256SUMS.txt` and, when this build pins a key ([`pinned_pubkey`]),
/// its signature — verified over the exact bytes BEFORE any line is trusted.
/// `sig_url` is the release's [`SIG_ASSET`] URL (`None` = the release has none,
/// which a pinned build refuses).
pub fn fetch_verified_sums(sums_url: &str, sig_url: Option<&str>) -> Result<String> {
    fetch_verified_sums_with(sums_url, sig_url, pinned_pubkey())
}

/// [`fetch_verified_sums`] with an explicit key (tests, other embedders).
pub fn fetch_verified_sums_with(
    sums_url: &str,
    sig_url: Option<&str>,
    pubkey: Option<&str>,
) -> Result<String> {
    // No extra context: each error already names the URL, i.e. the asset.
    let sums = fetch_bytes(sums_url)?;
    let signature = match (pubkey, sig_url) {
        (Some(_), Some(url)) => {
            let bytes = fetch_bytes(url)?;
            Some(String::from_utf8(bytes).with_context(|| format!("{SIG_ASSET} is not UTF-8"))?)
        }
        _ => None,
    };
    verify_sums_signature(&sums, signature.as_deref(), pubkey)?;
    String::from_utf8(sums).with_context(|| format!("{SUMS_ASSET} is not UTF-8"))
}

/// Check the minisign `signature` over the exact `sums` bytes against
/// `pubkey` (minisign base64). No key = nothing to check (unsigned builds keep
/// checksum-only updates); a key with a missing or bad signature is a refusal,
/// so whoever can publish release assets cannot swap the sums AND the binary.
pub fn verify_sums_signature(sums: &[u8], signature: Option<&str>, pubkey: Option<&str>) -> Result<()> {
    let Some(pubkey) = pubkey else {
        return Ok(());
    };
    let refuse = |why: String| {
        anyhow::anyhow!("update refused: {why} — this build only installs updates signed with its pinned key")
    };
    let signature = signature
        .ok_or_else(|| refuse(format!("the release has no {SIG_ASSET}")))?;
    let key = minisign_verify::PublicKey::from_base64(pubkey.trim())
        .map_err(|err| refuse(format!("the pinned update key is invalid ({err})")))?;
    let signature = minisign_verify::Signature::decode(signature)
        .map_err(|err| refuse(format!("{SIG_ASSET} is malformed ({err})")))?;
    // allow_legacy = false: only prehashed (BLAKE2b) signatures, minisign's default.
    key.verify(sums, &signature, false)
        .map_err(|err| refuse(format!("{SIG_ASSET} does not verify {SUMS_ASSET} ({err})")))?;
    log::info!("[updater] {SUMS_ASSET} signature ok ({})", signature.trusted_comment());
    Ok(())
}

/// Stream `url` to `dest`, reporting `(received, total)` after each chunk.
/// Writes `<dest>.part` and renames on success so `dest` is never partial.
/// A transient failure (connect, mid-stream drop, 5xx) restarts the download
/// from zero; the progress callback simply sees the count reset.
pub fn download(url: &str, dest: &Path, mut progress: impl FnMut(u64, Option<u64>)) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).context("create staging dir")?;
    }
    let part = dest.with_extension("part");
    let result = with_retries(|| download_once(url, &part, &mut progress));
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

/// One attempt of [`download`] into `part`.
fn download_once(
    url: &str,
    part: &Path,
    progress: &mut impl FnMut(u64, Option<u64>),
) -> Result<(), Attempt> {
    let local = |error: anyhow::Error| Attempt { error, transient: false };
    let mut response = get_once(url, DOWNLOAD_TIMEOUT)?;
    let total = response.content_length();
    let mut file = fs::File::create(part).context("create download file").map_err(local)?;
    let mut received = 0u64;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = response.read(&mut buf).map_err(|err| Attempt {
            error: anyhow::anyhow!("GET {url}: read download stream: {err}"),
            transient: true,
        })?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).context("write download file").map_err(local)?;
        received += n as u64;
        progress(received, total);
    }
    if let Some(total) = total {
        if received != total {
            return Err(Attempt {
                error: anyhow::anyhow!("GET {url}: download truncated: {received} of {total} bytes"),
                transient: true,
            });
        }
    }
    file.sync_all().ok();
    Ok(())
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

/// Apply the verified download. Returns the path to relaunch: on Windows the
/// exe's path read BEFORE the swap (FEED-189: the swap renames the running
/// file, so nothing asked afterwards can be trusted to name the new one). The
/// staged file is consumed (Linux) or deleted (Windows/macOS) on success.
pub fn install(strategy: &Strategy, staged: &Path) -> Result<Option<PathBuf>> {
    match strategy {
        Strategy::AppImage { appimage } => {
            install_appimage(appimage, staged).map(|()| Some(appimage.clone()))
        }
        Strategy::ReplaceExe => {
            let exe = std::env::current_exe().ok();
            install_exe(staged)?;
            let _ = fs::remove_file(staged);
            Ok(exe)
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

    // A throwaway feed key (`minisign -G -W`) and a signature made with it
    // (`minisign -S -t "tag desktop-v1.2.3"`) over FEED_SUMS — fixtures only.
    const FEED_PUBKEY: &str = "RWRRBEFNvlEu22NyHZg6KvyFpD5FWe5N/2lx0j3XrAzdb+UBlptio0Ke";
    const OTHER_PUBKEY: &str = "RWQvtZV0Y0tG1kYRc7i+tP0Q/6U9Mp4RB+nd6lyjSUbLSeNguJoNfRIx";
    const FEED_SUMS: &str = "aa11  asset-a.bin\nbb22  asset-b.bin\n";
    const FEED_SIG: &str = "untrusted comment: signature from minisign secret key
RURRBEFNvlEu20CHa+lEheDzo5QqGJYK0IhdOvlExVHxy3EITHEiYWxCn5CdzKcPcGEogYJW2r4QzQl82BWElnAnS+L+kN93CA8=
trusted comment: tag desktop-v1.2.3
GHJpf48YDEFXRQjHAQHezyTTT8wpq3juYC4VXTRKRxyi99FM7TCJNCj5A6gIBfJiSY92kVbKmmXWuZrLLJI9Ag==
";

    #[test]
    fn signature_accepts_the_signed_sums() {
        verify_sums_signature(FEED_SUMS.as_bytes(), Some(FEED_SIG), Some(FEED_PUBKEY)).unwrap();
    }

    #[test]
    fn signature_rejects_tampered_sums() {
        let tampered = FEED_SUMS.replace("aa11", "ff99");
        let err = verify_sums_signature(tampered.as_bytes(), Some(FEED_SIG), Some(FEED_PUBKEY))
            .unwrap_err();
        assert!(err.to_string().contains("update refused"), "{err}");
        // One appended line is a tamper too.
        let appended = format!("{FEED_SUMS}cc33  evil.bin\n");
        assert!(verify_sums_signature(appended.as_bytes(), Some(FEED_SIG), Some(FEED_PUBKEY)).is_err());
    }

    #[test]
    fn signature_required_when_a_key_is_pinned() {
        let err = verify_sums_signature(FEED_SUMS.as_bytes(), None, Some(FEED_PUBKEY)).unwrap_err();
        assert!(err.to_string().contains(SIG_ASSET), "{err}");
    }

    #[test]
    fn signature_rejects_a_different_key_and_garbage() {
        assert!(verify_sums_signature(FEED_SUMS.as_bytes(), Some(FEED_SIG), Some(OTHER_PUBKEY)).is_err());
        assert!(verify_sums_signature(FEED_SUMS.as_bytes(), Some("not a signature"), Some(FEED_PUBKEY)).is_err());
        assert!(verify_sums_signature(FEED_SUMS.as_bytes(), Some(FEED_SIG), Some("not-a-key")).is_err());
    }

    #[test]
    fn unpinned_builds_skip_the_signature() {
        verify_sums_signature(FEED_SUMS.as_bytes(), None, None).unwrap();
        verify_sums_signature(b"anything", Some("garbage"), None).unwrap();
    }

    /// Serve `bodies` (path prefix → body) on an ephemeral listener, one
    /// request per connection, `count` connections.
    fn serve(bodies: Vec<(&'static str, Vec<u8>)>, count: usize) -> (String, std::thread::JoinHandle<()>) {
        use std::io::Write as _;
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for _ in 0..count {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let n = std::io::Read::read(&mut stream, &mut buf).unwrap();
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                let body = bodies
                    .iter()
                    .find(|(path, _)| request.starts_with(&format!("GET {path} ")))
                    .map(|(_, body)| body.clone())
                    .unwrap_or_default();
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(header.as_bytes()).unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        (base, server)
    }

    #[test]
    fn fetch_verified_sums_checks_the_feed() {
        let (base, server) = serve(
            vec![("/sums", FEED_SUMS.into()), ("/sig", FEED_SIG.into())],
            2,
        );
        let sums = fetch_verified_sums_with(
            &format!("{base}/sums"),
            Some(&format!("{base}/sig")),
            Some(FEED_PUBKEY),
        )
        .unwrap();
        server.join().unwrap();
        assert_eq!(sums, FEED_SUMS);

        // A pinned key with no signature asset refuses after fetching the sums.
        let (base, server) = serve(vec![("/sums", FEED_SUMS.into())], 1);
        assert!(fetch_verified_sums_with(&format!("{base}/sums"), None, Some(FEED_PUBKEY)).is_err());
        server.join().unwrap();

        // No key: today's checksum-only behaviour, the signature is not fetched.
        let (base, server) = serve(vec![("/sums", FEED_SUMS.into())], 1);
        assert_eq!(fetch_verified_sums_with(&format!("{base}/sums"), None, None).unwrap(), FEED_SUMS);
        server.join().unwrap();
    }

    /// Answer connection `i` with `script[i]` (a raw HTTP response; `None` =
    /// drop the connection unanswered), returning the request lines seen.
    fn serve_script(script: Vec<Option<String>>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        use std::io::Write as _;
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for reply in script {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let n = std::io::Read::read(&mut stream, &mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                seen.push(request.lines().next().unwrap_or_default().to_string());
                if let Some(reply) = reply {
                    stream.write_all(reply.as_bytes()).unwrap();
                }
            }
            seen
        });
        (base, server)
    }

    fn reply(status: &str, headers: &str, body: &str) -> Option<String> {
        Some(format!(
            "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ))
    }

    /// The 0.14.91 failure: the first try never gets an answer (here: a
    /// dropped connection, there: a connect timeout), the next a 503, and the
    /// asset then sits behind GitHub's 302 to release-assets — the fetch must
    /// ride all three out and still verify the signature strictly.
    #[test]
    fn fetch_retries_transient_failures_and_follows_redirects() {
        let (base, server) = serve_script(vec![
            None,
            reply("503 Service Unavailable", "", "upstream hiccup"),
            reply("302 Found", "Location: /release-assets/sums\r\n", ""),
            reply("200 OK", "", FEED_SUMS),
            reply("200 OK", "", FEED_SIG),
        ]);
        let sums = fetch_verified_sums_with(
            &format!("{base}/download/sums"),
            Some(&format!("{base}/download/sig")),
            Some(FEED_PUBKEY),
        )
        .unwrap();
        assert_eq!(sums, FEED_SUMS);
        let seen = server.join().unwrap();
        assert_eq!(seen[2], "GET /download/sums HTTP/1.1");
        assert_eq!(seen[3], "GET /release-assets/sums HTTP/1.1");
    }

    /// A permanent answer (404: wrong tag or asset name) is not retried, and
    /// the error carries the status and the body — what the banner shows.
    #[test]
    fn fetch_reports_status_and_body_without_retrying_a_404() {
        let (base, server) = serve_script(vec![reply("404 Not Found", "", "Not Found")]);
        let err = fetch_bytes(&format!("{base}/download/sums")).unwrap_err();
        assert_eq!(server.join().unwrap().len(), 1);
        let message = format!("{err:#}");
        assert!(message.contains("HTTP 404 Not Found: Not Found"), "{message}");
    }

    /// Transient failures that never clear give up after [`ATTEMPTS`] and say so.
    #[test]
    fn fetch_gives_up_after_the_attempt_budget() {
        let script = (0..ATTEMPTS)
            .map(|_| reply("429 Too Many Requests", "", "{\"message\":\"API rate limit exceeded\"}"))
            .collect();
        let (base, server) = serve_script(script);
        let err = fetch_text(&format!("{base}/releases")).unwrap_err();
        assert_eq!(server.join().unwrap().len(), ATTEMPTS as usize);
        let message = format!("{err:#}");
        assert!(message.contains(&format!("gave up after {ATTEMPTS} attempts")), "{message}");
        assert!(message.contains("HTTP 429 Too Many Requests"), "{message}");
        assert!(message.contains("API rate limit exceeded"), "{message}");
    }

    /// A download cut short mid-stream restarts and lands the full file.
    #[test]
    fn download_restarts_after_a_truncated_stream() {
        let body = "x".repeat(10_000);
        let truncated = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            &body[..100]
        );
        let (base, server) = serve_script(vec![Some(truncated), reply("200 OK", "", &body)]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("asset.bin");
        download(&format!("{base}/asset"), &dest, |_, _| {}).unwrap();
        assert_eq!(server.join().unwrap().len(), 2);
        assert_eq!(fs::read_to_string(&dest).unwrap(), body);
        assert!(!dest.with_extension("part").exists());
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
    fn unavailable_reason_absent_iff_self_update_capable() {
        // Whatever environment the test runs in, the two probes must agree:
        // a reason exists exactly when the install can NOT self-update.
        let capable = matches!(capability(), Capability::SelfUpdate(_));
        assert_eq!(self_update_unavailable_reason().is_none(), capable);
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
