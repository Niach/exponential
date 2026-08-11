//! EXP-478: per-clone launch gate — a coding launch in flight pauses the
//! landed-prune, and a prune pass in flight pauses new launches.
//!
//! The auto-prune derives its protection sets (keep/busy/held branches) on
//! the foreground, then runs seconds later on the background executor after
//! a network fetch — while [`crate::prune::prune_landed`] re-reads worktrees
//! fresh from disk. A launch that creates its 0-commits-ahead worktree inside
//! that window is in no snapshot set and would be removed mid-launch. The
//! gate closes the in-process half of that window structurally:
//!
//! * the launcher takes a [`LaunchHold`] BEFORE creating the worktree and
//!   releases it only once the session is registered (every policy derived
//!   from then on protects the branch);
//! * the prune runs its whole pass inside [`try_exclusive`], which refuses
//!   while any hold is live and blocks new holds while it runs.
//!
//! Non-reentrant: nothing inside a `try_exclusive` closure may call [`hold`]
//! on the same clone (std `Mutex` deadlocks). The gate is process-local by
//! design — the headless CLI daemon launches into the same clone paths from
//! its own process, which the prune's creation-age grace covers instead
//! ([`crate::prune`]'s `LAUNCH_GRACE`; the CLI never prunes).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

/// The process-global registry (mirrors `clone_refreshers()`'s posture).
pub fn launch_gates() -> &'static LaunchGates {
    static GATES: OnceLock<LaunchGates> = OnceLock::new();
    GATES.get_or_init(LaunchGates::default)
}

/// [`LaunchGates::hold`] on the global registry.
pub fn hold(clone: &Path) -> LaunchHold {
    launch_gates().hold(clone)
}

/// [`LaunchGates::try_exclusive`] on the global registry.
pub fn try_exclusive<R>(clone: &Path, f: impl FnOnce() -> R) -> Option<R> {
    launch_gates().try_exclusive(clone, f)
}

#[derive(Default)]
struct Gate {
    count: Mutex<usize>,
}

/// Per-clone gates. Entries are never removed — an `Arc` + a `usize` per
/// distinct clone path, bounded like `token_cache()`'s map, and removal
/// would race a concurrent [`hold`] re-inserting the same key.
#[derive(Default)]
pub struct LaunchGates {
    inner: Mutex<HashMap<PathBuf, Arc<Gate>>>,
}

/// RAII count on a clone's gate; dropping releases.
pub struct LaunchHold {
    gate: Arc<Gate>,
}

impl std::fmt::Debug for LaunchHold {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LaunchHold").finish_non_exhaustive()
    }
}

impl LaunchGates {
    /// The map lock is released before any count lock is taken — the two
    /// levels never nest, so gate contention cannot deadlock the registry.
    fn gate(&self, clone: &Path) -> Arc<Gate> {
        let key = gate_key(clone);
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(inner.entry(key).or_default())
    }

    /// Register a launch on `clone`'s gate. Blocks while a prune pass holds
    /// the gate exclusively — intended: the pass is local git only (seconds),
    /// and waiting it out means the pass can never see a half-born worktree.
    pub fn hold(&self, clone: &Path) -> LaunchHold {
        let gate = self.gate(clone);
        *gate.count.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        LaunchHold { gate }
    }

    /// Run `f` with `clone`'s gate held exclusively; `None` (`f` not run)
    /// when any launch hold is live. The guard stays live across `f`, so no
    /// launch can begin mid-run.
    pub fn try_exclusive<R>(&self, clone: &Path, f: impl FnOnce() -> R) -> Option<R> {
        let gate = self.gate(clone);
        let guard = gate.count.lock().unwrap_or_else(PoisonError::into_inner);
        if *guard > 0 {
            return None;
        }
        let result = f();
        drop(guard);
        Some(result)
    }
}

impl Drop for LaunchHold {
    fn drop(&mut self) {
        let mut count = self
            .gate
            .count
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *count = count.saturating_sub(1);
    }
}

/// Key normalization — MUST agree between the launcher and prune sides or
/// the gate silently splits. Both pass the same `clone_path(repos_root, …)`
/// textual path; canonicalizing folds a symlinked repos root on top. The one
/// residual: a first-ever launch keys on the raw fallback (the clone dir
/// does not exist yet) while a later prune canonicalizes the now-existing
/// dir — accepted, a brand-new clone has nothing prunable and the prune's
/// creation-age grace covers the remainder.
fn gate_key(clone: &Path) -> PathBuf {
    clone.canonicalize().unwrap_or_else(|_| clone.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // Each test uses its OWN registry instance — the global one would couple
    // tests running in parallel.

    fn temp_clone(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "exp-launch-gate-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn holds_are_counted_and_raii_released() {
        let gates = LaunchGates::default();
        let clone = temp_clone("refcount");
        let first = gates.hold(&clone);
        let second = gates.hold(&clone);
        assert!(gates.try_exclusive(&clone, || ()).is_none(), "two holds");
        drop(first);
        assert!(
            gates.try_exclusive(&clone, || ()).is_none(),
            "one hold left"
        );
        drop(second);
        let mut ran = false;
        assert!(gates.try_exclusive(&clone, || ran = true).is_some());
        assert!(ran, "the closure runs once the gate is free");
        let _ = std::fs::remove_dir_all(&clone);
    }

    #[test]
    fn distinct_clones_do_not_couple() {
        let gates = LaunchGates::default();
        let a = temp_clone("multi-a");
        let b = temp_clone("multi-b");
        let _hold = gates.hold(&a);
        assert!(gates.try_exclusive(&b, || ()).is_some());
        assert!(gates.try_exclusive(&a, || ()).is_none());
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    /// A launch beginning mid-pass must block until the pass ends — the pass
    /// enumerated worktrees at its start and would otherwise race a half-born
    /// one into existence behind its back.
    #[test]
    fn exclusive_section_defers_new_holds() {
        let gates = Arc::new(LaunchGates::default());
        let clone = temp_clone("defer");
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let result = gates.try_exclusive(&clone, || {
            let gates = Arc::clone(&gates);
            let clone = clone.clone();
            let handle = std::thread::spawn(move || {
                let hold = gates.hold(&clone);
                tx.send(()).unwrap();
                drop(hold);
            });
            assert!(
                rx.recv_timeout(Duration::from_millis(100)).is_err(),
                "the hold must not be granted while the pass runs"
            );
            handle
        });
        let handle = result.expect("gate was free");
        rx.recv_timeout(Duration::from_secs(5))
            .expect("the hold is granted once the pass ends");
        handle.join().unwrap();
        let _ = std::fs::remove_dir_all(&clone);
    }

    #[test]
    fn nonexistent_paths_fall_back_to_the_raw_key() {
        let gates = LaunchGates::default();
        let clone = temp_clone("raw").join("never-created");
        let _hold = gates.hold(&clone);
        assert!(gates.try_exclusive(&clone, || ()).is_none());
        if let Some(parent) = clone.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_and_real_paths_share_a_gate() {
        let gates = LaunchGates::default();
        let real = temp_clone("sym-real");
        let link = std::env::temp_dir().join(format!(
            "exp-launch-gate-sym-link-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let _hold = gates.hold(&link);
        assert!(gates.try_exclusive(&real, || ()).is_none());
        let _ = std::fs::remove_file(&link);
        let _ = std::fs::remove_dir_all(&real);
    }
}
