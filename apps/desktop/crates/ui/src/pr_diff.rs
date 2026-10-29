//! A pull request's diff (EXP-181/EXP-889): the shared unified [`DiffView`]
//! over `pullRequests.files`.
//!
//! EXP-895/EXP-916: the pane IS the shared Changes layout
//! ([`crate::diff_pane`]) — the file tree beside the per-file cards, the same
//! thing Source Control renders, so every diff reads identically.
//!
//! EXP-1294: keyed on a `pull_requests` row (PR state left the issue). The
//! host is the thread's Stories tab (stories.json STORIES TAB, THE review
//! pane): the complete diff; the host draws the `‹ Stories` back row above
//! it ([`crate::story_tab`]).
//!
//! Same-id re-points are no-ops — the fetch must not re-run per render; the
//! diff is a snapshot of the PR at open time.

use gpui::{
    App, AppContext as _, Entity, FocusHandle, Focusable, IntoElement, Render, Window,
};
use gpui_component::ActiveTheme as _;

use crate::diff::DiffView;
use crate::queries;

/// The read-only PR diff behind the Stories tab's review pane.
pub struct PrDiffView {
    focus_handle: FocusHandle,
    diff: Entity<DiffView>,
    pull_request_id: Option<String>,
    /// EXP-895/EXP-916: the file tree's state — which row it highlights, the
    /// directories the reader folded, and the `Filter files` field.
    selected: usize,
    folded_dirs: std::collections::HashSet<String>,
    filter: Entity<gpui_component::input::InputState>,
    /// The pane's own painted width — [`crate::diff_pane`]'s tree gate (it
    /// writes and reads it; 0 before the first paint).
    pane_width: std::rc::Rc<std::cell::Cell<f32>>,
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
        let mut subscriptions = Vec::new();
        // The file counts come off the diff's own summaries.
        subscriptions.push(cx.observe(&diff, |_: &mut Self, _, cx| cx.notify()));
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
            pull_request_id: None,
            selected: 0,
            folded_dirs: std::collections::HashSet::new(),
            filter,
            pane_width: std::rc::Rc::new(std::cell::Cell::new(0.)),
            _subscriptions: subscriptions,
        }
    }

    /// Re-point at `pull_request_id` and fetch its files
    /// (`pullRequests.files`); a no-op on the same id.
    pub fn set_pull_request(&mut self, pull_request_id: String, cx: &mut gpui::Context<Self>) {
        if self.pull_request_id.as_deref() == Some(pull_request_id.as_str()) {
            return;
        }
        // A cold start can render this before the session validated: stay
        // Loading and leave the id unrecorded so the next call re-attempts.
        let Some(client) = queries::trpc_client(cx) else {
            self.diff.update(cx, |diff, cx| diff.set_loading(cx));
            return;
        };
        self.pull_request_id = Some(pull_request_id.clone());
        self.selected = 0;
        self.diff.update(cx, |diff, cx| diff.set_loading(cx));
        let theme = cx.theme().highlight_theme.clone();
        let diff = self.diff.clone();
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::sessions::pr_files(&client, &pull_request_id).map(|files| {
                        crate::diff::build_diff(
                            &diff_files(&files),
                            &theme,
                            crate::diff::DiffOptions::review(),
                        )
                    })
                })
                .await;
            let _ = diff.update(cx, |diff, cx| match result {
                Ok(prepared) => diff.set_prepared(prepared, cx),
                // Fetched on open (the Guide's counts): quiet, the empty
                // list says it.
                Err(err) => diff.set_error(err.to_string(), false, cx),
            });
        })
        .detach();
    }

    /// Name `index` in the file list and scroll the diff to it.
    pub(crate) fn select_file(&mut self, index: usize, cx: &mut gpui::Context<Self>) {
        self.selected = index;
        self.diff
            .update(cx, |diff, cx| diff.scroll_to_file(index, cx));
        cx.notify();
    }

    /// Fold `path` in the file tree, or open it again.
    pub(crate) fn toggle_dir(&mut self, path: String, cx: &mut gpui::Context<Self>) {
        if !self.folded_dirs.insert(path.clone()) {
            self.folded_dirs.remove(&path);
        }
        cx.notify();
    }

    /// EVERY file of the PR (the Changes row's counts, the review's tree).
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
                .with_previous_path(file.previous_path.as_deref())
            })
            .collect()
    }
}

/// `pullRequests.files` rows → the shared diff model (the wire carries no
/// status or previous path: every file reads as modified).
pub(crate) fn diff_files(files: &[api::sessions::PrFile]) -> Vec<domain::diff::DiffFile> {
    files
        .iter()
        .map(|file| {
            domain::diff::from_pull_file(
                &file.path,
                None,
                "modified",
                file.additions,
                file.deletions,
                file.patch.as_deref(),
            )
        })
        .collect()
}

impl Focusable for PrDiffView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for PrDiffView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let files = self.pane_files(cx);
        let selected = self.selected;
        crate::diff_pane::render(
            crate::diff_pane::DiffPaneSpec {
                files,
                selected,
                filter: Some(self.filter.clone()),
                folded_dirs: self.folded_dirs.clone(),
                diff: self.diff.clone(),
                pane_width: self.pane_width.clone(),
                on_pick: std::rc::Rc::new(|this: &mut Self, index, cx| this.select_file(index, cx)),
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

    /// EXP-1294: `pullRequests.files` rows map onto the shared model, path
    /// and counts kept.
    #[test]
    fn pull_request_files_map_onto_the_diff_model() {
        let files = diff_files(&[api::sessions::PrFile {
            path: "src/a.rs".into(),
            additions: 2,
            deletions: 1,
            patch: Some("@@ -1,1 +1,2 @@\n-a\n+b\n+c".into()),
        }]);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/a.rs");
        assert_eq!(files[0].status, domain::diff::DiffStatus::Modified);
    }
}
