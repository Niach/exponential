//! The §8.8 own-row Electric kill-switch (masterplan-v3) — the ONLY kill
//! path that survives a dead relay.
//!
//! `steer.killSession` flips the synced `coding_sessions` row to
//! `status = ended` (the relay `kill` fan-out is best-effort). This watch
//! subscribes to the [`crate::collections`] `coding_sessions` entity
//! (read-only — it consumes the public collections API and never writes) and
//! fires a one-shot callback when a locally-watched session's row transitions
//! to `ended`. The coding-flow glue registers each session it starts; its
//! callback kills the `claude` child (`Terminal::kill`), tells the steer
//! publisher (`PublisherHandle::session_ended()`), and marks the tab stopped.
//!
//! Dependency direction (§3.1): `steer` cannot depend on `sync` and `sync`
//! cannot depend on `steer`, so this lives here as a generic callback watch;
//! the app/ui layer wires the two.
//!
//! "The desktop didn't initiate it" (§8.8): locally-initiated teardown paths
//! must [`KillWatch::unwatch`] BEFORE ending the session so the callback only
//! fires for remote/out-of-band kills. Callbacks are fire-once (removed on
//! fire). Sign-out clears the collections without marking rows `ended` —
//! a vanished row deliberately does NOT fire (children owned by this desktop
//! are torn down by the sign-out path itself, not by the kill-switch).

use std::collections::HashMap;

use domain::contract::CODING_SESSION_STATUS_ENDED;
use domain::rows::CodingSession;
use gpui::{App, AppContext as _, Context, Entity};

use crate::collections::{Collection, Store};

/// One-shot teardown callback, invoked on the gpui foreground.
pub type OnSessionEnded = Box<dyn FnOnce() + 'static>;

/// The `running → ended` transition test, shared by the sweep and the
/// register-time race check. `None` status (absent/partial row) is NOT ended.
pub fn session_row_is_ended(row: Option<&CodingSession>) -> bool {
    row.and_then(|session| session.status.as_deref())
        .is_some_and(|status| status == CODING_SESSION_STATUS_ENDED)
}

/// The watch registry. Install ONCE per app (after [`Store::open`]) and share
/// the entity with the coding flow.
pub struct KillWatch {
    sessions: Entity<Collection<CodingSession>>,
    watched: HashMap<String, OnSessionEnded>,
}

impl KillWatch {
    /// Subscribe to the store's `coding_sessions` collection. The observer
    /// lives as long as the entity (subscription detached into the app).
    pub fn install(store: &Store, cx: &mut App) -> Entity<KillWatch> {
        let sessions = store.collections().coding_sessions.clone();
        cx.new(|cx: &mut Context<KillWatch>| {
            cx.observe(&sessions, |watch: &mut KillWatch, sessions, cx| {
                watch.sweep(&sessions, cx);
            })
            .detach();
            KillWatch {
                sessions,
                watched: HashMap::new(),
            }
        })
    }

    /// Watch a session this desktop started. If the row is ALREADY `ended`
    /// (the kill raced the registration), the callback fires immediately —
    /// the §8.8 gate must hold even when Electric beats the local wiring.
    pub fn watch(
        &mut self,
        session_id: impl Into<String>,
        on_ended: OnSessionEnded,
        cx: &Context<Self>,
    ) {
        let session_id = session_id.into();
        let already_ended = session_row_is_ended(self.sessions.read(cx).get(&session_id));
        if already_ended {
            log::info!("kill-watch: session {session_id} already ended at register");
            on_ended();
            return;
        }
        self.watched.insert(session_id, on_ended);
    }

    /// Drop a watch WITHOUT firing — locally-initiated teardown (child
    /// exited, user stopped the tab) calls this first so the kill-switch only
    /// reacts to kills the desktop didn't initiate (§8.8).
    pub fn unwatch(&mut self, session_id: &str) {
        self.watched.remove(session_id);
    }

    /// Sessions currently being watched (observability/tests).
    pub fn watched_ids(&self) -> Vec<String> {
        self.watched.keys().cloned().collect()
    }

    fn sweep(&mut self, sessions: &Entity<Collection<CodingSession>>, cx: &Context<Self>) {
        if self.watched.is_empty() {
            return;
        }
        let collection = sessions.read(cx);
        let fired: Vec<String> = self
            .watched
            .keys()
            .filter(|id| session_row_is_ended(collection.get(id)))
            .cloned()
            .collect();
        for session_id in fired {
            if let Some(on_ended) = self.watched.remove(&session_id) {
                log::info!("kill-watch: session {session_id} ended remotely — aborting");
                on_ended();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn session(status: &str) -> CodingSession {
        serde_json::from_value(json!({
            "id": "sess-1",
            "status": status,
        }))
        .unwrap()
    }

    #[test]
    fn only_an_explicit_ended_status_counts() {
        assert!(session_row_is_ended(Some(&session("ended"))));
        assert!(!session_row_is_ended(Some(&session("running"))));
        // Absent row (sign-out clear / not yet synced) must NOT fire (§8.8:
        // the kill signal is the row FLIP, not the row's absence).
        assert!(!session_row_is_ended(None));
        // Partial row without a status: not ended.
        let bare: CodingSession = serde_json::from_value(json!({"id": "sess-1"})).unwrap();
        assert!(!session_row_is_ended(Some(&bare)));
    }
}
