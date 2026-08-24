//! PR diff center screen (EXP-181): the Reviews tool window rows open this
//! instead of the issue detail — the shared side-by-side [`DiffView`] over
//! `issues.prFiles`, plus a thin header (identifier → issue detail,
//! title, `#N · branch`).
//!
//! One instance per window, re-pointed by the screens panel on tab switches
//! (the issue-detail / file-viewer model). Same-id re-points are no-ops —
//! `sync_tabs` re-fires on every navigation observer tick, and the fetch
//! must not re-run per tick; the diff is a snapshot of the PR at open time.

use std::sync::Arc;

use gpui::{
    div, prelude::FluentBuilder as _, App, AppContext as _, ClickEvent, Entity, FocusHandle,
    Focusable, InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _,
};
use sync::Store;

use crate::controls::WebControl as _;
use crate::diff::DiffView;
use crate::icons::ExpIcon;
use crate::navigation::{navigate, Screen};
use crate::queries;

/// The read-only PR diff center screen.
pub struct PrDiffView {
    focus_handle: FocusHandle,
    diff: Entity<DiffView>,
    issue_id: Option<String>,
    /// EXP-525: the diff lost its tab chip (and with it the chip's undock
    /// button), so the ScreensPanel-owned instance offers "open in new
    /// window" in its own header. Stays `false` on the instances
    /// `build_screen_content` creates — an undocked window must not offer
    /// undocking itself.
    pub(crate) show_undock: bool,
}

impl PrDiffView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            diff: cx.new(|cx| DiffView::new(window, cx)),
            issue_id: None,
            show_undock: false,
        }
    }

    /// Re-point at `issue_id` and fetch its PR files (no-op on the same id).
    pub fn set_issue(&mut self, issue_id: String, cx: &mut gpui::Context<Self>) {
        if self.issue_id.as_deref() == Some(issue_id.as_str()) {
            return;
        }
        // The screens panel drives this from its CONSTRUCTOR, which runs
        // while the session is still validating on a background thread — so a
        // cold start into a PR deep link finds no client yet. Latching an
        // error here would be terminal (same-id calls no-op, and the panel
        // only re-drives on a screen CHANGE); stay Loading and leave
        // `issue_id` unrecorded so the Synced re-drive actually re-attempts.
        let Some(client) = queries::trpc_client(cx) else {
            self.diff.update(cx, |diff, cx| diff.set_loading(cx));
            return;
        };
        self.issue_id = Some(issue_id.clone());
        self.diff
            .update(cx, |diff, cx| diff.fetch(Arc::new(client), issue_id, cx));
    }
}

impl Focusable for PrDiffView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for PrDiffView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme_colors = cx.theme();
        let muted = theme_colors.muted_foreground;
        let fg = theme_colors.foreground;
        // EXP-277: content headers use the faint glass row stroke, not the
        // heavier chrome border (fewer/softer section lines).
        let border = theme::tokens::glass::STROKE_ROW.to_hsla();
        let pr_green = theme::tokens::GREEN.to_hsla();

        // Header off the live synced issue row (identifier/title/PR fields
        // stay fresh); a deleted issue degrades to the bare diff.
        let issue = self.issue_id.as_ref().and_then(|id| {
            Store::global(cx)
                .collections()
                .issues
                .read(cx)
                .get(id)
                .cloned()
        });
        let header = issue.map(|issue| {
            let sub: String = match (issue.pr_number, issue.branch.as_deref()) {
                (Some(number), Some(branch)) => format!("#{number} \u{00B7} {branch}"),
                (Some(number), None) => format!("#{number}"),
                (None, Some(branch)) => branch.to_string(),
                (None, None) => String::new(),
            };
            let nav_id = issue.id.clone();
            h_flex()
                .w_full()
                .px_3()
                .py_1p5()
                .gap_1p5()
                .items_center()
                .border_b_1()
                .border_color(border)
                .child(
                    Icon::from(ExpIcon::GitPullRequest)
                        .xsmall()
                        .flex_shrink_0()
                        .text_color(pr_green),
                )
                // The identifier opens the issue detail — the row click no
                // longer does (it lands here), so this is the way back.
                .child(
                    div()
                        .id("pr-diff-open-issue")
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(muted)
                        .font_family(theme::terminal::FONT_FAMILY)
                        .hover(|this| this.text_color(fg))
                        .cursor_pointer()
                        .on_click(cx.listener(move |_, _, window, cx| {
                            navigate(
                                window,
                                cx,
                                Screen::IssueDetail {
                                    issue_id: nav_id.clone(),
                                },
                            );
                        }))
                        .child(SharedString::from(issue.identifier.clone())),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .truncate()
                        .text_color(fg)
                        .child(SharedString::from(issue.title.clone())),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from(sub)),
                )
                .when(self.show_undock, |row| {
                    let undock_id = issue.id.clone();
                    row.child(
                        Button::new("pr-diff-undock")
                            .ghost()
                            .web_icon_xs()
                            .icon(ExpIcon::ExternalLink)
                            .tooltip("Open in new window")
                            .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                                crate::undock::open_undocked_screen(
                                    Screen::PrDiff {
                                        issue_id: undock_id.clone(),
                                    },
                                    window.window_handle(),
                                    cx,
                                );
                                crate::navigation::set_screen(window, cx, None);
                            })),
                    )
                })
        });

        // EXP-282: no fill — the screen floats on the page gradient (the
        // dropped paint was `colors.list`, transparent since EXP-269, so this
        // is a dead-code removal, not a visual change).
        v_flex()
            .size_full()
            .children(header)
            .child(div().flex_1().min_h_0().child(self.diff.clone()))
    }
}
