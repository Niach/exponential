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

use std::path::Path;
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

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
    let process = match LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    #[cfg(unix)]
    {
        let file = open_lock_file(data_dir);
        SettingsGuard {
            _process: process,
            file,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = data_dir;
        SettingsGuard { _process: process }
    }
}

/// Open `settings.json.lock` and take it exclusively, blocking until it is
/// ours. `None` = carry on with the in-process mutex alone.
#[cfg(unix)]
fn open_lock_file(data_dir: &Path) -> Option<std::fs::File> {
    use std::os::unix::io::AsRawFd;
    let _ = std::fs::create_dir_all(data_dir);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(data_dir.join("settings.json.lock"))
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

        let _ = std::fs::remove_dir_all(&dir);
    }
}
