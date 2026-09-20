//! One workflow's detail (EXP-981): the graph, the picked node's panel and
//! the "How it runs" configuration.
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

use std::collections::HashMap;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, App, AppContext as _, ClickEvent, Entity, InteractiveElement as _, IntoElement,
    ParentElement, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use sync::Store;

use domain::workflow_view::{
    workflow_cycle_note, workflow_edge_style, workflow_final_pr_caption, workflow_merge_train,
    workflow_metric_rows, workflow_node_caption, workflow_node_needs_approval, workflow_node_title,
    workflow_node_tone, workflow_review_line, workflow_shape_line, workflow_start_blocker,
    workflow_train_step_label, CaptionNode, EdgeNode, EdgeRelation, ReviewLine, StartableWorkflow,
    TrainNode, WorkflowNodeTone, ADMIT_NODE_LABEL, AGENT_REVIEW_TITLE, APPROVE_NODE_LABEL,
    BUDGET_MINUTES_LABEL, BUDGET_TITLE, BUDGET_TOKENS_LABEL, CANCEL_WORKFLOW_CONFIRM,
    CANCEL_WORKFLOW_LABEL, CONTRACT_MODEL_LABEL, CONTRACT_PUBLISHED_LABEL, DELETE_WORKFLOW_LABEL,
    DISMISS_NODE_LABEL, FINAL_PR_TITLE, INTEGRATION_MODEL_LABEL, MERGES_IN_FIRST_LABEL,
    MERGE_TRAIN_EMPTY, MERGE_TRAIN_TITLE, METRICS_TITLE,
    OPEN_RUN_LABEL, RUNNING_NOW_LABEL, PAUSE_WORKFLOW_LABEL, PLAN_WORKFLOW_LABEL, PROPOSED_NODE_NOTE,
    RESUME_WORKFLOW_LABEL, RETRY_NODE_LABEL, REVIEW_MODEL_LABEL, RISK_MODEL_LABEL,
    SAME_AS_MODEL_LABEL, SKIP_NODE_CONFIRM, SKIP_NODE_LABEL, START_WORKFLOW_LABEL,
    WITHDRAW_APPROVAL_LABEL,
};

use crate::actions_view::page_scaffold_with;
use crate::icons::registry;
use crate::issue_graph::{grid_view, GridEdge, GridGeometry, GridNode};
use crate::navigation::{nav_for_window, ChatSeed, Navigation, Screen};
use crate::queries;

/// The page column's cap — wide enough for the graph beside its panel.
const WORKFLOW_COLUMN_W: f32 = 1024.;
/// The graph's viewport; past it the grid scrolls.
const GRAPH_VIEW_W: f32 = 640.;
const GRAPH_VIEW_H: f32 = 460.;
/// A workflow node is a CIRCLE with its two lines (the title over its ONE
/// caption) centred underneath. The edges run circle to circle, never
/// through a label.
const NODE_CIRCLE: f32 = 30.;
const NODE_W: f32 = 140.;
const NODE_H: f32 = NODE_CIRCLE + 4. + 16. + 16.;
/// The air an edge keeps from the circle it leaves or enters.
const NODE_EDGE_AIR: f32 = 4.;

fn graph_geometry() -> GridGeometry {
    GridGeometry {
        node_w: NODE_W,
        node_h: NODE_H,
        col_gap: 64.,
        lane_gap: 14.,
        view_w: GRAPH_VIEW_W,
        view_h: GRAPH_VIEW_H,
        edge_out: (NODE_W / 2. + NODE_CIRCLE / 2. + NODE_EDGE_AIR, NODE_CIRCLE / 2.),
        edge_in: (NODE_W / 2. - NODE_CIRCLE / 2. - NODE_EDGE_AIR, NODE_CIRCLE / 2.),
    }
}

/// The grid key of the final-PR box (EXP-982) — deliberately not a uuid, so
/// it can never collide with a `workflow_nodes` row id.
const FINAL_PR_KEY: &str = "workflow-final-pr";

/// The Gate picks, in contract order, with their shipped labels.
const GATE_CHOICES: [(&str, &str); 3] = [
    ("No gate", domain::contract::WF_GATE_NONE),
    ("Agent review", domain::contract::WF_GATE_AGENT),
    ("Human review", domain::contract::WF_GATE_HUMAN),
];

/// The Start picks, in contract order, with their shipped labels.
const START_ON_CHOICES: [(&str, &str); 3] = [
    ("On contract", domain::contract::WF_START_ON_CONTRACT),
    ("On PR open", domain::contract::WF_START_ON_PR_OPEN),
    ("When landed", domain::contract::WF_START_ON_LANDED),
];

/// `1` to `8` — the contract's `workflowMaxParallel` cap.
const MAX_PARALLEL_CAP: usize = 8;

/// EXP-984: the models an AGENT review may run on — claude's, plus the blank
/// "the engine picks one" (which is the author's model, swapped for a
/// high-risk node). Shown only under the Agent-review gate.
const REVIEW_MODEL_CHOICES: [(&str, &str); 4] = [
    ("Default", ""),
    ("Fable", "fable"),
    ("Opus", "opus"),
    ("Sonnet", "sonnet"),
];

/// EXP-1002: what a PHASE row offers — the agent's own models, with the blank
/// "Same as Model" in front. That blank is the workflow's Model row, NOT the
/// CLI default, so these are never [`crate::coding_selects::MODEL_CHOICES`]
/// with a "CLI default" head. Both are sliced off the model picks themselves,
/// so a new model reaches every row at once.
const PHASE_MODEL_CHOICES: [(&str, &str); 4] = [
    (SAME_AS_MODEL_LABEL, ""),
    crate::coding_selects::MODEL_CHOICES[0],
    crate::coding_selects::MODEL_CHOICES[1],
    crate::coding_selects::MODEL_CHOICES[2],
];
const CODEX_PHASE_MODEL_CHOICES: [(&str, &str); 4] = [
    (SAME_AS_MODEL_LABEL, ""),
    // Index 0 is codex's own blank "CLI default" — the phase rows have their
    // own blank and must not offer a second one.
    crate::coding_selects::CODEX_MODEL_CHOICES[1],
    crate::coding_selects::CODEX_MODEL_CHOICES[2],
    crate::coding_selects::CODEX_MODEL_CHOICES[3],
];

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
    /// EXP-984: the picked node's budget, saved on blur like the name.
    budget_minutes: Entity<InputState>,
    budget_tokens: Entity<InputState>,
    /// The node the two budget fields currently hold, so picking another one
    /// reseeds them (and a save can never write onto the wrong node).
    budget_node: Option<String>,
    /// EXP-984: whether the agent review's findings are unfolded.
    findings_expanded: bool,
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
        // EXP-984: the node budget's two fields. Both save on blur (and on
        // Enter), exactly like the workflow name above.
        let budget_minutes =
            cx.new(|cx| InputState::new(window, cx).placeholder(BUDGET_MINUTES_LABEL));
        let budget_tokens =
            cx.new(|cx| InputState::new(window, cx).placeholder(BUDGET_TOKENS_LABEL));
        for field in [&budget_minutes, &budget_tokens] {
            subscriptions.push(cx.subscribe_in(
                field,
                window,
                |this, _, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                        this.save_budget(cx);
                    }
                },
            ));
        }
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
            budget_minutes,
            budget_tokens,
            budget_node: None,
            findings_expanded: false,
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
        // The budget fields belong to a node of the OLD workflow: forget
        // them, or the next blur would save onto it.
        self.budget_node = None;
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

    /// EXP-984 — save the picked node's budget. Both fields empty (or
    /// unreadable) is a budget of NOTHING, which clears the column; anything
    /// else sends the positive whole numbers the server's schema takes.
    fn save_budget(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(node_id) = self.budget_node.clone() else {
            return;
        };
        let Some(issue_id) = self
            .nodes(cx)
            .into_iter()
            .find(|node| node.id == node_id)
            .and_then(|node| node.issue_id)
        else {
            return;
        };
        let minutes = budget_field(&self.budget_minutes, cx);
        let tokens = budget_field(&self.budget_tokens, cx);
        let mut input =
            api::workflows::WorkflowNodeUpdate::new(self.workflow_id.clone(), issue_id);
        input.budget = match (minutes, tokens) {
            (None, None) => api::Patch::Null,
            (minutes, tokens) => {
                api::Patch::Set(api::workflows::NodeBudget { minutes, tokens })
            }
        };
        spawn_update_node(input, cx);
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
            facts.insert(
                FINAL_PR_KEY.to_string(),
                NodeFacts {
                    title: FINAL_PR_TITLE.to_string(),
                    caption,
                    tone: match row.final_pr_state.as_deref() {
                        Some("merged") => WorkflowNodeTone::Success,
                        Some("closed") => WorkflowNodeTone::Muted,
                        _ => WorkflowNodeTone::Active,
                    },
                    compound: false,
                    glyph: Some(registry::NOTIFICATION_PR_MERGED),
                    proposed: false,
                    run: None,
                },
            );
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
        let grid = grid_view(&grid_nodes, &edges, graph_geometry(), &notes, &render, cx);
        // The runs that are up right now, one tap away: a node's circle says
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
                        Some(run_pill(&node.id, &facts.title, run, cx))
                    })),
            )
            .child(grid)
            .into_any_element()
    }

    /// The picked node's panel: its issue, its members, Kind / Risk, the
    /// `touches` globs and a way into the issue.
    fn render_node_panel(
        &mut self,
        row: &domain::rows::WorkflowRow,
        nodes: &[domain::rows::WorkflowNodeRow],
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let picked = self.picked.clone()?;
        let node = nodes.iter().find(|node| node.id == picked)?.clone();
        let issue_id = node.issue_id.clone()?;
        // EXP-984: the budget fields follow the picked node.
        self.seed_budget(&node, window, cx);
        let facts = NodeFacts::derive(&node, row, cx);
        let muted = cx.theme().muted_foreground;
        let workflow_id = self.workflow_id.clone();
        let draft = row.status_wire() == domain::contract::WF_STATUS_DRAFT;
        // EXP-984: a node nobody admitted yet is decided on, not run.
        let proposed = node.state_wire() == domain::contract::WF_NODE_STATE_PROPOSED;
        let settled = matches!(node.state_wire(), "landed" | "skipped");

        let members: Vec<gpui::AnyElement> = node
            .member_ids()
            .iter()
            .map(|member| issue_chip_for(member, cx))
            .collect();
        // EXP-983: the serialization edges, as the issue chips of the nodes
        // this one merges in first.
        let merges_first: Vec<gpui::AnyElement> = node
            .after_ids()
            .iter()
            .filter_map(|after| {
                let target = nodes.iter().find(|candidate| &candidate.id == after)?;
                Some(issue_chip_for(target.issue_id.as_deref()?, cx))
            })
            .collect();
        let touches: Vec<gpui::AnyElement> = node
            .touches
            .iter()
            .map(|glob| {
                div()
                    .font_family("monospace")
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(glob.clone()))
                    .into_any_element()
            })
            .collect();

        Some(
            v_flex()
                .w(gpui::px(280.))
                .flex_shrink_0()
                .min_w_0()
                .gap_3()
                // The badge IS the way into the issue.
                .child(issue_chip_link(&issue_id, cx))
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from(facts.caption.clone())),
                )
                .when(!members.is_empty(), |this| {
                    this.child(v_flex().min_w_0().gap_1().children(members))
                })
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
                // EXP-983: the contract this node announced, and the
                // siblings it has to merge in before it can land.
                .when_some(contract_published_line(&node), |this, line| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(SharedString::from(line)),
                    )
                })
                .when(!merges_first.is_empty(), |this| {
                    this.child(
                        v_flex()
                            .min_w_0()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(SharedString::from(MERGES_IN_FIRST_LABEL)),
                            )
                            .children(merges_first),
                    )
                })
                .child(crate::surface::glass_group_rows(vec![
                    pick_row(
                        "workflow-node-kind",
                        "Kind",
                        &WF_KIND_CHOICES,
                        node.kind_wire(),
                        draft,
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
                        // Risk stays adjustable after the plan is fixed.
                        true,
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
                ]))
                .when(!touches.is_empty(), |this| {
                    this.child(
                        v_flex()
                            .min_w_0()
                            .gap_1()
                            .child(div().text_xs().text_color(muted).child("Touches"))
                            .children(touches),
                    )
                })
                // EXP-984: the latest AGENT review — its verdict line in the
                // tone it earned, the findings themselves, and the command
                // the reviewer actually ran.
                .children(self.render_agent_review(&node, cx))
                // EXP-984: what this node may spend. A settled node has
                // spent it; a proposal has not started.
                .when(!proposed && !settled, |this| {
                    this.child(self.render_budget(window, cx))
                })
                // EXP-982: why the node is failed or waiting, in the
                // sentence the engine reported.
                .when_some(node.note.clone(), |this, note| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(SharedString::from(note)),
                    )
                })
                // EXP-982: the node's run, its pull request, the gate and
                // the two ways out of a failure. EXP-984: a proposal has
                // none of them — it is admitted or dismissed, nothing else.
                .children((!proposed).then(|| node_run_actions(&node, &issue_id, cx)).unwrap_or_default())
                .children((!proposed).then(|| node_gate_actions(&node, row)).unwrap_or_default())
                .into_any_element(),
        )
    }

    /// EXP-984 — the picked node's budget fields, reseeded whenever the
    /// picked node changes (never while the same node is being edited, or a
    /// synced echo would eat what is being typed).
    fn seed_budget(
        &mut self,
        node: &domain::rows::WorkflowNodeRow,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.budget_node.as_deref() == Some(node.id.as_str()) {
            return;
        }
        self.budget_node = Some(node.id.clone());
        let (minutes, tokens) = node.budget_limits();
        let text = |value: Option<i64>| {
            value.map(|value| value.to_string()).unwrap_or_default()
        };
        self.budget_minutes
            .update(cx, |input, cx| input.set_value(text(minutes), window, cx));
        self.budget_tokens
            .update(cx, |input, cx| input.set_value(text(tokens), window, cx));
    }

    /// EXP-984 — the `Budget` block: two optional whole numbers, saved on
    /// blur. Both empty clears the budget.
    fn render_budget(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let field = |state: &Entity<InputState>| {
            crate::surface::glass_row_input(crate::controls::glass_input(state, window, cx))
                .into_any_element()
        };
        v_flex()
            .min_w_0()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(BUDGET_TITLE)),
            )
            .child(crate::surface::glass_group_rows(vec![
                crate::surface::glass_input_row(
                    BUDGET_MINUTES_LABEL,
                    field(&self.budget_minutes),
                    cx,
                ),
                crate::surface::glass_input_row(
                    BUDGET_TOKENS_LABEL,
                    field(&self.budget_tokens),
                    cx,
                ),
            ]))
            .into_any_element()
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

    /// "How it runs": the start configuration, persisted with
    /// `workflows.update`. Reuses the composer's device rows and the same
    /// launch vocabulary the Agent page offers.
    fn render_how_it_runs(
        &self,
        row: &domain::rows::WorkflowRow,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let workflow_id = self.workflow_id.clone();
        // EXP-982: how a workflow runs is fixed the moment it leaves draft —
        // every row below reads, none of them writes. The server refuses the
        // same edits with a sentence.
        let draft = row.status_wire() == domain::contract::WF_STATUS_DRAFT;
        let launch = api::workflows::from_row(row).launch;
        let agent = launch.agent.clone().unwrap_or_default();
        let claude = agent.is_empty() || agent == "claude";

        let update_launch = {
            let workflow_id = workflow_id.clone();
            let launch = launch.clone();
            move |edit: Box<dyn FnOnce(&mut api::workflows::WorkflowLaunch)>, cx: &mut App| {
                let mut next = launch.clone();
                edit(&mut next);
                let mut input = api::workflows::WorkflowUpdate::new(workflow_id.clone());
                input.launch = Some(next);
                spawn_update(input, cx);
            }
        };

        let devices = queries::launch_devices(cx);
        let mut rows: Vec<gpui::Div> =
            vec![device_row(&workflow_id, row.device_id.as_deref(), devices, draft, cx)];
        rows.push(pick_row(
            "workflow-agent",
            "Agent",
            &crate::coding_selects::AGENT_CHOICES,
            &agent,
            draft,
            {
                let update = update_launch.clone();
                move |value: &str, cx: &mut App| {
                    let value = value.to_string();
                    update(
                        Box::new(move |launch| {
                            launch.agent = Some(value);
                            // A model belongs to ONE agent's closed set.
                            launch.model = None;
                            launch.effort = None;
                            launch.subagent_model = None;
                            // EXP-1002: so does a per-phase model.
                            launch.contract_model = None;
                            launch.integration_model = None;
                            launch.risk_model = None;
                            // EXP-984: a review model belongs to that same
                            // closed set.
                            launch.review_model = None;
                        }),
                        cx,
                    );
                }
            },
            cx,
        ));
        let model_choices = if claude {
            &crate::coding_selects::MODEL_CHOICES[..]
        } else {
            &crate::coding_selects::CODEX_MODEL_CHOICES[..]
        };
        rows.push(pick_row(
            "workflow-model",
            "Model",
            model_choices,
            launch.model.as_deref().unwrap_or_default(),
            draft,
            {
                let update = update_launch.clone();
                move |value: &str, cx: &mut App| {
                    let value = value.to_string();
                    update(
                        Box::new(move |launch| {
                            launch.model = (!value.is_empty()).then_some(value);
                        }),
                        cx,
                    );
                }
            },
            cx,
        ));
        // EXP-1002: the two phases that may opt OUT of the model above. Both
        // rows exist whatever the agent — a phase pin is not claude's.
        let phase_choices = if claude {
            &PHASE_MODEL_CHOICES[..]
        } else {
            &CODEX_PHASE_MODEL_CHOICES[..]
        };
        rows.push(pick_row(
            "workflow-contract-model",
            CONTRACT_MODEL_LABEL,
            phase_choices,
            launch.contract_model.as_deref().unwrap_or_default(),
            draft,
            {
                let update = update_launch.clone();
                move |value: &str, cx: &mut App| {
                    let value = value.to_string();
                    update(
                        Box::new(move |launch| {
                            launch.contract_model = (!value.is_empty()).then_some(value);
                        }),
                        cx,
                    );
                }
            },
            cx,
        ));
        rows.push(pick_row(
            "workflow-integration-model",
            INTEGRATION_MODEL_LABEL,
            phase_choices,
            launch.integration_model.as_deref().unwrap_or_default(),
            draft,
            {
                let update = update_launch.clone();
                move |value: &str, cx: &mut App| {
                    let value = value.to_string();
                    update(
                        Box::new(move |launch| {
                            launch.integration_model = (!value.is_empty()).then_some(value);
                        }),
                        cx,
                    );
                }
            },
            cx,
        ));
        // EXP-1002: the risk pin outranks both phases on a `risk: high` node.
        rows.push(pick_row(
            "workflow-risk-model",
            RISK_MODEL_LABEL,
            phase_choices,
            launch.risk_model.as_deref().unwrap_or_default(),
            draft,
            {
                let update = update_launch.clone();
                move |value: &str, cx: &mut App| {
                    let value = value.to_string();
                    update(
                        Box::new(move |launch| {
                            launch.risk_model = (!value.is_empty()).then_some(value);
                        }),
                        cx,
                    );
                }
            },
            cx,
        ));
        // EXP-981: claude only, hidden for every other agent (the composer's
        // `subagent_model_pin` rule).
        if claude {
            rows.push(pick_row(
                "workflow-subagent-model",
                "Subagent model",
                &crate::coding_selects::SUBAGENT_MODEL_CHOICES,
                launch.subagent_model.as_deref().unwrap_or_default(),
                draft,
                {
                    let update = update_launch.clone();
                    move |value: &str, cx: &mut App| {
                        let value = value.to_string();
                        update(
                            Box::new(move |launch| {
                                launch.subagent_model = (!value.is_empty()).then_some(value);
                            }),
                            cx,
                        );
                    }
                },
                cx,
            ));
        }
        let effort_choices = if claude {
            &crate::coding_selects::EFFORT_CHOICES[..]
        } else {
            &crate::coding_selects::CODEX_EFFORT_CHOICES[..]
        };
        rows.push(pick_row(
            "workflow-effort",
            "Effort",
            effort_choices,
            launch.effort.as_deref().unwrap_or_default(),
            draft,
            {
                let update = update_launch.clone();
                move |value: &str, cx: &mut App| {
                    let value = value.to_string();
                    update(
                        Box::new(move |launch| {
                            launch.effort = (!value.is_empty()).then_some(value);
                        }),
                        cx,
                    );
                }
            },
            cx,
        ));
        rows.push(parallel_row(row.max_parallel(), draft, {
            let update = update_launch.clone();
            move |value: usize, cx: &mut App| {
                update(
                    Box::new(move |launch| launch.max_parallel = Some(value as u32)),
                    cx,
                );
            }
        }, cx));
        rows.push(pick_row(
            "workflow-gate",
            "Gate",
            &GATE_CHOICES,
            row.gate.as_deref().unwrap_or(domain::contract::WF_GATE_HUMAN),
            draft,
            {
                let workflow_id = workflow_id.clone();
                move |value: &str, cx: &mut App| {
                    let mut input = api::workflows::WorkflowUpdate::new(workflow_id.clone());
                    input.gate = Some(value.to_string());
                    spawn_update(input, cx);
                }
            },
            cx,
        ));
        // EXP-984: what an AGENT review runs on — only worth a row when the
        // gate actually reviews.
        if row.gate.as_deref() == Some(domain::contract::WF_GATE_AGENT) {
            rows.push(pick_row(
                "workflow-review-model",
                REVIEW_MODEL_LABEL,
                &REVIEW_MODEL_CHOICES,
                launch.review_model.as_deref().unwrap_or_default(),
                draft,
                {
                    let update = update_launch.clone();
                    move |value: &str, cx: &mut App| {
                        let value = value.to_string();
                        update(
                            Box::new(move |launch| {
                                launch.review_model = (!value.is_empty()).then_some(value);
                            }),
                            cx,
                        );
                    }
                },
                cx,
            ));
        }
        rows.push(pick_row(
            "workflow-start-on",
            "Start",
            &START_ON_CHOICES,
            row.start_on
                .as_deref()
                .unwrap_or(domain::contract::WF_START_ON_CONTRACT),
            draft,
            {
                let workflow_id = workflow_id.clone();
                move |value: &str, cx: &mut App| {
                    let mut input = api::workflows::WorkflowUpdate::new(workflow_id.clone());
                    input.start_on = Some(value.to_string());
                    spawn_update(input, cx);
                }
            },
            cx,
        ));

        v_flex()
            .min_w_0()
            .child(crate::surface::glass_section_header("How it runs", None, cx))
            .child(crate::surface::glass_group_rows(rows))
            .into_any_element()
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
        let team_id = row.team_id.clone().unwrap_or_default();
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

        let graph = self.render_graph(&row, &nodes, cx);
        let panel = self.render_node_panel(&row, &nodes, _window, cx);
        let body = h_flex()
            .w_full()
            .min_w_0()
            .items_start()
            .gap_4()
            .child(div().flex_1().min_w_0().child(graph))
            .children(panel);

        let how = self.render_how_it_runs(&row, cx);
        let train = (!draft).then(|| render_merge_train(&row, &nodes, cx));
        // EXP-984: the run's counters. A draft has run nothing to count.
        let metrics = (!draft).then(|| render_metrics(&row, cx)).flatten();
        let _ = team_id;

        page_scaffold_with(
            "workflow-screen-scroll",
            &self.scroll,
            v_flex()
                .gap_6()
                .child(header)
                .child(body)
                .children(train)
                .child(how)
                .children(metrics),
            WORKFLOW_COLUMN_W,
        )
    }
}

/// The node panel's run + PR affordances: the coding session this node runs
/// in (opened the way every other run is), and its issue's open pull
/// request. Both only once they exist.
fn node_run_actions(
    node: &domain::rows::WorkflowNodeRow,
    issue_id: &str,
    cx: &App,
) -> Vec<gpui::AnyElement> {
    let mut actions = Vec::new();
    if let Some(session_id) = node.session_id.clone() {
        actions.push(
            Button::new("workflow-node-run")
                .ghost()
                .small()
                .icon(Icon::from(registry::ACTION_RUN))
                .label(OPEN_RUN_LABEL)
                .on_click(move |_, window, cx| {
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
    let pr = Store::try_global(cx)
        .and_then(|store| store.collections().issues.read(cx).get(issue_id).cloned())
        .and_then(|issue| issue.pr_url.clone());
    if let Some(url) = pr {
        actions.push(
            Button::new("workflow-node-pr")
                .ghost()
                .small()
                .icon(Icon::from(registry::NAV_REVIEWS))
                .label("Open pull request")
                .on_click(move |_, _window, cx| cx.open_url(&url))
                .into_any_element(),
        );
    }
    actions
}

/// The human gate and the failure exits: approve (or take it back) while the
/// PR is up, retry or skip once the node failed. Every rule is the server's;
/// these buttons only appear where it would say yes.
fn node_gate_actions(
    node: &domain::rows::WorkflowNodeRow,
    row: &domain::rows::WorkflowRow,
) -> Vec<gpui::AnyElement> {
    let gate = row.gate.as_deref().unwrap_or(domain::contract::WF_GATE_HUMAN);
    let state = node.state_wire();
    let approved = node.approved_at.is_some();
    let node_id = node.id.clone();
    let mut actions: Vec<gpui::AnyElement> = Vec::new();
    if state == "in_review" && workflow_node_needs_approval(gate, node.kind_wire()) && !approved {
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

/// EXP-983 — `Contract published · 12m`, once the node's run announced one
/// (`exponential_workflows_checkpoint`). `None` while it has not.
fn contract_published_line(node: &domain::rows::WorkflowNodeRow) -> Option<String> {
    let at = node.checkpoint_at.as_deref()?;
    let relative = crate::inbox::relative_time(at);
    Some(if relative.is_empty() {
        CONTRACT_PUBLISHED_LABEL.to_string()
    } else {
        format!("{CONTRACT_PUBLISHED_LABEL} · {relative}")
    })
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

/// EXP-982 — the merge train under the graph: every node whose PR is up, in
/// landing order, each with where it stands. Hidden on a draft.
fn render_merge_train(
    row: &domain::rows::WorkflowRow,
    nodes: &[domain::rows::WorkflowNodeRow],
    cx: &App,
) -> gpui::AnyElement {
    let muted = cx.theme().muted_foreground;
    let gate = row.gate.as_deref().unwrap_or(domain::contract::WF_GATE_HUMAN);
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
    let entries = workflow_merge_train(&train_nodes, gate);
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

/// EXP-984 — one budget field as a positive whole number, or `None` when it
/// is empty (or not one), which is what clears that half of the budget.
fn budget_field(state: &Entity<InputState>, cx: &App) -> Option<u32> {
    state
        .read(cx)
        .value()
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|value| *value > 0)
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

/// Everything one node's box renders, joined off the collections once.
struct NodeFacts {
    /// `EXP-14 +3` for a compound node, the bare identifier otherwise;
    /// EMPTY while the node's issue has not synced.
    title: String,
    /// The ONE caption under the title.
    caption: String,
    tone: WorkflowNodeTone,
    /// A compound node draws a second card edge peeking out behind.
    compound: bool,
    /// EXP-982: the state's GLYPH, so a state reads by shape as well as by
    /// colour. `None` for the states that are only a word.
    glyph: Option<crate::icons::ExpIcon>,
    /// EXP-984: a follow-up nobody admitted yet — drawn with a DASHED
    /// outline, because it is not part of the run.
    proposed: bool,
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
            busy: live && session.agent_busy.unwrap_or(false),
            tone: queries::session_dot_tone(
                queries::SessionDotFacts::from_display(display, !live, false),
                cx.theme().muted_foreground,
            ),
        })
    }
}

impl NodeFacts {
    fn derive(
        node: &domain::rows::WorkflowNodeRow,
        workflow: &domain::rows::WorkflowRow,
        cx: &App,
    ) -> Self {
        let members = node.member_ids();
        let identifier = node
            .issue_id
            .as_deref()
            .and_then(|issue_id| {
                Store::try_global(cx)?
                    .collections()
                    .issues
                    .read(cx)
                    .get(issue_id)
                    .map(|issue| issue.identifier.clone())
            })
            .unwrap_or_default();
        Self {
            // A node whose issue row is not synced renders its caption only.
            title: if identifier.is_empty() {
                String::new()
            } else {
                workflow_node_title(&identifier, members.len())
            },
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
            proposed: node.state_wire() == domain::contract::WF_NODE_STATE_PROPOSED,
            run: NodeRun::derive(node, cx),
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

/// ONE node: a CIRCLE in the ring its state earns — the state glyph inside,
/// the session's live dot while its run is up — over the title and its
/// caption. A compound node draws a second circle edge peeking out behind.
fn render_node_box(
    node_id: &str,
    facts: Option<&NodeFacts>,
    cycled: bool,
    selected: bool,
    on_pick: Rc<dyn Fn(&str, &mut App)>,
    cx: &App,
) -> gpui::AnyElement {
    let theme = cx.theme();
    let tone_color = match facts.map(|facts| facts.tone) {
        Some(WorkflowNodeTone::Amber) => theme.warning,
        Some(WorkflowNodeTone::Danger) => theme.danger,
        Some(WorkflowNodeTone::Success) => theme.success,
        Some(WorkflowNodeTone::Active) => theme.foreground,
        _ => theme.muted_foreground,
    };
    let live_run = facts
        .and_then(|facts| facts.run.clone())
        .filter(|run| run.live);
    // The ring: red on a cycle, the session's own tone while its run is up,
    // the state's tone once that says something, the hairline otherwise.
    let quiet = facts.is_none_or(|facts| facts.tone == WorkflowNodeTone::Muted);
    let ring = if cycled {
        theme.danger
    } else if let Some(run) = live_run.as_ref() {
        run.tone
    } else if quiet {
        theme::tokens::glass::STROKE_STRONG.to_hsla()
    } else {
        tone_color
    };
    let fill = if live_run.is_some() || !quiet {
        ring.opacity(0.14)
    } else {
        theme::tokens::glass::FILL_CARD.to_hsla()
    };
    let title = facts.map(|facts| facts.title.clone()).unwrap_or_default();
    let caption = facts.map(|facts| facts.caption.clone()).unwrap_or_default();
    let compound = facts.is_some_and(|facts| facts.compound);
    // EXP-984: a proposal is drawn with a DASHED outline — it is in the
    // graph to be decided on, not because it is part of the run.
    let proposed = facts.is_some_and(|facts| facts.proposed);
    let glyph = facts.and_then(|facts| facts.glyph.clone());
    let target = node_id.to_string();

    let circle = div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .when(proposed, |this| this.border_dashed())
        .border_color(ring)
        .bg(fill)
        .map(|this| match (live_run.as_ref(), glyph) {
            (Some(run), _) => this.child(crate::surface::live_dot(run.tone, run.busy)),
            (None, Some(glyph)) => this.child(Icon::from(glyph).xsmall().text_color(tone_color)),
            (None, None) => this,
        });
    let disc = div()
        .relative()
        .flex_shrink_0()
        .size(gpui::px(NODE_CIRCLE))
        // The stacked circle: a second edge peeking out behind a compound node.
        .when(compound, |this| {
            this.child(
                div()
                    .absolute()
                    .left(gpui::px(4.))
                    .top(gpui::px(-3.))
                    .size(gpui::px(NODE_CIRCLE))
                    .rounded_full()
                    .border_1()
                    .border_color(theme::tokens::glass::STROKE_STRONG.to_hsla()),
            )
        })
        // The pick: a halo OUTSIDE the ring, so the state colour stays put.
        .when(selected, |this| {
            this.child(
                div()
                    .absolute()
                    .left(gpui::px(-3.))
                    .top(gpui::px(-3.))
                    .size(gpui::px(NODE_CIRCLE + 6.))
                    .rounded_full()
                    .border_1()
                    .border_color(theme.ring),
            )
        })
        .child(circle);

    div()
        .id(SharedString::from(format!("workflow-node-{node_id}")))
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .cursor_pointer()
        .on_click(move |_: &ClickEvent, _window, cx| on_pick(&target, cx))
        .child(disc)
        .child(
            v_flex()
                .w_full()
                .min_w_0()
                .items_center()
                .when(!title.is_empty(), |this| {
                    this.child(
                        div()
                            .max_w_full()
                            .truncate()
                            .text_xs()
                            .when(selected, |this| this.font_weight(gpui::FontWeight::MEDIUM))
                            .child(SharedString::from(title)),
                    )
                })
                .child(
                    div()
                        .max_w_full()
                        .truncate()
                        .text_xs()
                        .text_color(if live_run.is_some() { ring } else { tone_color }.opacity(
                            if quiet && live_run.is_none() { 0.8 } else { 1. },
                        ))
                        .child(SharedString::from(caption)),
                ),
        )
        .into_any_element()
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

fn issue_chip_element(issue_id: &str, cx: &App) -> crate::issue_chip::IssueChip {
    let row = Store::try_global(cx)
        .and_then(|store| store.collections().issues.read(cx).get(issue_id).cloned());
    let identifier = row
        .as_ref()
        .map(|issue| issue.identifier.clone())
        .unwrap_or_else(|| issue_id.to_string());
    let title = row.map(|issue| issue.title.clone()).unwrap_or_default();
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

/// The Max-parallel row: 1 to the contract's cap.
fn parallel_row(
    picked: usize,
    enabled: bool,
    on_pick: impl Fn(usize, &mut App) + Clone + 'static,
    cx: &App,
) -> gpui::Div {
    let control = Button::new("workflow-max-parallel")
        .ghost()
        .small()
        .label(SharedString::from(picked.to_string()))
        .disabled(!enabled)
        .dropdown_caret(true)
        .dropdown_menu(move |mut menu, _window, _cx| {
            for value in 1..=MAX_PARALLEL_CAP {
                let on_pick = on_pick.clone();
                menu = menu.item(
                    PopupMenuItem::new(SharedString::from(value.to_string()))
                        .checked(value == picked)
                        .on_click(move |_, _window, cx| on_pick(value, cx)),
                );
            }
            menu
        })
        .into_any_element();
    crate::surface::glass_picker_row("Max parallel", None, control, cx)
}

/// The Runner-device row: this user's own and shared machines, the composer's
/// own device list.
fn device_row(
    workflow_id: &str,
    picked: Option<&str>,
    devices: Vec<queries::LaunchDevice>,
    enabled: bool,
    cx: &App,
) -> gpui::Div {
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
    let control = Button::new("workflow-device")
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
        .into_any_element();
    crate::surface::glass_picker_row("Runner device", None, control, cx)
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

    /// EXP-983 — the node panel's contract line: the shipped label with the
    /// platform's relative time, and the bare label when the stamp cannot be
    /// read. A node that never announced one has no line at all.
    #[test]
    fn the_contract_line_carries_the_label_and_a_relative_time() {
        assert_eq!(contract_published_line(&node_row(None)), None);
        assert_eq!(
            contract_published_line(&node_row(Some("not a date"))).as_deref(),
            Some(CONTRACT_PUBLISHED_LABEL)
        );
        let line = contract_published_line(&node_row(Some("2026-09-19T10:00:00.000Z")))
            .expect("a stamp reads");
        assert!(line.starts_with(&format!("{CONTRACT_PUBLISHED_LABEL} · ")), "{line}");
    }

    /// The serialization edges the panel lists as chips come off the row's
    /// jsonb array, TEXT-stored and re-parsed.
    #[test]
    fn the_serialization_targets_come_off_the_row() {
        assert_eq!(node_row(None).after_ids(), vec!["n-2", "n-3"]);
    }

    /// EXP-984 — the Review-model row offers claude's models and the blank
    /// "the engine picks one", in the Model picker's own order.
    #[test]
    fn the_review_model_choices_are_claudes_plus_the_default() {
        assert_eq!(REVIEW_MODEL_CHOICES[0], ("Default", ""));
        let offered: Vec<(&str, &str)> = REVIEW_MODEL_CHOICES[1..].to_vec();
        assert_eq!(offered, crate::coding_selects::MODEL_CHOICES.to_vec());
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
