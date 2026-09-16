//! EXP-852 — a `proper-lockfile`-compatible mkdir lock.
//!
//! The claude CLI (2.1.272) guards its OAuth refresh — and its credential
//! store writes — with the npm [`proper-lockfile`] library, which locks a
//! *directory* next to the file it protects (`.oauth_refresh.lock`,
//! `.storage-write.lock`). When we refresh a token for an account the CLI is
//! also using, two writers race over the same `credentials.json`: the loser
//! silently overwrites a refresh the other just landed and the account drops
//! to `needs_relogin`. There is no protocol to negotiate here — the CLI is
//! the incumbent and will not learn ours — so this module reimplements
//! `proper-lockfile`'s contract exactly, byte for byte where it is observable
//! on disk (the lock is a directory, the liveness signal is its mtime).
//!
//! ## The contract we mirror
//!
//! * **Acquire** = `mkdir(lock_path)`. `EEXIST` means someone holds it:
//!   `stat` the directory and compare its mtime against `now - stale`.
//!   * mtime is at or after the threshold ⇒ the holder is alive ⇒
//!     [`LockError::Contended`].
//!   * mtime is older ⇒ the holder is presumed dead ⇒ `rmdir` it and retry
//!     ONCE **with staleness disabled**. That single non-stale retry is what
//!     makes two reclaimers safe: one wins the `mkdir`, the other sees
//!     `EEXIST` again and, with staleness off, reports contention instead of
//!     ripping out the fresh lock it just lost.
//!   * `stat` fails with `ENOENT` (the holder released between our `mkdir`
//!     and our `stat`) ⇒ the same single retry with staleness disabled.
//! * **While held** a heartbeat every `update` rewrites the directory's mtime
//!   to "now". Before each write it stats the directory, and the lock is
//!   COMPROMISED (`proper-lockfile`'s `ECOMPROMISED`, the CLI's
//!   `isCompromised()`) if the directory is gone, if its mtime is not the one
//!   we last wrote (a stranger reclaimed it — whatever we are protecting is
//!   no longer ours to write), or if the write itself fails. A compromised
//!   guard stops its heartbeat and answers [`LockGuard::is_compromised`];
//!   callers must abandon the critical section rather than finish it.
//! * **Release** = `rmdir(lock_path)`.
//! * **Contention backoff**: `attempts` tries in total, sleeping
//!   `backoff + rand(0..jitter)` between them (the CLI's OAuth lock: 5
//!   attempts, 1000–2000 ms).
//!
//! Verified against the `proper-lockfile` bundled in claude 2.1.272 (its
//! `acquireLock`/`updateLock`, and the call sites below). Two deliberate
//! simplifications:
//!
//! * The CLI always passes `realpath: false`, so the lock path is taken
//!   verbatim — and so does [`acquire`]: callers pass the FINAL directory
//!   (`<dir>/.oauth_refresh.lock`), we never append `.lock` nor resolve
//!   symlinks.
//! * `proper-lockfile` treats a TRANSIENT (non-`ENOENT`) stat or mtime-write
//!   failure as retryable — it re-arms at 1s and only compromises once
//!   `lastUpdate + stale` has passed. We compromise immediately. Strictly
//!   safer (we abandon the critical section instead of writing while unsure
//!   we still hold it), and against a local `~/.claude` a transient failure
//!   here means something is already very wrong.
//!
//! ## Why the mtime is always a whole second
//!
//! `proper-lockfile` probes the backing store's mtime precision and rounds
//! the value it writes so that what it writes is what a later `stat` reads
//! back — its own "did a stranger touch this" comparison is an equality test,
//! so a value the filesystem cannot represent exactly would compromise the
//! lock on the very next tick. A whole second is the one value that
//! round-trips identically on every store we can land on (APFS/ext4
//! nanoseconds, HFS+/FAT seconds), and it compares exactly in BOTH
//! directions — ours against the CLI's and the CLI's against ours. The mtime
//! never moves backwards either ([`whole_second`] truncates, so the stamp can
//! sit up to a second behind the wall clock): every write is
//! `max(what we last wrote, now)`.
//!
//! The equality test is only ever against one's OWN writes, so the precision
//! choice is private to each side: on a millisecond-precision store the CLI
//! writes millisecond stamps and we write whole seconds, and neither notices.
//! The one thing the other side reads is our age, and truncating makes our
//! lock look at most a second older than it is — against a 15s or 60s stale
//! window, nothing.
//!
//! No async, no `rand`, no new crates — one `std::thread` per held lock,
//! sleeping in short slices so a release stops it promptly.
//!
//! [`proper-lockfile`]: https://github.com/moxystudio/node-proper-lockfile

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The longest a heartbeat thread sleeps before it re-checks the release
/// flag. Keeps `release`/`Drop` from leaving a thread running (and a doomed
/// mtime write pending) for a whole `update` period.
const SLEEP_SLICE: Duration = Duration::from_millis(50);

/// The knobs `proper-lockfile` exposes, with the same meanings.
#[derive(Clone, Copy, Debug)]
pub struct LockOptions {
    /// A lock dir whose mtime is older than this is reclaimable (proper-lockfile `stale`).
    pub stale: Duration,
    /// Heartbeat period (proper-lockfile `update`).
    pub update: Duration,
    /// Total mkdir attempts INCLUDING the first. 1 = fail fast with `Contended`.
    pub attempts: u32,
    /// Base sleep between attempts; actual sleep = backoff + rand(0..jitter).
    pub backoff: Duration,
    /// Upper bound of the random addition to `backoff`; zero = no jitter.
    pub jitter: Duration,
}

impl LockOptions {
    /// The CLI's `.oauth_refresh.lock`: stale 60s, update 5s, 5 attempts, 1000-2000ms.
    ///
    /// Byte-for-byte the CLI's own call: `{realpath:!1, stale:60000,
    /// update:5000}` retried `while (attempt < 5)` with
    /// `sleep(1000 + Math.random()*1000)`. Its MCP refresh locks
    /// (`mcp-refresh-<server>.lock`) use the identical shape.
    pub const fn oauth_refresh() -> Self {
        Self {
            stale: Duration::from_secs(60),
            update: Duration::from_secs(5),
            attempts: 5,
            backoff: Duration::from_millis(1_000),
            jitter: Duration::from_millis(1_000),
        }
    }

    /// The CLI's `.storage-write.lock`: stale 15s, update 5s, 11 attempts, 100-1000ms.
    ///
    /// The CLI's call is `{realpath:!1, retries:{retries:10, minTimeout:100,
    /// maxTimeout:1000}, stale:15000}` — 10 retries after the first try (11
    /// attempts) with the `retry` package's exponential 100→1000ms, which we
    /// approximate inside the same envelope. It passes no `update`, so
    /// proper-lockfile defaults to `stale/2` = 7.5s; we heartbeat at 5s
    /// instead, which is only ever safer (the mtime moves more often, well
    /// inside the same 15s staleness window).
    pub const fn storage_write() -> Self {
        Self {
            stale: Duration::from_secs(15),
            update: Duration::from_secs(5),
            attempts: 11,
            backoff: Duration::from_millis(100),
            jitter: Duration::from_millis(900),
        }
    }
}

/// Why an [`acquire`] did not hand back a guard.
#[derive(Debug)]
pub enum LockError {
    /// Someone else holds a live lock and the retry budget ran out
    /// (`proper-lockfile`'s `ELOCKED`). Never a reason to write anyway.
    Contended,
    /// The lock directory could not be created, stat'd or removed — a missing
    /// parent directory, a permission problem, a read-only store.
    Io(io::Error),
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Contended => write!(f, "lock is held by another process"),
            Self::Io(err) => write!(f, "lock i/o failed: {err}"),
        }
    }
}

impl std::error::Error for LockError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Contended => None,
            Self::Io(err) => Some(err),
        }
    }
}

/// Shared between the guard and its heartbeat thread. Everything is atomic so
/// the thread never needs the guard alive to make progress, and the guard
/// never blocks on the thread.
#[derive(Debug)]
struct LockState {
    /// The lock stopped being ours: removed, taken over, or unwritable.
    compromised: AtomicBool,
    /// `release`/`Drop` ran; the heartbeat must stop and stay quiet.
    released: AtomicBool,
    /// Seconds-since-epoch of the mtime we last wrote — the value a stat must
    /// read back if the lock is still ours.
    mtime_secs: AtomicU64,
}

/// A held lock. Dropping it releases; losing it (see
/// [`LockGuard::is_compromised`]) means the critical section must be
/// abandoned, not finished.
#[derive(Debug)]
pub struct LockGuard {
    path: PathBuf,
    state: Arc<LockState>,
}

impl LockGuard {
    /// The lock directory this guard owns.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// proper-lockfile's ECOMPROMISED / the CLI's `isCompromised()`.
    ///
    /// Check it before every write you were holding the lock for: a `true`
    /// here means another process is inside the same critical section.
    pub fn is_compromised(&self) -> bool {
        self.state.compromised.load(Ordering::SeqCst)
    }

    /// Explicit rmdir; Drop does the same best-effort.
    ///
    /// A COMPROMISED lock is not removed: the directory on disk is either
    /// already gone or belongs to whoever reclaimed it, and deleting theirs
    /// would hand a third writer the lock they are holding.
    pub fn release(self) -> io::Result<()> {
        self.shutdown()
    }

    /// The one release path: flip `released` (stopping the heartbeat within a
    /// slice) and rmdir once. Idempotent, so `release()` then `Drop` is a
    /// single removal.
    fn shutdown(&self) -> io::Result<()> {
        if self.state.released.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        if self.state.compromised.load(Ordering::SeqCst) {
            log::debug!(
                "lockfile: not removing compromised lock {} (it is no longer ours)",
                self.path.display()
            );
            return Ok(());
        }
        log::debug!("lockfile: releasing {}", self.path.display());
        std::fs::remove_dir(&self.path)
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        // Never panic on the way out: a lock whose directory a stranger
        // already removed is a logged curiosity, not a crash.
        if let Err(err) = self.shutdown() {
            log::debug!(
                "lockfile: release of {} failed: {err}",
                self.path.display()
            );
        }
    }
}

/// mkdir the lock dir per the module contract; spawns the heartbeat thread on
/// success.
///
/// The parent directory must exist — this creates exactly one level, like
/// `proper-lockfile`'s `mkdir`, so a missing parent surfaces as
/// [`LockError::Io`] rather than silently locking a path nobody else will
/// ever collide on.
pub fn acquire(path: &Path, options: LockOptions) -> Result<LockGuard, LockError> {
    let attempts = options.attempts.max(1);
    let mut attempt = 0_u32;
    loop {
        attempt += 1;
        match try_acquire(path, Some(options.stale)) {
            Ok(stamp) => {
                log::debug!(
                    "lockfile: acquired {} on attempt {attempt}/{attempts}",
                    path.display()
                );
                return Ok(spawn_guard(path.to_path_buf(), stamp, options));
            }
            Err(LockError::Contended) if attempt < attempts => {
                let wait = options.backoff + jitter(options.jitter);
                log::debug!(
                    "lockfile: {} is held, retrying in {}ms ({attempt}/{attempts})",
                    path.display(),
                    wait.as_millis()
                );
                std::thread::sleep(wait);
            }
            Err(err) => return Err(err),
        }
    }
}

/// One mkdir. `stale = None` is the "retry with staleness disabled" mode: an
/// `EEXIST` is contention, full stop, and no further recursion happens.
fn try_acquire(path: &Path, stale: Option<Duration>) -> Result<SystemTime, LockError> {
    match std::fs::create_dir(path) {
        Ok(()) => stamp_fresh(path),
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
            let Some(stale) = stale else {
                return Err(LockError::Contended);
            };
            match dir_mtime(path) {
                // Released between our mkdir and our stat — the one retry.
                Err(err) if err.kind() == io::ErrorKind::NotFound => try_acquire(path, None),
                Err(err) => Err(LockError::Io(err)),
                Ok(mtime) if is_live(mtime, stale) => Err(LockError::Contended),
                Ok(mtime) => {
                    log::debug!(
                        "lockfile: reclaiming {} (mtime {}s old, stale after {}s)",
                        path.display(),
                        age_secs(mtime),
                        stale.as_secs()
                    );
                    match std::fs::remove_dir(path) {
                        Ok(()) => try_acquire(path, None),
                        // Another reclaimer beat us to the removal; the
                        // non-stale retry decides who owns it now.
                        Err(err) if err.kind() == io::ErrorKind::NotFound => {
                            try_acquire(path, None)
                        }
                        Err(err) => Err(LockError::Io(err)),
                    }
                }
            }
        }
        Err(err) => Err(LockError::Io(err)),
    }
}

/// Stamp a freshly created lock dir with a whole-second "now" and report the
/// stamp, so the heartbeat knows what a stat must read back. `mkdir` leaves
/// whatever sub-second mtime the store fancies, which would never compare
/// equal on the next tick.
fn stamp_fresh(path: &Path) -> Result<SystemTime, LockError> {
    let stamp = whole_second(SystemTime::now());
    if let Err(err) = set_dir_mtime(path, stamp) {
        // We hold a lock we cannot keep alive: better to give it back than to
        // let it look stale to everyone (including us) for the next `stale`.
        let _ = std::fs::remove_dir(path);
        return Err(LockError::Io(err));
    }
    Ok(stamp)
}

/// Build the guard and start its heartbeat.
fn spawn_guard(path: PathBuf, stamp: SystemTime, options: LockOptions) -> LockGuard {
    let state = Arc::new(LockState {
        compromised: AtomicBool::new(false),
        released: AtomicBool::new(false),
        mtime_secs: AtomicU64::new(epoch_secs(stamp)),
    });
    let worker_state = Arc::clone(&state);
    let worker_path = path.clone();
    // One thread per held lock. Locks are short-lived (a token refresh), and
    // this crate has no runtime to schedule a timer on.
    std::thread::Builder::new()
        .name("exp-lockfile".to_string())
        .spawn(move || heartbeat(worker_path, worker_state, options.update))
        .map_err(|err| log::warn!("lockfile: heartbeat thread failed to start: {err}"))
        .ok();
    LockGuard { path, state }
}

/// Keep the lock's mtime moving so nobody reclaims it, and notice the moment
/// it stops being ours.
fn heartbeat(path: PathBuf, state: Arc<LockState>, update: Duration) {
    loop {
        if !sleep_unless_released(&state, update) {
            return;
        }
        match dir_mtime(&path) {
            Err(err) => {
                // ENOENT: someone removed it. Anything else: we can no longer
                // prove it is ours. proper-lockfile compromises on both.
                return compromise(&state, &path, format_args!("stat failed: {err}"));
            }
            Ok(found) => {
                let recorded = state.mtime_secs.load(Ordering::SeqCst);
                if epoch_secs(found) != recorded {
                    return compromise(
                        &state,
                        &path,
                        format_args!(
                            "mtime moved to {}s, we wrote {recorded}s",
                            epoch_secs(found)
                        ),
                    );
                }
                // Never backwards: a truncated "now" can sit behind a stamp we
                // already wrote (and a stranger's future mtime we must not
                // rewind past, though that already compromised us above).
                let next = whole_second(SystemTime::now()).max(recorded_time(recorded));
                if let Err(err) = set_dir_mtime(&path, next) {
                    return compromise(&state, &path, format_args!("mtime write failed: {err}"));
                }
                state.mtime_secs.store(epoch_secs(next), Ordering::SeqCst);
            }
        }
    }
}

/// Flag the lock as lost and stop the heartbeat. Silent when the guard was
/// already released — a lock removed by its own `release` is not a surprise.
fn compromise(state: &LockState, path: &Path, why: std::fmt::Arguments<'_>) {
    if state.released.load(Ordering::SeqCst) {
        return;
    }
    state.compromised.store(true, Ordering::SeqCst);
    log::warn!("lockfile: {} is compromised ({why})", path.display());
}

/// Sleep `total` in [`SLEEP_SLICE`] slices. `false` = the guard was released
/// and the heartbeat must stop.
fn sleep_unless_released(state: &LockState, total: Duration) -> bool {
    let deadline = std::time::Instant::now() + total;
    loop {
        if state.released.load(Ordering::SeqCst) {
            return false;
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return true;
        }
        std::thread::sleep(remaining.min(SLEEP_SLICE));
    }
}

/// `mtime >= now - stale`, i.e. the holder is presumed alive. A future mtime
/// (a clock skew, or a stamp from a machine sharing the store) counts as
/// live: we would rather wait than steal.
fn is_live(mtime: SystemTime, stale: Duration) -> bool {
    match SystemTime::now().checked_sub(stale) {
        Some(threshold) => mtime >= threshold,
        // `now - stale` underflowed the epoch: nothing can be stale yet.
        None => true,
    }
}

/// Age of an mtime in seconds, for logs only (0 for a future stamp).
fn age_secs(mtime: SystemTime) -> u64 {
    SystemTime::now()
        .duration_since(mtime)
        .map(|age| age.as_secs())
        .unwrap_or(0)
}

/// Seconds since the epoch; pre-epoch stamps clamp to 0 (they can only come
/// from a badly skewed clock, and 0 makes them look infinitely stale, which
/// is the safe reading).
fn epoch_secs(at: SystemTime) -> u64 {
    at.duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

/// The inverse of [`epoch_secs`], for the never-backwards comparison.
fn recorded_time(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

/// A cheap, dependency-free jitter in `0..max`. Nobody is attacking this — it
/// only has to keep two daemons that woke on the same tick from retrying in
/// lockstep — so the wall clock's nanoseconds plus a per-call counter, run
/// through splitmix64's finaliser, is plenty.
fn jitter(max: Duration) -> Duration {
    let span = max.as_nanos() as u64;
    if span == 0 {
        return Duration::ZERO;
    }
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos() as u64)
        .unwrap_or(0);
    let mut x = nanos ^ COUNTER.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    Duration::from_nanos(x % span)
}

/// The lock directory's modification time — the ONLY liveness signal
/// `proper-lockfile` publishes.
pub(crate) fn dir_mtime(path: &Path) -> io::Result<SystemTime> {
    std::fs::metadata(path)?.modified()
}

/// Set a DIRECTORY's mtime.
///
/// Unix: a read-only handle on the directory is enough — `futimens` on the fd
/// only needs ownership, which we have (we made it).
#[cfg(unix)]
pub(crate) fn set_dir_mtime(path: &Path, when: SystemTime) -> io::Result<()> {
    std::fs::File::open(path)?.set_modified(when)
}

/// Set a DIRECTORY's mtime.
///
/// Windows: `File::open` on a directory is `ERROR_ACCESS_DENIED` unless
/// `FILE_FLAG_BACKUP_SEMANTICS` is set, and `set_modified` needs write
/// access on the handle. UNVERIFIED — EXP-852 was developed on macOS and no
/// Windows machine ran this path; if the desktop's Windows build reports
/// access denied here, the narrower `access_mode(FILE_WRITE_ATTRIBUTES)` is
/// the next thing to try.
#[cfg(windows)]
pub(crate) fn set_dir_mtime(path: &Path, when: SystemTime) -> io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?
        .set_modified(when)
}

/// Truncate to a whole second — see the module doc on why every mtime we
/// write is one.
pub(crate) fn whole_second(at: SystemTime) -> SystemTime {
    match at.duration_since(UNIX_EPOCH) {
        Ok(since) => UNIX_EPOCH + Duration::from_secs(since.as_secs()),
        // Pre-epoch: a clock that broken has bigger problems than our lock.
        Err(_) => at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "exp-lockfile-{tag}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Fast options for the tests that are not about timing.
    fn options() -> LockOptions {
        LockOptions {
            stale: Duration::from_secs(60),
            update: Duration::from_millis(50),
            attempts: 1,
            backoff: Duration::from_millis(1),
            jitter: Duration::ZERO,
        }
    }

    /// Poll `check` until it holds or the deadline passes; keeps the timing
    /// tests honest on a loaded CI box without a long fixed sleep.
    fn wait_for(limit: Duration, mut check: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + limit;
        loop {
            if check() {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn subsec_nanos(at: SystemTime) -> u32 {
        at.duration_since(UNIX_EPOCH).unwrap().subsec_nanos()
    }

    #[test]
    fn acquire_makes_the_lock_directory_and_release_removes_it() {
        let dir = temp_dir("basic");
        let lock = dir.join("target.lock");

        let guard = acquire(&lock, options()).unwrap();
        assert!(lock.is_dir(), "the lock is a directory");
        assert_eq!(guard.path(), lock.as_path());
        assert!(!guard.is_compromised());

        guard.release().unwrap();
        assert!(!lock.exists(), "release rmdirs the lock");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_fresh_foreign_lock_is_contended() {
        let dir = temp_dir("fresh-foreign");
        let lock = dir.join("target.lock");
        std::fs::create_dir(&lock).unwrap();
        set_dir_mtime(&lock, whole_second(SystemTime::now())).unwrap();

        let err = acquire(&lock, options()).unwrap_err();
        assert!(matches!(err, LockError::Contended), "{err:?}");
        assert!(lock.is_dir(), "a live holder's lock is left alone");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_stale_lock_is_reclaimed() {
        let dir = temp_dir("stale");
        let lock = dir.join("target.lock");
        let options = options();
        std::fs::create_dir(&lock).unwrap();
        let dead = SystemTime::now() - options.stale - Duration::from_secs(5);
        set_dir_mtime(&lock, whole_second(dead)).unwrap();

        let guard = acquire(&lock, options).unwrap();
        let mtime = dir_mtime(guard.path()).unwrap();
        assert!(
            is_live(mtime, Duration::from_secs(5)),
            "the reclaimed lock carries a fresh mtime"
        );
        drop(guard);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_heartbeat_keeps_the_mtime_moving() {
        let dir = temp_dir("heartbeat");
        let lock = dir.join("target.lock");

        let guard = acquire(&lock, options()).unwrap();
        let first = dir_mtime(&lock).unwrap();

        // Whole seconds: the value can only change once a second has ticked.
        assert!(
            wait_for(Duration::from_secs(5), || {
                dir_mtime(&lock)
                    .map(|now| now >= first + Duration::from_secs(1))
                    .unwrap_or(false)
            }),
            "the heartbeat advanced the mtime by a second"
        );
        assert!(!guard.is_compromised());

        drop(guard);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_lock_removed_underneath_reads_as_compromised() {
        let dir = temp_dir("removed");
        let lock = dir.join("target.lock");

        let guard = acquire(&lock, options()).unwrap();
        std::fs::remove_dir(&lock).unwrap();

        assert!(
            wait_for(Duration::from_secs(5), || guard.is_compromised()),
            "a vanished lock is compromised"
        );
        // Dropping a guard whose directory is gone must not panic.
        drop(guard);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_lock_whose_mtime_a_stranger_rewrote_reads_as_compromised() {
        let dir = temp_dir("stranger");
        let lock = dir.join("target.lock");

        let guard = acquire(&lock, options()).unwrap();
        let stranger = whole_second(SystemTime::now() + Duration::from_secs(60));
        set_dir_mtime(&lock, stranger).unwrap();

        assert!(
            wait_for(Duration::from_secs(5), || guard.is_compromised()),
            "an mtime we did not write is compromised"
        );

        drop(guard);
        // The stranger's lock survives our release.
        assert!(lock.is_dir(), "a compromised lock is not removed");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mtimes_are_written_on_a_whole_second() {
        let dir = temp_dir("whole-second");
        let lock = dir.join("target.lock");

        let guard = acquire(&lock, options()).unwrap();
        let first = dir_mtime(&lock).unwrap();
        assert_eq!(subsec_nanos(first), 0, "the acquire stamp");

        assert!(
            wait_for(Duration::from_secs(5), || {
                dir_mtime(&lock).map(|now| now > first).unwrap_or(false)
            }),
            "waited for a heartbeat write"
        );
        assert_eq!(
            subsec_nanos(dir_mtime(&lock).unwrap()),
            0,
            "the heartbeat stamp"
        );

        drop(guard);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_retry_budget_gives_up_with_contended() {
        let dir = temp_dir("retry");
        let lock = dir.join("target.lock");
        std::fs::create_dir(&lock).unwrap();
        set_dir_mtime(&lock, whole_second(SystemTime::now())).unwrap();

        let started = Instant::now();
        let err = acquire(
            &lock,
            LockOptions {
                attempts: 3,
                backoff: Duration::from_millis(5),
                jitter: Duration::from_millis(5),
                ..options()
            },
        )
        .unwrap_err();
        let elapsed = started.elapsed();

        assert!(matches!(err, LockError::Contended), "{err:?}");
        assert!(
            elapsed >= Duration::from_millis(10),
            "three attempts sleep twice: {elapsed:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn drop_releases_without_an_explicit_release() {
        let dir = temp_dir("drop");
        let lock = dir.join("target.lock");

        {
            let _guard = acquire(&lock, options()).unwrap();
            assert!(lock.is_dir());
        }
        assert!(!lock.exists(), "Drop rmdirs the lock");

        // And the freed lock is immediately re-acquirable.
        let again = acquire(&lock, options()).unwrap();
        assert!(!again.is_compromised());
        drop(again);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
