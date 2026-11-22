//! Low-disk sweep of the shared cargo target: deletes stale build units
//! from `<clone>.cargo-target` (EXP-76's ONE target dir per repo) when the
//! volume holding it runs low (#EXP-1865).
//!
//! Cargo never evicts: every branch, feature set and bumped dep a session
//! builds leaves its own hashed copy in `deps/` and `build/<pkg>/<hash>/`.
//! On the MacBook parallel sessions grew the shared target to 42-49 GB in a
//! day twice (2026-11-14 and -16) and took the disk under 10 GB free. The
//! rule mirrors the manual cleanup that freed ~31 GB:
//!
//! - **Trigger:** free space on the target's volume below
//!   [`Config::free_disk_gb`] (settings.json `cargoSweep`, default 20 GB).
//!   Callers (the CLI daemon's 5-min beat, the IDE's AutoSync prune, the
//!   lanes through `exponential sweep-target --if-low`) pay a statvfs; a
//!   target is walked at most once per [`INTERVAL`] per process.
//! - **Units:** in every profile dir (`debug`, `release`, custom profiles,
//!   `<triple>/<profile>`), a `-<hash>` group in `deps/` and `examples/`
//!   (lib or bin), a hash dir of `build/<pkg>/` (or old-layout
//!   `build/<pkg>-<hash>/`; build-script compile or run), an
//!   `incremental/<crate>-<id>/` dir. Top-level artifacts are never touched.
//! - **Kept:** per crate and kind the [`KEEP_NEWEST`] newest units; any unit
//!   written in the last [`MIN_AGE`]; any unit a running process holds open.
//!   Everything else goes, with the `.fingerprint/<pkg>-<hash>` of a gone
//!   hash (cargo treats a missing output or fingerprint as stale and
//!   rebuilds that unit).
//! - **Lock:** deletion happens per profile only while WE hold all of its
//!   cargo locks (`.cargo-build-lock`, `.cargo-lock`,
//!   `.cargo-artifact-lock`, the flocks cargo itself takes while it builds,
//!   in its order). A running build skips the profile; a build starting
//!   mid-sweep waits for us. The walk and the open-file probe happen before
//!   the lock; each unit's mtime is re-read under it.
//! - **Report:** one log line per sweep, and the last sweep that found
//!   something in `<data_dir>/cargo-sweep.json` for the doctor's `Build
//!   cache` row. `dryRun` reports without deleting; `enabled: false` turns
//!   it off.

use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};

use crate::git_worktree::shared_cargo_target_dir;

/// The default free-disk threshold, decimal GB (the `disk` row's unit).
pub const DEFAULT_FREE_DISK_GB: u64 = 20;

/// Units kept per crate and kind, however old.
pub const KEEP_NEWEST: usize = 2;

/// A unit written more recently than this is never deleted.
pub const MIN_AGE: Duration = Duration::from_secs(2 * 60 * 60);

/// How often one target is walked while its disk stays low.
pub const INTERVAL: Duration = Duration::from_secs(15 * 60);

/// Cargo's per-profile locks, in the order cargo takes them.
const LOCK_FILES: [&str; 3] = [".cargo-build-lock", ".cargo-lock", ".cargo-artifact-lock"];

/// The settings.json key holding [`Config`].
const SETTINGS_KEY: &str = "cargoSweep";

/// The last sweep that found something, under the data dir.
const RECORD_FILE: &str = "cargo-sweep.json";

/// `settings.json` → `cargoSweep` (a missing or broken key = the defaults).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    /// The off switch.
    pub enabled: bool,
    /// Sweep while the target's volume has less free than this (decimal GB).
    pub free_disk_gb: u64,
    /// Report what would go, delete nothing.
    pub dry_run: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self { enabled: true, free_disk_gb: DEFAULT_FREE_DISK_GB, dry_run: false }
    }
}

impl Config {
    pub fn load(data_dir: &Path) -> Config {
        fs::read_to_string(crate::Settings::default_path(data_dir))
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
            .and_then(|root| root.get(SETTINGS_KEY).cloned())
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default()
    }

    pub fn threshold_bytes(&self) -> u64 {
        self.free_disk_gb.saturating_mul(1_000_000_000)
    }

    /// Whether `target`'s volume is below the threshold (unreadable = no).
    pub fn is_low(&self, target: &Path) -> bool {
        crate::worktree_lifecycle::free_bytes(target).is_some_and(|free| free < self.threshold_bytes())
    }
}

/// The rule's knobs ([`KEEP_NEWEST`], [`MIN_AGE`] in production).
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub keep: usize,
    pub min_age: Duration,
    pub dry_run: bool,
    pub now: SystemTime,
}

impl Options {
    pub fn new(dry_run: bool) -> Self {
        Self { keep: KEEP_NEWEST, min_age: MIN_AGE, dry_run, now: SystemTime::now() }
    }
}

/// Outcome of one sweep.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SweepReport {
    pub target: PathBuf,
    pub dry_run: bool,
    /// Bytes deleted (in a dry run: the bytes that would go).
    pub bytes_freed: u64,
    /// Units deleted (or that would go).
    pub units_removed: usize,
    /// Stale units left because a process holds a file in them.
    pub kept_open: usize,
    /// Profile dirs skipped because a build held their lock.
    pub skipped_locked: Vec<PathBuf>,
    /// The open-file probe failed: nothing was deleted.
    pub probe_failed: bool,
}

impl SweepReport {
    /// The daemon log line.
    pub fn summary(&self) -> String {
        if self.probe_failed {
            return format!(
                "cargo-target sweep {}: skipped, couldn't list open files",
                self.target.display()
            );
        }
        let mut line = format!(
            "cargo-target sweep {}: {} {} ({} bytes, {} units)",
            self.target.display(),
            if self.dry_run { "would free" } else { "freed" },
            crate::worktree_lifecycle::format_free(self.bytes_freed),
            self.bytes_freed,
            self.units_removed,
        );
        if self.kept_open > 0 {
            line.push_str(&format!(", kept {} open", self.kept_open));
        }
        if !self.skipped_locked.is_empty() {
            line.push_str(&format!(", skipped building {:?}", self.skipped_locked));
        }
        if self.dry_run {
            line.push_str(" (dry run)");
        }
        line
    }
}

/// The last sweep that freed (or, dry, found) something, and when the
/// sweep last ran at all.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SweepRecord {
    /// Unix seconds.
    pub at: i64,
    pub freed_bytes: u64,
    pub dry_run: bool,
    pub target: PathBuf,
    /// #EXP-1892: unix seconds of the last sweep, freeing or not (a stalled
    /// sweep shows as an old one); absent in records before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked_at: Option<i64>,
}

/// The record the doctor's `Build cache` row reads.
pub fn last_sweep(data_dir: &Path) -> Option<SweepRecord> {
    let raw = fs::read_to_string(data_dir.join(RECORD_FILE)).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Record one sweep: a sweep that freed something replaces the record; one
/// that freed nothing only moves `checkedAt` (no record yet = none).
fn record(data_dir: &Path, report: &SweepReport, at: SystemTime) {
    let at = at.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |since| since.as_secs() as i64);
    let record = if report.bytes_freed > 0 {
        SweepRecord {
            at,
            freed_bytes: report.bytes_freed,
            dry_run: report.dry_run,
            target: report.target.clone(),
            checked_at: Some(at),
        }
    } else {
        let Some(last) = last_sweep(data_dir) else { return };
        SweepRecord { checked_at: Some(at), ..last }
    };
    if let Ok(rendered) = serde_json::to_string(&record) {
        if let Err(err) = api::atomic_file::write_atomic(&data_dir.join(RECORD_FILE), &rendered) {
            log::warn!("cargo-target sweep: couldn't record the sweep: {err}");
        }
    }
}

/// Whether `dir` is a dir cargo made (it writes CACHEDIR.TAG into every
/// target).
pub fn is_cargo_target(dir: &Path) -> bool {
    dir.join("CACHEDIR.TAG").is_file()
}

/// Per-target "last walked" clock behind [`sweep_if_due`].
#[derive(Debug, Default)]
pub struct SweepThrottle {
    last_run: std::collections::HashMap<PathBuf, Instant>,
}

impl SweepThrottle {
    /// Whether `target` is due at `now`; a due target is stamped `now`.
    pub fn due(&mut self, target: &Path, now: Instant) -> bool {
        if self
            .last_run
            .get(target)
            .is_some_and(|at| now.saturating_duration_since(*at) < INTERVAL)
        {
            return false;
        }
        self.last_run.insert(target.to_path_buf(), now);
        true
    }
}

/// The clone's shared target, swept when the setting is on, the dir is a
/// cargo target, its disk is low and it was not walked in the last
/// [`INTERVAL`] (`None` = none of that held, nothing walked). The CLI daemon
/// and the IDE's AutoSync prune call this.
pub fn sweep_if_due(clone: &Path, data_dir: &Path) -> Option<SweepReport> {
    static THROTTLE: Mutex<Option<SweepThrottle>> = Mutex::new(None);
    let config = Config::load(data_dir);
    let target = shared_cargo_target_dir(clone);
    if !config.enabled || !is_cargo_target(&target) || !config.is_low(&target) {
        return None;
    }
    {
        let mut guard = THROTTLE.lock().unwrap_or_else(|poison| poison.into_inner());
        if !guard.get_or_insert_with(SweepThrottle::default).due(&target, Instant::now()) {
            return None;
        }
    }
    Some(sweep_now(&target, data_dir, config.dry_run))
}

/// One sweep of `target` now (no trigger check): probe the open files,
/// sweep, log one line, record it (what it freed, or only that it ran).
pub fn sweep_now(target: &Path, data_dir: &Path, dry_run: bool) -> SweepReport {
    let options = Options::new(dry_run);
    let report = match fs::canonicalize(target).ok().and_then(|root| Some((open_files_under(&root)?, root))) {
        Some((open, root)) => sweep_with(&root, &options, &open),
        None => SweepReport {
            target: target.to_path_buf(),
            dry_run,
            probe_failed: true,
            ..SweepReport::default()
        },
    };
    log::info!("{}", report.summary());
    record(data_dir, &report, options.now);
    report
}

/// The sweep over an explicit target, rule and open-file list (paths
/// under the CANONICAL target).
pub fn sweep_with(target: &Path, options: &Options, open: &[PathBuf]) -> SweepReport {
    sweep_inner(target, options, open, &mut |_| {})
}

/// [`sweep_with`] with a hook that runs while a profile's locks are held
/// (tests start a build there).
fn sweep_inner(
    target: &Path,
    options: &Options,
    open: &[PathBuf],
    under_lock: &mut dyn FnMut(&Path),
) -> SweepReport {
    let mut report = SweepReport {
        target: target.to_path_buf(),
        dry_run: options.dry_run,
        ..SweepReport::default()
    };
    if !is_cargo_target(target) {
        return report;
    }
    let cutoff = options.now.checked_sub(options.min_age).unwrap_or(SystemTime::UNIX_EPOCH);
    for profile in profile_dirs(target) {
        let doomed = stale_units(&profile, options.keep, cutoff, open, &mut report.kept_open);
        if doomed.is_empty() {
            continue;
        }
        let Some(_locks) = try_lock_profile(&profile) else {
            report.skipped_locked.push(profile);
            continue;
        };
        under_lock(&profile);
        for unit in doomed {
            // Under the lock: a unit a build rewrote since the walk stays.
            if newest_mtime_of(&unit.paths).is_some_and(|mtime| mtime >= cutoff) {
                continue;
            }
            let mut freed = 0;
            for path in &unit.paths {
                freed += if options.dry_run { tree_size(path) } else { remove(path) };
            }
            if freed > 0 {
                report.bytes_freed += freed;
                report.units_removed += 1;
            }
        }
    }
    report
}

/// What kind of output a unit is; the newest [`KEEP_NEWEST`] stay per crate
/// AND kind, so a crate's lib never pushes out its test binary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Lib,
    Bin,
    BuildCompile,
    BuildRun,
    Incremental,
}

/// One deletable unit: every path of one hash.
#[derive(Debug)]
struct Unit {
    /// (subdir, crate, kind): the group the newest-N rule counts in.
    group: (&'static str, String, Kind),
    hash: Option<String>,
    paths: Vec<PathBuf>,
    mtime: SystemTime,
}

/// The profile's units that go: everything but the newest `keep` per group,
/// the recently written and the held open (counted in `kept_open`), plus
/// the fingerprints of hashes that go entirely.
fn stale_units(
    profile: &Path,
    keep: usize,
    cutoff: SystemTime,
    open: &[PathBuf],
    kept_open: &mut usize,
) -> Vec<Unit> {
    let mut groups: BTreeMap<(&'static str, String, Kind), Vec<Unit>> = BTreeMap::new();
    for unit in units(profile) {
        groups.entry(unit.group.clone()).or_default().push(unit);
    }
    let mut doomed = Vec::new();
    let mut kept_hashes = HashSet::new();
    for (_, mut units) in groups {
        // Newest first; the path breaks ties so a sweep is reproducible.
        units.sort_by(|a, b| (b.mtime, &b.paths).cmp(&(a.mtime, &a.paths)));
        for (index, unit) in units.into_iter().enumerate() {
            let held = unit.paths.iter().any(|path| open.iter().any(|file| file.starts_with(path)));
            if index < keep || unit.mtime >= cutoff || held {
                if index >= keep && unit.mtime < cutoff {
                    *kept_open += 1;
                }
                kept_hashes.extend(unit.hash);
            } else {
                doomed.push(unit);
            }
        }
    }
    let fingerprints = fingerprint_dirs(profile);
    for unit in &mut doomed {
        let Some(hash) = &unit.hash else { continue };
        if kept_hashes.contains(hash) {
            continue;
        }
        if let Some(dirs) = fingerprints.get(hash) {
            unit.paths.extend(dirs.iter().cloned());
        }
    }
    // A hash split over two doomed units (deps + build) adds its
    // fingerprint once.
    let mut claimed = HashSet::new();
    for unit in &mut doomed {
        unit.paths.retain(|path| claimed.insert(path.clone()));
    }
    doomed
}

/// Every unit of one profile dir.
fn units(profile: &Path) -> Vec<Unit> {
    let mut out = Vec::new();
    for sub in ["deps", "examples"] {
        let mut by_hash: BTreeMap<String, (String, Vec<PathBuf>, bool)> = BTreeMap::new();
        for (name, path) in sorted_entries(&profile.join(sub)) {
            let Some((stem, hash, rest)) = split_hash(&name) else { continue };
            let is_lib = [".rlib", ".rmeta", ".a", ".so", ".dylib"].iter().any(|ext| rest.starts_with(ext));
            let krate = match stem.strip_prefix("lib") {
                Some(bare) if is_lib => bare,
                _ => stem,
            };
            let entry = by_hash.entry(hash.to_string()).or_insert_with(|| (krate.to_string(), Vec::new(), false));
            entry.2 |= rest.is_empty() || rest == ".exe";
            entry.1.push(path);
        }
        for (hash, (krate, paths, bin)) in by_hash {
            let kind = if bin { Kind::Bin } else { Kind::Lib };
            out.extend(unit((sub, krate, kind), Some(hash), paths));
        }
    }
    for (name, path) in sorted_entries(&profile.join("build")) {
        if !path.is_dir() {
            continue;
        }
        if let Some((pkg, hash, "")) = split_hash(&name) {
            // Old layout: `build/<pkg>-<hash>/`.
            let kind = build_kind(&path);
            out.extend(unit(("build", pkg.to_string(), kind), Some(hash.to_string()), vec![path]));
            continue;
        }
        // New layout: `build/<pkg>/<hash>/`.
        for (hash, dir) in sorted_entries(&path) {
            if is_hash(&hash) && dir.is_dir() {
                let kind = build_kind(&dir);
                out.extend(unit(("build", name.clone(), kind), Some(hash), vec![dir]));
            }
        }
    }
    for (name, path) in sorted_entries(&profile.join("incremental")) {
        if let Some((krate, _)) = name.rsplit_once('-').filter(|_| path.is_dir()) {
            out.extend(unit(("incremental", krate.to_string(), Kind::Incremental), None, vec![path]));
        }
    }
    out
}

fn unit(group: (&'static str, String, Kind), hash: Option<String>, paths: Vec<PathBuf>) -> Option<Unit> {
    let mtime = newest_mtime_of(&paths)?;
    Some(Unit { group, hash, paths, mtime })
}

/// A build-script RUN dir holds its output (`run/` in the new layout,
/// `output` in the old); anything else is the script's compile.
fn build_kind(dir: &Path) -> Kind {
    if dir.join("run").is_dir() || dir.join("output").is_file() {
        Kind::BuildRun
    } else {
        Kind::BuildCompile
    }
}

/// `.fingerprint/<pkg>-<hash>` dirs by hash.
fn fingerprint_dirs(profile: &Path) -> BTreeMap<String, Vec<PathBuf>> {
    let mut out: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for (name, path) in sorted_entries(&profile.join(".fingerprint")) {
        if let Some((_, hash, "")) = split_hash(&name) {
            out.entry(hash.to_string()).or_default().push(path);
        }
    }
    out
}

/// `name` = `<stem>-<16 hex><rest>` with `rest` empty or starting with `.`
/// (the LAST such hash wins).
fn split_hash(name: &str) -> Option<(&str, &str, &str)> {
    let bytes = name.as_bytes();
    let mut found = None;
    for (index, _) in name.match_indices('-') {
        let start = index + 1;
        let end = start + 16;
        if end > name.len() || !is_hash(&name[start..end]) {
            continue;
        }
        if end == name.len() || bytes[end] == b'.' {
            found = Some((&name[..index], &name[start..end], &name[end..]));
        }
    }
    found.filter(|(stem, _, _)| !stem.is_empty())
}

fn is_hash(text: &str) -> bool {
    text.len() == 16 && text.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// `(file name, path)` of a dir's entries, sorted; missing dir = none.
fn sorted_entries(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| Some((entry.file_name().into_string().ok()?, entry.path())))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// Profile dirs = dirs up to depth 2 holding a `.cargo-lock`.
fn profile_dirs(target: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut level = vec![target.to_path_buf()];
    for _ in 0..2 {
        let mut next = Vec::new();
        for dir in level {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    continue;
                }
                let path = entry.path();
                if path.join(".cargo-lock").is_file() {
                    out.push(path);
                } else {
                    next.push(path);
                }
            }
        }
        level = next;
    }
    out.sort();
    out
}

/// All of the profile's cargo locks, held until dropped; `None` = a build
/// holds one (or one can't be opened). Lock files an older cargo never
/// made are skipped.
fn try_lock_profile(profile: &Path) -> Option<Vec<File>> {
    let mut held = Vec::new();
    for name in LOCK_FILES {
        let path = profile.join(name);
        if !path.exists() {
            continue;
        }
        let file = OpenOptions::new().read(true).write(true).open(path).ok()?;
        file.try_lock().ok()?;
        held.push(file);
    }
    Some(held)
}

/// Every file under `root` a running process has open (`None` = the probe
/// failed, so nothing may go). Linux reads /proc; macOS asks lsof once;
/// Windows refuses to delete an open executable by itself.
pub fn open_files_under(root: &Path) -> Option<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = open_files()?.into_iter().filter(|path| path.starts_with(root)).collect();
    out.sort();
    out.dedup();
    Some(out)
}

#[cfg(target_os = "linux")]
fn open_files() -> Option<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in fs::read_dir("/proc").ok()?.flatten() {
        let pid = entry.path();
        if !entry.file_name().to_string_lossy().bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        out.extend(fs::read_link(pid.join("exe")));
        if let Ok(fds) = fs::read_dir(pid.join("fd")) {
            out.extend(fds.flatten().filter_map(|fd| fs::read_link(fd.path()).ok()));
        }
        if let Ok(maps) = fs::read_to_string(pid.join("maps")) {
            out.extend(
                maps.lines()
                    .filter_map(|line| line.split_whitespace().nth(5))
                    .filter(|path| path.starts_with('/'))
                    .map(PathBuf::from),
            );
        }
    }
    Some(out)
}

#[cfg(target_os = "macos")]
fn open_files() -> Option<Vec<PathBuf>> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use wait_timeout::ChildExt;
    // -F n: one `n<path>` line per open file; -b: never block in the kernel
    // on a hung mount; -w: no warnings.
    let mut child = Command::new("lsof")
        .args(["-n", "-b", "-w", "-F", "n"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        text
    });
    let status = match child.wait_timeout(Duration::from_secs(120)) {
        Ok(Some(status)) => status,
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
    };
    let text = reader.join().ok()?;
    // lsof exits 1 when some process could not be read; the list is still
    // complete for this user's processes.
    if !status.success() && text.is_empty() {
        return None;
    }
    Some(
        text.lines()
            .filter_map(|line| line.strip_prefix('n'))
            .filter(|path| path.starts_with('/'))
            .map(PathBuf::from)
            .collect(),
    )
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn open_files() -> Option<Vec<PathBuf>> {
    Some(Vec::new())
}

/// Newest mtime of the paths and (for real dirs) everything beneath them;
/// symlinks count as themselves, never followed.
fn newest_mtime_of(paths: &[PathBuf]) -> Option<SystemTime> {
    paths.iter().filter_map(|path| newest_mtime(path)).max()
}

fn newest_mtime(path: &Path) -> Option<SystemTime> {
    let meta = path.symlink_metadata().ok()?;
    let mut newest = meta.modified().ok()?;
    if meta.is_dir() {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                if let Some(mtime) = newest_mtime(&entry.path()) {
                    newest = newest.max(mtime);
                }
            }
        }
    }
    Some(newest)
}

/// Total bytes of files under `path` (symlinks not followed).
fn tree_size(path: &Path) -> u64 {
    let Ok(meta) = path.symlink_metadata() else {
        return 0;
    };
    if !meta.is_dir() {
        return meta.len();
    }
    fs::read_dir(path)
        .map(|entries| entries.flatten().map(|entry| tree_size(&entry.path())).sum())
        .unwrap_or(0)
}

/// Delete a file, symlink or dir tree; returns the bytes it held. Failures
/// are logged and count as nothing freed.
fn remove(path: &Path) -> u64 {
    let Ok(meta) = path.symlink_metadata() else {
        return 0;
    };
    let size = tree_size(path);
    let result = if meta.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    match result {
        Ok(()) => size,
        Err(err) => {
            log::warn!("cargo-target sweep: couldn't remove {}: {err}", path.display());
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn temp_dir(tag: &str) -> TempDir {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("exp-coding-gc-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        TempDir(fs::canonicalize(&path).unwrap())
    }

    const HOUR: Duration = Duration::from_secs(60 * 60);

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![0u8; bytes]).unwrap();
    }

    /// Set the mtime of `path` and everything under it to `hours` ago (in
    /// half hours: `age(path, 3)` = 1.5 h).
    fn age(path: &Path, half_hours: u32) {
        let meta = path.symlink_metadata().unwrap();
        if meta.is_dir() {
            for entry in fs::read_dir(path).unwrap().flatten() {
                age(&entry.path(), half_hours);
            }
        }
        let when = SystemTime::now() - HOUR / 2 * half_hours;
        File::open(path).unwrap().set_modified(when).unwrap();
    }

    fn profile(target: &Path, rel: &str) -> PathBuf {
        let profile = target.join(rel);
        for name in LOCK_FILES {
            write(&profile.join(name), 0);
        }
        profile
    }

    /// A lib unit of `krate` with `hash`: rlib + rmeta + .d + fingerprint,
    /// 100 + 10 + 1 + 1 bytes, `half_hours` old.
    fn lib(profile: &Path, krate: &str, hash: &str, half_hours: u32) {
        let paths = [
            profile.join(format!("deps/lib{krate}-{hash}.rlib")),
            profile.join(format!("deps/lib{krate}-{hash}.rmeta")),
            profile.join(format!("deps/{krate}-{hash}.d")),
            profile.join(format!(".fingerprint/{krate}-{hash}/lib-{krate}")),
        ];
        for (path, bytes) in paths.iter().zip([100, 10, 1, 1]) {
            write(path, bytes);
        }
        for path in &paths {
            age(path, half_hours);
        }
        age(&profile.join(format!(".fingerprint/{krate}-{hash}")), half_hours);
    }

    fn hash(n: u8) -> String {
        format!("{n:016x}")
    }

    /// The tree every rule test sweeps: a target with a `debug` profile and
    /// an iOS `release` one.
    ///
    /// - `foo` libs 1..=5, 0.5 h, 1 h, 1.5 h, 5 h and 10 h old: the two newest
    ///   stay by count, the 1.5 h one by age, 4 and 5 go.
    /// - `foo`'s only test binary, 20 h old: the newest of its kind, stays.
    /// - `bar` build-script runs 7..10 h old (new layout): two go; its 12 h
    ///   old compile dir is the newest of its kind and stays.
    /// - `baz` old-layout build dirs 5, 6, 7 h old: the 7 h one goes.
    /// - `qux` incremental sessions 3, 4, 5 h old: the 5 h one goes.
    /// - a 50 h old top-level binary: never touched.
    /// - iOS release: `ffi` libs 10, 11, 12 h old: the oldest goes.
    fn seed(root: &Path) -> PathBuf {
        let target = root.join("repo.cargo-target");
        write(&target.join("CACHEDIR.TAG"), 10);
        let debug = profile(&target, "debug");
        for (n, half_hours) in [(1, 1), (2, 2), (3, 3), (4, 10), (5, 20)] {
            lib(&debug, "foo", &hash(n), half_hours);
        }
        write(&debug.join(format!("deps/foo-{}", hash(6))), 50);
        write(&debug.join(format!("deps/foo-{}.d", hash(6))), 1);
        age(&debug.join(format!("deps/foo-{}", hash(6))), 40);
        age(&debug.join(format!("deps/foo-{}.d", hash(6))), 40);
        for (n, half_hours) in [(7, 14), (8, 16), (9, 18), (10, 20)] {
            let dir = debug.join(format!("build/bar/{}", hash(n)));
            write(&dir.join("run/stdout"), 20);
            write(&dir.join("out/gen.rs"), 20);
            age(&dir, half_hours);
        }
        let compile = debug.join(format!("build/bar/{}", hash(11)));
        write(&compile.join("out/build_script_build"), 30);
        age(&compile, 24);
        for (n, half_hours) in [(12, 10), (13, 12), (14, 14)] {
            let dir = debug.join(format!("build/baz-{}", hash(n)));
            write(&dir.join("output"), 15);
            age(&dir, half_hours);
        }
        for (id, half_hours) in [("aaa", 6), ("bbb", 8), ("ccc", 10)] {
            let dir = debug.join(format!("incremental/qux-{id}"));
            write(&dir.join("s-1/query-cache.bin"), 40);
            age(&dir, half_hours);
        }
        write(&debug.join("exp-desktop"), 70);
        age(&debug.join("exp-desktop"), 100);
        let ios = profile(&target, "aarch64-apple-ios/release");
        for (n, half_hours) in [(21, 20), (22, 22), (23, 24)] {
            lib(&ios, "ffi", &hash(n), half_hours);
        }
        target
    }

    fn sweep(target: &Path, open: &[PathBuf]) -> SweepReport {
        sweep_with(target, &Options::new(false), open)
    }

    fn rlib(profile: &Path, krate: &str, n: u8) -> PathBuf {
        profile.join(format!("deps/lib{krate}-{}.rlib", hash(n)))
    }

    #[test]
    fn keeps_the_newest_two_per_crate_and_kind_and_anything_recent() {
        let tmp = temp_dir("rule");
        let target = seed(&tmp.0);
        let debug = target.join("debug");
        let ios = target.join("aarch64-apple-ios/release");
        let report = sweep(&target, &[]);
        for kept in [1, 2, 3] {
            assert!(rlib(&debug, "foo", kept).exists(), "foo {kept}");
            assert!(debug.join(format!(".fingerprint/foo-{}", hash(kept))).exists());
        }
        for gone in [4, 5] {
            assert!(!rlib(&debug, "foo", gone).exists(), "foo {gone}");
            assert!(!debug.join(format!("deps/libfoo-{}.rmeta", hash(gone))).exists());
            assert!(!debug.join(format!("deps/foo-{}.d", hash(gone))).exists());
            assert!(!debug.join(format!(".fingerprint/foo-{}", hash(gone))).exists());
        }
        assert!(debug.join(format!("deps/foo-{}", hash(6))).exists(), "the only test binary");
        assert!(debug.join(format!("build/bar/{}", hash(7))).exists());
        assert!(debug.join(format!("build/bar/{}", hash(8))).exists());
        assert!(!debug.join(format!("build/bar/{}", hash(9))).exists());
        assert!(!debug.join(format!("build/bar/{}", hash(10))).exists());
        assert!(debug.join(format!("build/bar/{}", hash(11))).exists(), "the only compile dir");
        assert!(debug.join(format!("build/baz-{}", hash(12))).exists());
        assert!(debug.join(format!("build/baz-{}", hash(13))).exists());
        assert!(!debug.join(format!("build/baz-{}", hash(14))).exists());
        assert!(debug.join("incremental/qux-aaa").exists());
        assert!(debug.join("incremental/qux-bbb").exists());
        assert!(!debug.join("incremental/qux-ccc").exists());
        assert!(debug.join("exp-desktop").exists());
        assert!(rlib(&ios, "ffi", 21).exists());
        assert!(rlib(&ios, "ffi", 22).exists());
        assert!(!rlib(&ios, "ffi", 23).exists());
        // foo 4+5 (112 each), bar 9+10 (40 each), baz 14 (15), qux ccc (40),
        // ffi 23 (112).
        assert_eq!(report.bytes_freed, 2 * 112 + 2 * 40 + 15 + 40 + 112);
        assert_eq!(report.units_removed, 7);
        assert!(report.skipped_locked.is_empty());
        // Nothing stale left: a second sweep frees nothing.
        assert_eq!(sweep(&target, &[]).bytes_freed, 0);
    }

    #[test]
    fn a_dry_run_reports_and_deletes_nothing() {
        let tmp = temp_dir("dry");
        let target = seed(&tmp.0);
        let report = sweep_with(&target, &Options::new(true), &[]);
        assert!(report.dry_run);
        assert_eq!(report.bytes_freed, 2 * 112 + 2 * 40 + 15 + 40 + 112);
        assert!(rlib(&target.join("debug"), "foo", 5).exists());
        assert!(report.summary().contains("would free"), "{}", report.summary());
    }

    #[test]
    fn a_unit_a_process_holds_open_stays() {
        let tmp = temp_dir("open");
        let target = seed(&tmp.0);
        let debug = target.join("debug");
        let held = rlib(&debug, "foo", 5);
        let open = [held.clone(), debug.join(format!("build/bar/{}/run/stdout", hash(10)))];
        let report = sweep(&target, &open);
        assert!(held.exists());
        assert!(debug.join(format!(".fingerprint/foo-{}", hash(5))).exists());
        assert!(debug.join(format!("build/bar/{}", hash(10))).exists());
        assert!(!rlib(&debug, "foo", 4).exists());
        assert_eq!(report.kept_open, 2);
    }

    #[test]
    fn a_profile_a_build_holds_is_skipped() {
        let tmp = temp_dir("locked");
        let target = seed(&tmp.0);
        let debug = target.join("debug");
        // Any one of cargo's three locks is enough.
        let build = File::open(debug.join(".cargo-artifact-lock")).unwrap();
        build.lock().unwrap();
        let report = sweep(&target, &[]);
        assert_eq!(report.skipped_locked, vec![debug.clone()]);
        assert!(rlib(&debug, "foo", 5).exists());
        // The other profile still goes.
        assert!(!rlib(&target.join("aarch64-apple-ios/release"), "ffi", 23).exists());
        assert_eq!(report.bytes_freed, 112);
    }

    #[test]
    fn not_a_cargo_target_is_untouched() {
        let tmp = temp_dir("untagged");
        let target = seed(&tmp.0);
        fs::remove_file(target.join("CACHEDIR.TAG")).unwrap();
        assert_eq!(sweep(&target, &[]).bytes_freed, 0);
        assert!(rlib(&target.join("debug"), "foo", 5).exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed() {
        let tmp = temp_dir("symlink");
        let target = seed(&tmp.0);
        let outside = tmp.0.join("outside/keep.txt");
        write(&outside, 10);
        let link = target.join("debug/incremental/qux-ccc/link");
        std::os::unix::fs::symlink(outside.parent().unwrap(), &link).unwrap();
        // The new link is fresh: no age floor and no newest-N here.
        let options = Options { keep: 0, min_age: Duration::ZERO, ..Options::new(false) };
        sweep_with(&target, &options, &[]);
        assert!(outside.exists());
        assert!(!target.join("debug/incremental/qux-ccc").exists());
    }

    #[test]
    fn hashes_split_off_the_last_sixteen_hex_digits() {
        let h = "0123456789abcdef";
        assert_eq!(split_hash(&format!("libfoo-{h}.rlib")), Some(("libfoo", h, ".rlib")));
        assert_eq!(split_hash(&format!("foo-bar-{h}")), Some(("foo-bar", h, "")));
        assert_eq!(
            split_hash(&format!("etalk_ffi.etalk_ffi.{h}-cgu.0.rcgu.o")),
            None,
            "a codegen leftover is no unit"
        );
        assert_eq!(split_hash(&format!("foo-{h}.foo.{h}-cgu.0.rcgu.o")), Some(("foo", h, &*format!(".foo.{h}-cgu.0.rcgu.o"))));
        assert_eq!(split_hash("libfoo.rlib"), None);
        assert_eq!(split_hash(&format!("-{h}")), None);
        assert_eq!(split_hash("foo-0123456789ABCDEF"), None);
    }

    #[test]
    fn config_reads_settings_json_and_defaults() {
        let tmp = temp_dir("config");
        assert_eq!(Config::load(&tmp.0), Config::default());
        assert_eq!(Config::default().threshold_bytes(), 20_000_000_000);
        fs::write(tmp.0.join("settings.json"), r#"{"cargoSweep":{"freeDiskGb":50,"dryRun":true}}"#).unwrap();
        assert_eq!(Config::load(&tmp.0), Config { enabled: true, free_disk_gb: 50, dry_run: true });
        fs::write(tmp.0.join("settings.json"), r#"{"cargoSweep":{"enabled":false}}"#).unwrap();
        assert!(!Config::load(&tmp.0).enabled);
        fs::write(tmp.0.join("settings.json"), r#"{"cargoSweep":"nonsense"}"#).unwrap();
        assert_eq!(Config::load(&tmp.0), Config::default());
    }

    #[test]
    fn the_trigger_needs_low_disk_and_the_switch() {
        let tmp = temp_dir("trigger");
        let clone = tmp.0.join("acme/web");
        fs::create_dir_all(&clone).unwrap();
        let target = seed(&tmp.0.join("acme"));
        fs::rename(&target, tmp.0.join("acme/web.cargo-target")).unwrap();
        // A threshold of 0 GB is never low: nothing walked.
        fs::write(tmp.0.join("settings.json"), r#"{"cargoSweep":{"freeDiskGb":0}}"#).unwrap();
        assert_eq!(sweep_if_due(&clone, &tmp.0), None);
        // Off: nothing walked, however low.
        fs::write(tmp.0.join("settings.json"), r#"{"cargoSweep":{"enabled":false,"freeDiskGb":100000000}}"#).unwrap();
        assert_eq!(sweep_if_due(&clone, &tmp.0), None);
        // Low (any disk is below 100 PB): swept once, then throttled.
        fs::write(tmp.0.join("settings.json"), r#"{"cargoSweep":{"freeDiskGb":100000000}}"#).unwrap();
        let report = sweep_if_due(&clone, &tmp.0).expect("due");
        assert_eq!(report.bytes_freed, 2 * 112 + 2 * 40 + 15 + 40 + 112);
        let record = last_sweep(&tmp.0).expect("recorded");
        assert_eq!(record.freed_bytes, report.bytes_freed);
        assert!(!record.dry_run);
        assert_eq!(sweep_if_due(&clone, &tmp.0), None, "throttled");
        // No shared target beside a clone: nothing.
        let bare = tmp.0.join("acme/api");
        fs::create_dir_all(&bare).unwrap();
        assert_eq!(sweep_if_due(&bare, &tmp.0), None);
    }

    /// #EXP-1892: every sweep moves `checkedAt`; only a freeing one moves
    /// what it freed, and nothing is recorded before the first.
    #[test]
    fn every_sweep_moves_the_check_time_and_only_a_freeing_one_the_record() {
        let tmp = temp_dir("checked");
        let at = |secs: u64| SystemTime::UNIX_EPOCH + Duration::from_secs(secs);
        let empty = SweepReport { target: "/t".into(), ..SweepReport::default() };
        record(&tmp.0, &empty, at(100));
        assert_eq!(last_sweep(&tmp.0), None, "no record before the first freeing sweep");
        let freed = SweepReport { bytes_freed: 500, ..empty.clone() };
        record(&tmp.0, &freed, at(200));
        record(&tmp.0, &empty, at(300));
        let last = last_sweep(&tmp.0).unwrap();
        assert_eq!((last.at, last.freed_bytes, last.checked_at), (200, 500, Some(300)));
        // A record from before #EXP-1892 reads without a check time.
        fs::write(tmp.0.join(RECORD_FILE), r#"{"at":1,"freedBytes":2,"dryRun":false,"target":"/t"}"#).unwrap();
        assert_eq!(last_sweep(&tmp.0).unwrap().checked_at, None);
    }

    #[test]
    fn throttle_walks_each_target_once_per_interval() {
        let mut throttle = SweepThrottle::default();
        let start = Instant::now();
        let a = Path::new("/repos/acme/web.cargo-target");
        let b = Path::new("/repos/acme/api.cargo-target");
        assert!(throttle.due(a, start));
        assert!(!throttle.due(a, start + INTERVAL - Duration::from_secs(1)));
        assert!(throttle.due(b, start + Duration::from_secs(60)));
        assert!(throttle.due(a, start + INTERVAL));
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn the_probe_sees_a_file_this_process_holds() {
        let tmp = temp_dir("probe");
        let path = tmp.0.join("held.bin");
        write(&path, 1);
        let _held = File::open(&path).unwrap();
        let open = open_files_under(&tmp.0).expect("probe");
        assert_eq!(open, vec![path]);
    }

    /// A REAL build started mid-sweep waits for the sweep's locks, then
    /// builds and runs fine; a build already holding the lock makes the
    /// sweep skip the profile.
    #[test]
    fn a_build_that_starts_mid_sweep_waits_and_succeeds() {
        use std::process::{Command, Stdio};
        let tmp = temp_dir("cargo");
        let krate = tmp.0.join("sweepme");
        write(&krate.join("src/main.rs"), 0);
        fs::write(
            krate.join("Cargo.toml"),
            "[package]\nname = \"sweepme\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
        )
        .unwrap();
        fs::write(krate.join("src/main.rs"), "fn main() { println!(\"one\"); }\n").unwrap();
        let target = tmp.0.join("target");
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let build = || {
            let mut command = Command::new(&cargo);
            command
                .args(["build", "--offline", "--manifest-path"])
                .arg(krate.join("Cargo.toml"))
                .env("CARGO_TARGET_DIR", &target)
                .env_remove("RUSTC_WRAPPER")
                .stdout(Stdio::null())
                .stderr(Stdio::piped());
            command
        };
        assert!(build().status().unwrap().success());
        let debug = target.join("debug");
        // Stale copies of another crate the sweep removes.
        for (n, half_hours) in [(1, 10), (2, 12), (3, 14)] {
            lib(&debug, "junk", &hash(n), half_hours);
        }
        // The next build has to compile: new source.
        fs::write(krate.join("src/main.rs"), "fn main() { println!(\"two\"); }\n").unwrap();
        let mut started = None;
        let report = sweep_inner(&target, &Options::new(false), &[], &mut |profile| {
            assert_eq!(profile, debug.as_path());
            let mut child = build().spawn().unwrap();
            std::thread::sleep(Duration::from_secs(3));
            assert!(child.try_wait().unwrap().is_none(), "the build waits for the sweep's lock");
            started = Some(child);
        });
        assert_eq!(report.units_removed, 1);
        assert!(!rlib(&debug, "junk", 3).exists());
        let output = started.expect("the build started under the lock").wait_with_output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{stderr}");
        assert!(stderr.contains("Blocking"), "{stderr}");
        let ran = Command::new(debug.join("sweepme")).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&ran.stdout), "two\n");

        // And the other way round: a build holding the lock wins.
        lib(&debug, "junk", &hash(4), 16);
        let held = File::open(debug.join(".cargo-lock")).unwrap();
        held.lock().unwrap();
        let report = sweep(&target, &[]);
        assert_eq!(report.skipped_locked, vec![debug.clone()]);
        assert!(rlib(&debug, "junk", 4).exists());
    }

    /// Manual run against a COPY of a real target:
    /// `EXP_GC_TARGET=/copy [EXP_GC_DRY=1] [EXP_GC_HOLD=<secs>] cargo test -p coding gc_live -- --ignored --nocapture`.
    /// `EXP_GC_HOLD` keeps each profile's locks that long before deleting,
    /// so a build started meanwhile provably lands mid-sweep.
    #[test]
    #[ignore]
    fn gc_live() {
        let target = fs::canonicalize(std::env::var("EXP_GC_TARGET").unwrap()).unwrap();
        let dry_run = std::env::var("EXP_GC_DRY").is_ok();
        let hold = std::env::var("EXP_GC_HOLD").ok().and_then(|secs| secs.parse().ok()).map(Duration::from_secs);
        let started = Instant::now();
        let open = open_files_under(&target).expect("probe");
        println!("open under target: {}", open.len());
        let report = sweep_inner(&target, &Options::new(dry_run), &open, &mut |profile| {
            if let Some(hold) = hold {
                println!("{:?} holding {}", started.elapsed(), profile.display());
                std::thread::sleep(hold);
            }
        });
        println!("{}", report.summary());
        println!("took {:?}, after: {} bytes", started.elapsed(), tree_size(&target));
    }
}
