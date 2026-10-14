//! The §8.8 own-row Electric kill-switch (masterplan-v3) — the ONLY kill
//! path that survives a dead relay.
//!
//! EXP-1294 (Threads v2): an explicit end (`sessions.end`, ending a root ends
//! its whole tree server-side) flips the synced `sessions` row to
//! `status = ended`. `queued`, `running` and `idle` never fire; agent roots
//! never end; merging never ends a session. "Stop" (interrupt a turn) is not
//! an `ended` flip and travels another path.
//!
//! This watch subscribes to the [`crate::collections`] `sessions` entity
//! (read-only — it consumes the public collections API and never writes)
//! and fires a one-shot callback when a locally-watched session's row
//! transitions to `ended`. The host registers each session it runs; its
//! callback tears the engine down and tells the steer publisher.
//!
//! Dependency direction (§3.1): `steer` cannot depend on `sync` and `sync`
//! cannot depend on `steer`, so this lives here as a generic callback watch;
//! the app/ui layer wires the two.
//!
//! "The desktop didn't initiate it" (§8.8): locally-initiated teardown paths
//! (the device writes `ended` itself through `sessions.report`) must
//! [`KillWatch::unwatch`] BEFORE ending the session so the callback only
//! fires for remote/out-of-band ends. Callbacks are fire-once (removed on
//! fire). Sign-out clears the collections without marking rows `ended` — a
//! vanished row deliberately does NOT fire.
//!
//! Owner guard (EXP-105 F3, re-keyed by EXP-1294): `user_id` is now the
//! STARTER (a teammate may start a session on my device) and the host's
//! owner is server-only, so the guard is the HOST DEVICE: a row whose
//! `device_id` is not this device's id never fires.

#[cfg(feature = "gpui")]
use std::collections::HashMap;

use domain::rows::SessionRow;
#[cfg(feature = "gpui")]
use gpui::{App, AppContext as _, Context, Entity};

#[cfg(feature = "gpui")]
use crate::collections::{Collection, Store};

/// What the ended row says, handed to the teardown callback. EXP-1294: the
/// row carries no close-out reason any more, so this is empty;
/// it stays a type so the callback signature can grow without churn.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EndedFacts {}

/// One-shot teardown callback, invoked on the gpui foreground.
#[cfg(feature = "gpui")]
pub type OnSessionEnded = Box<dyn FnOnce(EndedFacts) + 'static>;

/// The `→ ended` transition test, shared by the sweep and the register-time
/// race check. An absent row (sign-out clear / not yet synced) or a row
/// without a status is NOT ended.
pub fn session_row_is_ended(row: Option<&SessionRow>) -> bool {
    row.is_some_and(SessionRow::is_ended)
}

/// Whether an ended row may actually fire the kill: it must be ended AND
/// hosted by THIS device (`own_device_id`). An unknowable device on either
/// side degrades to the plain ended check (the server always stamps
/// `device_id`; this only covers partial rows).
pub fn session_row_fires_kill(row: Option<&SessionRow>, own_device_id: Option<&str>) -> bool {
    if !session_row_is_ended(row) {
        return false;
    }
    match (own_device_id, row.and_then(|session| session.device_id.as_deref())) {
        (Some(own), Some(host)) => own == host,
        _ => true,
    }
}

/// One registered watch: the teardown callback plus this device's id at
/// registration time (the row's expected host — see
/// [`session_row_fires_kill`]).
#[cfg(feature = "gpui")]
struct Watched {
    own_device_id: Option<String>,
    on_ended: OnSessionEnded,
}

/// The watch registry. Install ONCE per app (after [`Store::open`]) and share
/// the entity with the session host.
#[cfg(feature = "gpui")]
pub struct KillWatch {
    sessions: Entity<Collection<SessionRow>>,
    watched: HashMap<String, Watched>,
}

#[cfg(feature = "gpui")]
impl KillWatch {
    /// Subscribe to the store's `sessions` collection. The observer lives as
    /// long as the entity (subscription detached into the app).
    pub fn install(store: &Store, cx: &mut App) -> Entity<KillWatch> {
        let sessions = store.collections().sessions.clone();
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

    /// Watch a session this device hosts. `own_device_id` is THIS device's
    /// steer device id — the row's expected `device_id`
    /// ([`session_row_fires_kill`]). If the row is ALREADY `ended` (the end
    /// raced the registration), the callback fires immediately — the §8.8
    /// gate must hold even when Electric beats the local wiring.
    pub fn watch(
        &mut self,
        session_id: impl Into<String>,
        own_device_id: Option<String>,
        on_ended: OnSessionEnded,
        cx: &Context<Self>,
    ) {
        let session_id = session_id.into();
        let already_ended = session_row_fires_kill(
            self.sessions.read(cx).get(&session_id),
            own_device_id.as_deref(),
        );
        if already_ended {
            log::info!("kill-watch: session {session_id} already ended at register");
            on_ended(EndedFacts::default());
            return;
        }
        self.watched.insert(
            session_id,
            Watched {
                own_device_id,
                on_ended,
            },
        );
    }

    /// Drop a watch WITHOUT firing — locally-initiated teardown calls this
    /// first so the kill-switch only reacts to ends the device didn't
    /// initiate (§8.8).
    pub fn unwatch(&mut self, session_id: &str) {
        self.watched.remove(session_id);
    }

    /// Sessions currently being watched (observability/tests).
    pub fn watched_ids(&self) -> Vec<String> {
        self.watched.keys().cloned().collect()
    }

    fn sweep(&mut self, sessions: &Entity<Collection<SessionRow>>, cx: &Context<Self>) {
        if self.watched.is_empty() {
            return;
        }
        let collection = sessions.read(cx);
        let fired: Vec<String> = self
            .watched
            .iter()
            .filter(|(id, watched)| {
                session_row_fires_kill(collection.get(id.as_str()), watched.own_device_id.as_deref())
            })
            .map(|(id, _)| id.clone())
            .collect();
        for session_id in fired {
            if let Some(watched) = self.watched.remove(&session_id) {
                log::info!("kill-watch: session {session_id} ended remotely — aborting");
                (watched.on_ended)(EndedFacts::default());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn session(status: &str) -> SessionRow {
        serde_json::from_value(json!({
            "id": "sess-1",
            "status": status,
        }))
        .unwrap()
    }

    fn hosted(status: &str, device_id: &str) -> SessionRow {
        serde_json::from_value(json!({
            "id": "sess-1",
            "status": status,
            "device_id": device_id,
            "user_id": "teammate",
        }))
        .unwrap()
    }

    #[test]
    fn only_an_explicit_ended_status_counts() {
        assert!(session_row_is_ended(Some(&session("ended"))));
        for live in ["queued", "running", "idle"] {
            assert!(!session_row_is_ended(Some(&session(live))), "{live} never fires");
        }
        // Absent row (sign-out clear / not yet synced) must NOT fire (§8.8:
        // the kill signal is the row FLIP, not the row's absence).
        assert!(!session_row_is_ended(None));
        // Partial row without a status: not ended.
        let bare: SessionRow = serde_json::from_value(json!({"id": "sess-1"})).unwrap();
        assert!(!session_row_is_ended(Some(&bare)));
    }

    #[test]
    fn only_this_devices_ended_row_fires() {
        // A teammate started it on MY device: the starter is not me, the
        // host is — it fires.
        assert!(session_row_fires_kill(Some(&hosted("ended", "dev-me")), Some("dev-me")));
        // Another device's session never kills mine.
        assert!(!session_row_fires_kill(Some(&hosted("ended", "dev-other")), Some("dev-me")));
        // Not-ended rows never fire regardless of host.
        assert!(!session_row_fires_kill(Some(&hosted("idle", "dev-me")), Some("dev-me")));
        assert!(!session_row_fires_kill(None, Some("dev-me")));
        // Unknowable device on either side degrades to the plain ended check.
        assert!(session_row_fires_kill(Some(&session("ended")), Some("dev-me")));
        assert!(session_row_fires_kill(Some(&hosted("ended", "dev-other")), None));
    }
}
