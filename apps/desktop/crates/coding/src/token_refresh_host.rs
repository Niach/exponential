//! Headless per-clone token keep-alive (EXP-447) — the gpui-free counterpart
//! of the desktop's `ui::coding_flow::TokenRefreshers`. The CLI's sessions
//! (`exponential code`/`run` and every daemon remote start) used to install
//! ambient git auth exactly once at launch; GitHub hard-caps installation
//! tokens at 1 h, so any session outliving that stranded the agent's
//! `git push` with no recovery (the credential helper is a deliberate dumb
//! `cat` — see `git_credentials`).
//!
//! Same loop shape as the gpui refresher: [`crate::token_refresh::refresh_clone_token`]
//! → [`crate::token_refresh::next_refresh_delay`] → cancellable sleep.
//! Ref-counted per CLONE (the credential file lives in the clone's shared
//! `.git`, so one loop covers every worktree; concurrent daemon sessions on
//! one clone share it), released via the RAII [`RefresherHold`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

use api::trpc::TrpcClient;

use crate::token_refresh::{next_refresh_delay, refresh_clone_token, TOKEN_REFRESH_RETRY};

/// The process-global registry (mirrors `token_cache()`'s posture).
pub fn clone_refreshers() -> &'static CloneRefreshers {
    static REFRESHERS: OnceLock<CloneRefreshers> = OnceLock::new();
    REFRESHERS.get_or_init(CloneRefreshers::default)
}

/// Cancellable-sleep stop flag: `wait_timeout` on the pair wakes either on
/// the schedule or the moment the last hold drops.
struct Stop {
    stopped: Mutex<bool>,
    wake: Condvar,
}

impl Stop {
    /// `true` = stop was requested (either before or during the wait).
    fn wait(&self, timeout: std::time::Duration) -> bool {
        let guard = match self.stopped.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if *guard {
            return true;
        }
        match self.wake.wait_timeout(guard, timeout) {
            Ok((guard, _)) => *guard,
            Err(poisoned) => *poisoned.into_inner().0,
        }
    }

    fn set(&self) {
        let mut guard = match self.stopped.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        *guard = true;
        self.wake.notify_all();
    }
}

struct Entry {
    count: usize,
    stop: Arc<Stop>,
}

type Registry = Arc<Mutex<HashMap<PathBuf, Entry>>>;

#[derive(Default)]
pub struct CloneRefreshers {
    inner: Registry,
}

/// RAII hold on a clone's refresher — dropping the last one stops the loop
/// promptly (mid-sleep included), so release is structural, never a call the
/// teardown path can miss. Carries its registry so a drop always releases
/// where it retained.
pub struct RefresherHold {
    inner: Registry,
    clone: PathBuf,
}

impl Drop for RefresherHold {
    fn drop(&mut self) {
        release(&self.inner, &self.clone);
    }
}

fn release(inner: &Registry, clone: &Path) {
    let mut inner = match inner.lock() {
        Ok(inner) => inner,
        Err(poisoned) => poisoned.into_inner(),
    };
    let Some(entry) = inner.get_mut(clone) else {
        return; // spawn failed at retain time — nothing to stop
    };
    entry.count -= 1;
    if entry.count == 0 {
        entry.stop.set();
        inner.remove(clone);
    }
}

impl CloneRefreshers {
    /// Hold a refresher for `clone`. The first hold spawns the loop thread;
    /// later holds just bump the count. The first pass is a token-cache hit
    /// right after launch (the launcher's own mint seeded it), so retaining
    /// immediately post-spawn is cheap.
    pub fn retain(
        &self,
        trpc: Arc<TrpcClient>,
        repository_id: &str,
        clone: &Path,
    ) -> RefresherHold {
        let mut inner = match self.inner.lock() {
            Ok(inner) => inner,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(entry) = inner.get_mut(clone) {
            entry.count += 1;
            return RefresherHold {
                inner: Arc::clone(&self.inner),
                clone: clone.to_path_buf(),
            };
        }
        let stop = Arc::new(Stop {
            stopped: Mutex::new(false),
            wake: Condvar::new(),
        });
        inner.insert(
            clone.to_path_buf(),
            Entry {
                count: 1,
                stop: Arc::clone(&stop),
            },
        );
        let repository_id = repository_id.to_string();
        let clone_path = clone.to_path_buf();
        let spawned = std::thread::Builder::new()
            .name("exp-token-refresh".to_string())
            .spawn(move || loop {
                let delay = match refresh_clone_token(&trpc, &repository_id, &clone_path) {
                    Ok(minted) => {
                        next_refresh_delay(minted.expires_at.as_deref(), std::time::SystemTime::now())
                    }
                    Err(err) => {
                        log::warn!(
                            "token refresh failed for {}: {err} — retrying in {}s",
                            clone_path.display(),
                            TOKEN_REFRESH_RETRY.as_secs()
                        );
                        TOKEN_REFRESH_RETRY
                    }
                };
                if stop.wait(delay) {
                    return;
                }
            });
        if spawned.is_err() {
            log::warn!("token refresh thread spawn failed — mid-session refresh disabled");
            inner.remove(clone);
        }
        RefresherHold {
            inner: Arc::clone(&self.inner),
            clone: clone.to_path_buf(),
        }
    }

    #[cfg(test)]
    fn active_count(&self) -> usize {
        match self.inner.lock() {
            Ok(inner) => inner.len(),
            Err(poisoned) => poisoned.into_inner().len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use api::StaticToken;
    use std::time::{Duration, Instant};

    // Each test uses its OWN registry instance (the global one would couple
    // tests) and unique repository ids (the token cache IS process-global).

    fn temp_clone(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "exp-refresh-host-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn client() -> Arc<TrpcClient> {
        // An unreachable server: every refresh errors into the retry branch —
        // these tests exercise the registry/cancellation mechanics, not the
        // mint (token_refresh.rs covers that against a canned server).
        Arc::new(TrpcClient::new(
            "http://127.0.0.1:9",
            Arc::new(StaticToken("tok".to_string())),
        ))
    }

    fn wait_until(deadline: Duration, mut done: impl FnMut() -> bool) -> bool {
        let start = Instant::now();
        while start.elapsed() < deadline {
            if done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        done()
    }

    #[test]
    fn holds_are_ref_counted_per_clone() {
        let refreshers = CloneRefreshers::default();
        let clone = temp_clone("refcount");
        let first = refreshers.retain(client(), "repo-host-rc", &clone);
        let second = refreshers.retain(client(), "repo-host-rc", &clone);
        assert_eq!(refreshers.active_count(), 1, "one loop per clone");
        drop(first);
        assert_eq!(refreshers.active_count(), 1, "still held");
        drop(second);
        assert_eq!(refreshers.active_count(), 0, "last drop releases");
        let _ = std::fs::remove_dir_all(&clone);
    }

    #[test]
    fn distinct_clones_get_distinct_loops() {
        let refreshers = CloneRefreshers::default();
        let a = temp_clone("multi-a");
        let b = temp_clone("multi-b");
        let hold_a = refreshers.retain(client(), "repo-host-ma", &a);
        let hold_b = refreshers.retain(client(), "repo-host-mb", &b);
        assert_eq!(refreshers.active_count(), 2);
        drop(hold_a);
        drop(hold_b);
        assert_eq!(refreshers.active_count(), 0);
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    /// The Condvar wake, not the timeout: a drop mid-backoff (5 min here —
    /// the unreachable server puts the loop in the retry branch) must end the
    /// thread within moments.
    #[test]
    fn dropping_the_last_hold_cancels_a_long_sleep_promptly() {
        let refreshers = CloneRefreshers::default();
        let clone = temp_clone("cancel");
        let stop = {
            let hold = refreshers.retain(client(), "repo-host-cancel", &clone);
            // Reach into the registry for the stop flag so the test can
            // observe the loop actually parking on it.
            let stop = {
                let inner = refreshers.inner.lock().unwrap();
                Arc::clone(&inner.get(clone.as_path()).unwrap().stop)
            };
            drop(hold);
            stop
        };
        assert_eq!(refreshers.active_count(), 0);
        assert!(
            wait_until(Duration::from_secs(2), || *stop.stopped.lock().unwrap()),
            "stop flag must be set by the last drop"
        );
        let _ = std::fs::remove_dir_all(&clone);
    }

    /// The session_host contract: repo-less runs retain nothing.
    #[test]
    fn repo_less_sessions_take_no_hold() {
        let refreshers = CloneRefreshers::default();
        let repository_id: Option<String> = None;
        let hold = repository_id
            .as_deref()
            .map(|repo| refreshers.retain(client(), repo, Path::new("/nowhere")));
        assert!(hold.is_none());
        assert_eq!(refreshers.active_count(), 0);
    }
}
