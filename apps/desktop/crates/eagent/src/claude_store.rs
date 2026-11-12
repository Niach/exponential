//! EXP-1794 — ONE conversation store for every claude login on a device.
//!
//! A login is a config dir (`{agent root}/<id>/`, `CLAUDE_CONFIG_DIR`): the
//! credential, `.claude.json`, the locks and the setup token are bound to
//! it (the macOS Keychain entry is keyed to its path), so N logins stay N
//! dirs. What a CONVERSATION needs is not: B0 on CLI 2.1.322 found three
//! entries a resume under another login has to see, and they are linked
//! into one store beside the profiles:
//!
//! ```text
//! {agent root}/store/projects        transcripts, subagents, auto-memory
//! {agent root}/store/plans           plan-mode files (`<slug>.md`)
//! {agent root}/store/file-history    /rewind checkpoints (interactive runs)
//! {agent root}/<id>/<entry>  ->  ../store/<entry>   (Windows: a junction)
//! ```
//!
//! Disk is the truth: a profile is linked when its entry is a link
//! ([`is_linked`], one `symlink_metadata`); nothing is stamped in
//! `profiles.json`. An older build reads through the link as a directory and
//! behaves as before.
//!
//! [`migrate`] moves an existing profile's entries into the store. Renames
//! only, under one machine-wide lock ([`LOCK_NAME`]), never for a profile
//! with a live claude process ([`profile_is_live`]: the CLI's own registry
//! `<id>/sessions/<pid>.json`), idempotent and resumable after a crash at
//! any step. It never opens a file outside the three entries, so a
//! credential is never read, moved or copied.
//!
//! A claude spawn on a profile waits while the lock is held
//! ([`wait_for_migration`]), so no run starts inside a migration.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use crate::lockfile::{self, LockGuard, LockOptions};

/// The entries of a claude config dir that belong to the CONVERSATION and
/// live in the store (EXP-1794 B0).
pub const STORE_ENTRIES: [&str; 3] = ["projects", "plans", "file-history"];

/// The store dir beside the profile dirs.
pub const STORE_DIR: &str = "store";

/// Where a diverged copy goes: `store/conflicts/<id>/<entry>/<path>`, `<id>`
/// being the profile whose migration found the clash (whichever copy was the
/// older). Outside `projects/`, so it never lists as a conversation.
pub const CONFLICTS_DIR: &str = "conflicts";

/// The machine-wide migration lock (a `proper-lockfile` mkdir lock).
pub const LOCK_NAME: &str = ".store.lock";

/// The index file that marks `{agent root}` as a managed profile root.
const INDEX_FILE: &str = "profiles.json";

/// Auto-memory: `projects/<project>/memory/MEMORY.md` is an INDEX of the
/// memory files beside it, so two diverged copies are united line by line
/// instead of one being parked.
const MEMORY_DIR: &str = "memory";
const MEMORY_INDEX: &str = "MEMORY.md";

/// A migration holds the lock this long after taking it before it looks at
/// any profile, so a run whose spawn passed [`wait_for_migration`] just
/// before the lock was taken has registered its pid by then.
const SETTLE: Duration = Duration::from_secs(10);

/// The longest a spawn waits for a migration. A migration is renames only;
/// past this the run starts anyway (a run is never refused over this).
const SPAWN_WAIT: Duration = Duration::from_secs(120);

fn lock_options() -> LockOptions {
    LockOptions {
        stale: Duration::from_secs(30),
        update: Duration::from_secs(5),
        attempts: 1,
        backoff: Duration::ZERO,
        jitter: Duration::ZERO,
    }
}

/// `{agent root}/store`.
pub fn store_root(agent_root: &Path) -> PathBuf {
    agent_root.join(STORE_DIR)
}

/// The store dir `entry` of a profile links to.
pub fn store_entry(agent_root: &Path, entry: &str) -> PathBuf {
    store_root(agent_root).join(entry)
}

/// Whether `profile_dir/entry` is a link (symlink or junction) rather than a
/// real directory. One `symlink_metadata`, never followed.
pub fn is_linked(profile_dir: &Path, entry: &str) -> bool {
    fs::symlink_metadata(profile_dir.join(entry)).is_ok_and(|meta| meta.file_type().is_symlink())
}

/// The STORE dir `profile_dir/entry` resolves to when it is linked: the
/// canonical answer for a profile's tree (the projects root a resume probe
/// and a hand-over look in). `None` for an unlinked profile.
pub fn linked_store_entry(profile_dir: &Path, entry: &str) -> Option<PathBuf> {
    if !is_linked(profile_dir, entry) {
        return None;
    }
    Some(store_entry(profile_dir.parent()?, entry))
}

/// Link every [`STORE_ENTRIES`] of `profile_dir` into the store beside it,
/// where the entry is ABSENT (a fresh profile). An existing link whose
/// target went missing gets its target back. A real directory is left alone:
/// moving its contents is [`migrate`]'s job ([`merge_into_store`]).
pub fn link_store(profile_dir: &Path) -> io::Result<()> {
    let agent_root = parent_of(profile_dir)?;
    for entry in STORE_ENTRIES {
        let target = store_entry(&agent_root, entry);
        fs::create_dir_all(&target)?;
        let link = profile_dir.join(entry);
        match fs::symlink_metadata(&link) {
            Err(err) if err.kind() == io::ErrorKind::NotFound => make_link(&link, entry, &target)?,
            Err(err) => return Err(err),
            Ok(meta) if meta.file_type().is_symlink() => {
                // A junction holds an ABSOLUTE target: a moved data dir
                // leaves it pointing at the old place. Re-point it.
                if !link.is_dir() {
                    remove_link(&link)?;
                    make_link(&link, entry, &target)?;
                }
            }
            Ok(_) => {}
        }
    }
    Ok(())
}

/// Remove `profile_dir`'s store links — the LINKS, never what they point at.
/// [`crate`] callers do this before deleting a profile dir, so a removed
/// login's conversations stay in the store.
pub fn unlink_store(profile_dir: &Path) -> io::Result<()> {
    for entry in STORE_ENTRIES {
        let link = profile_dir.join(entry);
        if fs::symlink_metadata(&link).is_ok_and(|meta| meta.file_type().is_symlink()) {
            remove_link(&link)?;
        }
    }
    Ok(())
}

fn parent_of(profile_dir: &Path) -> io::Result<PathBuf> {
    profile_dir
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "a profile dir has a parent"))
}

/// Unix: a RELATIVE symlink (`../store/<entry>`), so a moved data dir keeps
/// working.
#[cfg(unix)]
fn make_link(link: &Path, entry: &str, _target: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(Path::new("..").join(STORE_DIR).join(entry), link)
}

/// Windows: a directory JUNCTION (no privilege needed, unlike a symlink),
/// which only takes an absolute target. `mklink` is the one std-free way to
/// make one.
#[cfg(windows)]
fn make_link(link: &Path, _entry: &str, target: &Path) -> io::Result<()> {
    let output = crate::process::background_command("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(link)
        .arg(target)
        .output()?;
    if output.status.success() && is_linked(link.parent().unwrap_or(link), entry_name(link)) {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "mklink /J {} failed: {}",
        link.display(),
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

#[cfg(windows)]
fn entry_name(link: &Path) -> &str {
    link.file_name().and_then(|name| name.to_str()).unwrap_or_default()
}

#[cfg(unix)]
fn remove_link(link: &Path) -> io::Result<()> {
    fs::remove_file(link)
}

/// A junction (and a directory symlink) is removed with `RemoveDirectory`,
/// which deletes the reparse point and never its target.
#[cfg(windows)]
fn remove_link(link: &Path) -> io::Result<()> {
    fs::remove_dir(link).or_else(|_| fs::remove_file(link))
}

// ---------------------------------------------------------------------------
// liveness + the lock
// ---------------------------------------------------------------------------

/// Whether a claude process is live on `profile_dir`: the CLI registers
/// every running instance as `<config dir>/sessions/<pid>.json` and removes
/// it on exit. A stale file of a crashed CLI whose pid is gone does not
/// count; `is_alive` answers for the pid (conservative: unsure = alive).
pub fn profile_is_live(profile_dir: &Path, is_alive: &dyn Fn(u32) -> bool) -> bool {
    let Ok(entries) = fs::read_dir(profile_dir.join("sessions")) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        name.strip_suffix(".json")
            .and_then(|pid| pid.parse::<u32>().ok())
            .is_some_and(is_alive)
    })
}

/// Whether `config_dir` is a managed profile (its parent holds the profiles
/// index) — the only dirs a migration ever touches.
fn managed_agent_root(config_dir: &Path) -> Option<PathBuf> {
    let root = config_dir.parent()?;
    root.join(INDEX_FILE).is_file().then(|| root.to_path_buf())
}

/// Called before a claude spawn on `config_dir`: while a migration holds the
/// store lock of that profile's root, wait (polling; a stale lock of a
/// crashed migration stops counting after its 30s). Answers whether it
/// waited. Not a managed profile, or no lock = returns at once.
pub fn wait_for_migration(config_dir: &Path) -> bool {
    let Some(root) = managed_agent_root(config_dir) else {
        return false;
    };
    let lock = root.join(LOCK_NAME);
    let started = Instant::now();
    let mut waited = false;
    while lock_is_fresh(&lock) {
        if started.elapsed() > SPAWN_WAIT {
            log::warn!("claude store: a migration held {} past {SPAWN_WAIT:?}; starting anyway", lock.display());
            break;
        }
        waited = true;
        std::thread::sleep(Duration::from_millis(200));
    }
    waited
}

fn lock_is_fresh(lock: &Path) -> bool {
    lockfile::dir_mtime(lock).is_ok_and(|mtime| {
        SystemTime::now()
            .duration_since(mtime)
            .map_or(true, |age| age < lock_options().stale)
    })
}

// ---------------------------------------------------------------------------
// the migration
// ---------------------------------------------------------------------------

/// What one [`migrate`] pass did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct MigrationReport {
    /// Profiles whose every entry is now a link.
    pub linked: Vec<String>,
    /// Profiles skipped because a claude process is live on them.
    pub skipped_live: Vec<String>,
    /// Profiles left unlinked after an error (retried at the next start).
    pub failed: Vec<String>,
    /// Diverged files parked under `store/conflicts/`, store-relative.
    pub conflicts: Vec<PathBuf>,
    /// The lock was held by another host: nothing was looked at.
    pub contended: bool,
}

impl MigrationReport {
    pub fn is_noop(&self) -> bool {
        self.linked.is_empty() && self.skipped_live.is_empty() && self.failed.is_empty() && self.conflicts.is_empty()
    }
}

/// Whether any of `ids` still needs a migration (an entry that is not a
/// link). The cheap check a host runs at start: one `symlink_metadata` per
/// entry.
pub fn needs_migration(agent_root: &Path, ids: &[String]) -> bool {
    ids.iter().any(|id| {
        let dir = agent_root.join(id);
        dir.is_dir() && STORE_ENTRIES.iter().any(|entry| !is_linked(&dir, entry) || !dir.join(entry).is_dir())
    })
}

/// Move every profile of `ids` (the `profiles.json` rows; staging and
/// unknown dirs are never passed) onto the store, under the machine-wide
/// lock. `is_alive` answers for a pid ([`profile_is_live`]).
pub fn migrate(agent_root: &Path, ids: &[String], is_alive: &dyn Fn(u32) -> bool) -> MigrationReport {
    migrate_with(agent_root, ids, is_alive, SETTLE)
}

fn migrate_with(
    agent_root: &Path,
    ids: &[String],
    is_alive: &dyn Fn(u32) -> bool,
    settle: Duration,
) -> MigrationReport {
    let mut report = MigrationReport::default();
    if !needs_migration(agent_root, ids) {
        return report;
    }
    let guard = match lockfile::acquire(&agent_root.join(LOCK_NAME), lock_options()) {
        Ok(guard) => guard,
        Err(err) => {
            log::info!("claude store: migration lock busy ({err:?}); the next start retries");
            report.contended = true;
            return report;
        }
    };
    std::thread::sleep(settle);
    for id in ids {
        if guard.is_compromised() {
            log::warn!("claude store: migration lock compromised; stopping");
            break;
        }
        let profile = agent_root.join(id);
        if !profile.is_dir() {
            continue;
        }
        if profile_is_live(&profile, is_alive) {
            report.skipped_live.push(id.clone());
            continue;
        }
        match migrate_profile(agent_root, id, &mut report.conflicts) {
            Ok(true) => report.linked.push(id.clone()),
            Ok(false) => report.failed.push(id.clone()),
            Err(err) => {
                log::warn!("claude store: profile {id} stays unlinked for now: {err}");
                report.failed.push(id.clone());
            }
        }
    }
    release(guard);
    report
}

fn release(guard: LockGuard) {
    if let Err(err) = guard.release() {
        log::warn!("claude store: releasing the migration lock: {err}");
    }
}

/// One profile: merge each real entry into the store, then replace the
/// emptied dir by the link. Answers whether every entry ended up linked.
/// Takes no lock and checks no liveness: [`migrate`] does both; a sign-in's
/// freshly adopted staging dir (no run can be on it yet) calls this directly.
pub fn migrate_profile(agent_root: &Path, id: &str, conflicts: &mut Vec<PathBuf>) -> io::Result<bool> {
    let profile = agent_root.join(id);
    let mut all_linked = true;
    for entry in STORE_ENTRIES {
        let source = profile.join(entry);
        let meta = match fs::symlink_metadata(&source) {
            Ok(meta) => Some(meta),
            Err(err) if err.kind() == io::ErrorKind::NotFound => None,
            Err(err) => return Err(err),
        };
        if let Some(meta) = &meta {
            if !meta.file_type().is_symlink() && meta.is_dir() {
                merge_into_store(agent_root, id, entry, &source, conflicts)?;
                // Step 3: empty → the link; not empty → stays as it is.
                if fs::remove_dir(&source).is_err() {
                    log::warn!("claude store: {} kept entries; retrying at the next start", source.display());
                    all_linked = false;
                    continue;
                }
            } else if !meta.file_type().is_symlink() {
                // A FILE where a dir belongs: not ours to touch.
                all_linked = false;
                continue;
            }
        }
    }
    link_store(&profile)?;
    Ok(all_linked && STORE_ENTRIES.iter().all(|entry| is_linked(&profile, entry)))
}

/// Merge the REAL dir `source` (a profile's `entry`) into `store/<entry>`
/// by the B2 rules. Renames only, except for a united memory index. Public
/// for `adopt_dir` (a sign-in's staging dir that already holds an entry).
pub fn merge_into_store(
    agent_root: &Path,
    id: &str,
    entry: &str,
    source: &Path,
    conflicts: &mut Vec<PathBuf>,
) -> io::Result<()> {
    let target = store_entry(agent_root, entry);
    fs::create_dir_all(&target)?;
    let parked = store_root(agent_root).join(CONFLICTS_DIR).join(id).join(entry);
    let mut merge = Merge {
        store: store_root(agent_root),
        conflicts,
    };
    merge.dir(source, &target, &parked)
}

struct Merge<'a> {
    store: PathBuf,
    conflicts: &'a mut Vec<PathBuf>,
}

impl Merge<'_> {
    fn dir(&mut self, source: &Path, target: &Path, parked: &Path) -> io::Result<()> {
        for item in fs::read_dir(source)? {
            let item = item?;
            let name = item.file_name();
            let from = item.path();
            let to = target.join(&name);
            let park = parked.join(&name);
            let from_meta = fs::symlink_metadata(&from)?;
            let to_meta = match fs::symlink_metadata(&to) {
                Ok(meta) => meta,
                Err(err) if err.kind() == io::ErrorKind::NotFound => {
                    // 1. absent in the store: the whole thing moves.
                    fs::rename(&from, &to)?;
                    continue;
                }
                Err(err) => return Err(err),
            };
            let (from_kind, to_kind) = (from_meta.file_type(), to_meta.file_type());
            if from_kind.is_dir() && to_kind.is_dir() {
                self.dir(&from, &to, &park)?;
                // Emptied → gone; still holding something (an error above) →
                // stays, and the profile stays unlinked.
                let _ = fs::remove_dir(&from);
            } else if from_kind.is_file() && to_kind.is_file() {
                self.file(&from, &to, &park)?;
            } else {
                // A type clash (a file against a dir, a stray link): neither
                // side is ours to judge — the source is parked.
                self.park(&from, &park)?;
            }
        }
        Ok(())
    }

    fn file(&mut self, from: &Path, to: &Path, park: &Path) -> io::Result<()> {
        if is_memory_index(to) {
            return unite_lines(from, to);
        }
        match compare(from, to)? {
            Order::Same | Order::FromIsPrefix => fs::remove_file(from),
            // The store's copy is the shorter: the longer one replaces it.
            Order::ToIsPrefix => replace(from, to),
            Order::Diverged => {
                let from_newer = modified(from)? > modified(to)?;
                if from_newer {
                    // The newer stays in place: the store's older copy is
                    // parked and the source takes its place.
                    let parked_to = unique(park);
                    self.park_path(to, &parked_to)?;
                    replace(from, to)
                } else {
                    self.park(from, park)
                }
            }
        }
    }

    fn park(&mut self, from: &Path, park: &Path) -> io::Result<()> {
        let park = unique(park);
        self.park_path(from, &park)
    }

    fn park_path(&mut self, from: &Path, park: &Path) -> io::Result<()> {
        if let Some(parent) = park.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(from, park)?;
        let shown = park.strip_prefix(&self.store).unwrap_or(park).to_path_buf();
        log::warn!(
            "claude store: diverged copy kept at {} (the newer one stays in the store)",
            shown.display()
        );
        self.conflicts.push(shown);
        Ok(())
    }
}

/// `rename` over an existing file (Windows' `rename` replaces as well).
fn replace(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}

fn modified(path: &Path) -> io::Result<SystemTime> {
    fs::metadata(path)?.modified()
}

/// `path`, or `path.1`, `path.2`, … — the first name not taken, so a second
/// conflict never overwrites a first.
fn unique(path: &Path) -> PathBuf {
    if fs::symlink_metadata(path).is_err() {
        return path.to_path_buf();
    }
    (1u32..)
        .map(|n| {
            let mut name = path.file_name().unwrap_or_default().to_os_string();
            name.push(format!(".{n}"));
            path.with_file_name(name)
        })
        .find(|candidate| fs::symlink_metadata(candidate).is_err())
        .expect("an unused suffix")
}

fn is_memory_index(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == MEMORY_INDEX)
        && path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|dir| dir == MEMORY_DIR)
}

/// The memory index of two logins, united: the store's lines in order, then
/// every line of the source the store lacks. Written atomically before the
/// source goes, so a crash in between re-runs to "identical → drop".
fn unite_lines(from: &Path, to: &Path) -> io::Result<()> {
    let ours = fs::read_to_string(to)?;
    let theirs = fs::read_to_string(from)?;
    let known: std::collections::HashSet<&str> = ours.lines().collect();
    let missing: Vec<&str> = theirs
        .lines()
        .filter(|line| !line.trim().is_empty() && !known.contains(line))
        .collect();
    if !missing.is_empty() {
        let mut united = ours.clone();
        if !united.is_empty() && !united.ends_with('\n') {
            united.push('\n');
        }
        for line in missing {
            united.push_str(line);
            united.push('\n');
        }
        crate::atomic_file::write_atomic(to, &united)?;
    }
    fs::remove_file(from)
}

#[derive(Debug, PartialEq, Eq)]
enum Order {
    Same,
    FromIsPrefix,
    ToIsPrefix,
    Diverged,
}

/// Byte comparison, streamed (a transcript runs to megabytes).
fn compare(from: &Path, to: &Path) -> io::Result<Order> {
    let mut a = io::BufReader::new(fs::File::open(from)?);
    let mut b = io::BufReader::new(fs::File::open(to)?);
    let (mut buf_a, mut buf_b) = (vec![0u8; 64 * 1024], vec![0u8; 64 * 1024]);
    loop {
        let n = read_full(&mut a, &mut buf_a)?;
        let m = read_full(&mut b, &mut buf_b)?;
        let common = n.min(m);
        if buf_a[..common] != buf_b[..common] {
            return Ok(Order::Diverged);
        }
        if n != m {
            return Ok(if n < m { Order::FromIsPrefix } else { Order::ToIsPrefix });
        }
        if n < buf_a.len() {
            return Ok(Order::Same);
        }
    }
}

fn read_full(reader: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..])? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "exp-claude-store-{tag}-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(INDEX_FILE), "{}").unwrap();
        dir
    }

    fn write(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn set_mtime(path: &Path, secs_ago: u64) {
        let file = fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(secs_ago)).unwrap();
    }

    fn dead(_: u32) -> bool {
        false
    }

    fn run(root: &Path, ids: &[&str]) -> MigrationReport {
        let ids: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
        migrate_with(root, &ids, &dead, Duration::ZERO)
    }

    const PROJECT: &str = "-home-me-repo";

    #[test]
    fn a_fresh_profile_links_every_entry_and_writes_land_in_the_store() {
        let root = temp_root("fresh");
        let profile = root.join("aaaaaaaa");
        fs::create_dir_all(&profile).unwrap();
        link_store(&profile).unwrap();
        for entry in STORE_ENTRIES {
            assert!(is_linked(&profile, entry), "{entry}");
            assert!(profile.join(entry).is_dir(), "{entry}");
        }
        write(&profile.join("projects").join(PROJECT).join("s1.jsonl"), "{}\n");
        assert!(store_entry(&root, "projects").join(PROJECT).join("s1.jsonl").is_file());
        assert_eq!(
            linked_store_entry(&profile, "projects"),
            Some(store_entry(&root, "projects"))
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn the_unix_link_is_relative_so_a_moved_data_dir_keeps_working() {
        let root = temp_root("relative");
        let profile = root.join("aaaaaaaa");
        fs::create_dir_all(&profile).unwrap();
        link_store(&profile).unwrap();
        assert_eq!(
            fs::read_link(profile.join("projects")).unwrap(),
            Path::new("../store/projects")
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// The store survives a `remove_dir_all` of a linked profile — on every
    /// OS, the Windows junction included (this test runs there too).
    #[test]
    fn removing_a_linked_profile_dir_keeps_the_store() {
        let root = temp_root("remove");
        let profile = root.join("aaaaaaaa");
        fs::create_dir_all(&profile).unwrap();
        link_store(&profile).unwrap();
        write(&profile.join("projects").join(PROJECT).join("s1.jsonl"), "kept\n");
        write(&profile.join("plans").join("slug.md"), "# plan\n");
        // Without unlinking first: the raw delete must not follow the link.
        fs::remove_dir_all(&profile).unwrap();
        assert_eq!(
            fs::read_to_string(store_entry(&root, "projects").join(PROJECT).join("s1.jsonl")).unwrap(),
            "kept\n"
        );
        assert!(store_entry(&root, "plans").join("slug.md").is_file());
        // And with it.
        let other = root.join("bbbbbbbb");
        fs::create_dir_all(&other).unwrap();
        link_store(&other).unwrap();
        unlink_store(&other).unwrap();
        assert!(!other.join("projects").exists());
        fs::remove_dir_all(&other).unwrap();
        assert!(store_entry(&root, "projects").join(PROJECT).join("s1.jsonl").is_file());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_link_whose_target_is_gone_gets_it_back() {
        let root = temp_root("repair");
        let profile = root.join("aaaaaaaa");
        fs::create_dir_all(&profile).unwrap();
        link_store(&profile).unwrap();
        fs::remove_dir_all(store_root(&root)).unwrap();
        assert!(!profile.join("projects").is_dir());
        link_store(&profile).unwrap();
        assert!(profile.join("projects").is_dir());
        assert!(is_linked(&profile, "projects"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_project_absent_from_the_store_moves_whole() {
        let root = temp_root("absent");
        let a = root.join("aaaaaaaa");
        write(&a.join("projects").join(PROJECT).join("s1.jsonl"), "one\n");
        write(&a.join("projects").join(PROJECT).join("s1").join("subagents").join("agent-x.jsonl"), "sub\n");
        write(&a.join("plans").join("slug.md"), "# plan\n");
        write(&a.join(".credentials.json"), "SECRET");
        let report = run(&root, &["aaaaaaaa"]);
        assert_eq!(report.linked, vec!["aaaaaaaa".to_string()]);
        let store = store_entry(&root, "projects").join(PROJECT);
        assert_eq!(fs::read_to_string(store.join("s1.jsonl")).unwrap(), "one\n");
        assert!(store.join("s1").join("subagents").join("agent-x.jsonl").is_file());
        assert!(store_entry(&root, "plans").join("slug.md").is_file());
        for entry in STORE_ENTRIES {
            assert!(is_linked(&a, entry), "{entry}");
        }
        // The login's own files never move.
        assert_eq!(fs::read_to_string(a.join(".credentials.json")).unwrap(), "SECRET");
        let _ = fs::remove_dir_all(&root);
    }

    /// A transcript handed over by an EXP-849 switch exists twice: each case
    /// of step 2.
    #[test]
    fn present_on_both_sides_follows_the_merge_rules() {
        let root = temp_root("both");
        let (a, b) = (root.join("aaaaaaaa"), root.join("bbbbbbbb"));
        let pa = a.join("projects").join(PROJECT);
        let pb = b.join("projects").join(PROJECT);
        write(&pa.join("same.jsonl"), "x\ny\n");
        write(&pb.join("same.jsonl"), "x\ny\n");
        write(&pa.join("short.jsonl"), "x\n");
        write(&pb.join("short.jsonl"), "x\ny\nz\n");
        write(&pa.join("long.jsonl"), "x\ny\nz\n");
        write(&pb.join("long.jsonl"), "x\n");
        write(&pa.join("fork.jsonl"), "x\nA\n");
        write(&pb.join("fork.jsonl"), "x\nB\n");
        set_mtime(&pa.join("fork.jsonl"), 100);
        write(&pa.join("fork2.jsonl"), "x\nA-newer\n");
        write(&pb.join("fork2.jsonl"), "x\nB-older\n");
        set_mtime(&pb.join("fork2.jsonl"), 100);
        write(&pb.join("only-b.jsonl"), "b\n");

        let report = run(&root, &["aaaaaaaa", "bbbbbbbb"]);
        assert_eq!(report.linked.len(), 2, "{report:?}");
        let store = store_entry(&root, "projects").join(PROJECT);
        let read = |name: &str| fs::read_to_string(store.join(name)).unwrap();
        assert_eq!(read("same.jsonl"), "x\ny\n");
        assert_eq!(read("short.jsonl"), "x\ny\nz\n");
        assert_eq!(read("long.jsonl"), "x\ny\nz\n");
        // Diverged: the newer stays in place, the other is parked.
        assert_eq!(read("fork.jsonl"), "x\nB\n");
        assert_eq!(
            fs::read_to_string(store_root(&root).join("conflicts/bbbbbbbb/projects").join(PROJECT).join("fork.jsonl"))
                .unwrap(),
            "x\nA\n"
        );
        assert_eq!(read("fork2.jsonl"), "x\nA-newer\n");
        assert_eq!(
            fs::read_to_string(store_root(&root).join("conflicts/bbbbbbbb/projects").join(PROJECT).join("fork2.jsonl"))
                .unwrap(),
            "x\nB-older\n"
        );
        assert_eq!(read("only-b.jsonl"), "b\n");
        assert_eq!(report.conflicts.len(), 2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn memory_indexes_are_united_and_memory_files_follow_the_rules() {
        let root = temp_root("memory");
        let (a, b) = (root.join("aaaaaaaa"), root.join("bbbbbbbb"));
        let ma = a.join("projects").join(PROJECT).join("memory");
        let mb = b.join("projects").join(PROJECT).join("memory");
        write(&ma.join("MEMORY.md"), "- [One](one.md) — a\n- [Two](two.md) — b\n");
        write(&mb.join("MEMORY.md"), "- [One](one.md) — a\n- [Three](three.md) — c\n");
        write(&ma.join("one.md"), "one\n");
        write(&mb.join("one.md"), "one\n");
        write(&ma.join("two.md"), "two\n");
        write(&mb.join("three.md"), "three\n");
        let report = run(&root, &["aaaaaaaa", "bbbbbbbb"]);
        assert_eq!(report.linked.len(), 2);
        assert!(report.conflicts.is_empty());
        let store = store_entry(&root, "projects").join(PROJECT).join("memory");
        assert_eq!(
            fs::read_to_string(store.join("MEMORY.md")).unwrap(),
            "- [One](one.md) — a\n- [Two](two.md) — b\n- [Three](three.md) — c\n"
        );
        for name in ["one.md", "two.md", "three.md"] {
            assert!(store.join(name).is_file(), "{name}");
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_hash_suffixed_long_project_dir_moves_by_its_own_name() {
        let root = temp_root("long");
        let a = root.join("aaaaaaaa");
        let long = format!("-home-me-{}--71h4ur", "x".repeat(190));
        write(&a.join("projects").join(&long).join("s.jsonl"), "s\n");
        run(&root, &["aaaaaaaa"]);
        assert!(store_entry(&root, "projects").join(&long).join("s.jsonl").is_file());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_crash_between_merge_and_link_finishes_on_the_next_run() {
        let root = temp_root("crash");
        let a = root.join("aaaaaaaa");
        write(&a.join("projects").join(PROJECT).join("s.jsonl"), "s\n");
        // The merge happened, the link did not: an empty real dir is left.
        fs::create_dir_all(store_entry(&root, "projects").join(PROJECT)).unwrap();
        fs::rename(
            a.join("projects").join(PROJECT).join("s.jsonl"),
            store_entry(&root, "projects").join(PROJECT).join("s.jsonl"),
        )
        .unwrap();
        assert!(!is_linked(&a, "projects"));
        let report = run(&root, &["aaaaaaaa"]);
        assert_eq!(report.linked, vec!["aaaaaaaa".to_string()]);
        assert!(a.join("projects").join(PROJECT).join("s.jsonl").is_file());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_second_run_is_a_noop() {
        let root = temp_root("noop");
        let a = root.join("aaaaaaaa");
        write(&a.join("projects").join(PROJECT).join("s.jsonl"), "s\n");
        assert!(!run(&root, &["aaaaaaaa"]).is_noop());
        assert!(!needs_migration(&root, &["aaaaaaaa".to_string()]));
        assert_eq!(run(&root, &["aaaaaaaa"]), MigrationReport::default());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_live_run_keeps_its_profile_as_it_is() {
        let root = temp_root("live");
        let (a, b) = (root.join("aaaaaaaa"), root.join("bbbbbbbb"));
        write(&a.join("projects").join(PROJECT).join("s.jsonl"), "a\n");
        write(&a.join("sessions").join("4242.json"), "{}");
        write(&b.join("projects").join(PROJECT).join("t.jsonl"), "b\n");
        write(&b.join("sessions").join("999.json"), "{}"); // a crashed CLI's leftover
        let ids = vec!["aaaaaaaa".to_string(), "bbbbbbbb".to_string()];
        let report = migrate_with(&root, &ids, &|pid| pid == 4242, Duration::ZERO);
        assert_eq!(report.skipped_live, vec!["aaaaaaaa".to_string()]);
        assert_eq!(report.linked, vec!["bbbbbbbb".to_string()]);
        assert!(!is_linked(&a, "projects"));
        assert!(a.join("projects").join(PROJECT).join("s.jsonl").is_file());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_held_lock_blocks_the_migration_and_holds_a_spawn() {
        let root = temp_root("lock");
        let a = root.join("aaaaaaaa");
        write(&a.join("projects").join(PROJECT).join("s.jsonl"), "a\n");
        let held = lockfile::acquire(&root.join(LOCK_NAME), lock_options()).unwrap();
        let report = run(&root, &["aaaaaaaa"]);
        assert!(report.contended);
        assert!(!is_linked(&a, "projects"));
        let waiter = {
            let a = a.clone();
            std::thread::spawn(move || wait_for_migration(&a))
        };
        std::thread::sleep(Duration::from_millis(500));
        assert!(!waiter.is_finished());
        held.release().unwrap();
        assert!(waiter.join().unwrap());
        // No lock: a spawn never waits; an unmanaged dir never waits either.
        assert!(!wait_for_migration(&a));
        assert!(!wait_for_migration(Path::new("/nonexistent/claude")));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn staging_and_unknown_dirs_are_never_touched() {
        let root = temp_root("staging");
        write(&root.join(".login-12345678").join("projects").join(PROJECT).join("s.jsonl"), "s\n");
        write(&root.join("not-a-profile").join("projects").join(PROJECT).join("s.jsonl"), "s\n");
        write(&root.join("aaaaaaaa").join("projects").join(PROJECT).join("t.jsonl"), "t\n");
        run(&root, &["aaaaaaaa"]);
        assert!(root.join(".login-12345678/projects").join(PROJECT).join("s.jsonl").is_file());
        assert!(root.join("not-a-profile/projects").join(PROJECT).join("s.jsonl").is_file());
        assert!(!store_entry(&root, "projects").join(PROJECT).join("s.jsonl").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn compare_orders_bytes_across_buffer_boundaries() {
        let root = temp_root("compare");
        let big: String = "line of a transcript\n".repeat(10_000);
        write(&root.join("a"), &big);
        write(&root.join("b"), &format!("{big}more\n"));
        assert_eq!(compare(&root.join("a"), &root.join("b")).unwrap(), Order::FromIsPrefix);
        assert_eq!(compare(&root.join("b"), &root.join("a")).unwrap(), Order::ToIsPrefix);
        assert_eq!(compare(&root.join("a"), &root.join("a")).unwrap(), Order::Same);
        write(&root.join("c"), &format!("{}X", &big[..big.len() - 1]));
        assert_eq!(compare(&root.join("a"), &root.join("c")).unwrap(), Order::Diverged);
        write(&root.join("e1"), "");
        write(&root.join("e2"), "");
        assert_eq!(compare(&root.join("e1"), &root.join("e2")).unwrap(), Order::Same);
        let _ = fs::remove_dir_all(&root);
    }
}
