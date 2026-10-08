//! EXP-1244: the app-wide `repositories.openPulls` store (the web
//! `lib/open-pulls-store.ts`). The Reviews page AND the rail's Reviews dot
//! read it through the ONE queue (`domain::reviews_queue`, rule 7 +
//! `_navDoc`), so the page and its dot never disagree.
//!
//! One entry per team (`fetched_at` + the repos), one in-flight fetch per
//! team (a second ask while one runs is a no-op). The page force-refreshes on
//! entry; the rail refreshes a team older than [`STALE`] when the active team
//! changes and on window activation — never on a timer (the server caches
//! 60 s too). A failed fetch lists nothing; a merged pull has no Electric
//! echo, so [`OpenPulls::remove_merged`] drops it locally.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use gpui::{App, AppContext as _, Entity};

use crate::queries;

/// A non-forced refresh refetches a team older than this.
pub const STALE: Duration = Duration::from_secs(60);

struct Entry {
    repos: Vec<api::repositories::OpenPullsRepo>,
    fetched_at: Instant,
}

#[derive(Default)]
pub struct OpenPulls {
    entries: HashMap<String, Entry>,
    in_flight: HashSet<String>,
    /// Bumped on every change — the rail's memo key.
    revision: u64,
}

struct OpenPullsGlobal(Entity<OpenPulls>);

impl gpui::Global for OpenPullsGlobal {}

/// Whether a refresh should fetch: never while one runs; forced always;
/// otherwise only a missing or [`STALE`] entry.
fn should_fetch(in_flight: bool, force: bool, fetched_at: Option<Instant>, now: Instant) -> bool {
    if in_flight {
        return false;
    }
    force || fetched_at.is_none_or(|at| now.saturating_duration_since(at) >= STALE)
}

impl OpenPulls {
    pub fn global(cx: &mut App) -> Entity<OpenPulls> {
        if let Some(global) = cx.try_global::<OpenPullsGlobal>() {
            return global.0.clone();
        }
        let store = cx.new(|_| OpenPulls::default());
        cx.set_global(OpenPullsGlobal(store.clone()));
        store
    }

    /// `team_id`'s fetched repos (`[]` until the first fetch lands).
    pub fn repos(&self, team_id: &str) -> &[api::repositories::OpenPullsRepo] {
        self.entries
            .get(team_id)
            .map(|entry| entry.repos.as_slice())
            .unwrap_or(&[])
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Fetch `team_id`'s open pulls unless one is in flight or (unforced) a
    /// fresh entry exists.
    pub fn refresh(team_id: &str, force: bool, cx: &mut App) {
        let store = Self::global(cx);
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let team = team_id.to_string();
        store.update(cx, |this, cx| {
            let fetched_at = this.entries.get(&team).map(|entry| entry.fetched_at);
            if !should_fetch(this.in_flight.contains(&team), force, fetched_at, Instant::now()) {
                return;
            }
            this.in_flight.insert(team.clone());
            cx.spawn(async move |this, cx| {
                let call_team = team.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move { api::repositories::open_pulls(&trpc, &call_team) })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    this.in_flight.remove(&team);
                    let repos = result.unwrap_or_else(|err| {
                        // The synced rows still render; the unlinked bands
                        // stay absent (the web's degradation).
                        log::warn!("[ui] repositories.openPulls failed: {err}");
                        Vec::new()
                    });
                    this.entries.insert(
                        team,
                        Entry {
                            repos,
                            fetched_at: Instant::now(),
                        },
                    );
                    this.revision += 1;
                    cx.notify();
                });
            })
            .detach();
        });
    }

    /// A merged pull has no Electric echo: drop it from every team's entry.
    pub fn remove_merged(repository_id: &str, number: u64, cx: &mut App) {
        Self::global(cx).update(cx, |this, cx| {
            for entry in this.entries.values_mut() {
                queries::remove_merged_pull(&mut entry.repos, repository_id, number);
            }
            this.revision += 1;
            cx.notify();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One fetch per team at a time; the page forces, the rail only
    /// refetches a missing or 60 s-old entry.
    #[test]
    fn should_fetch_dedupes_and_honours_staleness() {
        let now = Instant::now() + Duration::from_secs(600);
        let fresh = Some(now - Duration::from_secs(10));
        let stale = Some(now - STALE);
        assert!(!should_fetch(true, true, None, now));
        assert!(!should_fetch(true, false, stale, now));
        assert!(should_fetch(false, false, None, now));
        assert!(should_fetch(false, false, stale, now));
        assert!(!should_fetch(false, false, fresh, now));
        assert!(should_fetch(false, true, fresh, now));
    }
}
