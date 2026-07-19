//! The sidebar Source Control tool window's FLOW view — the branch graph
//! (default branch → `exp/batch-*` batch branches → issue branches, drawn as
//! a connected tree with PR-state tones) that replaced both the flat branch
//! list and the center screen's flow strip. Lanes come from
//! [`crate::flow_lanes`] joined over the shared [`GitBar`]'s live branch
//! list and the synced issue rows; per-lane ↑/↓ counts fill in from
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
    build_lanes, connectors, lane_indicator, FlowModel, IssueLite, Lane, LaneIndicator, LaneKind,
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
    /// branch → (ahead, behind) vs origin, filled by the background pass.
    counts: HashMap<String, (u32, u32)>,
    /// branch → uncommitted-changes probe of its worktree (the trunk clone
    /// for the checked-out lane) — feeds the yellow in-progress indicator.
    dirty: HashMap<String, bool>,
    /// The lane set (branch, current, worktree) `counts`/`dirty` belong to —
    /// a changed set refires the pass exactly once (render-time rebuilds
    /// stay cheap and git-free). The flags are part of the key: a checkout
    /// or a worktree add/remove moves the dirty-probe target even when the
    /// branch names are unchanged.
    counts_for: Vec<(String, bool, bool)>,
    /// A delete/sweep op is in flight (the lane + sweep buttons disable).
    busy: bool,
    error: Option<SharedString>,
    /// The last sweep's summary line (EXP-93) — informational, muted.
    notice: Option<SharedString>,
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
            // the next auto-sync tick. Also re-render directly: the EXP-102
            // sweep/delete guards read the live-session set.
            cx.observe(&local_sessions, |this: &mut Self, _, cx| {
                this.git_bar.update(cx, |bar, cx| bar.reread_local(cx));
                cx.notify();
            }),
            // The view-branch highlight lives on the rail state.
            cx.observe(&shared, |_, _, cx| cx.notify()),
            // The lane joins read synced rows.
            cx.observe(&collections.issues, |_, _, cx| cx.notify()),
            // Team/board switches change the join scope.
            cx.observe(&nav, |_, _, cx| cx.notify()),
        ];
        Self {
            nav,
            git_bar,
            counts: HashMap::new(),
            dirty: HashMap::new(),
            counts_for: Vec::new(),
            busy: false,
            error: None,
            notice: None,
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
        let issues = self.join_rows(cx);
        build_lanes(&branches, &default_branch, &branch_prefix, &issues)
    }

    /// Snapshot the synced rows the lane join reads (active team).
    fn join_rows(&self, cx: &App) -> Vec<IssueLite> {
        let Some(team_id) = navigation::active_team_id(&self.nav, cx) else {
            return Vec::new();
        };
        Store::global(cx)
            .collections()
            .issues_in_team(&team_id, cx)
            .into_iter()
            .map(|issue| IssueLite {
                id: issue.id,
                identifier: issue.identifier,
                title: issue.title,
                branch: issue.branch,
                pr_state: issue.pr_state,
            })
            .collect()
    }

    /// Refire the background ↑/↓ + dirty pass when the VISIBLE lane set
    /// changed (bounded git cost — never per render). The same pass probes
    /// each lane's worktree (the trunk clone for the checked-out lane) for
    /// uncommitted changes — the EXP-67 in-progress indicator's input.
    fn ensure_counts(&mut self, flow: &FlowModel, cx: &mut gpui::Context<Self>) {
        let Some(clone) = self.git_bar.read(cx).clone_dir() else {
            return;
        };
        let mut wanted: Vec<(String, bool, bool)> = flow
            .lanes
            .iter()
            .map(|lane| (lane.branch.clone(), lane.current, lane.worktree))
            .collect();
        wanted.sort();
        wanted.dedup();
        if wanted == self.counts_for {
            return;
        }
        self.counts_for = wanted.clone();
        self.generation += 1;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let (counts, dirty) = cx
                .background_executor()
                .spawn(async move {
                    let mut counts = HashMap::new();
                    let mut dirty = HashMap::new();
                    for (branch, current, worktree) in wanted {
                        if let Ok(pair) = clone_manager::ahead_behind(&clone, &branch) {
                            counts.insert(branch.clone(), pair);
                        }
                        // Where this lane's working tree lives: the trunk
                        // clone when checked out there, else its session
                        // worktree (the deterministic launch path).
                        let tree = if current {
                            Some(clone.clone())
                        } else if worktree {
                            Some(coding::worktree_path(&clone, &branch))
                        } else {
                            None
                        };
                        if let Some(tree) = tree {
                            if let Ok(status) = scm::status(&tree) {
                                dirty.insert(branch, !status.changes.is_empty());
                            }
                        }
                    }
                    (counts, dirty)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.counts = counts;
                this.dirty = dirty;
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
                     uncommitted changes and unpushed commits. Branches on \
                     origin are untouched.",
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
        // EXP-102: re-check at confirm time — a session could have spawned
        // onto this lane while the dialog sat open, and removing its
        // worktree would pull the running claude PTY's cwd out from under it.
        if crate::coding_flow::LocalSessions::global(cx)
            .read(cx)
            .is_branch_live(&branch)
        {
            self.error = Some(
                format!("{branch} has a running coding session — stop it before deleting.").into(),
            );
            cx.notify();
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

    /// The sweep's targets (EXP-93): every merged lane that isn't checked out
    /// — the default lane never carries a merged PR tone, and the current
    /// branch can't lose its working tree. Lanes hosting a LIVE local coding
    /// session are excluded (EXP-102): a merged-but-still-running lane must
    /// not lose the session's cwd.
    pub fn sweep_candidates(&self, cx: &mut gpui::Context<Self>) -> Vec<String> {
        let flow = self.build_flow(cx);
        let sessions = crate::coding_flow::LocalSessions::global(cx);
        let sessions = sessions.read(cx);
        flow.lanes
            .iter()
            .filter(|lane| {
                matches!(lane.pr, crate::flow_lanes::PrTone::Merged)
                    && !lane.current
                    && !matches!(lane.kind, LaneKind::Default)
                    && !sessions.is_branch_live(&lane.branch)
            })
            .map(|lane| lane.branch.clone())
            .collect()
    }

    /// A delete/sweep op is in flight (the header sweep button disables).
    pub fn is_busy(&self) -> bool {
        self.busy
    }

    /// Confirm-then-sweep for ALL merged lanes (EXP-93, the header broom):
    /// each merged branch loses its worktree and local branch, but — unlike
    /// the per-lane delete — a worktree with uncommitted changes is skipped
    /// and reported, never force-removed (a bulk action mustn't eat work).
    pub fn prompt_sweep_merged(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.busy {
            return;
        }
        let branches = self.sweep_candidates(cx);
        if branches.is_empty() {
            return;
        }
        let view = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _window, _cx| {
            let view = view.clone();
            let branches_for_ok = branches.clone();
            let noun = if branches.len() == 1 { "branch" } else { "branches" };
            alert
                .confirm()
                .overlay_closable(true)
                .close_button(true)
                .width(px(440.))
                .title(SharedString::from(format!(
                    "Sweep {} merged {noun}?",
                    branches.len()
                )))
                .description(
                    "Deletes the local branch and worktree of every merged lane. \
                     A lane with uncommitted changes or unpushed commits is \
                     skipped — nothing is force-removed. Branches on origin are \
                     untouched.",
                )
                .button_props(DialogButtonProps::default().ok_text("Sweep"))
                .on_ok(move |_, _, cx| {
                    if let Some(view) = view.upgrade() {
                        let branches = branches_for_ok.clone();
                        view.update(cx, |view, cx| view.sweep_merged(branches, cx));
                    }
                    true
                })
        });
    }

    /// The confirm path: [`scm::sweep_branches`] off the foreground, then a
    /// git-bar refresh; the removed/skipped summary lands in `notice`.
    fn sweep_merged(&mut self, branches: Vec<String>, cx: &mut gpui::Context<Self>) {
        if self.busy {
            return;
        }
        // EXP-102: same confirm-time re-check as the per-lane delete — drop
        // any lane a session spawned onto while the dialog sat open.
        let branches: Vec<String> = {
            let sessions = crate::coding_flow::LocalSessions::global(cx);
            let sessions = sessions.read(cx);
            branches
                .into_iter()
                .filter(|branch| !sessions.is_branch_live(branch))
                .collect()
        };
        if branches.is_empty() {
            return;
        }
        let Some(clone) = self.git_bar.read(cx).clone_dir() else {
            return;
        };
        self.busy = true;
        self.error = None;
        self.notice = None;
        cx.notify();
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { scm::sweep_branches(&clone, &branches) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                if this.generation == generation {
                    // Invalidate the counts cache — the lane set changed.
                    this.counts_for.clear();
                }
                this.notice = Some(format_sweep_result(&result));
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
        session_live: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let rail_color = muted.opacity(0.35);
        let (green, yellow, red) = (theme.green, theme.yellow, theme.red);
        let foreground = theme.foreground;
        let accent = theme.accent;
        let radius = theme.radius;

        // EXP-67 indicator: merged = green check, local work without a PR =
        // yellow in-progress, default = nothing; open/closed keep their dots.
        let indicator = lane_indicator(
            lane.kind,
            lane.pr,
            self.counts.get(&lane.branch).map(|(ahead, _)| *ahead),
            self.dirty.get(&lane.branch).copied().unwrap_or(false),
        );
        let label_color = match lane.kind {
            LaneKind::Other => muted,
            _ => foreground,
        };
        let branch = lane.branch.clone();
        let current = lane.current;
        let delete_branch = lane.branch.clone();
        // Never the trunk's default lane or the checked-out branch —
        // everything else (stale issue/batch/other lanes) can go.
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
                // Clicking VIEWS the branch's history in the changes screen —
                // never a checkout (that's the top-bar chip). Issue lanes too
                // (EXP-179): the issue-detail Changes tab is gone (web parity,
                // EXP-157) — Source Control is the one diff surface.
                crate::sidebar::set_view_branch(
                    window,
                    cx,
                    if current { None } else { Some(branch.clone()) },
                );
                navigation::navigate(window, cx, Screen::SourceControl);
            }))
            .child(gutter)
            .child({
                // Fixed slot so dot lanes and icon lanes keep their labels
                // aligned (and the indicator-less default lane too).
                let slot = div()
                    .w(px(12.))
                    .h(px(ROW_H))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center();
                let dot = |color: gpui::Hsla| div().size_1p5().rounded_full().bg(color);
                match indicator {
                    LaneIndicator::None => slot,
                    LaneIndicator::MergedCheck => slot.child(
                        Icon::from(crate::icons::ExpIcon::CircleCheck)
                            .xsmall()
                            .text_color(green),
                    ),
                    LaneIndicator::Progress => slot.child(
                        Icon::from(crate::icons::ExpIcon::Timer).xsmall().text_color(yellow),
                    ),
                    LaneIndicator::OpenDot => slot.child(dot(green)),
                    LaneIndicator::ClosedDot => slot.child(dot(red)),
                    LaneIndicator::IdleDot => slot.child(dot(muted.opacity(0.5))),
                }
            })
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
                        // EXP-102: a lane with a live local coding session
                        // must keep its worktree — the session's cwd.
                        .tooltip(if session_live {
                            "A coding session is running on this branch — stop it first"
                        } else {
                            "Delete branch and its worktree…"
                        })
                        .disabled(self.busy || session_live)
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
                "No repository connected to this board."
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
        // Highlight follows the ACTIVE center tab (EXP-67): an issue lane is
        // selected while its issue detail is the active screen OR while the
        // Source Control screen views its branch (lane clicks land there
        // since EXP-179); branch lanes highlight on the view-branch path
        // alone (falling back to the checked-out lane).
        let active_screen = navigation::resolved_screen(&self.nav, cx);
        let active_issue = match &active_screen {
            Some(Screen::IssueDetail { issue_id }) => Some(issue_id.clone()),
            _ => None,
        };
        let sc_active = matches!(active_screen, Some(Screen::SourceControl));
        let view_branch = crate::sidebar::rail_shared_for_window(window, cx)
            .read(cx)
            .view_branch()
            .map(str::to_string);

        let local_sessions = crate::coding_flow::LocalSessions::global(cx);

        let mut section = v_flex().w_full().p_1().px_2();
        for (lane, connector) in flow.lanes.iter().zip(&lane_connectors) {
            let session_live = local_sessions.read(cx).is_branch_live(&lane.branch);
            let branch_viewed = sc_active
                && match &view_branch {
                    Some(viewed) => *viewed == lane.branch,
                    None => lane.current,
                };
            let viewing = branch_viewed
                || lane
                    .issue_id
                    .as_deref()
                    .is_some_and(|issue_id| active_issue.as_deref() == Some(issue_id));
            section = section.child(self.render_lane(lane, connector, viewing, session_live, cx));
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
        if let Some(notice) = &self.notice {
            section = section.child(
                div()
                    .px_1()
                    .py_0p5()
                    .text_xs()
                    .text_color(muted)
                    .child(notice.clone()),
            );
        }
        section.into_any_element()
    }
}

/// The sweep's summary line (`Swept 3 branches. Skipped exp/EXP-2
/// (uncommitted changes).`).
fn format_sweep_result(result: &scm::SweepResult) -> SharedString {
    let mut parts = Vec::new();
    if result.removed > 0 {
        let noun = if result.removed == 1 { "branch" } else { "branches" };
        parts.push(format!("Swept {} {noun}.", result.removed));
    }
    if !result.skipped.is_empty() {
        let detail = result
            .skipped
            .iter()
            .map(|(branch, reason)| format!("{branch} ({reason})"))
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!("Skipped {detail}."));
    }
    if parts.is_empty() {
        return "Nothing to sweep.".into();
    }
    parts.join(" ").into()
}
