//! The sidebar Source Control tool window's FLOW view — the branch graph
//! (default branch → `exp/rel-*` releases → issue branches, drawn as a
//! connected tree with PR-state tones) that replaced both the flat branch
//! list and the center screen's flow strip. Lanes come from
//! [`crate::flow_lanes`] joined over the shared [`GitBar`]'s live branch
//! list and the synced issue/release rows; per-lane ↑/↓ counts fill in from
//! a background pass keyed on the visible branch set. Clicking a lane views
//! that branch's history in the changes screen (issue lanes open the issue);
//! hover reveals a per-lane delete (forced worktree removal + `branch -D`,
//! alert-confirmed) — the stale-branch cleanup path.

use std::collections::HashMap;

use gpui::{
    div, prelude::FluentBuilder as _, px, App, ClickEvent, Entity, InteractiveElement as _,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    dialog::DialogButtonProps,
    h_flex, v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
    WindowExt as _,
};
use sync::Store;

use coding::{clone_manager, scm};

use crate::coding_flow::CodingHub;
use crate::flow_lanes::{
    build_lanes, connectors, FlowModel, IssueLite, Lane, LaneKind, PrTone, ReleaseLite,
};
use crate::git_bar::GitBar;
use crate::navigation::{self, Navigation, Screen};

/// Fixed graph row height — the connector geometry positions against it.
const ROW_H: f32 = 24.;
/// One gutter column per tree depth.
const COL_W: f32 = 14.;
/// The rails' x inside a gutter column (dot-centered).
const RAIL_X: f32 = 6.;
/// Lane hover group — reveals the per-lane delete button.
const FLOW_GROUP: &str = "flow-lane";

pub struct FlowView {
    nav: Entity<Navigation>,
    git_bar: Entity<GitBar>,
    /// "+N more" toggle.
    expanded: bool,
    /// branch → (ahead, behind) vs origin, filled by the background pass.
    counts: HashMap<String, (u32, u32)>,
    /// The sorted branch set `counts` belongs to — a changed set refires the
    /// pass exactly once (render-time rebuilds stay cheap and git-free).
    counts_for: Vec<String>,
    /// A delete op is in flight (the lane buttons disable).
    busy: bool,
    error: Option<SharedString>,
    /// Stale-pass guard for counts + delete completions.
    generation: u64,
    _subscriptions: Vec<Subscription>,
}

impl FlowView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = navigation::nav_for_window(window, cx);
        let shared = crate::sidebar::rail_shared_for_window(window, cx);
        let git_bar = shared.read(cx).git_bar().clone();
        let collections = Store::global(cx).collections().clone();
        let local_sessions = crate::coding_flow::LocalSessions::global(cx);
        let subscriptions = vec![
            // Branch list + trunk state ride the shared git bar.
            cx.observe(&git_bar, |_, _, cx| cx.notify()),
            // A session spawn just created a worktree + branch on disk (and
            // an exit may precede a cleanup) — nudge a LOCAL branch re-read
            // so the graph gains the lane immediately instead of waiting for
            // the next auto-sync tick.
            cx.observe(&local_sessions, |this: &mut Self, _, cx| {
                this.git_bar.update(cx, |bar, cx| bar.reread_local(cx));
            }),
            // The view-branch highlight lives on the rail state.
            cx.observe(&shared, |_, _, cx| cx.notify()),
            // The lane joins read synced rows.
            cx.observe(&collections.issues, |_, _, cx| cx.notify()),
            cx.observe(&collections.releases, |_, _, cx| cx.notify()),
            // Workspace/project switches change the join scope.
            cx.observe(&nav, |_, _, cx| cx.notify()),
        ];
        Self {
            nav,
            git_bar,
            expanded: false,
            counts: HashMap::new(),
            counts_for: Vec::new(),
            busy: false,
            error: None,
            generation: 0,
            _subscriptions: subscriptions,
        }
    }

    /// Build the lane model from the git bar's current branch list + synced
    /// rows — pure and cheap, recomputed every render.
    fn build_flow(&self, cx: &mut gpui::Context<Self>) -> FlowModel {
        let bar = self.git_bar.read(cx);
        let branches = bar.branches().to_vec();
        let default_branch = bar
            .default_branch()
            .or_else(|| {
                Some(bar.branch().to_string()).filter(|branch| !branch.is_empty())
            })
            .unwrap_or_default();
        if default_branch.is_empty() {
            return FlowModel::default();
        }
        let branch_prefix = CodingHub::global(cx).read(cx).settings.branch_prefix.clone();
        let (issues, releases) = self.join_rows(cx);
        build_lanes(
            &branches,
            &default_branch,
            &branch_prefix,
            &issues,
            &releases,
            self.expanded,
        )
    }

    /// Snapshot the synced rows the lane join reads (active workspace).
    fn join_rows(&self, cx: &App) -> (Vec<IssueLite>, Vec<ReleaseLite>) {
        let Some(workspace_id) = navigation::active_workspace_id(&self.nav, cx) else {
            return (Vec::new(), Vec::new());
        };
        let collections = Store::global(cx).collections();
        let issues = collections
            .issues_in_workspace(&workspace_id, cx)
            .into_iter()
            .map(|issue| IssueLite {
                id: issue.id,
                identifier: issue.identifier,
                title: issue.title,
                branch: issue.branch,
                pr_state: issue.pr_state,
                release_id: issue.release_id,
            })
            .collect();
        let releases = collections
            .releases
            .read(cx)
            .iter()
            .filter(|release| release.workspace_id.as_deref() == Some(workspace_id.as_str()))
            .map(|release| ReleaseLite {
                id: release.id.clone(),
                name: release.name.clone().unwrap_or_default(),
                pr_state: release.pr_state.clone(),
            })
            .collect();
        (issues, releases)
    }

    /// Refire the background ↑/↓ pass when the VISIBLE branch set changed
    /// (bounded git cost — never per render).
    fn ensure_counts(&mut self, flow: &FlowModel, cx: &mut gpui::Context<Self>) {
        let Some(clone) = self.git_bar.read(cx).clone_dir() else {
            return;
        };
        let mut wanted: Vec<String> = flow.lanes.iter().map(|lane| lane.branch.clone()).collect();
        wanted.sort();
        wanted.dedup();
        if wanted == self.counts_for {
            return;
        }
        self.counts_for = wanted.clone();
        self.generation += 1;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let counts = cx
                .background_executor()
                .spawn(async move {
                    let mut counts = HashMap::new();
                    for branch in wanted {
                        if let Ok(pair) = clone_manager::ahead_behind(&clone, &branch) {
                            counts.insert(branch, pair);
                        }
                    }
                    counts
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.counts = counts;
                cx.notify();
            });
        })
        .detach();
    }

    /// Confirm-then-delete for a lane: the local branch AND (forced) the
    /// session worktree holding it — the stale-branch cleanup path.
    /// Current/default lanes never offer it; origin is never touched.
    fn prompt_delete_lane(
        &mut self,
        branch: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _window, _cx| {
            let view = view.clone();
            let branch_for_ok = branch.clone();
            alert
                .confirm()
                // Dismissable like any dialog: overlay click, Esc, and the ✕
                // all cancel (AlertDialog's default locks all three off).
                .overlay_closable(true)
                .close_button(true)
                .width(px(440.))
                .title(SharedString::from(format!("Delete {branch}?")))
                .description(
                    "Deletes the local branch and its worktree — including any \
                     uncommitted changes in that worktree. Branches on origin \
                     are untouched.",
                )
                .button_props(DialogButtonProps::default().ok_text("Delete branch"))
                .on_ok(move |_, _, cx| {
                    if let Some(view) = view.upgrade() {
                        let branch = branch_for_ok.clone();
                        view.update(cx, |view, cx| view.delete_lane(branch, cx));
                    }
                    true
                })
        });
    }

    /// The confirm path: forced worktree removal (when the branch lives in
    /// one) + `git branch -D`, then a git-bar refresh (the flow rebuilds off
    /// its next branch read).
    fn delete_lane(&mut self, branch: String, cx: &mut gpui::Context<Self>) {
        if self.busy {
            return;
        }
        let Some(clone) = self.git_bar.read(cx).clone_dir() else {
            return;
        };
        self.busy = true;
        self.error = None;
        cx.notify();
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { scm::delete_branch_and_worktree(&clone, &branch) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                if this.generation == generation {
                    // Invalidate the counts cache — the lane set changed.
                    this.counts_for.clear();
                }
                if let Err(err) = result {
                    this.error = Some(format!("{err}").into());
                }
                this.git_bar.update(cx, |bar, cx| bar.refresh(cx));
                cx.notify();
            });
        })
        .detach();
    }

    /// One lane row: connector gutter, PR-tone dot, label, ↑/↓, worktree
    /// tag, current ✓, hover delete.
    fn render_lane(
        &self,
        lane: &Lane,
        connector: &crate::flow_lanes::LaneConnector,
        viewing: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let rail_color = muted.opacity(0.35);
        let (green, blue, red) = (theme.green, theme.blue, theme.red);
        let foreground = theme.foreground;
        let accent = theme.accent;
        let radius = theme.radius;

        let tone = match lane.pr {
            PrTone::Open => green,
            PrTone::Merged => blue,
            PrTone::Closed => red,
            PrTone::None => muted.opacity(0.5),
        };
        let label_color = match lane.kind {
            LaneKind::Other => muted,
            _ => foreground,
        };
        let branch = lane.branch.clone();
        let issue_id = lane.issue_id.clone();
        let current = lane.current;
        let delete_branch = lane.branch.clone();
        // Never the trunk's default lane or the checked-out branch —
        // everything else (stale issue/release/other lanes) can go.
        let deletable = !lane.current && !matches!(lane.kind, LaneKind::Default);

        // The connector gutter: pass-through rails for the ancestor columns,
        // then this lane's ├/└ elbow feeding the dot.
        let mut gutter = h_flex().flex_shrink_0().h(px(ROW_H));
        for rail in &connector.rails {
            let mut cell = div().w(px(COL_W)).h(px(ROW_H)).relative().flex_shrink_0();
            if *rail {
                cell = cell.child(
                    div()
                        .absolute()
                        .left(px(RAIL_X))
                        .top_0()
                        .h(px(ROW_H))
                        .w(px(1.))
                        .bg(rail_color),
                );
            }
            gutter = gutter.child(cell);
        }
        if let Some(last) = connector.elbow {
            gutter = gutter.child(
                div()
                    .w(px(COL_W))
                    .h(px(ROW_H))
                    .relative()
                    .flex_shrink_0()
                    .child(
                        div()
                            .absolute()
                            .left(px(RAIL_X))
                            .top_0()
                            .h(px(if last { ROW_H / 2. } else { ROW_H }))
                            .w(px(1.))
                            .bg(rail_color),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(RAIL_X))
                            .top(px(ROW_H / 2.))
                            .w(px(COL_W - RAIL_X))
                            .h(px(1.))
                            .bg(rail_color),
                    ),
            );
        }

        let mut row = h_flex()
            .id(SharedString::from(format!("flow-lane-{}", lane.branch)))
            .group(FLOW_GROUP)
            .w_full()
            .h(px(ROW_H))
            .items_center()
            .gap_1p5()
            .px_1()
            .rounded(radius)
            .when(viewing, |this| this.bg(accent.opacity(0.4)))
            .when(!viewing, |this| {
                this.hover(|style| style.bg(accent.opacity(0.25)))
            })
            .cursor_pointer()
            .on_click(cx.listener(move |_, _, window, cx| {
                if let Some(issue_id) = issue_id.clone() {
                    navigation::navigate(window, cx, Screen::IssueDetail { issue_id });
                } else {
                    // Clicking VIEWS the branch's history in the changes
                    // screen — never a checkout (that's the top-bar chip).
                    crate::sidebar::set_view_branch(
                        window,
                        cx,
                        if current { None } else { Some(branch.clone()) },
                    );
                    navigation::navigate(window, cx, Screen::SourceControl);
                }
            }))
            .child(gutter)
            .child(div().size_1p5().flex_shrink_0().rounded_full().bg(tone))
            .child(
                // The label IS the flexible element (`flex_1`, never a
                // separate spacer): inside the sidebar's scroll container
                // the row width is content-derived, and a zero-basis spacer
                // lets a truncatable label collapse to its "…" min-content.
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .truncate()
                    .when(matches!(lane.kind, LaneKind::Default), |this| {
                        this.font_weight(gpui::FontWeight::MEDIUM)
                    })
                    .text_color(label_color)
                    .child(SharedString::from(lane.label.clone())),
            );
        let mut counts = String::new();
        if let Some((ahead, behind)) = self.counts.get(&lane.branch) {
            if *ahead > 0 {
                counts.push_str(&format!("\u{2191}{ahead}"));
            }
            if *behind > 0 {
                if !counts.is_empty() {
                    counts.push(' ');
                }
                counts.push_str(&format!("\u{2193}{behind}"));
            }
        }
        if !counts.is_empty() {
            row = row.child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(counts)),
            );
        }
        if lane.worktree {
            row = row.child(
                div().flex_shrink_0().text_xs().text_color(muted).child("worktree"),
            );
        }
        if lane.current {
            row = row.child(
                Icon::from(crate::icons::ExpIcon::Check).xsmall().text_color(muted),
            );
        }
        if deletable {
            row = row.child(
                div()
                    .flex_shrink_0()
                    .invisible()
                    .group_hover(FLOW_GROUP, |style| style.visible())
                    .child(
                        Button::new(SharedString::from(format!(
                            "flow-delete-{}",
                            lane.branch
                        )))
                        .ghost()
                        .xsmall()
                        .icon(Icon::new(IconName::Delete))
                        .tooltip("Delete branch and its worktree…")
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.prompt_delete_lane(delete_branch.clone(), window, cx);
                        })),
                    ),
            );
        }
        row.into_any_element()
    }
}

impl Render for FlowView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        let flow = self.build_flow(cx);
        if flow.lanes.is_empty() {
            let note = if self.git_bar.read(cx).branches_ready() {
                "No repository connected to this project."
            } else {
                "Loading branches…"
            };
            return v_flex()
                .p_2()
                .child(div().px_1().py_1().text_xs().text_color(muted).child(note))
                .into_any_element();
        }
        self.ensure_counts(&flow, cx);
        let lane_connectors = connectors(&flow.lanes);
        // The rail's view-branch selection highlights the viewed lane (the
        // checked-out lane while nothing is explicitly viewed).
        let view_branch = crate::sidebar::rail_shared_for_window(window, cx)
            .read(cx)
            .view_branch()
            .map(str::to_string);

        let mut section = v_flex().w_full().p_1().px_2();
        for (lane, connector) in flow.lanes.iter().zip(&lane_connectors) {
            let viewing = match &view_branch {
                Some(viewed) => *viewed == lane.branch,
                None => lane.current,
            };
            section = section.child(self.render_lane(lane, connector, viewing, cx));
        }
        if flow.hidden > 0 || self.expanded {
            let label = if self.expanded {
                "Show fewer".to_string()
            } else {
                format!("+{} more", flow.hidden)
            };
            section = section.child(
                Button::new("flow-toggle")
                    .ghost()
                    .xsmall()
                    .label(SharedString::from(label))
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.expanded = !this.expanded;
                        cx.notify();
                    })),
            );
        }
        if let Some(error) = &self.error {
            section = section.child(
                div()
                    .px_1()
                    .py_0p5()
                    .text_xs()
                    .text_color(danger)
                    .child(error.clone()),
            );
        }
        section.into_any_element()
    }
}
