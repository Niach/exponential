//! EXP-781 — the machine-wide read-modify-write section for
//! `{data_dir}/settings.json`.
//!
//! The file is a merge-preserving store several subsystems own keys in, and
//! the desktop app and the CLI daemon share ONE data dir (REV-20). So two
//! processes can load the same object, each merge their own key and each
//! write it back by rename, the second silently dropping the first's — and
//! for the device identity that is not a lost preference but a machine that
//! minted two ids and shows up twice on the relay.
//!
//! Shape copied verbatim from [`coding::run_registry`]'s `locked`: an
//! in-process `Mutex` (threads) wrapped around an advisory `flock` on a
//! sibling `settings.json.lock` (processes). The lock file is separate on
//! purpose — `settings.json` is REPLACED by rename
//! ([`crate::atomic_file::write_atomic`]), so a lock taken on it would guard
//! an unlinked inode as soon as anyone wrote.
//!
//! Every failure degrades to the in-process mutex: a read-only or
//! lock-refusing data dir (some network mounts) must not make writing the
//! settings impossible.
//!
//! Non-unix keeps the process mutex only — the shared-data-dir deployment is
//! unix, and there is no flock without another dependency.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// One in-process mutex PER lock file. A single shared mutex self-deadlocked
/// the moment two DIFFERENT files were locked nested (a legacy migration
/// holds settings.json's section while it writes the device store). Nesting
/// is always settings.json first, then a store, never the reverse.
static LOCKS: Mutex<Option<HashMap<PathBuf, &'static Mutex<()>>>> = Mutex::new(None);

fn process_lock(lock_file: &Path) -> &'static Mutex<()> {
    let mut locks = match LOCKS.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    // Leaked on purpose: a handful of lock files per process, each living as
    // long as the process does.
    *locks
        .get_or_insert_with(HashMap::new)
        .entry(lock_file.to_path_buf())
        .or_insert_with(|| Box::leak(Box::new(Mutex::new(()))))
}

/// Held for one whole load-modify-save of `settings.json`. Released on drop,
/// in both halves.
pub struct SettingsGuard {
    _process: std::sync::MutexGuard<'static, ()>,
    #[cfg(unix)]
    file: Option<std::fs::File>,
}

#[cfg(unix)]
impl Drop for SettingsGuard {
    fn drop(&mut self) {
        use std::os::unix::io::AsRawFd;
        if let Some(file) = self.file.as_ref() {
            // SAFETY: our own open fd; LOCK_UN cannot fail meaningfully here
            // (closing the file would release it anyway).
            unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
        }
    }
}

/// Take the section, blocking until it is ours. Hold the guard across the
/// read AND the write — a lock released between them guards nothing.
pub fn locked(data_dir: &Path) -> SettingsGuard {
    locked_at(&data_dir.join("settings.json.lock"))
}

/// EXP-1102: the same section for ANY small file store that is replaced by
/// rename — `lock_file` is the sibling lock file to take (never the store
/// itself). Each lock file gets its own in-process mutex, so a store write
/// nested inside the settings section does not wait on itself.
pub fn locked_at(lock_file: &Path) -> SettingsGuard {
    let process = match process_lock(lock_file).lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    #[cfg(unix)]
    {
        let file = open_lock_file(lock_file);
        SettingsGuard {
            _process: process,
            file,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = lock_file;
        SettingsGuard { _process: process }
    }
}

/// Open `lock_file` and take it exclusively, blocking until it is ours.
/// `None` = carry on with the in-process mutex alone.
#[cfg(unix)]
fn open_lock_file(lock_file: &Path) -> Option<std::fs::File> {
    use std::os::unix::io::AsRawFd;
    if let Some(dir) = lock_file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_file)
        .ok()?;
    loop {
        // SAFETY: our own open fd.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } == 0 {
            return Some(file);
        }
        // A signal interrupted the wait — keep waiting; anything else means
        // the filesystem cannot lock, so carry on without it.
        if std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The guard is re-entrant across SEQUENTIAL takes (it is released on
    /// drop, both halves) and creates the lock file beside the settings —
    /// never on `settings.json` itself, which is replaced by rename.
    #[test]
    fn the_lock_file_is_a_sibling_and_releases_on_drop() {
        let dir = std::env::temp_dir().join(format!(
            "exp-settings-lock-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);

        drop(locked(&dir));
        assert!(
            dir.join("settings.json.lock").exists(),
            "the section locks a sibling file"
        );
        assert!(
            !dir.join("settings.json").exists(),
            "taking the lock must not create the settings themselves"
        );
        // A second take blocks forever if the first never released.
        drop(locked(&dir));

        // Nested sections on DIFFERENT files (settings, then a store) must
        // not wait on each other (EXP-1102's legacy migrations do this).
        let outer = locked(&dir);
        drop(locked_at(&dir.join("store").join(".lock")));
        drop(outer);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
