//! One workflow's detail (EXP-981): the graph and the picked node's panel.
//!
//! The geometry is the SERVER's — `workflow_nodes.wave`/`lane`/`on_cycle`
//! come off the wire and this view only turns them into pixels, through the
//! same [`crate::issue_graph::grid_view`] the EXP-980 blocks mini-graph
//! draws. The edges are `domain::workflow_view::workflow_edges` over the
//! synced `blocks` relations; the captions, the shape line and the cycle
//! note are that module's too. Nothing here decides an order, a position or
//! a word.
//!
//! EXP-982 adds the RUN: Start/Pause/Resume/Cancel by status, the node
//! states painted by tone AND glyph, the merge-train strip under the graph
//! and the final-PR node after the last wave. The engine itself is
//! `crate::workflow_host`; nothing on this page decides anything — every
//! button is one `workflows.*` call and the server owns the rule.
//!
//! EXP-1014 — the screen CONFIGURES nothing: the launch is two models
//! (`coding::workflows::launch`), the gate is the agent review, `start_on` is
//! fixed, so the whole "How it runs" block is gone and only the runner
//! DEVICE (which a draft cannot start without) survives, in the header. A
//! node is the app's own ISSUE CHIP (`crate::issue_chip`) — a compound one a
//! DECK of them — with the state caption trailing inside it; the graph
//! SCALES into the width it is given rather than scrolling inside a fixed
//! viewport.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    canvas, div, px, App, AppContext as _, ClickEvent, Entity, InteractiveElement as _,
    IntoElement, ParentElement, Pixels, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
};
use sync::Store;

use coding::workflows::launch::{model_for_node, normalize_workflow_launch, WorkflowLaunch};
use domain::workflow_view::{
    workflow_cycle_note, workflow_edge_style, workflow_final_pr_caption, workflow_merge_train,
    workflow_metric_rows, workflow_node_caption, workflow_node_state_label, workflow_node_title,
    workflow_node_tone, workflow_review_line, workflow_shape_line, workflow_start_blocker,
    workflow_train_step_label, CaptionNode, EdgeNode, EdgeRelation, ReviewLine, StartableWorkflow,
    TrainNode, WorkflowNodeTone, ADMIT_NODE_LABEL, AGENT_REVIEW_TITLE, APPROVE_NODE_LABEL,
    CANCEL_WORKFLOW_CONFIRM, CANCEL_WORKFLOW_LABEL, CONTRACT_PUBLISHED_LABEL,
    DELETE_WORKFLOW_LABEL, DISMISS_NODE_LABEL, FINAL_PR_TITLE, MERGES_IN_FIRST_LABEL,
    MERGE_FINAL_PR_CONFIRM, MERGE_FINAL_PR_LABEL, MERGE_TRAIN_EMPTY, MERGE_TRAIN_TITLE,
    METRICS_TITLE, NODE_MODEL_LABEL, NODE_UNSYNCED_TITLE, PAUSE_WORKFLOW_LABEL,
    PLAN_WORKFLOW_LABEL, PROPOSED_NODE_NOTE, RESUME_WORKFLOW_LABEL, RETRY_NODE_LABEL,
    RUNNING_NOW_LABEL, SKIP_NODE_CONFIRM, SKIP_NODE_LABEL, START_WORKFLOW_LABEL,
    WITHDRAW_APPROVAL_LABEL,
};

use crate::actions_view::page_scaffold_with;
use crate::icons::registry;
use crate::issue_graph::{grid_view, GridEdge, GridGeometry, GridNode};
use crate::navigation::{nav_for_window, ChatSeed, Navigation, Screen};
use crate::queries;

/// The page column's cap — wide enough for the graph beside its panel.
const WORKFLOW_COLUMN_W: f32 = 1024.;
/// The picked node's panel, and the air between it and the graph.
const NODE_PANEL_W: f32 = 280.;
const NODE_PANEL_GAP: f32 = 16.;
/// The page's own gutter (`page_scaffold_with`'s `px_4`, both sides).
const PAGE_GUTTER: f32 = 32.;

/// ONE node = the app's issue chip in a fixed box: the glyph slot, the mono
/// identifier, as much title as fits and the state caption trailing inside
/// it. Nothing is drawn UNDER a node.
const NODE_W: f32 = 200.;
const NODE_H: f32 = 26.;
/// The gap an edge crosses between two waves, and the one between two lanes.
const COL_GAP: f32 = 56.;
const LANE_GAP: f32 = 12.;
/// How far the graph may shrink before it stops (a chip narrower than this
/// says nothing at all, so the page's own column scrolls instead).
const MIN_GRAPH_SCALE: f32 = 0.5;

/// The natural width of a graph `waves + 1` columns wide — what the fill rule
/// scales DOWN from.
fn natural_graph_width(waves: usize) -> f32 {
    waves as f32 * (NODE_W + COL_GAP) + NODE_W
}

/// EXP-1014 — the FILL rule: the graph never scrolls inside itself and is
/// never clipped, so it draws at the fraction of its natural width that fits
/// `available`. Never above 1 (a two-node workflow is not stretched across
/// the page) and never below [`MIN_GRAPH_SCALE`].
fn graph_scale(waves: usize, available: f32) -> f32 {
    let natural = natural_graph_width(waves);
    if available <= 0. || natural <= 0. || natural <= available {
        return 1.;
    }
    (available / natural).max(MIN_GRAPH_SCALE)
}

/// EXP-1014 — the laid-out width the grid is finally given: never wider than
/// the cell it sits in, so a plan the floor scale cannot fit scrolls INSIDE
/// its own cell instead of painting over the node panel beside it. The clamp
/// is unconditional; its own floor is one chip ([`NODE_W`]), which is what a
/// cell narrower than that (a phone-width window with the panel open, an
/// unmeasured cell) gets.
fn clamped_view_w(view_w: f32, available: f32) -> f32 {
    view_w.min(available.max(NODE_W))
}

/// The grid at `scale`: the column pitch, the lane pitch and the box width
/// shrink together; the chip's own font (and so its height) never does — a
/// title simply truncates harder. `view_*` is the laid-out size itself, so
/// [`grid_view`] clips nothing and scrolls nowhere.
fn graph_geometry(scale: f32, waves: usize, lanes: usize) -> GridGeometry {
    let node_w = NODE_W * scale;
    GridGeometry {
        node_w,
        node_h: NODE_H,
        col_gap: COL_GAP * scale,
        lane_gap: LANE_GAP * scale,
        view_w: waves as f32 * (node_w + COL_GAP * scale) + node_w,
        view_h: lanes as f32 * (NODE_H + LANE_GAP * scale) + NODE_H,
        // Chip to chip: out of the right middle, into the left middle.
        edge_out: (node_w, NODE_H / 2.),
        edge_in: (0., NODE_H / 2.),
    }
}

/// The grid key of the final-PR box (EXP-982) — deliberately not a uuid, so
/// it can never collide with a `workflow_nodes` row id.
const FINAL_PR_KEY: &str = "workflow-final-pr";

/// EXP-984: how many lines of a review's findings show before the fold.
const FINDINGS_PREVIEW_LINES: usize = 4;

pub struct WorkflowView {
    #[allow(dead_code)] // held for the team-switch re-render subscription
    nav: Entity<Navigation>,
    /// The workflow this view is pointed at (`set_workflow`).
    workflow_id: String,
    /// The picked node's `workflow_nodes` row id; `None` = nothing picked.
    picked: Option<String>,
    name_input: Entity<InputState>,
    /// The name last pushed into the input, so a remote rename repaints it
    /// while a local edit in flight does not bounce.
    name_seeded: String,
    /// EXP-984: whether the agent review's findings are unfolded.
    findings_expanded: bool,
    /// EXP-1014: the width the graph's cell actually got, read back at
    /// prepaint — the fill rule scales into it. `0` on the first frame (the
    /// window's own width stands in until the probe lands).
    graph_width: Rc<Cell<Pixels>>,
    /// The width the last frame scaled for, so a resize re-renders ONCE.
    scaled_for: f32,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl WorkflowView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Workflow name"));
        let mut subscriptions = vec![cx.subscribe_in(
            &name_input,
            window,
            |this, _, event: &InputEvent, _window, cx| {
                // The name saves on blur when changed, and Enter commits —
                // the issue title's rule.
                if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    this.save_name(cx);
                }
            },
        )];
        let nav = nav_for_window(window, cx);
        subscriptions.push(cx.observe(&nav, |_, _, cx| cx.notify()));
        if let Some(store) = Store::try_global(cx) {
            let collections = store.collections().clone();
            // The graph joins all four: the workflow, its nodes, the issues
            // the nodes represent and the `blocks` rows between them.
            subscriptions.push(cx.observe_in(
                &collections.workflows,
                window,
                |this, _, window, cx| {
                    this.sync_name(window, cx);
                    cx.notify();
                },
            ));
            subscriptions.push(cx.observe(&collections.workflow_nodes, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&collections.issues, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&collections.issue_relations, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&collections.devices, |_, _, cx| cx.notify()));
        }
        Self {
            nav,
            workflow_id: String::new(),
            picked: None,
            name_input,
            name_seeded: String::new(),
            findings_expanded: false,
            graph_width: Rc::new(Cell::new(px(0.))),
            scaled_for: 0.,
            scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    /// Re-point at another workflow (the tab activation path).
    pub(crate) fn set_workflow(
        &mut self,
        workflow_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.workflow_id == workflow_id {
            return;
        }
        self.workflow_id = workflow_id.to_string();
        self.picked = None;
        self.findings_expanded = false;
        // Swap the name UNCONDITIONALLY on a workflow switch, or the next
        // blur would write the previous workflow's name onto this one.
        self.name_seeded = String::new();
        self.sync_name(window, cx);
        cx.notify();
    }

    fn row(&self, cx: &App) -> Option<domain::rows::WorkflowRow> {
        Store::try_global(cx)?
            .collections()
            .workflows
            .read(cx)
            .get(&self.workflow_id)
            .cloned()
    }

    /// Mirror a synced rename into the input, unless the person is editing.
    fn sync_name(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(name) = self.row(cx).and_then(|row| row.name) else {
            return;
        };
        if name == self.name_seeded {
            return;
        }
        self.name_seeded = name.clone();
        self.name_input
            .update(cx, |input, cx| input.set_value(name, window, cx));
    }

    fn save_name(&mut self, cx: &mut gpui::Context<Self>) {
        let name = self.name_input.read(cx).value().trim().to_string();
        // A blank name is a mis-edit, not a rename: the server's schema
        // refuses it anyway, so the input simply keeps the old one.
        if name.is_empty() || name == self.name_seeded {
            return;
        }
        self.name_seeded = name.clone();
        let mut input = api::workflows::WorkflowUpdate::new(self.workflow_id.clone());
        input.name = Some(name);
        spawn_update(input, cx);
    }

    /// The nodes in the server's layout order, with the issue each one
    /// represents joined in (`None` while that issue has not synced).
    fn nodes(&self, cx: &App) -> Vec<domain::rows::WorkflowNodeRow> {
        queries::workflow_nodes(cx, &self.workflow_id)
    }

    /// The edges between the nodes, from the synced `blocks` relations.
    fn edges(
        &self,
        nodes: &[domain::rows::WorkflowNodeRow],
        row: &domain::rows::WorkflowRow,
        cx: &App,
    ) -> Vec<GridEdge> {
        let Some(store) = Store::try_global(cx) else {
            return Vec::new();
        };
        let members: Vec<Vec<String>> = nodes.iter().map(|node| node.member_ids()).collect();
        let after: Vec<Vec<String>> = nodes.iter().map(|node| node.after_ids()).collect();
        let edge_nodes: Vec<EdgeNode<'_>> = nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| {
                Some(EdgeNode {
                    id: node.id.as_str(),
                    issue_id: node.issue_id.as_deref()?,
                    member_issue_ids: members[index].iter().map(String::as_str).collect(),
                    // EXP-983: the engine's serialization edges join the
                    // `blocks` ones, drawn dashed.
                    after_node_ids: after[index].iter().map(String::as_str).collect(),
                })
            })
            .collect();
        let relations = store.collections().issue_relations.read(cx);
        let edge_relations: Vec<EdgeRelation<'_>> = relations
            .iter()
            .map(|relation| EdgeRelation {
                kind: relation.kind.as_deref().unwrap_or_default(),
                issue_id: &relation.issue_id,
                related_issue_id: &relation.related_issue_id,
            })
            .collect();
        // EXP-983: the line SAYS something — green once the blocker landed,
        // red when upstream moved under a dependent, dashed while a
        // dependent is running on work that has not landed.
        let state_of: HashMap<&str, &str> = nodes
            .iter()
            .map(|node| (node.id.as_str(), node.state_wire()))
            .collect();
        domain::workflow_view::workflow_edges(&edge_nodes, &edge_relations, &row.cycle_edges())
            .into_iter()
            .map(|edge| GridEdge {
                style: workflow_edge_style(
                    &edge,
                    state_of.get(edge.from.as_str()).copied().unwrap_or_default(),
                    state_of.get(edge.to.as_str()).copied().unwrap_or_default(),
                ),
                from: edge.from,
                to: edge.to,
            })
            .collect()
    }

    /// The graph, or a note while the workflow has no nodes yet.
    fn render_graph(
        &self,
        row: &domain::rows::WorkflowRow,
        nodes: &[domain::rows::WorkflowNodeRow],
        available: f32,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        if nodes.is_empty() {
            return div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("This workflow has no issues yet.")
                .into_any_element();
        }
        let edges = self.edges(nodes, row, cx);
        let mut grid_nodes: Vec<GridNode> = nodes
            .iter()
            .map(|node| GridNode {
                key: node.id.clone(),
                wave: node.wave_index(),
                lane: node.lane_index(),
            })
            .collect();
        // Everything a box needs, keyed by node id — the render closure gets
        // only a `&App`, so the joins happen here.
        let mut facts: HashMap<String, NodeFacts> = nodes
            .iter()
            .map(|node| (node.id.clone(), NodeFacts::derive(node, row, cx)))
            .collect();
        // EXP-982: the final pull request, ONE extra box after the last wave
        // once every node is in. It is not a `workflow_nodes` row, so it
        // carries no edges and cannot be picked.
        let states: Vec<&str> = nodes.iter().map(|node| node.state_wire()).collect();
        let final_caption = workflow_final_pr_caption(
            &states,
            row.final_pr_state.as_deref(),
            row.final_pr_number,
        );
        let final_url = row.final_pr_url.clone();
        if let Some(caption) = final_caption {
            let wave = nodes.iter().map(|node| node.wave_index()).max().unwrap_or(0) + 1;
            grid_nodes.push(GridNode {
                key: FINAL_PR_KEY.to_string(),
                wave,
                lane: 0,
            });
            facts.insert(FINAL_PR_KEY.to_string(), NodeFacts::final_pr(&caption, row));
        }
        let picked = self.picked.clone();
        let view = cx.entity().downgrade();
        let on_pick: Rc<dyn Fn(&str, &mut App)> = Rc::new(move |node_id: &str, cx: &mut App| {
            let Some(view) = view.upgrade() else {
                return;
            };
            let node_id = node_id.to_string();
            // The final-PR box is not a node: clicking it opens the PR.
            if node_id == FINAL_PR_KEY {
                if let Some(url) = final_url.clone() {
                    cx.open_url(&url);
                }
                return;
            }
            view.update(cx, |this, cx| {
                // A second click on the open node closes the panel.
                this.picked = (this.picked.as_deref() != Some(node_id.as_str())).then_some(node_id);
                cx.notify();
            });
        });
        let render = |node: &GridNode, cycled: bool, cx: &App| {
            let facts = facts.get(&node.key);
            let selected = picked.as_deref() == Some(node.key.as_str());
            render_node_box(&node.key, facts, cycled, selected, on_pick.clone(), cx)
        };
        let mut notes: Vec<SharedString> = Vec::new();
        if let Some(note) = workflow_cycle_note(&row.shape()) {
            notes.push(SharedString::from(note));
        }
        // EXP-1014: the graph fills the width it was given — scaled down
        // when the plan is wider than the page, never clipped, never a
        // scroller of its own.
        let waves = grid_nodes.iter().map(|node| node.wave).max().unwrap_or(0);
        let lanes = grid_nodes.iter().map(|node| node.lane).max().unwrap_or(0);
        let mut geometry = graph_geometry(graph_scale(waves, available), waves, lanes);
        // The one case the fill rule cannot fill: a plan so deep that even
        // the floor scale overruns the cell. The grid then scrolls inside
        // it rather than painting over the panel beside it.
        geometry.view_w = clamped_view_w(geometry.view_w, available);
        let grid = grid_view(&grid_nodes, &edges, geometry, &notes, &render, cx);
        // The runs that are up right now, one tap away: a node's chip says
        // THAT it runs, this strip is the way in.
        let mut running: Vec<(&domain::rows::WorkflowNodeRow, &NodeFacts)> = nodes
            .iter()
            .filter_map(|node| {
                let facts = facts.get(&node.id)?;
                facts.run.as_ref().filter(|run| run.live)?;
                Some((node, facts))
            })
            .collect();
        running.sort_by_key(|(node, _)| (node.wave_index(), node.lane_index()));
        if running.is_empty() {
            return grid.into_any_element();
        }
        let muted = cx.theme().muted_foreground;
        v_flex()
            .min_w_0()
            .gap_3()
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .flex_wrap()
                    .items_center()
                    .gap_1p5()
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(SharedString::from(RUNNING_NOW_LABEL)),
                    )
                    .children(running.into_iter().filter_map(|(node, facts)| {
                        let run = facts.run.clone()?;
                        Some(run_pill(&node.id, &facts.identifier, run, cx))
                    })),
            )
            .child(grid)
            .into_any_element()
    }

    /// The picked node's panel, and EXACTLY this (EXP-1014): the issue chip,
    /// Kind / Risk / Model, the node's state with the actions that state
    /// earns, the faces strip and the latest agent review. The node's
    /// `touches` globs are the planner's bookkeeping and stay off it (web
    /// too), and so do the plan's own bookkeeping lines.
    fn render_node_panel(
        &mut self,
        row: &domain::rows::WorkflowRow,
        nodes: &[domain::rows::WorkflowNodeRow],
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let picked = self.picked.clone()?;
        let node = nodes.iter().find(|node| node.id == picked)?.clone();
        let issue_id = node.issue_id.clone()?;
        let muted = cx.theme().muted_foreground;
        let workflow_id = self.workflow_id.clone();
        let draft = row.status_wire() == domain::contract::WF_STATUS_DRAFT;
        // EXP-984: a node nobody admitted yet is decided on, not run.
        let proposed = node.state_wire() == domain::contract::WF_NODE_STATE_PROPOSED;
        // EXP-1014: what this node's run spawns on, read from the ONE rule
        // both hosts launch by — a LINE, not a pick: the workflow's two
        // models are its own, the node only picks which of them it earns.
        let launch = workflow_launch(row);
        let model = model_label(
            &launch,
            &model_for_node(&launch, node.kind_wire(), node.risk_wire()),
        );
        // The node's state, and the run that is up on it.
        let state = workflow_node_state_label(node.state_wire());
        let state_color = tone_color(workflow_node_tone(node.state_wire()), cx);
        let run = NodeRun::derive(&node, cx).filter(|run| run.live);

        Some(
            v_flex()
                .w(px(NODE_PANEL_W))
                .flex_shrink_0()
                .min_w_0()
                .gap_3()
                // The badge IS the way into the issue.
                .child(issue_chip_link(&issue_id, cx))
                // EXP-984: a node filed mid-run that nobody admitted yet —
                // what it is, and the two decisions a member can take.
                .when(proposed, |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(SharedString::from(PROPOSED_NODE_NOTE)),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("workflow-node-admit")
                                    .primary()
                                    .small()
                                    .icon(Icon::from(registry::UI_CHECK))
                                    .label(ADMIT_NODE_LABEL)
                                    .on_click({
                                        let node_id = node.id.clone();
                                        move |_, _window, cx| {
                                            spawn_admit(node_id.clone(), true, cx)
                                        }
                                    }),
                            )
                            .child(
                                Button::new("workflow-node-dismiss")
                                    .ghost()
                                    .small()
                                    .label(DISMISS_NODE_LABEL)
                                    .on_click({
                                        let node_id = node.id.clone();
                                        move |_, _window, cx| {
                                            spawn_admit(node_id.clone(), false, cx)
                                        }
                                    }),
                            ),
                    )
                })
                .child(crate::surface::glass_group_rows(vec![
                    pick_row(
                        "workflow-node-kind",
                        "Kind",
                        &WF_KIND_CHOICES,
                        node.kind_wire(),
                        node_pick_enabled(NodePick::Kind, draft),
                        {
                            let workflow_id = workflow_id.clone();
                            let issue_id = issue_id.clone();
                            move |value: &str, cx: &mut App| {
                                let mut input = api::workflows::WorkflowNodeUpdate::new(
                                    workflow_id.clone(),
                                    issue_id.clone(),
                                );
                                input.kind = Some(value.to_string());
                                spawn_update_node(input, cx);
                            }
                        },
                        cx,
                    ),
                    pick_row(
                        "workflow-node-risk",
                        "Risk",
                        &WF_RISK_CHOICES,
                        node.risk_wire(),
                        node_pick_enabled(NodePick::Risk, draft),
                        {
                            let workflow_id = workflow_id.clone();
                            let issue_id = issue_id.clone();
                            move |value: &str, cx: &mut App| {
                                let mut input = api::workflows::WorkflowNodeUpdate::new(
                                    workflow_id.clone(),
                                    issue_id.clone(),
                                );
                                input.risk = Some(value.to_string());
                                spawn_update_node(input, cx);
                            }
                        },
                        cx,
                    ),
                    // EXP-1014: read-only — `coding::workflows::launch` is
                    // the ONE place a model comes from, and the workflow's
                    // own two are fixed at create.
                    crate::surface::glass_picker_row(
                        NODE_MODEL_LABEL,
                        None,
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(SharedString::from(model))
                            .into_any_element(),
                        cx,
                    ),
                ]))
                // EXP-982: where the node stands, why it is failed or
                // waiting in the sentence the engine reported, and the run
                // that is up on it. EXP-984: a proposal has no state worth
                // reading — it is admitted or dismissed, nothing else.
                .when(!proposed, |this| {
                    this.child(
                        v_flex()
                            .min_w_0()
                            .gap_1p5()
                            .child(
                                h_flex()
                                    .min_w_0()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(state_color)
                                            .child(SharedString::from(state)),
                                    )
                                    .children(run.map(|run| {
                                        run_pill(&node.id, &issue_identifier(&issue_id, cx), run, cx)
                                    })),
                            )
                            .when_some(node.note.clone(), |this, note| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(SharedString::from(note)),
                                )
                            })
                            .map(|this| {
                                let actions = node_gate_actions(&node);
                                if actions.is_empty() {
                                    this
                                } else {
                                    this.child(h_flex().gap_2().children(actions))
                                }
                            }),
                    )
                })
                // The three facts the panel is the only place for (web and
                // Android say them too): what a COMPOUND node covers, the
                // moment this one published its contract — its dependents
                // may have been building on it since — and the siblings it
                // has to merge in before it can land, which is the only
                // explanation a node waiting on work it does not depend on
                // has.
                .children(node_members_block(&node, cx))
                .children(checkpoint_line(&node, cx))
                .children(merges_in_first_block(&node, nodes, cx))
                // EXP-1024: the node's faces — Issue · Run · Changes.
                .children((!proposed).then(|| node_face_strip(&node, &issue_id, cx)).flatten())
                // EXP-984: the latest AGENT review — its verdict line in the
                // tone it earned, the findings themselves, and the command
                // the reviewer actually ran.
                .children(self.render_agent_review(&node, cx))
                .into_any_element(),
        )
    }

    /// EXP-984 — the `Agent review` block, or nothing while no review was
    /// submitted: the verdict line in its tone, the findings (folded once
    /// they run long) and the oracle command in mono.
    fn render_agent_review(
        &self,
        node: &domain::rows::WorkflowNodeRow,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let review = node.review_facts()?;
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let tone = if review.verdict == domain::contract::WF_REVIEW_VERDICT_APPROVE {
            theme.success
        } else {
            theme.danger
        };
        let line = workflow_review_line(ReviewLine {
            verdict: &review.verdict,
            round: review.round,
            oracle: review.oracle_passed,
            approved: node.approved_at.is_some(),
        });
        let findings = review.findings.trim().to_string();
        let lines = findings.lines().count();
        let folds = lines > FINDINGS_PREVIEW_LINES;
        let shown = if folds && !self.findings_expanded {
            findings
                .lines()
                .take(FINDINGS_PREVIEW_LINES)
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            findings.clone()
        };
        let expanded = self.findings_expanded;
        let view = cx.entity().downgrade();
        Some(
            v_flex()
                .min_w_0()
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from(AGENT_REVIEW_TITLE)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(tone)
                        .child(SharedString::from(line)),
                )
                .when(!shown.is_empty(), |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(theme.foreground)
                            .child(SharedString::from(shown)),
                    )
                })
                .when(folds, |this| {
                    this.child(
                        crate::controls::text_button(
                            "workflow-node-findings-fold",
                            if expanded { "Show less" } else { "Show more" },
                            crate::controls::TextButtonVariant::Text,
                            cx,
                        )
                        .on_click(move |_, _window, cx| {
                            let Some(view) = view.upgrade() else {
                                return;
                            };
                            view.update(cx, |this, cx| {
                                this.findings_expanded = !this.findings_expanded;
                                cx.notify();
                            });
                        }),
                    )
                })
                .when_some(review.oracle_command.clone(), |this, command| {
                    this.child(
                        div()
                            .font_family("monospace")
                            .text_xs()
                            .text_color(muted)
                            .child(SharedString::from(command)),
                    )
                })
                .into_any_element(),
        )
    }
}

impl Render for WorkflowView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(row) = self.row(cx) else {
            // The row has not synced (or was deleted elsewhere) — the list
            // is where a person recovers from that.
            return page_scaffold_with(
                "workflow-screen-scroll",
                &self.scroll,
                v_flex().child(crate::controls::empty_state(
                    Icon::from(registry::NAV_WORKFLOWS),
                    "Workflow not found",
                    "It may have been deleted, or it has not synced to this machine yet.",
                    cx,
                )),
                WORKFLOW_COLUMN_W,
            );
        };
        let theme = cx.theme();
        let (muted, danger) = (theme.muted_foreground, theme.danger);
        let nodes = self.nodes(cx);
        let shape = row.shape();
        let cycle_note = workflow_cycle_note(&shape);
        let workflow_id = self.workflow_id.clone();
        let delete_id = workflow_id.clone();
        let delete_view = cx.entity().downgrade();
        let status = row.status_wire().to_string();
        let draft = status == domain::contract::WF_STATUS_DRAFT;
        let live = matches!(status.as_str(), "running" | "paused");
        // Only a draft shows the Start blocker; a started workflow's "already
        // started" sentence would be noise.
        let start_caption = draft
            .then(|| start_blocker(&row, &shape))
            .flatten()
            // The cycle note already says this one, above and in red.
            .filter(|note| cycle_note.as_deref() != Some(note.as_str()));

        let header = v_flex()
            .min_w_0()
            .gap_1()
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(crate::controls::glass_input(&self.name_input, _window, cx)),
                    )
                    // EXP-1014: the ONE thing this screen still configures —
                    // a draft cannot start without a machine to run on. It
                    // is fixed the moment the workflow leaves draft.
                    .child(device_control(
                        &workflow_id,
                        row.device_id.as_deref(),
                        queries::launch_devices(cx),
                        draft,
                        cx,
                    ))
                    // EXP-982: Start/Pause/Resume, by status. A draft that
                    // cannot start says why in the caption below, and the
                    // server refuses with the same sentence.
                    .children(status_actions(&row, &shape, cx))
                    // EXP-981: Plan seeds the Agent composer with the hidden
                    // planner builtin AND this workflow — the run is
                    // meaningless without the id. Draft only: a started
                    // workflow's plan is fixed.
                    .when(draft, |this| {
                        this.child(
                            Button::new("workflow-plan")
                                .primary()
                                .small()
                                .icon(Icon::from(registry::ACTION_RUN))
                                .label(PLAN_WORKFLOW_LABEL)
                                .on_click(move |_, window, cx| {
                                    crate::navigation::navigate_to_chat(
                                        window,
                                        cx,
                                        ChatSeed::plan_workflow(&workflow_id),
                                    );
                                }),
                        )
                    })
                    .child(
                        crate::controls::ghost_icon_button(
                            "workflow-menu",
                            Icon::from(registry::UI_MORE),
                            cx,
                        )
                        .disabled(live)
                        .dropdown_menu(move |menu, _window, _cx| {
                            let delete_view = delete_view.clone();
                            let delete_id = delete_id.clone();
                            menu.item(
                                crate::controls::danger_menu_item(
                                    DELETE_WORKFLOW_LABEL,
                                    Icon::from(registry::UI_DELETE),
                                    _cx,
                                )
                                .on_click(move |_, window, cx| {
                                    let Some(view) = delete_view.upgrade() else {
                                        return;
                                    };
                                    let id = delete_id.clone();
                                    view.update(cx, |_, cx| {
                                        prompt_delete(id, window, cx);
                                    });
                                }),
                            )
                        }),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(workflow_shape_line(&shape))),
            )
            .when_some(cycle_note, |this, note| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(danger)
                        .child(SharedString::from(note)),
                )
            })
            // EXP-982: why Start is disabled, verbatim. The cycle note IS
            // one of those reasons, so it is never said twice.
            .when_some(start_caption, |this, note| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from(note)),
                )
            });

        let panel = self.render_node_panel(&row, &nodes, cx);
        // EXP-1014: the graph fills the cell it is given. The probe below
        // reports that cell's width back for the NEXT frame; the first one
        // (and any frame before the probe lands) reads the window instead,
        // so the graph is never laid out against a zero.
        let measured = f32::from(self.graph_width.get());
        let available = if measured > 1. {
            measured
        } else {
            fallback_graph_width(_window, cx, panel.is_some())
        };
        self.scaled_for = available;
        let graph = self.render_graph(&row, &nodes, available, cx);
        let probe = self.graph_width.clone();
        let scaled_for = self.scaled_for;
        let view = cx.entity().downgrade();
        let body = h_flex()
            .w_full()
            .min_w_0()
            .items_start()
            .gap(px(NODE_PANEL_GAP))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .child(graph)
                    .child(
                        // The width probe (the list's canvas idiom): record
                        // the cell at prepaint and re-render ONCE when it
                        // moved. The notify is DEFERRED — this view is
                        // leased while its own tree prepaints.
                        canvas(
                            move |bounds, _, cx| {
                                probe.set(bounds.size.width);
                                if (f32::from(bounds.size.width) - scaled_for).abs() > 0.5 {
                                    let view = view.clone();
                                    cx.defer(move |cx| {
                                        if let Some(view) = view.upgrade() {
                                            view.update(cx, |_, cx| cx.notify());
                                        }
                                    });
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    ),
            )
            .children(panel);

        let train = (!draft).then(|| render_merge_train(&nodes, cx));
        // EXP-984: the run's counters. A draft has run nothing to count.
        let metrics = (!draft).then(|| render_metrics(&row, cx)).flatten();

        page_scaffold_with(
            "workflow-screen-scroll",
            &self.scroll,
            v_flex()
                .gap_6()
                .child(header)
                .child(body)
                .children(train)
                .children(metrics),
            WORKFLOW_COLUMN_W,
        )
    }
}

/// The width the graph may use before its own cell has been measured: the
/// page's column (capped, gutters off) minus the picked node's panel. The
/// WINDOW is not the page — the rail (with whatever list is folded beside
/// it) and the shell's cutout never belong to the graph — so the first frame
/// of every open would otherwise lay out far too wide and visibly re-scale
/// the moment the probe lands.
fn fallback_graph_width(window: &Window, cx: &App, panel: bool) -> f32 {
    graph_cell_width(
        f32::from(window.viewport_size().width),
        crate::shell::window_left_column_width(window, cx),
        panel,
    )
}

/// [`fallback_graph_width`]'s arithmetic, as a pure function: the window
/// minus the left column and the shell's own cutout margins, capped at the
/// page column, minus the page gutter and the picked node's panel.
fn graph_cell_width(viewport_w: f32, left_column_w: f32, panel: bool) -> f32 {
    let shell = viewport_w - left_column_w - 2. * crate::shell::PANEL_MARGIN;
    let column = shell.min(WORKFLOW_COLUMN_W) - PAGE_GUTTER;
    let taken = if panel { NODE_PANEL_W + NODE_PANEL_GAP } else { 0. };
    (column - taken).max(NODE_W)
}

/// A COMPOUND node's sub-issues, as chips: the node runs them as ONE batch,
/// and the graph's `EXP-14 +3` only counts them. A member whose row has not
/// synced here is left out rather than drawn as a uuid (web's rule).
fn node_members_block(
    node: &domain::rows::WorkflowNodeRow,
    cx: &App,
) -> Option<gpui::AnyElement> {
    let members: Vec<gpui::AnyElement> = node
        .member_ids()
        .into_iter()
        .filter(|issue_id| crate::issue_chip::synced_issue_status(issue_id, cx).is_some())
        .map(|issue_id| issue_chip_for(&issue_id, cx))
        .collect();
    if members.is_empty() {
        return None;
    }
    Some(
        h_flex()
            .min_w_0()
            .flex_wrap()
            .gap_1p5()
            .children(members)
            .into_any_element(),
    )
}

/// EXP-983 — the node announced its CONTRACT, so its dependents may already
/// be building on it: when that happened, in the app's usual relative words.
fn checkpoint_line(node: &domain::rows::WorkflowNodeRow, cx: &App) -> Option<gpui::AnyElement> {
    let at = node.checkpoint_at.as_deref()?;
    let when = crate::comments::relative_time(at, chrono::Utc::now().timestamp());
    if when.is_empty() {
        return None;
    }
    Some(
        div()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(SharedString::from(format!("{CONTRACT_PUBLISHED_LABEL} {when}")))
            .into_any_element(),
    )
}

/// EXP-983 — the nodes this one merges in FIRST: the engine wrote them after
/// its work collided with theirs, and the graph draws those edges dashed.
/// Named by their issues, `EXP-14 +3` for a compound one; a node whose issue
/// has not synced is left out rather than drawn as a uuid.
fn merges_in_first_block(
    node: &domain::rows::WorkflowNodeRow,
    nodes: &[domain::rows::WorkflowNodeRow],
    cx: &App,
) -> Option<gpui::AnyElement> {
    let chips: Vec<gpui::AnyElement> = node
        .after_ids()
        .into_iter()
        .filter_map(|id| nodes.iter().find(|candidate| candidate.id == id))
        .filter_map(|candidate| node_chip(candidate, cx))
        .collect();
    if chips.is_empty() {
        return None;
    }
    Some(
        v_flex()
            .min_w_0()
            .gap_1p5()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(MERGES_IN_FIRST_LABEL)),
            )
            .child(h_flex().min_w_0().flex_wrap().gap_1p5().children(chips))
            .into_any_element(),
    )
}

/// One NODE as a chip — its representative issue, named the way the graph
/// names it (`EXP-14 +3` for a compound node). `None` while that issue has
/// not synced here.
fn node_chip(node: &domain::rows::WorkflowNodeRow, cx: &App) -> Option<gpui::AnyElement> {
    let issue_id = node.issue_id.as_deref()?;
    let issue = Store::try_global(cx)?
        .collections()
        .issues
        .read(cx)
        .get(issue_id)
        .cloned()?;
    let mut chip = crate::issue_chip::issue_chip(
        SharedString::from(format!("workflow-node-chip-{}", node.id)),
        workflow_node_title(&issue.identifier, node.member_ids().len()),
        issue.title.clone(),
    )
    .flexible();
    if let Some(status) = crate::issue_chip::synced_issue_status(issue_id, cx) {
        chip = chip.status(status);
    }
    Some(chip.into_any_element())
}

/// EXP-1002 — the node's FACES, as the app's own segmented capsule: Issue ·
/// Run · Changes, the web's `availableFaces` rule verbatim, so a node with no
/// run shows no Run and one with no pull request shows no Changes. Nothing is
/// active: the reader is on the graph, so every segment is a way out of it.
fn node_face_strip(
    node: &domain::rows::WorkflowNodeRow,
    issue_id: &str,
    cx: &App,
) -> Option<gpui::AnyElement> {
    let has_pr = Store::try_global(cx)
        .and_then(|store| store.collections().issues.read(cx).get(issue_id).cloned())
        .is_some_and(|issue| issue.pr_url.is_some());

    let mut segments: Vec<gpui::AnyElement> = Vec::new();
    let issue_target = issue_id.to_string();
    segments.push(
        face_segment(
            "workflow-node-face-issue",
            registry::UI_ISSUE,
            crate::work_header::ISSUE_FACE_LABEL,
            cx,
        )
            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                crate::navigation::navigate(
                    window,
                    cx,
                    Screen::IssueDetail {
                        issue_id: issue_target.clone(),
                    },
                );
            })
            .into_any_element(),
    );
    if let Some(session_id) = node.session_id.clone() {
        segments.push(
            face_segment(
                "workflow-node-face-run",
                registry::NAV_DEVICES,
                crate::work_header::RUN_FACE_LABEL,
                cx,
            )
            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                crate::navigation::navigate(
                    window,
                    cx,
                    Screen::Session {
                        session_id: session_id.clone(),
                    },
                );
            })
            .into_any_element(),
        );
    }
    if has_pr {
        let issue_target = issue_id.to_string();
        segments.push(
            face_segment(
                "workflow-node-face-changes",
                registry::CODING_DIFF,
                crate::work_header::CHANGES_FACE_LABEL,
                cx,
            )
            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                crate::navigation::navigate(
                    window,
                    cx,
                    Screen::PrDiff {
                        issue_id: issue_target.clone(),
                    },
                );
            })
            .into_any_element(),
        );
    }
    // One lone segment is a button wearing a capsule: the strip earns its
    // chrome only once there is a choice in it.
    (segments.len() > 1).then(|| {
        crate::controls::segmented(cx)
            .id("workflow-node-faces")
            .children(segments)
            .into_any_element()
    })
}

/// One segment of [`node_face_strip`] — never active, since the panel is not
/// one of the faces it points at.
fn face_segment(
    id: &'static str,
    icon: crate::icons::ExpIcon,
    label: &'static str,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    crate::controls::segmented_item(false, cx)
        .id(id)
        .child(Icon::from(icon).size_4())
        .child(SharedString::from(label))
}

/// The by-hand approval and the failure exits: approve (or take it back) while the
/// PR is up, retry or skip once the node failed. Every rule is the server's;
/// these buttons only appear where it would say yes.
fn node_gate_actions(node: &domain::rows::WorkflowNodeRow) -> Vec<gpui::AnyElement> {
    let state = node.state_wire();
    let approved = node.approved_at.is_some();
    let node_id = node.id.clone();
    let mut actions: Vec<gpui::AnyElement> = Vec::new();
    if state == "in_review" && !approved {
        actions.push(
            Button::new("workflow-node-approve")
                .primary()
                .small()
                .icon(Icon::from(registry::UI_CHECK))
                .label(APPROVE_NODE_LABEL)
                .on_click({
                    let node_id = node_id.clone();
                    move |_, _window, cx| spawn_approve(node_id.clone(), true, cx)
                })
                .into_any_element(),
        );
    }
    if approved && state != "landed" {
        actions.push(
            Button::new("workflow-node-withdraw")
                .ghost()
                .small()
                .label(WITHDRAW_APPROVAL_LABEL)
                .on_click({
                    let node_id = node_id.clone();
                    move |_, _window, cx| spawn_approve(node_id.clone(), false, cx)
                })
                .into_any_element(),
        );
    }
    // A budget pause (EXP-984) is resolved the same way as a failure.
    if state == "failed" || state == "paused" {
        actions.push(
            Button::new("workflow-node-retry")
                .small()
                .label(RETRY_NODE_LABEL)
                .on_click({
                    let node_id = node_id.clone();
                    move |_, _window, cx| spawn_resolve(node_id.clone(), "retry", cx)
                })
                .into_any_element(),
        );
        actions.push(
            Button::new("workflow-node-skip")
                .danger()
                .small()
                .label(SKIP_NODE_LABEL)
                .on_click(move |_, window, cx| {
                    let node_id = node_id.clone();
                    crate::native_dialog::open_alert(
                        window,
                        cx,
                        crate::native_dialog::AlertSpec::new(
                            SKIP_NODE_LABEL,
                            SKIP_NODE_CONFIRM,
                            SKIP_NODE_LABEL,
                        )
                        .ok_variant(ButtonVariant::Danger)
                        .on_ok(move |_window, cx| {
                            spawn_resolve(node_id.clone(), "skip", cx);
                            true
                        }),
                    );
                })
                .into_any_element(),
        );
    }
    actions
}

/// EXP-1014 — the workflow's strict launch, whatever vintage its stored
/// jsonb is: the ONE rule the desktop engine and the CLI daemon launch every
/// node run by (`coding::workflows::launch`).
fn workflow_launch(row: &domain::rows::WorkflowRow) -> WorkflowLaunch {
    normalize_workflow_launch(row.launch.as_ref().unwrap_or(&serde_json::Value::Null))
}

/// A model alias as the pickers spell it (`opus`, not `Opus`, is the wire) —
/// the agent's own list, falling back to the raw alias for anything a newer
/// server sent.
fn model_label(launch: &WorkflowLaunch, model: &str) -> String {
    let choices: &[(&str, &str)] = match launch.agent.as_str() {
        "codex" => &crate::coding_selects::CODEX_MODEL_CHOICES[..],
        _ => &crate::coding_selects::MODEL_CHOICES[..],
    };
    choices
        .iter()
        .find(|(_, value)| *value == model)
        .map(|(label, _)| (*label).to_string())
        .unwrap_or_else(|| model.to_string())
}

/// Why Start is disabled, or `None` when the draft can go — the ONE rule
/// ([`workflow_start_blocker`]), which the server refuses with verbatim.
fn start_blocker(
    row: &domain::rows::WorkflowRow,
    shape: &domain::workflow_view::WorkflowShape,
) -> Option<String> {
    workflow_start_blocker(
        StartableWorkflow {
            status: row.status_wire(),
            device_id: row.device_id.as_deref(),
            repository_id: row.repository_id.as_deref(),
            // The column is NOT NULL DEFAULT `contract` server-side; every
            // mode starts since EXP-983, so this never blocks either way.
            start_on: row
                .start_on
                .as_deref()
                .unwrap_or(domain::contract::WF_START_ON_CONTRACT),
        },
        shape,
    )
}

/// The header's run controls, by status: a draft starts (disabled with the
/// blocker's sentence under it), a running workflow pauses or is cancelled,
/// a paused one resumes or is cancelled, a finished one offers neither.
fn status_actions(
    row: &domain::rows::WorkflowRow,
    shape: &domain::workflow_view::WorkflowShape,
    cx: &App,
) -> Vec<gpui::AnyElement> {
    let id = row.id.clone();
    let name = row.name.clone().unwrap_or_default();
    match row.status_wire() {
        domain::contract::WF_STATUS_DRAFT => {
            let blocked = start_blocker(row, shape).is_some();
            vec![Button::new("workflow-start")
                .primary()
                .small()
                .icon(Icon::from(registry::ACTION_RUN))
                .label(START_WORKFLOW_LABEL)
                .disabled(blocked)
                .on_click(move |_, _window, cx| spawn_command(Command::Start, id.clone(), cx))
                .into_any_element()]
        }
        "running" => vec![
            Button::new("workflow-pause")
                .small()
                .icon(Icon::from(registry::UI_STOP))
                .label(PAUSE_WORKFLOW_LABEL)
                .on_click({
                    let id = id.clone();
                    move |_, _window, cx| spawn_command(Command::Pause, id.clone(), cx)
                })
                .into_any_element(),
            cancel_button(id, name, cx),
        ],
        "paused" => vec![
            Button::new("workflow-resume")
                .primary()
                .small()
                .icon(Icon::from(registry::ACTION_RUN))
                .label(RESUME_WORKFLOW_LABEL)
                .on_click({
                    let id = id.clone();
                    move |_, _window, cx| spawn_command(Command::Resume, id.clone(), cx)
                })
                .into_any_element(),
            cancel_button(id, name, cx),
        ],
        // done / cancelled / a newer server's status: nothing to press.
        _ => Vec::new(),
    }
}

/// Cancelling is destructive (live runs end, the branch goes), so it
/// confirms with the shared sentence.
fn cancel_button(id: String, name: String, _cx: &App) -> gpui::AnyElement {
    Button::new("workflow-cancel")
        .danger()
        .small()
        .icon(Icon::from(registry::UI_CLOSE))
        .label(CANCEL_WORKFLOW_LABEL)
        .on_click(move |_, window, cx| {
            let id = id.clone();
            let title = if name.is_empty() {
                CANCEL_WORKFLOW_LABEL.to_string()
            } else {
                format!("Cancel {name}?")
            };
            crate::native_dialog::open_alert(
                window,
                cx,
                crate::native_dialog::AlertSpec::new(
                    title,
                    CANCEL_WORKFLOW_CONFIRM,
                    CANCEL_WORKFLOW_LABEL,
                )
                .ok_variant(ButtonVariant::Danger)
                .on_ok(move |_window, cx| {
                    spawn_command(Command::Cancel, id.clone(), cx);
                    true
                }),
            );
        })
        .into_any_element()
}

/// EXP-1032 — "Merge", ON the final-PR chip: squash-merging that one pull
/// request is the whole run's single human review, so the action sits where
/// the pull request is named. It confirms with the shared sentence first
/// (the Cancel workflow pattern), and the click never reaches the chip under
/// it, whose own job is to open the pull request in a browser.
fn merge_final_pr_button(workflow_id: String, cx: &App) -> gpui::AnyElement {
    crate::controls::text_button(
        "workflow-final-pr-merge",
        MERGE_FINAL_PR_LABEL,
        crate::controls::TextButtonVariant::Text,
        cx,
    )
    .on_click(move |_, window, cx| {
        cx.stop_propagation();
        let workflow_id = workflow_id.clone();
        crate::native_dialog::open_alert(
            window,
            cx,
            crate::native_dialog::AlertSpec::new(
                format!("{MERGE_FINAL_PR_LABEL} {}", FINAL_PR_TITLE.to_lowercase()),
                MERGE_FINAL_PR_CONFIRM,
                MERGE_FINAL_PR_LABEL,
            )
            .on_ok(move |window, cx| {
                spawn_merge_final_pr(workflow_id.clone(), window, cx);
                true
            }),
        );
    })
    .into_any_element()
}

/// EXP-982 — the merge train under the graph: every node whose PR is up, in
/// landing order, each with where it stands. Hidden on a draft.
fn render_merge_train(nodes: &[domain::rows::WorkflowNodeRow], cx: &App) -> gpui::AnyElement {
    let muted = cx.theme().muted_foreground;
    let train_nodes: Vec<TrainNode<'_>> = nodes
        .iter()
        .map(|node| TrainNode {
            id: &node.id,
            kind: node.kind_wire(),
            state: node.state_wire(),
            wave: node.wave_index() as i64,
            lane: node.lane_index() as i64,
            approved: node.approved_at.is_some(),
        })
        .collect();
    let entries = workflow_merge_train(&train_nodes);
    let title = div()
        .text_xs()
        .text_color(muted)
        .child(SharedString::from(MERGE_TRAIN_TITLE));
    if entries.is_empty() {
        return v_flex()
            .min_w_0()
            .gap_1()
            .child(title)
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(MERGE_TRAIN_EMPTY)),
            )
            .into_any_element();
    }
    let cars: Vec<gpui::AnyElement> = entries
        .iter()
        .filter_map(|entry| {
            let node = nodes.iter().find(|node| node.id == entry.id)?;
            let issue_id = node.issue_id.clone()?;
            Some(
                v_flex()
                    .min_w_0()
                    .gap_1()
                    .child(issue_chip_for(&issue_id, cx))
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(SharedString::from(workflow_train_step_label(entry.step))),
                    )
                    .into_any_element(),
            )
        })
        .collect();
    v_flex()
        .min_w_0()
        .gap_2()
        .child(title)
        .child(h_flex().min_w_0().flex_wrap().gap_4().children(cars))
        .into_any_element()
}

/// EXP-984 — the detail's `Metrics` section: the counters the server and the
/// engine accumulate inside `workflows.metrics`, as label/value rows. `None`
/// while the blob has nothing to say at all.
fn render_metrics(row: &domain::rows::WorkflowRow, cx: &App) -> Option<gpui::AnyElement> {
    let metrics = row.metrics.clone()?;
    let rows = workflow_metric_rows(&metrics);
    if rows.is_empty() {
        return None;
    }
    let value_rows: Vec<gpui::Div> = rows
        .into_iter()
        .map(|entry| {
            crate::surface::glass_picker_row(
                SharedString::from(entry.label),
                None,
                div()
                    .child(SharedString::from(entry.value))
                    .into_any_element(),
                cx,
            )
        })
        .collect();
    Some(
        v_flex()
            .min_w_0()
            .child(crate::surface::glass_section_header(METRICS_TITLE, None, cx))
            .child(crate::surface::glass_group_rows(value_rows))
            .into_any_element(),
    )
}

/// The two things the node panel still picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodePick {
    Kind,
    Risk,
}

/// Which of the node panel's picks a workflow's status still allows — the
/// server's rule verbatim (`workflows.updateNode`: "Kind and touches shape
/// the PLAN; risk stays adjustable", and only the first asserts a draft).
/// Risk is the ONE lever onto the strong model (`model_for_node`), so it
/// stays a pick while the run is up; the plan's shape does not.
fn node_pick_enabled(pick: NodePick, draft: bool) -> bool {
    match pick {
        NodePick::Kind => draft,
        NodePick::Risk => true,
    }
}

/// The contract's node KIND picks with their shipped labels.
const WF_KIND_CHOICES: [(&str, &str); 3] = [
    ("Contract", domain::contract::WF_NODE_KIND_CONTRACT),
    ("Leaf", domain::contract::WF_NODE_KIND_LEAF),
    ("Integration", domain::contract::WF_NODE_KIND_INTEGRATION),
];

/// The contract's RISK picks. Capitalized here only; the graph's caption
/// spells high risk out through `workflow_node_caption`.
const WF_RISK_CHOICES: [(&str, &str); 3] = [
    ("Low", domain::contract::WF_RISK_LOW),
    ("Medium", domain::contract::WF_RISK_MEDIUM),
    ("High", domain::contract::WF_RISK_HIGH),
];

/// Everything one node's CHIP renders, joined off the collections once.
struct NodeFacts {
    /// `EXP-14 +3` for a compound node, the bare identifier otherwise;
    /// EMPTY while the node's issue has not synced.
    identifier: String,
    /// The issue's own title — the chip truncates it into whatever the
    /// scaled box leaves.
    title: String,
    /// The ONE caption, trailing INSIDE the chip in the node's tone.
    caption: String,
    tone: WorkflowNodeTone,
    /// A compound node (a parent run with its sub-issues) is drawn as a DECK
    /// of chips.
    compound: bool,
    /// EXP-982: the state's GLYPH, so a state reads by shape as well as by
    /// colour. `None` for the states that are only a word — the chip then
    /// leads with the issue's own status, as every other chip in the app.
    glyph: Option<crate::icons::ExpIcon>,
    /// The issue's resolved status, for that default glyph.
    status: Option<domain::statuses::ResolvedStatus>,
    /// EXP-984: a follow-up nobody admitted yet — drawn with a DASHED
    /// outline, because it is not part of the run.
    proposed: bool,
    /// EXP-1014: the node's ISSUE row has not synced here yet — the chip
    /// names it by the id's first 8 characters and its glyph slot stays
    /// EMPTY rather than claiming a status nobody knows (×4).
    unsynced: bool,
    /// EXP-1032: this is the final-PR chip and its pull request is OPEN —
    /// the workflow id the Merge action on it calls with.
    merge_final_pr: Option<String>,
    /// The node's coding session, once it has one that synced.
    run: Option<NodeRun>,
}

/// One node's run as the graph reads it: where a tap goes, and what the dot
/// says (the ONE session tone table, `queries::session_dot_tone`).
#[derive(Clone)]
struct NodeRun {
    session_id: String,
    /// Still up (`running` / `in_review`) — what the Running strip lists.
    live: bool,
    /// The agent is mid-turn: the dot pings.
    busy: bool,
    tone: gpui::Hsla,
}

impl NodeRun {
    fn derive(node: &domain::rows::WorkflowNodeRow, cx: &App) -> Option<Self> {
        let session_id = node.session_id.clone()?;
        let collections = Store::try_global(cx)?.collections().clone();
        let session = collections.coding_sessions.read(cx).get(&session_id).cloned()?;
        let pr_state = node.issue_id.as_deref().and_then(|issue_id| {
            collections.issues.read(cx).get(issue_id)?.pr_state.clone()
        });
        let live = matches!(session.status.as_deref(), Some("running" | "in_review"));
        let display = queries::coding_session_display(&session, pr_state.as_deref());
        Some(Self {
            session_id,
            live,
            // EXP-848: the ONE turn rule every session list keys its ping
            // on (no in-process turn signal reaches a list, so `None` as in
            // `RunListFacts::derive`), and only while the row DISPLAYS as
            // running (the x4 rule): a `needs_input` / `in_review` row whose
            // `agent_busy` went stale must not pulse.
            busy: node_run_busy(&session, display, chrono::Utc::now().timestamp()),
            tone: queries::session_dot_tone(
                queries::SessionDotFacts::from_display(display, !live, false),
                cx.theme().muted_foreground,
            ),
        })
    }
}

/// Does a node's dot ping? [`NodeRun::derive`]'s rule, as a pure function.
fn node_run_busy(
    session: &domain::rows::CodingSession,
    display: queries::CodingSessionDisplay,
    now_epoch: i64,
) -> bool {
    display == queries::CodingSessionDisplay::Running
        && queries::session_agent_busy(session, None, now_epoch)
}

impl NodeFacts {
    fn derive(
        node: &domain::rows::WorkflowNodeRow,
        workflow: &domain::rows::WorkflowRow,
        cx: &App,
    ) -> Self {
        let members = node.member_ids();
        let issue = node.issue_id.as_deref().and_then(|issue_id| {
            Store::try_global(cx)?
                .collections()
                .issues
                .read(cx)
                .get(issue_id)
                .cloned()
        });
        // EXP-1014 — a node whose issue row has not synced here still names
        // itself (×4): the issue id's first 8 characters in the mono slot,
        // `+N` for a compound one, and the ONE line that says why there is
        // no title yet.
        let identifier = issue
            .as_ref()
            .map(|issue| issue.identifier.clone())
            .filter(|identifier| !identifier.is_empty())
            .unwrap_or_else(|| {
                node.issue_id
                    .as_deref()
                    .unwrap_or_default()
                    .chars()
                    .take(8)
                    .collect()
            });
        Self {
            identifier: if identifier.is_empty() {
                String::new()
            } else {
                workflow_node_title(&identifier, members.len())
            },
            title: issue
                .as_ref()
                .map(|issue| issue.title.clone())
                .unwrap_or_else(|| NODE_UNSYNCED_TITLE.to_string()),
            caption: workflow_node_caption(
                CaptionNode {
                    kind: node.kind_wire(),
                    state: node.state_wire(),
                    risk: node.risk_wire(),
                },
                workflow.status_wire(),
            ),
            tone: workflow_node_tone(node.state_wire()),
            compound: !members.is_empty(),
            glyph: state_glyph(node.state_wire(), workflow.status_wire()),
            status: issue
                .as_ref()
                .map(|issue| queries::resolve_issue_status(cx, issue)),
            proposed: node.state_wire() == domain::contract::WF_NODE_STATE_PROPOSED,
            unsynced: issue.is_none(),
            merge_final_pr: None,
            run: NodeRun::derive(node, cx),
        }
    }

    /// EXP-982 — the FINAL pull request: one more chip after the last wave,
    /// the merged-PR mark in its slot and the workflow's own PR state as its
    /// caption. It is not a `workflow_nodes` row, so it has no issue, no
    /// edges and cannot be picked.
    fn final_pr(caption: &str, row: &domain::rows::WorkflowRow) -> Self {
        Self {
            identifier: String::new(),
            title: FINAL_PR_TITLE.to_string(),
            caption: caption.to_string(),
            tone: match row.final_pr_state.as_deref() {
                Some("merged") => WorkflowNodeTone::Success,
                Some("closed") => WorkflowNodeTone::Muted,
                _ => WorkflowNodeTone::Active,
            },
            compound: false,
            glyph: Some(registry::NOTIFICATION_PR_MERGED),
            status: None,
            proposed: false,
            unsynced: false,
            // EXP-1032: merging it is the run's ONE human review, and the
            // button sits on the chip that IS the pull request.
            merge_final_pr: (row.final_pr_state.as_deref() == Some("open"))
                .then(|| row.id.clone()),
            run: None,
        }
    }
}

/// The glyph one node STATE earns. Existing concepts only: a run in flight
/// borrows the session's play mark, a wall the warning triangle, a landed
/// node the merged-PR mark, a failure the error mark, and anything waiting
/// on a pull request the reviews mark. A draft has no state worth a glyph.
fn state_glyph(state: &str, workflow_status: &str) -> Option<crate::icons::ExpIcon> {
    if workflow_status == domain::contract::WF_STATUS_DRAFT {
        return None;
    }
    match state {
        "running" => Some(registry::ACTION_RUN),
        "waiting" => Some(registry::UI_WARNING),
        "landed" => Some(registry::NOTIFICATION_PR_MERGED),
        "failed" => Some(registry::UI_ERROR),
        "in_review" | "updating" => Some(registry::NAV_REVIEWS),
        _ => None,
    }
}

/// ONE node: the app's own ISSUE CHIP (EXP-1014) in a fixed box — the state
/// glyph (or the live run's dot) in the chip's glyph slot, the mono
/// identifier, as much title as the scaled box holds, and the state caption
/// trailing inside it in the node's tone. A COMPOUND node is a deck of
/// chips; nothing is ever drawn under one.
fn render_node_box(
    node_id: &str,
    facts: Option<&NodeFacts>,
    cycled: bool,
    selected: bool,
    on_pick: Rc<dyn Fn(&str, &mut App)>,
    cx: &App,
) -> gpui::AnyElement {
    let theme = cx.theme();
    let tone = facts.map_or(WorkflowNodeTone::Muted, |facts| facts.tone);
    let tint = tone_color(tone, cx);
    let live_run = facts
        .and_then(|facts| facts.run.clone())
        .filter(|run| run.live);
    let quiet = tone == WorkflowNodeTone::Muted;
    let identifier = facts.map(|facts| facts.identifier.clone()).unwrap_or_default();
    let title = facts.map(|facts| facts.title.clone()).unwrap_or_default();
    let caption = facts.map(|facts| facts.caption.clone()).unwrap_or_default();
    let compound = facts.is_some_and(|facts| facts.compound);
    // EXP-984: a proposal is drawn with a DASHED outline — it is in the
    // graph to be decided on, not because it is part of the run.
    let proposed = facts.is_some_and(|facts| facts.proposed);
    let glyph = facts.and_then(|facts| facts.glyph.clone());
    let status = facts.and_then(|facts| facts.status.clone());
    let unsynced = facts.is_some_and(|facts| facts.unsynced);
    let merge_final_pr = facts.and_then(|facts| facts.merge_final_pr.clone());
    let target = node_id.to_string();

    let mut chip = crate::issue_chip::issue_chip(
        SharedString::from(format!("workflow-node-{node_id}")),
        identifier,
        title,
    )
    .flexible()
    .on_click(move |_: &ClickEvent, _window, cx| on_pick(&target, cx));
    if let Some(status) = status {
        chip = chip.status(status);
    }
    // The ONE slot rule, ×4: the live dot of the run that is up on it, else
    // the state's own mark, else the issue's status (set above) — and
    // nothing at all while that issue has not synced, since a chip with no
    // status must not invent one.
    if let Some(run) = live_run.as_ref() {
        chip = chip.slot(crate::surface::live_dot(run.tone, run.busy));
    } else if let Some(glyph) = glyph {
        chip = chip.slot(Icon::from(glyph).xsmall().text_color(tint));
    } else if unsynced {
        chip = chip.slot(div());
    }
    // The caption trails INSIDE the chip, in the node's tone. It is set even
    // when it is empty (a draft says nothing yet): a chip with a note takes
    // the whole box, which is where its edges anchor.
    chip = chip.note(
        caption,
        if live_run.is_some() {
            live_run.as_ref().map_or(tint, |run| run.tone)
        } else if quiet {
            tint.opacity(0.8)
        } else {
            tint
        },
    );
    // The hairline SAYS something: red inside a cycle, dashed while the node
    // is only proposed, the session's own tone while its run is up.
    if cycled {
        chip = chip.outline(theme.danger, false);
    } else if proposed {
        chip = chip.outline(theme.muted_foreground, true);
    } else if let Some(run) = live_run.as_ref() {
        chip = chip.outline(run.tone, false);
    } else if !quiet {
        chip = chip.outline(tint, false);
    }
    if compound {
        chip = chip.stacked();
    }
    // EXP-1032 — the final pull request is the run's ONE human review: its
    // chip carries Merge, which confirms before it lands anything.
    if let Some(workflow_id) = merge_final_pr {
        chip = chip.trailing(merge_final_pr_button(workflow_id, cx));
    }

    // The pick: an accent ring OUTSIDE the chip, so its own hairline keeps
    // whatever it was saying. A deck's ghosts step up and to the right, so
    // the ring gives way on those two sides.
    let out = 3.;
    let deck = 3. + 2. * crate::issue_chip::ISSUE_CHIP_STACK_STEP;
    div()
        .w_full()
        .h_full()
        .flex()
        .items_center()
        .child(
            div()
                .relative()
                .w_full()
                .min_w_0()
                .flex()
                .when(selected, |this| {
                    this.child(
                        div()
                            .absolute()
                            .left(px(-out))
                            .bottom(px(-out))
                            .top(px(if compound { -deck } else { -out }))
                            .right(px(if compound { -deck } else { -out }))
                            .rounded(px(6.))
                            .border_1()
                            .border_color(theme.ring),
                    )
                })
                .child(chip),
        )
        .into_any_element()
}

/// The colour ONE node tone paints in.
fn tone_color(tone: WorkflowNodeTone, cx: &App) -> gpui::Hsla {
    let theme = cx.theme();
    match tone {
        WorkflowNodeTone::Amber => theme.warning,
        WorkflowNodeTone::Danger => theme.danger,
        WorkflowNodeTone::Success => theme.success,
        WorkflowNodeTone::Active => theme.foreground,
        WorkflowNodeTone::Muted => theme.muted_foreground,
    }
}

/// One live run of the Running strip: the session's dot and the node's
/// title, and a tap lands IN that run.
fn run_pill(node_id: &str, title: &str, run: NodeRun, cx: &App) -> gpui::AnyElement {
    let session_id = run.session_id.clone();
    crate::surface::glass_pill_button(
        SharedString::from(format!("workflow-run-{node_id}")),
        crate::surface::PillSize::Sm,
        cx,
    )
    .child(
        h_flex()
            .items_center()
            .gap_1p5()
            .child(crate::surface::live_dot(run.tone, run.busy))
            .child(SharedString::from(title.to_string())),
    )
    .on_click(move |_, window, cx| {
        crate::navigation::navigate(
            window,
            cx,
            Screen::Session {
                session_id: session_id.clone(),
            },
        );
    })
    .into_any_element()
}

/// [`issue_chip_for`], opening the issue on a tap.
fn issue_chip_link(issue_id: &str, cx: &App) -> gpui::AnyElement {
    let target = issue_id.to_string();
    issue_chip_element(issue_id, cx)
        .on_click(move |_, window, cx| {
            crate::navigation::navigate(
                window,
                cx,
                Screen::IssueDetail {
                    issue_id: target.clone(),
                },
            );
        })
        .into_any_element()
}

/// The shared issue chip for one issue id, degrading to the raw id while the
/// row has not synced.
fn issue_chip_for(issue_id: &str, cx: &App) -> gpui::AnyElement {
    issue_chip_element(issue_id, cx).into_any_element()
}

/// One issue's identifier, or the raw id while its row has not synced.
fn issue_identifier(issue_id: &str, cx: &App) -> String {
    Store::try_global(cx)
        .and_then(|store| store.collections().issues.read(cx).get(issue_id).cloned())
        .map(|issue| issue.identifier.clone())
        .unwrap_or_else(|| issue_id.to_string())
}

fn issue_chip_element(issue_id: &str, cx: &App) -> crate::issue_chip::IssueChip {
    let row = Store::try_global(cx)
        .and_then(|store| store.collections().issues.read(cx).get(issue_id).cloned());
    // EXP-1014, as in the graph's own chips: a row that has not synced here
    // reads as the issue id's first 8 characters and the ONE line that says
    // why there is no title — never a bare uuid.
    let identifier = row
        .as_ref()
        .map(|issue| issue.identifier.clone())
        .unwrap_or_else(|| issue_id.chars().take(8).collect());
    let title = row
        .map(|issue| issue.title.clone())
        .unwrap_or_else(|| NODE_UNSYNCED_TITLE.to_string());
    let mut chip = crate::issue_chip::issue_chip(
        SharedString::from(format!("workflow-issue-{issue_id}")),
        identifier,
        title,
    )
    .flexible();
    if let Some(status) = crate::issue_chip::synced_issue_status(issue_id, cx) {
        chip = chip.status(status);
    }
    chip
}

/// ONE labelled pick row over a (label, value) list — the launch cluster's
/// `glass_picker_row` shape with a dropdown trigger.
fn pick_row(
    id: &'static str,
    label: &'static str,
    choices: &'static [(&'static str, &'static str)],
    picked: &str,
    enabled: bool,
    on_pick: impl Fn(&str, &mut App) + Clone + 'static,
    cx: &App,
) -> gpui::Div {
    let current = choices
        .iter()
        .find(|(_, value)| *value == picked)
        .map(|(label, _)| (*label).to_string())
        .unwrap_or_else(|| crate::launch_options::CLI_DEFAULT_LABEL.to_string());
    let picked = picked.to_string();
    let control = Button::new(id)
        .ghost()
        .small()
        .label(SharedString::from(current))
        .dropdown_caret(true)
        .disabled(!enabled)
        .dropdown_menu(move |mut menu, _window, _cx| {
            for (label, value) in choices {
                let on_pick = on_pick.clone();
                let value = *value;
                menu = menu.item(
                    PopupMenuItem::new(SharedString::from(*label))
                        .checked(value == picked)
                        .on_click(move |_, _window, cx| on_pick(value, cx)),
                );
            }
            menu
        })
        .into_any_element();
    crate::surface::glass_picker_row(label, None, control, cx)
}

/// EXP-1014 — the RUNNER device, as ONE compact control in the header (a
/// draft cannot start without a machine; everything else a workflow used to
/// configure is gone). Fixed the moment the workflow leaves draft, which is
/// what the server enforces too.
fn device_control(
    workflow_id: &str,
    picked: Option<&str>,
    devices: Vec<queries::LaunchDevice>,
    enabled: bool,
    _cx: &App,
) -> gpui::AnyElement {
    let label = picked
        .and_then(|device_id| {
            devices
                .iter()
                .find(|device| device.device_id == device_id)
                .map(|device| device.label.clone())
        })
        .unwrap_or_else(|| "No machine".to_string());
    let workflow_id = workflow_id.to_string();
    let picked = picked.map(str::to_string);
    Button::new("workflow-device")
        .ghost()
        .small()
        .icon(Icon::from(registry::NAV_DEVICES))
        .label(SharedString::from(label))
        .disabled(!enabled)
        .dropdown_caret(true)
        .dropdown_menu(move |mut menu, _window, _cx| {
            if devices.is_empty() {
                return menu.item(PopupMenuItem::new("No machines").disabled(true));
            }
            for device in &devices {
                let device_id = device.device_id.clone();
                let name = device.label.clone();
                let on = picked.as_deref() == Some(device_id.as_str());
                let workflow_id = workflow_id.clone();
                menu = menu.item(
                    PopupMenuItem::new(SharedString::from(name))
                        .checked(on)
                        .on_click(move |_, _window, cx| {
                            let mut input =
                                api::workflows::WorkflowUpdate::new(workflow_id.clone());
                            input.device_id = api::Patch::Set(device_id.clone());
                            spawn_update(input, cx);
                        }),
                );
            }
            menu
        })
        .into_any_element()
}

/// Destructive actions confirm first (the machines Remove pattern).
fn prompt_delete(workflow_id: String, window: &mut Window, cx: &mut App) {
    crate::native_dialog::open_alert(
        window,
        cx,
        crate::native_dialog::AlertSpec::new(
            DELETE_WORKFLOW_LABEL,
            "This deletes the workflow and its plan. The issues themselves stay.",
            "Delete",
        )
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |window, cx| {
            spawn_delete(workflow_id.clone(), window, cx);
            true
        }),
    );
}

/// EXP-982 — the four status flips, one tRPC call each. The server owns
/// every precondition and answers a refusal with a sentence, so this only
/// picks the procedure and lets the notice carry it.
#[derive(Clone, Copy)]
enum Command {
    Start,
    Pause,
    Resume,
    Cancel,
}

fn spawn_command(command: Command, workflow_id: String, cx: &mut App) {
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    cx.background_executor()
        .spawn(async move {
            let result = match command {
                Command::Start => api::workflows::start(&trpc, &workflow_id),
                Command::Pause => api::workflows::pause(&trpc, &workflow_id),
                Command::Resume => api::workflows::resume(&trpc, &workflow_id),
                Command::Cancel => api::workflows::cancel(&trpc, &workflow_id),
            };
            if let Err(err) = result {
                log::warn!("workflows: {workflow_id} command failed: {err}");
            }
        })
        .detach();
}

/// EXP-1032 — `workflows.mergeFinalPr`: the workflow's branch is
/// squash-merged into the default branch and the run is done. Idempotent
/// server-side; a refusal (no final PR yet, GitHub said no) is a sentence
/// worth reading, so it lands as an error notification rather than a log
/// line nobody sees.
fn spawn_merge_final_pr(workflow_id: String, window: &mut Window, cx: &mut App) {
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::workflows::merge_final_pr(&trpc, &workflow_id) })
            .await;
        let _ = handle.update(cx, |_, window, cx| {
            // The synced echo repaints the chip; only a refusal has to say
            // anything.
            if let Err(err) = result {
                window.push_notification(
                    Notification::error(SharedString::from(err.user_message())),
                    cx,
                );
            }
        });
    })
    .detach();
}

/// The node-level calls a person makes: the human gate, and unsticking a
/// failed node.
fn spawn_approve(node_id: String, approved: bool, cx: &mut App) {
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::workflows::approve_node(&trpc, &node_id, approved) {
                log::warn!("workflows: approveNode {node_id} failed: {err}");
            }
        })
        .detach();
}

/// EXP-984 — a member's call on a `proposed` node: admit it into the run, or
/// dismiss the follow-up altogether.
fn spawn_admit(node_id: String, admit: bool, cx: &mut App) {
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::workflows::admit_node(&trpc, &node_id, admit) {
                log::warn!("workflows: admitNode {node_id} failed: {err}");
            }
        })
        .detach();
}

fn spawn_resolve(node_id: String, action: &'static str, cx: &mut App) {
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::workflows::resolve_node(&trpc, &node_id, action) {
                log::warn!("workflows: resolveNode {node_id} failed: {err}");
            }
        })
        .detach();
}

fn spawn_update(input: api::workflows::WorkflowUpdate, cx: &mut App) {
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::workflows::update(&trpc, &input).map(|_| ()) })
            .await;
        let _ = cx.update(|_| {
            // The synced echo repaints the row; a refusal is a server
            // sentence worth reading.
            if let Err(err) = result {
                log::warn!("workflows: updating the workflow failed: {err}");
            }
        });
    })
    .detach();
}

fn spawn_update_node(input: api::workflows::WorkflowNodeUpdate, cx: &mut App) {
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::workflows::update_node(&trpc, &input) })
            .await;
        let _ = cx.update(|_| {
            if let Err(err) = result {
                log::warn!("workflows: updating the node failed: {err}");
            }
        });
    })
    .detach();
}

fn spawn_delete(workflow_id: String, window: &mut Window, cx: &mut App) {
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::workflows::delete(&trpc, &workflow_id) })
            .await;
        let _ = cx.update(|cx| match result {
            // Back to the list: the detail's subject is gone.
            Ok(()) => {
                let _ = handle.update(cx, |_, window, cx| {
                    crate::navigation::navigate(window, cx, Screen::Workflows);
                });
            }
            Err(err) => log::warn!("workflows: deleting the workflow failed: {err}"),
        });
    })
    .detach();
}

/// EXP-1014 — the IDE styleguide's `workflow-graph` entry: ONE fixed sample
/// of the real graph (contract → three leaves, one of them a deck, one
/// running, one landed → integration → the final pull request), drawn by the
/// very code the workflow screen draws. No store, no workflow row: the facts
/// are spelled out here so the entry renders on any machine.
pub(crate) fn styleguide_sample_graph(cx: &App) -> gpui::AnyElement {
    use domain::statuses::constructed_default;
    use domain::IssueStatus;

    let leaf = |identifier: &str, title: &str| (identifier.to_string(), title.to_string());
    let (nodes, facts): (Vec<GridNode>, HashMap<String, NodeFacts>) = {
        let mut grid: Vec<GridNode> = Vec::new();
        let mut facts: HashMap<String, NodeFacts> = HashMap::new();
        let mut push = |key: &str, wave: usize, lane: usize, node: NodeFacts| {
            grid.push(GridNode { key: key.to_string(), wave, lane });
            facts.insert(key.to_string(), node);
        };
        let (identifier, title) = leaf("EXP-1029", "Workflow contract");
        push(
            "contract",
            0,
            0,
            NodeFacts {
                identifier,
                title,
                caption: "Landed".to_string(),
                tone: WorkflowNodeTone::Success,
                compound: false,
                glyph: Some(registry::NOTIFICATION_PR_MERGED),
                status: Some(constructed_default(IssueStatus::Done)),
                proposed: false,
                unsynced: false,
                merge_final_pr: None,
                run: None,
            },
        );
        let (identifier, title) = leaf("EXP-1032 +2", "Workflow screen (web)");
        push(
            "deck",
            1,
            0,
            NodeFacts {
                identifier,
                title,
                caption: "Ready".to_string(),
                tone: WorkflowNodeTone::Muted,
                compound: true,
                glyph: None,
                status: Some(constructed_default(IssueStatus::Backlog)),
                proposed: false,
                unsynced: false,
                merge_final_pr: None,
                run: None,
            },
        );
        let (identifier, title) = leaf("EXP-1034", "Workflow screen (IDE)");
        push(
            "running",
            1,
            1,
            NodeFacts {
                identifier,
                title,
                caption: "Running".to_string(),
                tone: WorkflowNodeTone::Active,
                compound: false,
                glyph: Some(registry::ACTION_RUN),
                status: Some(constructed_default(IssueStatus::InProgress)),
                proposed: false,
                unsynced: false,
                merge_final_pr: None,
                run: Some(NodeRun {
                    session_id: "styleguide".to_string(),
                    live: true,
                    busy: true,
                    tone: queries::session_dot_tone(
                        queries::SessionDotFacts::from_display(
                            queries::CodingSessionDisplay::Running,
                            false,
                            false,
                        ),
                        cx.theme().muted_foreground,
                    ),
                }),
            },
        );
        let (identifier, title) = leaf("EXP-1035", "Workflow screen (iOS)");
        push(
            "landed",
            1,
            2,
            NodeFacts {
                identifier,
                title,
                caption: "Landed".to_string(),
                tone: WorkflowNodeTone::Success,
                compound: false,
                glyph: Some(registry::NOTIFICATION_PR_MERGED),
                status: Some(constructed_default(IssueStatus::Done)),
                proposed: false,
                unsynced: false,
                merge_final_pr: None,
                run: None,
            },
        );
        let (identifier, title) = leaf("EXP-1036", "One graph, four clients");
        push(
            "integration",
            2,
            0,
            NodeFacts {
                identifier,
                title,
                caption: "Blocked".to_string(),
                tone: WorkflowNodeTone::Muted,
                compound: false,
                glyph: None,
                status: Some(constructed_default(IssueStatus::Backlog)),
                proposed: false,
                unsynced: false,
                merge_final_pr: None,
                run: None,
            },
        );
        push(
            FINAL_PR_KEY,
            3,
            0,
            NodeFacts {
                identifier: String::new(),
                title: FINAL_PR_TITLE.to_string(),
                caption: "#812 · Open".to_string(),
                tone: WorkflowNodeTone::Active,
                compound: false,
                glyph: Some(registry::NOTIFICATION_PR_MERGED),
                status: None,
                proposed: false,
                unsynced: false,
                // EXP-1032: the sample's final pull request is OPEN, so the
                // entry shows the Merge action that rides on that chip.
                merge_final_pr: Some("styleguide".to_string()),
                run: None,
            },
        );
        (grid, facts)
    };
    // The edge styles are the shared rule's, off the sample's own states.
    let state_of: HashMap<&str, &str> = HashMap::from([
        ("contract", "landed"),
        ("deck", "ready"),
        ("running", "running"),
        ("landed", "landed"),
        ("integration", "blocked"),
    ]);
    let edges: Vec<GridEdge> = [
        ("contract", "deck"),
        ("contract", "running"),
        ("contract", "landed"),
        ("deck", "integration"),
        ("running", "integration"),
        ("landed", "integration"),
    ]
    .into_iter()
    .map(|(from, to)| {
        let edge = domain::workflow_view::WorkflowEdge {
            from: from.to_string(),
            to: to.to_string(),
            cycle: false,
            serial: false,
        };
        let style = workflow_edge_style(
            &edge,
            state_of.get(from).copied().unwrap_or_default(),
            state_of.get(to).copied().unwrap_or_default(),
        );
        GridEdge { from: edge.from, to: edge.to, style }
    })
    .collect();

    let on_pick: Rc<dyn Fn(&str, &mut App)> = Rc::new(|_, _| {});
    let render = |node: &GridNode, cycled: bool, cx: &App| {
        render_node_box(
            &node.key,
            facts.get(&node.key),
            cycled,
            // The pick reads as the accent ring outside the chip.
            node.key == "running",
            on_pick.clone(),
            cx,
        )
    };
    let waves = nodes.iter().map(|node| node.wave).max().unwrap_or(0);
    let lanes = nodes.iter().map(|node| node.lane).max().unwrap_or(0);
    let geometry = graph_geometry(graph_scale(waves, STYLEGUIDE_GRAPH_W), waves, lanes);
    grid_view(&nodes, &edges, geometry, &[], &render, cx)
}

/// The width the styleguide entry gives its sample — the page's own column,
/// so the fill rule is the one a real screen runs.
const STYLEGUIDE_GRAPH_W: f32 = 720.;

#[cfg(test)]
mod tests {
    use super::*;

    fn node_row(checkpoint_at: Option<&str>) -> domain::rows::WorkflowNodeRow {
        serde_json::from_value(serde_json::json!({
            "id": "n-1",
            "workflow_id": "wf-1",
            "issue_id": "i-1",
            "checkpoint_at": checkpoint_at,
            "after_node_ids": r#"["n-2","n-3"]"#,
        }))
        .expect("the row hydrates")
    }

    /// EXP-848: a node's dot pings on the ONE turn rule, and only while the
    /// row displays as running — a stale `agent_busy` on a parked, in-review
    /// or ended row never pulses.
    #[test]
    fn a_node_dot_pings_only_for_a_running_row_mid_turn() {
        let session = |status: &str, needs_input: bool, agent_busy: bool| {
            serde_json::from_value::<domain::rows::CodingSession>(serde_json::json!({
                "id": "s-1",
                "status": status,
                "needs_input": needs_input,
                "agent_busy": agent_busy,
            }))
            .expect("the row hydrates")
        };
        let busy = |row: &domain::rows::CodingSession| {
            node_run_busy(row, queries::coding_session_display(row, Some("open")), 0)
        };
        assert!(busy(&session("running", false, true)));
        assert!(!busy(&session("running", false, false)));
        assert!(!busy(&session("running", true, true)), "parked on a question");
        assert!(!busy(&session("in_review", false, true)), "its PR is up");
        assert!(!busy(&session("ended", false, true)), "not live");
    }

    /// The engine's serialization edges (drawn dashed in the graph) come
    /// off the row's jsonb array, TEXT-stored and re-parsed.
    #[test]
    fn the_serialization_targets_come_off_the_row() {
        assert_eq!(node_row(None).after_ids(), vec!["n-2", "n-3"]);
    }

    /// EXP-1014 — the FILL rule: a graph that fits is drawn at its natural
    /// size (never stretched), a wider one shrinks to exactly the width it
    /// was given, and the shrink stops at [`MIN_GRAPH_SCALE`] rather than
    /// grinding the chips into nothing.
    #[test]
    fn the_graph_scales_down_into_the_width_it_was_given() {
        // One column, plenty of room: natural size.
        assert_eq!(graph_scale(0, 900.), 1.);
        // Exactly as wide as it needs: still natural.
        let three = natural_graph_width(2);
        assert_eq!(graph_scale(2, three), 1.);
        // Half the room: the layout is exactly that wide afterwards.
        let scale = graph_scale(2, three / 2.);
        assert!((scale - 0.5).abs() < 1e-5, "{scale}");
        assert!((three * scale - three / 2.).abs() < 0.01);
        // Narrower than the floor: it stops there and the page scrolls.
        assert_eq!(graph_scale(8, 10.), MIN_GRAPH_SCALE);
        // An unmeasured cell never scales at all.
        assert_eq!(graph_scale(8, 0.), 1.);
    }

    /// EXP-1014 — the grid is NEVER laid out wider than the cell it sits in,
    /// whatever the floor scale left over: a deep plan scrolls inside its own
    /// cell instead of painting over the node panel beside it. The clamp's
    /// only floor is one chip, which is what a cell narrower than that (a
    /// window ~500px wide with a node picked) gets.
    #[test]
    fn the_grid_never_overruns_the_cell_it_was_given() {
        // Room to spare: the natural layout is untouched.
        assert_eq!(clamped_view_w(460., 900.), 460.);
        // A cell UNDER one chip wide — the old guard switched the clamp off
        // exactly here and the grid painted over the panel.
        assert_eq!(clamped_view_w(460., 150.), NODE_W);
        // Wider than one chip: clamped to the cell itself.
        assert_eq!(clamped_view_w(460., 320.), 320.);
        // An unmeasured cell still clamps, to the same floor.
        assert_eq!(clamped_view_w(460., 0.), NODE_W);
    }

    /// EXP-1014 — the first frame measures the PAGE's cell, not the window:
    /// the rail (with whatever list is folded beside it) and the shell's
    /// cutout are not the graph's to use, or every open would lay out too
    /// wide and visibly re-scale the moment the probe lands.
    #[test]
    fn the_first_frame_budgets_around_the_rail_and_the_panel() {
        let rail = crate::shell::left_column_width_for(crate::shell::LeftOccupant::Rail);
        // A wide window: the page column caps it, panel or no panel.
        let wide = graph_cell_width(2200., rail, false);
        assert_eq!(wide, WORKFLOW_COLUMN_W - PAGE_GUTTER);
        assert_eq!(
            graph_cell_width(2200., rail, true),
            wide - NODE_PANEL_W - NODE_PANEL_GAP
        );
        // A narrow one: the rail and the cutout come off the top, so the
        // cell is smaller than the window says.
        let narrow = graph_cell_width(900., rail, false);
        assert_eq!(
            narrow,
            900. - rail - 2. * crate::shell::PANEL_MARGIN - PAGE_GUTTER
        );
        assert!(narrow < 900. - PAGE_GUTTER);
        // Nothing left at all: one chip, and the page's own column scrolls.
        assert_eq!(graph_cell_width(520., rail, true), NODE_W);
    }

    /// EXP-1014 — what the node panel may still pick, by status. The
    /// server's `updateNode` asserts a draft for the KIND (it shapes the
    /// plan) and never for the RISK, which is the one lever onto the strong
    /// model and stays adjustable while the run is up.
    #[test]
    fn risk_stays_a_pick_after_the_plan_is_fixed() {
        assert!(node_pick_enabled(NodePick::Kind, true));
        assert!(!node_pick_enabled(NodePick::Kind, false));
        assert!(node_pick_enabled(NodePick::Risk, true));
        assert!(node_pick_enabled(NodePick::Risk, false));
    }

    /// EXP-1014 — a node's Model line reads the ONE launch rule, and spells
    /// the alias the way every picker in the app does. An alias this build
    /// does not know still renders (a newer server).
    #[test]
    fn the_node_model_line_reads_the_launch_rule() {
        let launch = |raw: serde_json::Value| normalize_workflow_launch(&raw);
        let claude = launch(serde_json::json!({ "model": "fable", "strongModel": "opus" }));
        // A leaf runs on the cheap model, a contract/integration/high-risk
        // node on the strong one.
        assert_eq!(
            model_label(&claude, &model_for_node(&claude, "leaf", "low")),
            "Fable"
        );
        assert_eq!(
            model_label(&claude, &model_for_node(&claude, "contract", "low")),
            "Opus"
        );
        assert_eq!(
            model_label(&claude, &model_for_node(&claude, "leaf", "high")),
            "Opus"
        );
        // An empty launch falls back to the agent's contract defaults, and
        // a codex workflow is labelled off codex's own list.
        let codex = launch(serde_json::json!({ "agent": "codex" }));
        assert_eq!(
            model_label(&codex, &model_for_node(&codex, "leaf", "low")),
            "GPT-5.6 Sol"
        );
        assert_eq!(
            model_label(&codex, &model_for_node(&codex, "integration", "low")),
            "GPT-5.6 Luna"
        );
        assert_eq!(model_label(&claude, "brand-new"), "brand-new");
    }

    /// EXP-984 — the node panel's review block reads the synced blob: the
    /// line, the findings and the command the reviewer ran.
    #[test]
    fn the_review_block_reads_the_synced_verdict() {
        let row: domain::rows::WorkflowNodeRow = serde_json::from_value(serde_json::json!({
            "id": "n-1",
            "workflow_id": "wf-1",
            "issue_id": "i-1",
            "review_round": 2,
            "review": r#"{"verdict":"request_changes","findings":"src/a.rs:4 off by one",
                "oracle":{"command":"cargo test -p coding","passed":false},"round":2}"#,
        }))
        .expect("the row hydrates");
        let review = row.review_facts().expect("a verdict");
        assert_eq!(
            domain::workflow_view::workflow_review_line(ReviewLine {
                verdict: &review.verdict,
                round: review.round,
                oracle: review.oracle_passed,
                approved: row.approved_at.is_some(),
            }),
            "Changes requested · round 2 · checks failed"
        );
        assert_eq!(review.findings, "src/a.rs:4 off by one");
        assert_eq!(
            review.oracle_command.as_deref(),
            Some("cargo test -p coding")
        );
        // A node nobody reviewed renders no block at all.
        assert!(node_row(None).review_facts().is_none());
    }
}
