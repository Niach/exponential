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
//! The panel owns NOTHING: the files, the selection, the folded folders and
//! the `Filter files` field are all [`crate::pr_diff::PrDiffView`]'s, read
//! through the window's [`crate::screens::ScreensPanel`]; a pick scrolls
//! that view's diff exactly like a pick in its own tree did. An undocked
//! review window has no left column and keeps the tree in its pane.
//!
//! [`Shell`]: crate::shell::Shell

use gpui::{
    div, ClickEvent, Entity, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{v_flex, ActiveTheme as _};

use crate::navigation::Screen;
use crate::pr_diff::PrDiffView;

/// The review's file tree as the left column's panel.
pub struct ReviewFilesNav {
    /// The PR diff view this panel mirrors, and the repaint subscription on
    /// it. Resolved lazily: the screens panel is built after the shell's
    /// chrome, so the first render is the earliest it can be looked up.
    watched: Option<(Entity<PrDiffView>, Subscription)>,
}

impl ReviewFilesNav {
    pub fn new(_window: &mut Window, _cx: &mut gpui::Context<Self>) -> Self {
        Self { watched: None }
    }

    /// The window's shared PR diff view, observed so a fetch landing or a
    /// pick repaints this panel too.
    fn pr_diff(&mut self, window: &Window, cx: &mut gpui::Context<Self>) -> Option<Entity<PrDiffView>> {
        let panel = crate::screens::screens_for_window(window, cx)?;
        let view = panel.read(cx).pr_diff().clone();
        let stale = self
            .watched
            .as_ref()
            .is_none_or(|(watched, _)| watched != &view);
        if stale {
            let subscription = cx.observe(&view, |_, _, cx| cx.notify());
            self.watched = Some((view.clone(), subscription));
        }
        Some(view)
    }
}

impl Render for ReviewFilesNav {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // EXP-870: the same back row the ListNav and the settings nav wear,
        // aimed at the Reviews page — the one list a review belongs to.
        let back = crate::settings::nav_back_row("review-files-back", "Reviews", cx).on_click(
            cx.listener(|_, _: &ClickEvent, window, cx| {
                crate::navigation::go_back_to(window, cx, Screen::Reviews);
            }),
        );
        let tree = match self.pr_diff(window, cx) {
            Some(view) => {
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
