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
//! The server's staleness sweep leans on the same distinction: it DELETES a
//! `running` row whose heartbeat stopped (badge cleanup that must never kill
//! a possibly-live child), so vanish-does-not-fire is load-bearing — only an
//! explicit `ended` flip may ever tear a session down.

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

/// Whether an ended row may actually fire the kill (EXP-105 F3). After the
/// server's staleness sweep DELETEs a live session's row (laptop-suspend
/// case), any workspace member can resurrect the id via the scoped heartbeat
/// — the server re-inserts with THE SENDER as owner — then flip it to
/// `ended`, remote-killing a run they never owned. The original owner is
/// unknowable server-side (the row was deleted), so the desktop enforces it:
/// a row whose `user_id` differs from the signed-in user's id never fires.
/// Legit kills (`steer.killSession` by a workspace owner) flip status WITHOUT
/// changing the row's owner, so they still pass. An unknowable owner on
/// either side degrades to firing (the server always stamps `user_id`; this
/// only covers partial/legacy rows).
pub fn session_row_fires_kill(row: Option<&CodingSession>, own_user_id: Option<&str>) -> bool {
    if !session_row_is_ended(row) {
        return false;
    }
    match (own_user_id, row.and_then(|session| session.user_id.as_deref())) {
        (Some(own), Some(owner)) => own == owner,
        _ => true,
    }
}

/// One registered watch: the teardown callback plus the signed-in user's id
/// at registration time (the row's expected owner — see
/// [`session_row_fires_kill`]).
struct Watched {
    own_user_id: Option<String>,
    on_ended: OnSessionEnded,
}

/// The watch registry. Install ONCE per app (after [`Store::open`]) and share
/// the entity with the coding flow.
pub struct KillWatch {
    sessions: Entity<Collection<CodingSession>>,
    watched: HashMap<String, Watched>,
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

    /// Watch a session this desktop started. `own_user_id` is the signed-in
    /// user's id — the row's expected owner ([`session_row_fires_kill`]). If
    /// the row is ALREADY `ended` (the kill raced the registration), the
    /// callback fires immediately — the §8.8 gate must hold even when
    /// Electric beats the local wiring.
    pub fn watch(
        &mut self,
        session_id: impl Into<String>,
        own_user_id: Option<String>,
        on_ended: OnSessionEnded,
        cx: &Context<Self>,
    ) {
        let session_id = session_id.into();
        let already_ended = session_row_fires_kill(
            self.sessions.read(cx).get(&session_id),
            own_user_id.as_deref(),
        );
        if already_ended {
            log::info!("kill-watch: session {session_id} already ended at register");
            on_ended();
            return;
        }
        self.watched.insert(
            session_id,
            Watched {
                own_user_id,
                on_ended,
            },
        );
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
            .iter()
            .filter(|(id, watched)| {
                session_row_fires_kill(collection.get(id.as_str()), watched.own_user_id.as_deref())
            })
            .map(|(id, _)| id.clone())
            .collect();
        for session_id in fired {
            if let Some(watched) = self.watched.remove(&session_id) {
                log::info!("kill-watch: session {session_id} ended remotely — aborting");
                (watched.on_ended)();
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

    fn owned_session(status: &str, user_id: &str) -> CodingSession {
        serde_json::from_value(json!({
            "id": "sess-1",
            "status": status,
            "user_id": user_id,
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

    #[test]
    fn foreign_owner_ended_flip_never_fires() {
        // EXP-105 F3: a swept-then-resurrected row carries the resurrector as
        // owner — its `ended` flip must NOT kill the real owner's run.
        let foreign = owned_session("ended", "attacker");
        assert!(!session_row_fires_kill(Some(&foreign), Some("me")));
        // A legit kill keeps the original owner on the row and still fires.
        let own = owned_session("ended", "me");
        assert!(session_row_fires_kill(Some(&own), Some("me")));
        // Not-ended rows never fire regardless of owner.
        assert!(!session_row_fires_kill(
            Some(&owned_session("running", "me")),
            Some("me")
        ));
        assert!(!session_row_fires_kill(None, Some("me")));
        // Unknowable owner on either side degrades to the plain ended check.
        assert!(session_row_fires_kill(Some(&session("ended")), Some("me")));
        assert!(session_row_fires_kill(Some(&foreign), None));
    }
}
