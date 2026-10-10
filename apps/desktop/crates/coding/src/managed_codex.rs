//! EXP-1232 — Codex as a MANAGED download.
//!
//! Nobody installs the `codex` CLI for Exponential any more. This module
//! owns ONE pinned upstream build ([`PINNED_VERSION`], OpenAI's GitHub
//! release [`RELEASE_TAG`]): a host fetches the tarball for its target the
//! first time Codex is needed — the Codex sign-in, or (after an app update)
//! as soon as a stored Codex login is found with no binary beside it —
//! verifies it against the sha256 pinned below, unpacks it to
//! `{data_dir}/codex/<version>/codex`, and every Codex spawn
//! ([`Settings::resolved_path_for`]) points there. The IDE and the CLI
//! daemon share one data dir, so one download serves both.
//!
//! Why a sidecar EXECUTABLE and not a linked `codex-core`: Rust has no
//! stable ABI (a shared library would need a hand-written C surface over an
//! async crate), a dylib loaded at runtime into the notarized hardened-runtime
//! app needs library validation off, and the engine's app-server adapter
//! stays byte-for-byte — the upstream `codex` binary IS `codex app-server`,
//! `codex login --device-auth` and `codex --version`.
//!
//! Why straight from OpenAI's release and not re-hosted on ours: six ~100 MB
//! tarballs per Exponential release for no gain — the digests pinned here
//! make a tampered asset fail exactly like a tampered mirror would.
//! [`RELEASE_BASE_ENV`] points a mirror or an air-gapped box at another
//! base; a non-default `codexPath` in settings.json bypasses all of this
//! (used verbatim, [`Settings::codex_is_managed`]).
//!
//! Nothing of Codex ships in the app bundle and nothing is redistributed by
//! us (`docs/third-party-licences.md`).
//!
//! [`Settings::resolved_path_for`]: crate::settings::Settings::resolved_path_for
//! [`Settings::codex_is_managed`]: crate::settings::Settings::codex_is_managed

use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use sha2::Digest as _;

use crate::agent::CodingAgent;
use crate::lockfile::{self, LockError, LockOptions};
use crate::settings::Settings;

/// The upstream build every Codex spawn runs. Bumping it = this constant,
/// [`RELEASE_TAG`] and the six digests (`gh api
/// repos/openai/codex/releases/tags/<tag>` lists them as `digest`), then a
/// fresh install picks the new dir up and prunes the old one.
pub const PINNED_VERSION: &str = "0.160.1";
/// The GitHub release tag the tarballs live under.
pub const RELEASE_TAG: &str = "rust-v0.160.1";
/// Where the tarballs are fetched from unless [`RELEASE_BASE_ENV`] says
/// otherwise: `<base>/<tag>/<asset>`.
pub const DEFAULT_RELEASE_BASE: &str = "https://github.com/openai/codex/releases/download";
/// A mirror for the release assets (same `<base>/<tag>/<asset>` layout).
pub const RELEASE_BASE_ENV: &str = "EXP_CODEX_RELEASE_BASE";

/// One target's tarball: a single executable named like the asset minus
/// `.tar.gz`, which [`fetch`] renames to [`binary_name`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinnedAsset {
    pub target: &'static str,
    pub asset: &'static str,
    pub sha256: &'static str,
}

/// The six builds OpenAI publishes that this app runs on. Linux is the
/// static musl build (no glibc floor on a self-hosted box).
pub const ASSETS: [PinnedAsset; 6] = [
    PinnedAsset {
        target: "aarch64-apple-darwin",
        asset: "codex-aarch64-apple-darwin.tar.gz",
        sha256: "670af2b049d9c95afb74d7da385f30c5033d13a07175001dd8958c51944984d0",
    },
    PinnedAsset {
        target: "x86_64-apple-darwin",
        asset: "codex-x86_64-apple-darwin.tar.gz",
        sha256: "8d938ddb93c4424b1d45f1606984ed514c5aa70e463302a6a2227fba7af02db7",
    },
    PinnedAsset {
        target: "aarch64-unknown-linux-musl",
        asset: "codex-aarch64-unknown-linux-musl.tar.gz",
        sha256: "f54dc5852042445bf41da3aa31156f3cb02f52c5a1a04074de73dc5598f7e1f7",
    },
    PinnedAsset {
        target: "x86_64-unknown-linux-musl",
        asset: "codex-x86_64-unknown-linux-musl.tar.gz",
        sha256: "9226581be592d18f7e7f740a352fdb63aa61e45e39f7eb9b09d3888c84bba33f",
    },
    PinnedAsset {
        target: "aarch64-pc-windows-msvc",
        asset: "codex-aarch64-pc-windows-msvc.exe.tar.gz",
        sha256: "c122926788e8cf2215d93a70b93bb36a486a9a94b389fba1756c023d0751439a",
    },
    PinnedAsset {
        target: "x86_64-pc-windows-msvc",
        asset: "codex-x86_64-pc-windows-msvc.exe.tar.gz",
        sha256: "12cf8aa4f5e4ef3e6602f6c0e98838f8092f63357f01bd3c7379b216df27cc78",
    },
];

/// The doctor's `error` while the fetch runs — the launch gate's refusal and
/// `device_doctor`'s key for the `Downloading…` row.
pub const DOWNLOADING_MESSAGE: &str = "Codex is downloading on this machine. Try again in a minute.";
/// The doctor's `error` prefix after a failed fetch (the reason follows) —
/// `device_doctor`'s key for the `Download failed` row.
pub const DOWNLOAD_FAILED_PREFIX: &str = "Codex could not be downloaded:";

const LOCK_DIR: &str = "download.lock";
const FAILURE_FILE: &str = "last-error.txt";
/// Automatic retries since the last explicit [`ensure`]: a decimal count
/// beside [`FAILURE_FILE`] (absent = 0).
const RETRY_COUNT_FILE: &str = "retry-count";
const DOWNLOADS_DIR: &str = "downloads";
/// A lock dir whose heartbeat (5 s) stopped this long ago belonged to a
/// crashed host; the next fetch reclaims it.
const LOCK_STALE: Duration = Duration::from_secs(120);
/// `tar` on a ~250 MB executable.
const UNPACK_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// How long a second host waits for the one holding the lock.
const WAIT_FOR_PEER: Duration = Duration::from_secs(30 * 60);
/// A fetch nobody asked for that failed is retried on its own once the
/// recorded failure ([`FAILURE_FILE`]'s mtime) is this old…
pub const RETRY_COOLDOWN: Duration = Duration::from_secs(10 * 60);
/// …at most this many times since the last explicit [`ensure`] (a sign-in
/// or Update); then the row stays `Download failed · Update` until a person
/// acts. A headless daemon that booted during a network blip thus recovers
/// on its own instead of waiting for a click from another client.
pub const RETRY_CAP: u32 = 3;

/// The `ASSETS` target for the running host (`None` = no upstream build).
pub fn host_target() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "aarch64") => "aarch64-unknown-linux-musl",
        ("linux", "x86_64") => "x86_64-unknown-linux-musl",
        ("windows", "aarch64") => "aarch64-pc-windows-msvc",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        _ => return None,
    })
}

/// The pinned tarball for the running host.
pub fn host_asset() -> Option<&'static PinnedAsset> {
    let target = host_target()?;
    ASSETS.iter().find(|asset| asset.target == target)
}

/// [`DEFAULT_RELEASE_BASE`] or the [`RELEASE_BASE_ENV`] override, without a
/// trailing slash.
pub fn release_base() -> String {
    std::env::var(RELEASE_BASE_ENV)
        .ok()
        .map(|base| base.trim().trim_end_matches('/').to_string())
        .filter(|base| !base.is_empty())
        .unwrap_or_else(|| DEFAULT_RELEASE_BASE.to_string())
}

/// `<base>/<tag>/<asset>`.
pub fn asset_url(asset: &PinnedAsset) -> String {
    asset_url_at(&release_base(), asset)
}

fn asset_url_at(base: &str, asset: &PinnedAsset) -> String {
    format!("{base}/{RELEASE_TAG}/{}", asset.asset)
}

/// Everything managed lives under here: `<version>/` dirs, the download
/// staging dir, the lock and the last failure.
pub fn install_root(data_dir: &Path) -> PathBuf {
    data_dir.join("codex")
}

/// The executable's name inside its version dir.
pub fn binary_name() -> &'static str {
    if cfg!(windows) {
        "codex.exe"
    } else {
        "codex"
    }
}

fn version_dir(data_dir: &Path) -> PathBuf {
    install_root(data_dir).join(PINNED_VERSION)
}

/// Where every Codex spawn points — whether or not the file exists yet.
pub fn binary_path(data_dir: &Path) -> PathBuf {
    version_dir(data_dir).join(binary_name())
}

/// Whether the pinned build is in place.
pub fn installed(data_dir: &Path) -> bool {
    binary_path(data_dir).is_file()
}

/// Whether this machine has any Codex login — the ambient `$CODEX_HOME` /
/// `~/.codex` credential or any account profile's. A login with no binary
/// is what makes a host fetch without being asked (an app update onto a
/// machine that already used Codex).
pub fn wanted(data_dir: &Path) -> bool {
    let has_credential = |home: &Path| home.join(crate::agent_login::CODEX_AUTH_FILE).is_file();
    if crate::codex_trust::codex_home(None).is_some_and(|home| has_credential(&home)) {
        return true;
    }
    crate::agent_profiles::list(data_dir, CodingAgent::Codex)
        .into_iter()
        .filter_map(|profile| {
            crate::agent_profiles::profile_dir(data_dir, CodingAgent::Codex, &profile.id)
        })
        .any(|dir| has_credential(&dir))
}

/// What the doctor says about the managed build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// The pinned build is in place — probe it like any CLI.
    Installed,
    /// A fetch is running here or in the other host, or a login is stored
    /// and the host is about to start one ([`should_fetch`]).
    Downloading,
    /// The last fetch failed; the reason. `agent_update codex` retries, as
    /// does the host itself: [`RETRY_CAP`] times, [`RETRY_COOLDOWN`] apart.
    Failed(String),
    /// No binary and no login: nothing to do until a sign-in.
    NotWanted,
}

/// [`State`] for `data_dir` (reads [`wanted`]).
pub fn state(data_dir: &Path) -> State {
    state_in(data_dir, || wanted(data_dir))
}

fn state_in(data_dir: &Path, wanted: impl FnOnce() -> bool) -> State {
    if installed(data_dir) {
        return State::Installed;
    }
    let root = install_root(data_dir);
    if lock_live(&root) || IN_FLIGHT.load(Ordering::SeqCst) {
        return State::Downloading;
    }
    if let Some(reason) = last_failure(&root) {
        return State::Failed(reason);
    }
    if wanted() {
        State::Downloading
    } else {
        State::NotWanted
    }
}

fn lock_live(root: &Path) -> bool {
    let Ok(metadata) = fs::metadata(root.join(LOCK_DIR)) else {
        return false;
    };
    let Ok(modified) = metadata.modified() else {
        return true;
    };
    SystemTime::now()
        .duration_since(modified)
        .map(|age| age < LOCK_STALE)
        .unwrap_or(true)
}

fn last_failure(root: &Path) -> Option<String> {
    let text = fs::read_to_string(root.join(FAILURE_FILE)).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn record_failure(root: &Path, reason: &str) {
    let _ = fs::create_dir_all(root);
    if let Err(err) = write_atomic(&root.join(FAILURE_FILE), reason.as_bytes()) {
        log::debug!("managed codex: could not record the failure: {err}");
    }
}

fn clear_failure(root: &Path) {
    let _ = fs::remove_file(root.join(FAILURE_FILE));
    reset_automatic_retries(root);
}

/// Write beside, then rename over: a reader never sees a torn file.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

/// Age of the recorded failure at `now`; `None` when there is none.
fn failure_age(root: &Path, now: SystemTime) -> Option<Duration> {
    let modified = fs::metadata(root.join(FAILURE_FILE)).ok()?.modified().ok()?;
    Some(now.duration_since(modified).unwrap_or(Duration::ZERO))
}

fn automatic_retries(root: &Path) -> u32 {
    fs::read_to_string(root.join(RETRY_COUNT_FILE))
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(0)
}

/// A fetch nobody asked for is starting over a recorded failure: count it
/// against [`RETRY_CAP`]. Nothing to count without a failure.
fn note_automatic_retry(root: &Path) {
    if failure_age(root, SystemTime::now()).is_none() {
        return;
    }
    let count = automatic_retries(root).saturating_add(1);
    let _ = fs::create_dir_all(root);
    if let Err(err) = write_atomic(&root.join(RETRY_COUNT_FILE), count.to_string().as_bytes()) {
        log::debug!("managed codex: could not count the retry: {err}");
    }
}

fn reset_automatic_retries(root: &Path) {
    let _ = fs::remove_file(root.join(RETRY_COUNT_FILE));
}

/// Whether the recorded failure still holds an automatic fetch back: it is
/// younger than [`RETRY_COOLDOWN`], or [`RETRY_CAP`] retries already failed
/// since the last explicit [`ensure`]. No failure holds nothing back.
fn failure_blocks_retry(root: &Path, now: SystemTime) -> bool {
    match failure_age(root, now) {
        None => false,
        Some(age) => age < RETRY_COOLDOWN || automatic_retries(root) >= RETRY_CAP,
    }
}

/// Who wants the build: a person (sign-in, Update, a launch) or the host
/// itself over a stored login ([`should_fetch`]). Only the latter spends
/// the automatic-retry budget; the former starts it over.
#[derive(Clone, Copy)]
enum Intent {
    Explicit,
    Automatic,
}

type Fetch = fn(&Path, &PinnedAsset, &mut dyn FnMut(u64, Option<u64>)) -> Result<PathBuf, String>;

/// Make sure the pinned build is in place; returns its path. Blocking for
/// the download (callers run it off the UI thread). ONE fetch per machine
/// at a time: a second caller — the daemon beside the IDE, a sign-in beside
/// the upgrade fetch — waits for the holder and then reads the result.
/// A failure is recorded for the doctor ([`State::Failed`]) and returned.
/// Asked for explicitly, so the automatic-retry budget starts over.
pub fn ensure(
    data_dir: &Path,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<PathBuf, String> {
    ensure_with(data_dir, progress, Intent::Explicit, fetch)
}

/// [`ensure`] for the host's own fetch over a stored login — one more
/// automatic retry if a failure is on record ([`should_fetch`] said it is
/// due).
fn ensure_automatic(data_dir: &Path) -> Result<PathBuf, String> {
    ensure_with(data_dir, &mut |_, _| {}, Intent::Automatic, fetch)
}

fn ensure_with(
    data_dir: &Path,
    progress: &mut dyn FnMut(u64, Option<u64>),
    intent: Intent,
    fetch: Fetch,
) -> Result<PathBuf, String> {
    let path = binary_path(data_dir);
    if path.is_file() {
        return Ok(path);
    }
    let root = install_root(data_dir);
    match intent {
        Intent::Explicit => reset_automatic_retries(&root),
        Intent::Automatic => note_automatic_retry(&root),
    }
    let Some(asset) = host_asset() else {
        let reason = format!(
            "Codex has no build for this machine ({} {}).",
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        record_failure(&root, &reason);
        return Err(reason);
    };
    fs::create_dir_all(&root)
        .map_err(|err| format!("Could not create {}: {err}", root.display()))?;
    let lock = root.join(LOCK_DIR);
    let options = LockOptions {
        stale: LOCK_STALE,
        update: Duration::from_secs(5),
        attempts: 1,
        backoff: Duration::ZERO,
        jitter: Duration::ZERO,
    };
    match lockfile::acquire(&lock, options) {
        Ok(guard) => {
            let result = fetch(data_dir, asset, progress);
            match &result {
                Ok(_) => clear_failure(&root),
                Err(reason) => record_failure(&root, reason),
            }
            let _ = guard.release();
            result
        }
        Err(LockError::Contended) => wait_for_peer(data_dir),
        Err(LockError::Io(err)) => {
            let reason = format!("Could not lock {}: {err}", lock.display());
            record_failure(&root, &reason);
            Err(reason)
        }
    }
}

/// The other host holds the lock: wait for it to finish, then read what it
/// left behind.
fn wait_for_peer(data_dir: &Path) -> Result<PathBuf, String> {
    let root = install_root(data_dir);
    let started = Instant::now();
    while started.elapsed() < WAIT_FOR_PEER {
        if installed(data_dir) {
            return Ok(binary_path(data_dir));
        }
        if !lock_live(&root) {
            return Err(last_failure(&root)
                .unwrap_or_else(|| "The other Codex download did not finish.".to_string()));
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    Err("Waited too long for the Codex download running in another window.".to_string())
}

/// Download → verify → unpack into `<version>.tmp` → rename into place →
/// prune older pins. The version dir appears atomically, so a reader never
/// sees a half-written binary.
fn fetch(
    data_dir: &Path,
    asset: &PinnedAsset,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<PathBuf, String> {
    let root = install_root(data_dir);
    let archive = root.join(DOWNLOADS_DIR).join(asset.asset);
    let url = asset_url(asset);
    log::info!("managed codex: fetching {url}");
    updater::download(&url, &archive, |received, total| progress(received, total))
        .map_err(|err| format!("download of {} failed: {err:#}", asset.asset))?;
    let actual = sha256_file(&archive)
        .map_err(|err| format!("could not read {}: {err}", archive.display()))?;
    if !actual.eq_ignore_ascii_case(asset.sha256) {
        let _ = fs::remove_file(&archive);
        return Err(format!(
            "{} did not match its pinned digest (expected {}, got {actual}).",
            asset.asset, asset.sha256
        ));
    }
    let stage = root.join(format!("{PINNED_VERSION}.tmp"));
    let _ = fs::remove_dir_all(&stage);
    fs::create_dir_all(&stage).map_err(|err| format!("could not create {}: {err}", stage.display()))?;
    let unpacked = (|| -> Result<(), String> {
        unpack(&archive, &stage)?;
        let file = single_file(&stage)?;
        let target = stage.join(binary_name());
        if file != target {
            fs::rename(&file, &target).map_err(|err| format!("could not place the binary: {err}"))?;
        }
        make_executable(&target)?;
        Ok(())
    })();
    let _ = fs::remove_file(&archive);
    if let Err(reason) = unpacked {
        let _ = fs::remove_dir_all(&stage);
        return Err(reason);
    }
    let final_dir = version_dir(data_dir);
    let _ = fs::remove_dir_all(&final_dir);
    fs::rename(&stage, &final_dir)
        .map_err(|err| format!("could not move the build into place: {err}"))?;
    prune_other_versions(&root);
    log::info!("managed codex: {} in place", final_dir.display());
    Ok(binary_path(data_dir))
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = sha2::Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// `tar -xzf <archive> -C <into>` — bsdtar ships with macOS and Windows 10+,
/// GNU tar with every Linux we run on; both read gzip tarballs.
fn unpack(archive: &Path, into: &Path) -> Result<(), String> {
    let mut cmd = terminal::process::background_command("tar");
    cmd.arg("-xzf").arg(archive).arg("-C").arg(into);
    let output = crate::doctor::output_with_timeout(cmd, UNPACK_TIMEOUT)
        .map_err(|err| format!("could not run tar: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "tar failed: {}",
        stderr.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or("no output")
    ))
}

/// The one regular file a tarball leaves behind (the upstream tarballs hold
/// exactly the executable).
fn single_file(dir: &Path) -> Result<PathBuf, String> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|err| format!("could not read {}: {err}", dir.display()))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    match files.len() {
        1 => Ok(files.remove(0)),
        0 => Err("the Codex tarball held no file.".to_string()),
        n => Err(format!("the Codex tarball held {n} files, expected one.")),
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|err| format!("could not mark the binary executable: {err}"))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// Older pins under the root go (a live run keeps its mapped binary on
/// unix; on Windows the locked file simply stays until the next prune).
fn prune_other_versions(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !path.is_dir() || name == PINNED_VERSION || name == LOCK_DIR || name == DOWNLOADS_DIR {
            continue;
        }
        if let Err(err) = fs::remove_dir_all(&path) {
            log::debug!("managed codex: could not prune {}: {err}", path.display());
        }
    }
}

static IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// Whether a host should start a background fetch now: Codex is managed,
/// the build is missing, a login is stored, nobody is fetching yet, and no
/// recorded failure holds it back — one does for [`RETRY_COOLDOWN`] after
/// it was written, and for good once [`RETRY_CAP`] automatic retries have
/// failed since the last explicit [`ensure`] ([`failure_blocks_retry`]).
pub fn should_fetch(settings: &Settings, data_dir: &Path) -> bool {
    should_fetch_in(settings, data_dir, SystemTime::now(), || wanted(data_dir))
}

fn should_fetch_in(
    settings: &Settings,
    data_dir: &Path,
    now: SystemTime,
    wanted: impl FnOnce() -> bool,
) -> bool {
    let root = install_root(data_dir);
    settings.codex_is_managed()
        && !installed(data_dir)
        && !IN_FLIGHT.load(Ordering::SeqCst)
        && !lock_live(&root)
        && !failure_blocks_retry(&root, now)
        && wanted()
}

/// [`ensure`] guarded by the process-wide in-flight flag: `None` when this
/// process already has a fetch running (the doctor reads it as
/// [`State::Downloading`] meanwhile). Blocking, like [`ensure`].
pub fn ensure_once(data_dir: &Path) -> Option<Result<PathBuf, String>> {
    if IN_FLIGHT.swap(true, Ordering::SeqCst) {
        return None;
    }
    let result = ensure_automatic(data_dir);
    IN_FLIGHT.store(false, Ordering::SeqCst);
    if let Err(reason) = &result {
        log::warn!("managed codex: {reason}");
    }
    Some(result)
}

/// [`ensure_once`] on its own thread; `on_done` runs on that thread with
/// the result. `false` = one is already running or no thread could start.
pub fn fetch_in_background(
    data_dir: PathBuf,
    on_done: impl FnOnce(Result<PathBuf, String>) + Send + 'static,
) -> bool {
    if IN_FLIGHT.swap(true, Ordering::SeqCst) {
        return false;
    }
    let spawned = std::thread::Builder::new()
        .name("exp-managed-codex".to_string())
        .spawn(move || {
            let result = ensure_automatic(&data_dir);
            IN_FLIGHT.store(false, Ordering::SeqCst);
            if let Err(reason) = &result {
                log::warn!("managed codex: {reason}");
            }
            on_done(result);
        });
    if spawned.is_err() {
        IN_FLIGHT.store(false, Ordering::SeqCst);
        return false;
    }
    true
}

/// The sign-in's prelude: a managed Codex login needs the binary first. A
/// no-op for claude and for a custom `codexPath`.
pub fn ensure_for(settings: &Settings, data_dir: &Path, agent: CodingAgent) -> Result<(), String> {
    if agent != CodingAgent::Codex || !settings.codex_is_managed() {
        return Ok(());
    }
    ensure(data_dir, &mut |_, _| {}).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "exp-managed-codex-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn every_target_has_one_asset_named_after_it() {
        for asset in ASSETS {
            assert!(asset.asset.starts_with(&format!("codex-{}", asset.target)), "{}", asset.asset);
            assert!(asset.asset.ends_with(".tar.gz"));
            assert_eq!(asset.sha256.len(), 64, "{}", asset.target);
            assert!(asset.sha256.chars().all(|c| c.is_ascii_hexdigit()));
        }
        let mut targets: Vec<_> = ASSETS.iter().map(|asset| asset.target).collect();
        targets.dedup();
        assert_eq!(targets.len(), 6);
    }

    #[test]
    fn this_host_has_a_build_and_its_url_is_the_release_asset() {
        let asset = host_asset().expect("dev hosts are one of the six targets");
        assert_eq!(
            asset_url_at(DEFAULT_RELEASE_BASE, asset),
            format!("https://github.com/openai/codex/releases/download/{RELEASE_TAG}/{}", asset.asset)
        );
        assert_eq!(asset_url_at("https://mirror.example/codex", asset), format!("https://mirror.example/codex/{RELEASE_TAG}/{}", asset.asset));
    }

    #[test]
    fn the_binary_lives_under_the_pinned_version_dir() {
        let data_dir = Path::new("/data");
        assert_eq!(
            binary_path(data_dir),
            Path::new("/data").join("codex").join(PINNED_VERSION).join(binary_name())
        );
        assert!(!installed(data_dir));
    }

    #[test]
    fn state_reads_installed_lock_failure_then_login() {
        let dir = TempDir::new("state");
        let data_dir = &dir.0;
        assert_eq!(state_in(data_dir, || false), State::NotWanted);
        assert_eq!(state_in(data_dir, || true), State::Downloading);

        let root = install_root(data_dir);
        record_failure(&root, "no network");
        assert_eq!(state_in(data_dir, || true), State::Failed("no network".into()));
        // A live lock outranks the recorded failure: a retry is running.
        fs::create_dir_all(root.join(LOCK_DIR)).unwrap();
        assert_eq!(state_in(data_dir, || false), State::Downloading);
        fs::remove_dir_all(root.join(LOCK_DIR)).unwrap();
        clear_failure(&root);
        assert_eq!(state_in(data_dir, || false), State::NotWanted);

        fs::create_dir_all(version_dir(data_dir)).unwrap();
        fs::write(binary_path(data_dir), b"#!/bin/sh\n").unwrap();
        assert_eq!(state_in(data_dir, || false), State::Installed);
    }

    #[test]
    fn a_failed_automatic_fetch_retries_after_the_cooldown_at_most_cap_times() {
        let dir = TempDir::new("retry");
        let data_dir = &dir.0;
        let root = install_root(data_dir);
        let settings = Settings::default();
        let now = SystemTime::now();
        let later = now + RETRY_COOLDOWN + Duration::from_secs(1);
        assert!(should_fetch_in(&settings, data_dir, now, || true));
        assert!(!should_fetch_in(&settings, data_dir, now, || false));

        // Nothing to count without a failure on record.
        note_automatic_retry(&root);
        assert_eq!(automatic_retries(&root), 0);
        assert!(!root.join(RETRY_COUNT_FILE).exists());

        // A fresh failure: no fetch yet…
        record_failure(&root, "no network");
        assert!(!should_fetch_in(&settings, data_dir, now, || true));
        assert!(!should_fetch_in(&settings, data_dir, now + RETRY_COOLDOWN / 2, || true));
        // …past the cooldown with the budget unspent: due.
        assert!(should_fetch_in(&settings, data_dir, later, || true));
        assert!(!should_fetch_in(&settings, data_dir, later, || false));
        // Each automatic retry spends one; at the cap it stays sticky
        // however old the failure gets.
        for spent in 1..RETRY_CAP {
            note_automatic_retry(&root);
            assert_eq!(automatic_retries(&root), spent);
            assert!(should_fetch_in(&settings, data_dir, later, || true), "{spent} spent");
        }
        note_automatic_retry(&root);
        assert_eq!(automatic_retries(&root), RETRY_CAP);
        assert!(!should_fetch_in(&settings, data_dir, later, || true));
        assert!(!should_fetch_in(&settings, data_dir, later + Duration::from_secs(86_400), || true));
        assert_eq!(state_in(data_dir, || true), State::Failed("no network".into()));

        // A live lock refuses regardless; clearing the failure clears the count.
        reset_automatic_retries(&root);
        fs::create_dir_all(root.join(LOCK_DIR)).unwrap();
        assert!(!should_fetch_in(&settings, data_dir, later, || true));
        fs::remove_dir_all(root.join(LOCK_DIR)).unwrap();
        note_automatic_retry(&root);
        clear_failure(&root);
        assert_eq!(automatic_retries(&root), 0);
        assert!(!root.join(FAILURE_FILE).exists());
        assert!(should_fetch_in(&settings, data_dir, now, || true));
    }

    fn failing_fetch(
        _: &Path,
        _: &PinnedAsset,
        _: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<PathBuf, String> {
        Err("stub: no network".to_string())
    }

    fn installing_fetch(
        data_dir: &Path,
        _: &PinnedAsset,
        _: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<PathBuf, String> {
        fs::create_dir_all(version_dir(data_dir)).unwrap();
        fs::write(binary_path(data_dir), b"#!/bin/sh\n").unwrap();
        Ok(binary_path(data_dir))
    }

    #[test]
    fn an_explicit_ensure_resets_the_retry_budget_an_automatic_one_spends_it() {
        let dir = TempDir::new("retry-reset");
        let data_dir = &dir.0;
        let root = install_root(data_dir);
        record_failure(&root, "no network");
        for _ in 0..RETRY_CAP {
            note_automatic_retry(&root);
        }
        assert_eq!(automatic_retries(&root), RETRY_CAP);

        // Sign-in / Update: the budget starts over; the failure is
        // re-recorded fresh, under the lock, and the lock is released.
        let err = ensure_with(data_dir, &mut |_, _| {}, Intent::Explicit, failing_fetch).unwrap_err();
        assert_eq!(err, "stub: no network");
        assert_eq!(state_in(data_dir, || true), State::Failed("stub: no network".into()));
        assert_eq!(automatic_retries(&root), 0);
        assert!(!root.join(LOCK_DIR).exists());

        // The host's own retry counts against the cap and keeps the record.
        let err = ensure_with(data_dir, &mut |_, _| {}, Intent::Automatic, failing_fetch).unwrap_err();
        assert_eq!(err, "stub: no network");
        assert_eq!(automatic_retries(&root), 1);
        assert_eq!(state_in(data_dir, || true), State::Failed("stub: no network".into()));

        // A success clears the failure and the count alike.
        let path = ensure_with(data_dir, &mut |_, _| {}, Intent::Automatic, installing_fetch).unwrap();
        assert_eq!(path, binary_path(data_dir));
        assert_eq!(state_in(data_dir, || false), State::Installed);
        assert!(!root.join(FAILURE_FILE).exists());
        assert!(!root.join(RETRY_COUNT_FILE).exists());
        // Installed: no accounting touches the disk any more.
        record_failure(&root, "stale");
        assert!(ensure_with(data_dir, &mut |_, _| {}, Intent::Automatic, failing_fetch).is_ok());
        assert_eq!(automatic_retries(&root), 0);
    }

    #[test]
    fn a_tarball_with_one_file_unpacks_to_the_binary_name() {
        let dir = TempDir::new("unpack");
        let src = dir.0.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("codex-some-target"), b"#!/bin/sh\necho codex-cli 0.0.0\n").unwrap();
        let archive = dir.0.join("codex.tar.gz");
        let status = std::process::Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(&src)
            .arg("codex-some-target")
            .status()
            .unwrap();
        assert!(status.success());
        let stage = dir.0.join("stage");
        fs::create_dir_all(&stage).unwrap();
        unpack(&archive, &stage).unwrap();
        let file = single_file(&stage).unwrap();
        assert_eq!(file.file_name().unwrap(), "codex-some-target");
        let sum = sha256_file(&archive).unwrap();
        assert_eq!(sum.len(), 64);
    }

    #[test]
    fn prune_keeps_the_pin_the_lock_and_the_downloads() {
        let dir = TempDir::new("prune");
        let root = install_root(&dir.0);
        for name in [PINNED_VERSION, "0.1.0", LOCK_DIR, DOWNLOADS_DIR] {
            fs::create_dir_all(root.join(name)).unwrap();
        }
        fs::write(root.join(FAILURE_FILE), "x").unwrap();
        prune_other_versions(&root);
        assert!(root.join(PINNED_VERSION).is_dir());
        assert!(root.join(LOCK_DIR).is_dir());
        assert!(root.join(DOWNLOADS_DIR).is_dir());
        assert!(root.join(FAILURE_FILE).is_file());
        assert!(!root.join("0.1.0").exists());
    }

    /// The real thing, once per pin bump: fetch this host's tarball from
    /// OpenAI's release, verify, unpack, answer `--version` with the pin and
    /// complete the app-server handshake the engine relies on. Network +
    /// ~100 MB, so `--ignored`.
    #[test]
    #[ignore]
    fn the_pinned_build_fetches_and_answers_the_app_server_handshake() {
        let dir = TempDir::new("real-fetch");
        let path = ensure(&dir.0, &mut |_, _| {}).unwrap();
        assert_eq!(path, binary_path(&dir.0));
        let check = crate::doctor::check_tool(crate::doctor::Tool::Codex, &path.to_string_lossy());
        assert_eq!(check.version.as_deref(), Some(PINNED_VERSION), "{:?}", check.error);
        crate::codex_app_server::probe(
            &path.to_string_lossy(),
            &terminal::pty::login_path(),
            Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(state_in(&dir.0, || false), State::Installed);
    }

    #[test]
    fn ensure_for_is_a_no_op_for_claude_and_a_custom_path() {
        let dir = TempDir::new("ensure-for");
        let mut settings = Settings::default();
        assert_eq!(ensure_for(&settings, &dir.0, CodingAgent::Claude), Ok(()));
        settings.codex_path = "/opt/codex/bin/codex".to_string();
        assert_eq!(ensure_for(&settings, &dir.0, CodingAgent::Codex), Ok(()));
    }
}
