//! The issue-detail PR section (masterplan-v3 §4.2 / §7.8; web reference: the
//! PR affordances of `issue-detail-view.tsx` + the `diff-view.tsx` embed).
//!
//! Renders nothing while the issue has no linked PR (one issue = one PR = one
//! `exp/<IDENTIFIER>` branch). With a PR: a compact strip — PR glyph, `#N`,
//! state, branch, an "Open on GitHub" link (`api::opener`) — plus a
//! Show/Hide-changes toggle that lazily embeds the standalone
//! [`crate::diff::DiffView`] (the P5 track's component) and fetches
//! `issues.prFiles` on first expand. The embed is deliberately thin: this
//! section owns only the strip + lazy fetch; everything diff-shaped stays in
//! `diff.rs` (the integrator may later reroute the toggle to a dedicated
//! center tab instead — the seam is `set_issue` + `DiffView::fetch`).

use std::sync::Arc;

use gpui::{
    div, px, App, AppContext as _, Entity, FontWeight, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, Icon, IconName, Sizable as _,
};
use sync::Store;

use domain::rows::Issue;

use crate::diff::DiffView;
use crate::icons::ExpIcon;
use crate::queries;

pub struct PrSection {
    issue_id: Option<String>,
    expanded: bool,
    diff: Option<Entity<DiffView>>,
    /// The issue id the current diff was fetched for (refetch on change).
    fetched_for: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl PrSection {
    pub fn new(_window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![cx.observe(&collections.issues, |_, _, cx| cx.notify())];
        Self {
            issue_id: None,
            expanded: false,
            diff: None,
            fetched_for: None,
            _subscriptions: subscriptions,
        }
    }

    /// Point the section at another issue; collapses + drops the old diff.
    pub fn set_issue(&mut self, issue_id: Option<String>, cx: &mut gpui::Context<Self>) {
        if self.issue_id == issue_id {
            return;
        }
        self.issue_id = issue_id;
        self.expanded = false;
        self.diff = None;
        self.fetched_for = None;
        cx.notify();
    }

    fn issue(&self, cx: &App) -> Option<Issue> {
        let issue_id = self.issue_id.as_deref()?;
        Store::global(cx)
            .collections()
            .issues
            .read(cx)
            .get(issue_id)
            .cloned()
    }

    fn toggle_expanded(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.expanded = !self.expanded;
        if self.expanded {
            self.ensure_diff(window, cx);
        }
        cx.notify();
    }

    /// Lazily create the embedded [`DiffView`] and fetch `issues.prFiles`
    /// once per issue (background HTTP + row build, §7.8).
    fn ensure_diff(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(issue_id) = self.issue_id.clone() else {
            return;
        };
        if self.diff.is_none() {
            self.diff = Some(cx.new(|cx| DiffView::new(window, cx)));
        }
        if self.fetched_for.as_deref() == Some(issue_id.as_str()) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        if let Some(diff) = &self.diff {
            diff.update(cx, |diff, cx| {
                diff.fetch(Arc::new(trpc), issue_id.clone(), cx);
            });
            self.fetched_for = Some(issue_id);
        }
    }
}

impl Render for PrSection {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(issue) = self.issue(cx) else {
            return div().into_any_element();
        };
        // No PR → no section (web renders the steer/PR block only when live).
        let Some(pr_url) = issue.pr_url.clone().filter(|url| !url.is_empty()) else {
            return div().into_any_element();
        };

        let state = issue.pr_state.clone().unwrap_or_else(|| "open".to_string());
        let merged = state == "merged" || issue.pr_merged_at.is_some();
        let (state_label, state_icon) = if merged {
            ("Merged", ExpIcon::GitMerge)
        } else if state == "closed" {
            ("Closed", ExpIcon::GitPullRequest)
        } else {
            ("Open", ExpIcon::GitPullRequest)
        };
        let pr_label = issue
            .pr_number
            .map(|number| format!("PR #{number}"))
            .unwrap_or_else(|| "Pull request".to_string());

        let mut strip = h_flex()
            .w_full()
            .px_4()
            .py_2()
            .gap_2()
            .items_center()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                Icon::from(state_icon)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .font_family(theme::terminal::FONT_FAMILY)
                    .child(SharedString::from(pr_label)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(state_label)),
            );

        if let Some(branch) = issue.branch.clone().filter(|branch| !branch.is_empty()) {
            strip = strip.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .font_family(theme::terminal::FONT_FAMILY)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(SharedString::from(branch)),
            );
        }

        strip = strip.child(div().flex_1()).child(
            Button::new("pr-open-github")
                .ghost()
                .xsmall()
                .icon(
                    Icon::new(IconName::ExternalLink)
                        .text_color(cx.theme().muted_foreground),
                )
                .tooltip("Open on GitHub")
                .on_click(move |_, _, _| {
                    if let Err(err) = api::opener::open_in_browser(&pr_url) {
                        log::warn!("[ui] opening PR url failed: {err}");
                    }
                }),
        );
        strip = strip.child(
            Button::new("pr-toggle-diff")
                .ghost()
                .xsmall()
                .label(if self.expanded {
                    "Hide changes"
                } else {
                    "View changes"
                })
                .on_click(cx.listener(|this, _, window, cx| this.toggle_expanded(window, cx))),
        );

        let mut section = v_flex().w_full().child(strip);
        if self.expanded {
            if let Some(diff) = &self.diff {
                section = section.child(
                    div()
                        .w_full()
                        .h(px(420.))
                        .border_t_1()
                        .border_color(cx.theme().border.opacity(0.5))
                        .child(diff.clone()),
                );
            }
        }
        section.into_any_element()
    }
}
