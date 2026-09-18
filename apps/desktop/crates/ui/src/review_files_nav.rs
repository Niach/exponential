//! EXP-916 — the window's left column beside a REVIEW: the review's file
//! tree, where every other detail keeps its context.
//!
//! EXP-870 made the left column the rail plus ONE panel slot — the list an
//! open detail was picked from ([`crate::sidebar::ListPanel`] in Nav mode),
//! or the settings nav. A review's context is not a list of reviews: it is
//! the files the pull request touches. So the review screen
//! ([`crate::pr_diff`]) hands its tree to this panel ([`Shell`]'s fourth
//! [`crate::shell::LeftOccupant`]) instead of painting it beside its own
//! column — the web twin moves `FileDiffTree` into the sidebar's panel slot
//! the same way (`ReviewFilesNav`). Back row → the Reviews page.
//!
//! EXP-945: a RUN's Changes face publishes the SAME slot. A run's diff is the
//! same kind of context a review's is — its files, not the list it was opened
//! from — so the two share this one channel and this one panel rather than the
//! run growing a floating tree of its own inside the reading column. Only the
//! back row differs, which is why the source names its own: a review goes back
//! to Reviews, a run back to its Run face.
//!
//! The panel owns NOTHING: the files, the selection, the folded folders and
//! the `Filter files` field are all the SOURCE's ([`crate::pr_diff::
//! PrDiffView`] or the run's [`crate::steer_viewer::SteerSessionView`]), read
//! through the window's [`crate::screens::ScreensPanel`]; a pick scrolls that
//! view's diff exactly like a pick in its own tree did. An undocked window has
//! no left column and keeps the tree in its pane.
//!
//! [`Shell`]: crate::shell::Shell

use gpui::{
    div, AnyElement, ClickEvent, Entity, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, WeakEntity, Window,
};
use gpui_component::{v_flex, ActiveTheme as _};

use crate::navigation::Screen;
use crate::pr_diff::PrDiffView;
use crate::session_screen::SessionScreenView;

/// EXP-945 — whose files the panel is painting. Both sources hand over the
/// same six things (files, selection, filter, folds, a pick, a fold toggle);
/// only the back row differs.
#[derive(Clone, PartialEq)]
enum FilesSource {
    /// The review screen's shared diff view (EXP-916).
    Review(Entity<PrDiffView>),
    /// A run sitting on its Changes face (EXP-945). Held WEAK: the panel
    /// mirrors the screen, it must not keep a closed one alive (release
    /// review R5); a source that no longer upgrades clears the watch.
    Run(WeakEntity<SessionScreenView>),
}

/// The review's (or the run's) file tree as the left column's panel.
pub struct ReviewFilesNav {
    /// The view this panel mirrors, and the repaint subscription on it.
    /// Resolved lazily: the screens panel is built after the shell's chrome,
    /// so the first render is the earliest it can be looked up.
    watched: Option<(FilesSource, Subscription)>,
}

impl ReviewFilesNav {
    pub fn new(_window: &mut Window, _cx: &mut gpui::Context<Self>) -> Self {
        Self { watched: None }
    }

    /// Whose tree this window's left column should paint right now, observed
    /// so a fetch landing or a pick repaints this panel too. The RUN wins when
    /// the active screen is one on its Changes face — that is exactly when the
    /// shell chose this occupant for it ([`crate::shell::window_run_diff`]).
    fn source(&mut self, window: &Window, cx: &mut gpui::Context<Self>) -> Option<FilesSource> {
        let source = self.resolve_source(window, cx);
        if source.is_none() {
            // Nothing to mirror: drop the watch (and the handle it held).
            self.watched = None;
        }
        source
    }

    fn resolve_source(
        &mut self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<FilesSource> {
        let panel = crate::screens::screens_for_window(window, cx)?;
        let run = crate::shell::window_run_diff(window, cx)
            .then(|| {
                let nav =
                    crate::navigation::nav_for_window_id(window.window_handle().window_id(), cx)?;
                match crate::navigation::resolved_screen(&nav, cx)? {
                    Screen::Session { session_id } => panel.read(cx).run_screen(&session_id),
                    _ => None,
                }
            })
            .flatten();
        let source = match run {
            Some(view) => FilesSource::Run(view.downgrade()),
            None => FilesSource::Review(panel.read(cx).pr_diff().clone()),
        };
        let stale = self
            .watched
            .as_ref()
            .is_none_or(|(watched, _)| watched != &source);
        if stale {
            let subscription = match &source {
                FilesSource::Review(view) => cx.observe(view, |_, _, cx| cx.notify()),
                // EXP-945: the run's own viewer, not the screen wrapper — the
                // selection, the filter and the folds all live there.
                FilesSource::Run(view) => {
                    let inner = view.upgrade()?.read(cx).inner().clone();
                    cx.observe(&inner, |_, _, cx| cx.notify())
                }
            };
            self.watched = Some((source.clone(), subscription));
        }
        Some(source)
    }

    /// The back row for `source`: a review returns to the Reviews queue, a run
    /// to its own Run face (the Changes face is a face, not a screen, so there
    /// is no history entry to pop).
    fn back_row(&self, source: &FilesSource, cx: &mut gpui::Context<Self>) -> AnyElement {
        match source {
            FilesSource::Review(_) => crate::settings::nav_back_row(
                "review-files-back",
                SharedString::from("Reviews"),
                cx,
            )
            .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                crate::navigation::go_back_to(window, cx, Screen::Reviews);
            }))
            .into_any_element(),
            FilesSource::Run(view) => {
                let view = view.clone();
                crate::settings::nav_back_row(
                    "review-files-back",
                    SharedString::from(crate::work_header::RUN_FACE_LABEL),
                    cx,
                )
                .on_click(move |_: &ClickEvent, _window, cx| {
                    if let Some(view) = view.upgrade() {
                        view.update(cx, |view, cx| {
                            view.set_run_face(crate::screens::RunFace::Run, cx);
                        });
                    }
                })
                .into_any_element()
            }
        }
    }
}

impl Render for ReviewFilesNav {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let source = self.source(window, cx);
        // EXP-870: the same back row the ListNav and the settings nav wear.
        let back = match source.as_ref() {
            Some(source) => self.back_row(source, cx),
            None => div().into_any_element(),
        };
        let tree = match source {
            Some(FilesSource::Review(view)) => {
                let (files, selected, filter, folded) = {
                    let view = view.read(cx);
                    (
                        view.pane_files(cx),
                        view.selected(),
                        view.filter().clone(),
                        view.folded_dirs().clone(),
                    )
                };
                let pick_view = view.clone();
                let toggle_view = view;
                crate::diff_pane::file_tree(
                    &files,
                    selected,
                    Some(&filter),
                    &folded,
                    std::rc::Rc::new(move |_: &mut Self, index, cx| {
                        pick_view.update(cx, |view, cx| view.select_file(index, cx));
                    }),
                    std::rc::Rc::new(move |_: &mut Self, path: String, cx| {
                        toggle_view.update(cx, |view, cx| view.toggle_dir(path, cx));
                    }),
                    crate::diff_pane::TreeChrome::Panel,
                    window,
                    cx,
                )
            }
            // EXP-945: the SAME tree, the same chrome, the same callbacks —
            // only the owner of the state changed.
            // A screen that is gone since `source()` ran: nothing to paint,
            // and the watch on it goes.
            Some(FilesSource::Run(screen)) if screen.upgrade().is_none() => {
                self.watched = None;
                div().into_any_element()
            }
            Some(FilesSource::Run(screen)) => {
                let screen = screen.upgrade().expect("checked by the arm above");
                let view = screen.read(cx).inner().clone();
                let (files, selected, filter, folded) = {
                    let view = view.read(cx);
                    (
                        view.diff_pane_files(),
                        view.diff_selected(),
                        view.diff_filter().clone(),
                        view.diff_folded_dirs().clone(),
                    )
                };
                let pick_view = view.clone();
                let toggle_view = view;
                crate::diff_pane::file_tree(
                    &files,
                    selected,
                    Some(&filter),
                    &folded,
                    std::rc::Rc::new(move |_: &mut Self, index, cx| {
                        pick_view.update(cx, |view, cx| view.select_diff_file(index, cx));
                    }),
                    std::rc::Rc::new(move |_: &mut Self, path: String, cx| {
                        toggle_view.update(cx, |view, cx| view.toggle_diff_dir(path, cx));
                    }),
                    crate::diff_pane::TreeChrome::Panel,
                    window,
                    cx,
                )
            }
            None => div().into_any_element(),
        };
        v_flex()
            .size_full()
            .min_w_0()
            .overflow_hidden()
            .text_color(cx.theme().sidebar_foreground)
            .child(back)
            .child(crate::settings::nav_back_rule(cx))
            .child(div().flex_1().min_h_0().w_full().min_w_0().child(tree))
    }
}
