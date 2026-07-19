//! Zero-team self-heal (EXP-43).
//!
//! v7 makes `teams.create` instance-admin-only, so a user whose LAST
//! team was deleted (settings → Danger Zone) — or whose personal
//! team bootstrap failed at signup — lands in a dead end: no team,
//! no way to create one, and (before this module) no caller of
//! `teams.ensureDefault` anywhere in the desktop app, so even a restart
//! never healed the state.
//!
//! The heal: an App-level observer on the synced **teams collection**.
//! Whenever it changes (a delete's Electric echo, the initial sync catching
//! up, a refetch) and the state is provably "signed in, fully synced, zero
//! teams", call `teams.ensureDefault` ONCE and switch the active
//! window to the returned team (idempotent server-side — it returns the
//! existing personal team when one survives).
//!
//! Guards, in order:
//! - session must be `Synced` (never fire for signed-out/logging-in states);
//! - the teams shape must be `is_ready()` — empty-because-still-loading
//!   must never trigger a create (§4.1 skeleton-vs-empty rule);
//! - a one-shot latch: a failed/slow ensureDefault must not be re-fired by
//!   every subsequent collection notify (loop protection). The latch re-arms
//!   once the collection holds a row again, so a LATER delete-the-last-
//!   team heals too.

use gpui::{App, Global};
use sync::{SessionPhase, Store};

/// One-shot latch: the account id we already fired `ensureDefault` for.
#[derive(Default)]
struct HealAttempted {
    account_id: Option<String>,
}

impl Global for HealAttempted {}

/// Install the teams-collection observer. Call once at bootstrap, after
/// the `Store` global exists; the observer survives sign-out/sign-in cycles
/// (the collection entities are process-lived).
pub fn install(cx: &mut App) {
    let teams = Store::global(cx).collections().teams.clone();
    cx.observe(&teams, |_, cx| maybe_heal(cx)).detach();
}

fn maybe_heal(cx: &mut App) {
    let store = Store::global(cx).clone();
    let SessionPhase::Synced { account_id } = store.session(cx) else {
        // Sign-out clears the collections (a notify lands here) — re-arm so
        // the next session gets a fresh attempt.
        cx.default_global::<HealAttempted>().account_id = None;
        return;
    };
    {
        let teams = store.collections().teams.read(cx);
        if !teams.is_empty() {
            // Healthy again (the heal's echo, an invite accept, …) — re-arm
            // the latch so a future last-team delete heals as well.
            cx.default_global::<HealAttempted>().account_id = None;
            return;
        }
        if !teams.is_ready() {
            return;
        }
    }

    let attempted = cx.default_global::<HealAttempted>();
    if attempted.account_id.as_deref() == Some(account_id.as_str()) {
        return;
    }
    attempted.account_id = Some(account_id);

    let Some(trpc) = crate::queries::trpc_client(cx) else {
        return;
    };
    log::info!("[ui] team heal: synced with zero teams — teams.ensureDefault");

    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::teams::teams_ensure_default(&trpc) })
            .await;
        let _ = cx.update(|cx| match result {
            Ok(output) => {
                // Adopt the (re)created team: the explicit selection makes
                // every window surface resolve it the moment its Electric echo
                // lands (`active_team_id` falls back to the first synced
                // team anyway — this just pins it and persists the choice).
                let team_id = output.team.id;
                crate::navigation::on_active_window(cx, move |window, cx| {
                    crate::navigation::switch_team(window, cx, team_id);
                });
            }
            Err(err) => {
                // Deliberately NOT retried (one-shot latch): a persistent
                // server error would otherwise fire on every collection
                // notify. Sign-out/sign-in (or a team appearing by any
                // other path) re-arms the latch.
                log::warn!("[ui] team heal: teams.ensureDefault failed: {err}");
            }
        });
    })
    .detach();
}
