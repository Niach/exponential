//! The issue's CHANGES face over its pull request (EXP-181/EXP-889): the
//! shared unified [`DiffView`] over `issues.prFiles`, embedded under an issue
//! tab's work header.
//!
//! EXP-895/EXP-916: the pane IS the shared Changes layout
//! ([`crate::diff_pane`]) — the file tree beside the per-file cards, the same
//! thing the run's Changes face renders, so a review and a run read
//! identically.
//!
//! EXP-1154: there is no standalone review screen any more. The review of a
//! PR is the issue's Work screen on this face: the work header above it
//! names the issue and carries the merge pill, the GitHub link and the `…`
//! menu (Close PR lives there), so this pane draws no header of its own.
//!
//! One instance per issue tab, re-pointed on issue switches. Same-id
//! re-points are no-ops — the fetch must not re-run per render; the diff is a
//! snapshot of the PR at open time.

use std::sync::Arc;

use gpui::{
    App, AppContext as _, Entity, FocusHandle, Focusable, IntoElement, Render, Window,
};
use sync::Store;

use crate::diff::DiffView;
use crate::pr_merge::MergeState;
use crate::queries;

/// The read-only PR diff, embedded as an issue tab's Changes face.
pub struct PrDiffView {
    focus_handle: FocusHandle,
    diff: Entity<DiffView>,
    issue_id: Option<String>,
    /// EXP-895/EXP-916: the file tree's state — which row it highlights, the
    /// directories the reader folded, and the `Filter files` field.
    selected: usize,
    folded_dirs: std::collections::HashSet<String>,
    filter: Entity<gpui_component::input::InputState>,
    /// EXP-1154: a file the Results Guide asked for (a file row click) before
    /// the PR's files had landed — selected as soon as they do.
    pending_path: Option<String>,
    _subscriptions: Vec<gpui::Subscription>,
}

impl PrDiffView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let diff = cx.new(|cx| {
            let mut diff = DiffView::new(window, cx);
            // EXP-706/EXP-895: the review diff is a stack of per-file cards.
            diff.set_options(crate::diff::DiffOptions::review());
            diff
        });
        let filter = cx.new(|cx| {
            gpui_component::input::InputState::new(window, cx)
                .placeholder(domain::contract::DIFF_UI_FILTER_PLACEHOLDER)
        });
        // The caption mirrors the shared two-click state (EXP-325).
        let merge_state = MergeState::global(cx);
        let mut subscriptions = vec![cx.observe(&merge_state, |_, _, cx| cx.notify())];
        if let Some(store) = Store::try_global(cx) {
            let issues = store.collections().issues.clone();
            subscriptions.push(cx.observe(&issues, |_, _, cx| cx.notify()));
        }
        // The file counts come off the diff's own summaries; a pending Guide
        // pick lands the moment they arrive.
        subscriptions.push(cx.observe(&diff, |this: &mut Self, _, cx| {
            this.apply_pending_path(cx);
            cx.notify();
        }));
        subscriptions.push(cx.subscribe(
            &filter,
            |_, _, event: &gpui_component::input::InputEvent, cx| {
                if matches!(event, gpui_component::input::InputEvent::Change) {
                    cx.notify();
                }
            },
        ));
        Self {
            focus_handle: cx.focus_handle(),
            diff,
            issue_id: None,
            selected: 0,
            folded_dirs: std::collections::HashSet::new(),
            filter,
            pending_path: None,
            _subscriptions: subscriptions,
        }
    }

    /// Re-point at `issue_id` and fetch its PR files (no-op on the same id).
    pub fn set_issue(&mut self, issue_id: String, cx: &mut gpui::Context<Self>) {
        if self.issue_id.as_deref() == Some(issue_id.as_str()) {
            return;
        }
        // The issue tab builds this pane from `render`, which can run while
        // the session is still validating on a background thread — so a cold
        // start into a deep link finds no client yet. Latching an error here
        // would be terminal (same-id calls no-op); stay Loading and leave
        // `issue_id` unrecorded so the next call actually re-attempts.
        let Some(client) = queries::trpc_client(cx) else {
            self.diff.update(cx, |diff, cx| diff.set_loading(cx));
            return;
        };
        // Moving OFF another issue's PR: a refusal captioned on the PREVIOUS
        // PR describes a snapshot that is no longer on screen, and leaving it
        // standing would keep "Fix conflicts" parked in the Merge slot. A
        // first load never clears it: the Results face prefetches these files
        // for its Guide counts, and that must not wipe the CURRENT issue's
        // visible merge/Close PR failure.
        let clear = clears_merge_error(self.issue_id.as_deref(), &issue_id);
        self.issue_id = Some(issue_id.clone());
        self.selected = 0;
        // A Guide pick held for the previous PR must not land on this one.
        self.pending_path = None;
        if clear {
            MergeState::clear_error(cx);
        }
        self.diff
            .update(cx, |diff, cx| diff.fetch(Arc::new(client), issue_id, cx));
    }

    /// The issue whose PR files the pane holds (or is loading).
    pub(crate) fn issue_id(&self) -> Option<&str> {
        self.issue_id.as_deref()
    }

    /// EXP-889 — the counts the pane is showing (`+N −M`), for the work
    /// header's Changes item. `None` until the files land (and for a pull
    /// request with none): the item then wears the word `Changes`, exactly
    /// like the web's switcher row without `diffStats`.
    pub(crate) fn totals(&self, cx: &App) -> Option<(u32, u32)> {
        let files = self.diff.read(cx).files();
        let totals = domain::diff::Totals {
            files: files.len(),
            additions: files.iter().map(|file| file.additions).sum(),
            deletions: files.iter().map(|file| file.deletions).sum(),
        };
        (totals.files > 0).then_some((totals.additions, totals.deletions))
    }

    /// Name `index` in the file list and scroll the diff to it.
    pub(crate) fn select_file(&mut self, index: usize, cx: &mut gpui::Context<Self>) {
        self.selected = index;
        self.diff
            .update(cx, |diff, cx| diff.scroll_to_file(index, cx));
        cx.notify();
    }

    /// EXP-1154 — select the file at `path` (a Results Guide row click). The
    /// files may not have landed yet (the face just opened): the path is
    /// held and applied when they do. An unknown path selects nothing.
    pub(crate) fn select_path(&mut self, path: String, cx: &mut gpui::Context<Self>) {
        self.pending_path = Some(path);
        self.apply_pending_path(cx);
    }

    fn apply_pending_path(&mut self, cx: &mut gpui::Context<Self>) {
        let outcome = resolve_pending_path(
            self.pending_path.as_deref(),
            self.diff
                .read(cx)
                .files()
                .iter()
                .map(|file| file.filename.as_ref()),
        );
        match outcome {
            PendingPath::Hold => {}
            PendingPath::Drop => self.pending_path = None,
            PendingPath::Select(index) => {
                self.pending_path = None;
                self.select_file(index, cx);
            }
        }
    }

    /// Fold `path` in the file tree, or open it again.
    pub(crate) fn toggle_dir(&mut self, path: String, cx: &mut gpui::Context<Self>) {
        if !self.folded_dirs.insert(path.clone()) {
            self.folded_dirs.remove(&path);
        }
        cx.notify();
    }

    /// EXP-916: the tree's inputs (and, EXP-1154, the Results Guide's file
    /// rows' counts).
    pub(crate) fn pane_files(&self, cx: &App) -> Vec<crate::diff_pane::PaneFile> {
        self.diff
            .read(cx)
            .files()
            .iter()
            .map(|file| {
                crate::diff_pane::PaneFile::from_parts(
                    &file.filename,
                    file.status,
                    file.additions,
                    file.deletions,
                )
            })
            .collect()
    }
}

/// EXP-1154 — whether re-pointing the pane from `previous` to `next` clears
/// the shared merge refusal: only when it moves OFF a different, previously
/// loaded issue (never a first load, never a same-issue re-point).
pub(crate) fn clears_merge_error(previous: Option<&str>, next: &str) -> bool {
    previous.is_some_and(|previous| previous != next)
}

/// What a held Guide file pick does against the files on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PendingPath {
    /// Nothing held, or the files have not landed yet: keep waiting.
    Hold,
    /// The files landed without that path: forget it.
    Drop,
    /// Select the file at this index.
    Select(usize),
}

/// EXP-1154 — resolve a held Guide pick. An empty file list means the files
/// are still loading (a load clears them), so the pick waits; a loaded list
/// without the path drops it, so it can never select a file on a later PR.
pub(crate) fn resolve_pending_path<'a>(
    pending: Option<&str>,
    files: impl IntoIterator<Item = &'a str>,
) -> PendingPath {
    let Some(pending) = pending else {
        return PendingPath::Hold;
    };
    let mut any = false;
    for (index, name) in files.into_iter().enumerate() {
        any = true;
        if name == pending {
            return PendingPath::Select(index);
        }
    }
    if any { PendingPath::Drop } else { PendingPath::Hold }
}

impl Focusable for PrDiffView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for PrDiffView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // The merge/close refusal captions under the work header's pill; the
        // pane only repeats it over the diff, where the reader is looking.
        let caption = self
            .issue_id
            .as_ref()
            .and_then(|id| MergeState::global(cx).read(cx).error(id));
        crate::diff_pane::render(
            crate::diff_pane::DiffPaneSpec {
                files: self.pane_files(cx),
                selected: self.selected,
                tree: true,
                filter: Some(self.filter.clone()),
                folded_dirs: self.folded_dirs.clone(),
                caption,
                diff: self.diff.clone(),
                on_pick: std::rc::Rc::new(|this: &mut Self, index, cx| {
                    this.select_file(index, cx);
                }),
                on_toggle_dir: std::rc::Rc::new(|this: &mut Self, path: String, cx| {
                    this.toggle_dir(path, cx);
                }),
            },
            window,
            cx,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_first_load_or_same_issue_repoint_keeps_the_merge_error() {
        assert!(!clears_merge_error(None, "a"));
        assert!(!clears_merge_error(Some("a"), "a"));
        assert!(clears_merge_error(Some("a"), "b"));
    }

    #[test]
    fn a_pending_path_waits_for_files_then_selects_or_drops() {
        assert_eq!(resolve_pending_path(None, ["x"]), PendingPath::Hold);
        assert_eq!(resolve_pending_path(Some("x"), []), PendingPath::Hold);
        assert_eq!(
            resolve_pending_path(Some("b"), ["a", "b"]),
            PendingPath::Select(1)
        );
        assert_eq!(resolve_pending_path(Some("z"), ["a", "b"]), PendingPath::Drop);
    }
}
