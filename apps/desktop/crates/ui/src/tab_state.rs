//! EXP-894 — per-TAB view state that survives a tab switch.
//!
//! The centre's heavyweight detail views (issue detail, support thread) are
//! ONE shared instance each, re-pointed on every tab switch (`screens.rs`).
//! Re-pointing used to reset their local state, so a half-typed comment was
//! gone the moment you looked at another tab and came back.
//!
//! The choice (documented here once, cited at the call sites): keep the
//! shared single instances and stash the small USER state a switch would
//! throw away — composer text, an open reply draft, pending picks, the body's
//! scroll offset — in a per-tab store keyed by the tab's subject id, restored
//! when the view is pointed back at it and dropped when the tab closes.
//! Caching one whole `IssueDetailView` per tab was rejected: each instance
//! observes seven synced collections, spawns fetches and owns an editor, so
//! N open issue tabs would run N× that work on every sync burst while hidden.
//! A stash costs nothing while its tab is in the background, and only the
//! active view is ever rendered. Session tabs need none of this: they already
//! keep ONE `SessionScreenView` per open tab (each owns its feed), so the
//! steer composer and the transcript scroll survive a switch as is.

use std::collections::HashMap;

/// Whether a stashed state is worth keeping. An empty one (nothing typed, the
/// top of the page) is dropped rather than stored, so the store only ever
/// holds tabs with something to restore.
pub(crate) trait TabStateEmpty {
    fn is_empty_state(&self) -> bool;
}

/// The per-tab stash itself: `key` = the tab's subject (an issue id, a
/// support-thread id). Bounded by the open tabs — the panel calls
/// [`TabStateStore::forget`] from every tab-removal path.
pub(crate) struct TabStateStore<T> {
    entries: HashMap<String, T>,
    /// A subject whose tab closed WHILE its state was still live in the view
    /// (the active tab): its next [`TabStateStore::stash`] is a discard, so a
    /// closed tab's draft never resurrects when the issue is opened again.
    closed: Option<String>,
}

impl<T> Default for TabStateStore<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            closed: None,
        }
    }
}

impl<T: TabStateEmpty> TabStateStore<T> {
    /// Keep `state` for `key` as the view leaves it. An empty state clears
    /// any older stash instead; a key whose tab has closed stores nothing.
    pub(crate) fn stash(&mut self, key: &str, state: T) {
        if self.closed.as_deref() == Some(key) {
            self.closed = None;
            self.entries.remove(key);
            return;
        }
        if state.is_empty_state() {
            self.entries.remove(key);
        } else {
            self.entries.insert(key.to_string(), state);
        }
    }

    /// The state to restore as the view is pointed at `key` — consumed, so a
    /// restore happens once and the live view owns it again.
    pub(crate) fn take(&mut self, key: &str) -> Option<T> {
        if self.closed.as_deref() == Some(key) {
            self.closed = None;
        }
        self.entries.remove(key)
    }

    /// The view is (re)pointed at `key` without a switch (a tab reopened on
    /// the subject still pointed at). `true` = its tab had CLOSED meanwhile,
    /// so the caller resets the live state it still holds — a closed tab's
    /// draft must not come back with the reopened one.
    pub(crate) fn reopened(&mut self, key: &str) -> bool {
        if self.closed.as_deref() == Some(key) {
            self.closed = None;
            return true;
        }
        false
    }

    /// `key`'s tab closed. `live` = the view is still pointed at it, so the
    /// state it holds right now must not be stashed on the way out either.
    pub(crate) fn forget(&mut self, key: &str, live: bool) {
        self.entries.remove(key);
        if live {
            self.closed = Some(key.to_string());
        }
    }

    /// Every tab went (a team switch). `live` = the subject still on show.
    pub(crate) fn clear(&mut self, live: Option<&str>) {
        self.entries.clear();
        self.closed = live.map(str::to_string);
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Draft(&'static str);

    impl TabStateEmpty for Draft {
        fn is_empty_state(&self) -> bool {
            self.0.is_empty()
        }
    }

    #[test]
    fn a_switch_away_and_back_restores_the_draft() {
        let mut store = TabStateStore::default();
        store.stash("issue-a", Draft("half a comment"));
        assert_eq!(store.take("issue-b"), None);
        assert_eq!(store.take("issue-a"), Some(Draft("half a comment")));
        // Consumed: the live view owns it again.
        assert_eq!(store.take("issue-a"), None);
    }

    #[test]
    fn an_empty_state_is_not_stored_and_clears_an_older_one() {
        let mut store = TabStateStore::default();
        store.stash("issue-a", Draft(""));
        assert_eq!(store.len(), 0);
        store.stash("issue-a", Draft("typed"));
        store.stash("issue-a", Draft(""));
        assert_eq!(store.take("issue-a"), None);
    }

    #[test]
    fn closing_a_background_tab_drops_its_stash() {
        let mut store = TabStateStore::default();
        store.stash("issue-a", Draft("typed"));
        store.forget("issue-a", false);
        assert_eq!(store.take("issue-a"), None);
    }

    #[test]
    fn closing_the_active_tab_discards_the_live_state_on_the_way_out() {
        let mut store = TabStateStore::default();
        store.forget("issue-a", true);
        // The view leaves the closed issue for another tab: nothing is kept.
        store.stash("issue-a", Draft("typed before the close"));
        assert_eq!(store.take("issue-a"), None);
        // The discard is one-shot — a later visit stashes normally.
        store.stash("issue-a", Draft("typed after reopening"));
        assert_eq!(store.take("issue-a"), Some(Draft("typed after reopening")));
    }

    #[test]
    fn reopening_a_closed_issue_still_on_show_asks_for_a_reset_once() {
        let mut store = TabStateStore::<Draft>::default();
        assert!(!store.reopened("issue-a"), "an open tab keeps its live state");
        store.forget("issue-a", true);
        assert!(store.reopened("issue-a"), "the closed tab's live draft resets");
        assert!(!store.reopened("issue-a"));
        store.stash("issue-a", Draft("still typing"));
        assert_eq!(store.take("issue-a"), Some(Draft("still typing")));
    }

    #[test]
    fn a_team_switch_clears_every_stash() {
        let mut store = TabStateStore::default();
        store.stash("issue-a", Draft("a"));
        store.stash("issue-b", Draft("b"));
        store.clear(Some("issue-c"));
        assert_eq!(store.len(), 0);
        store.stash("issue-c", Draft("c"));
        assert_eq!(store.take("issue-c"), None);
    }
}
