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
//! P2 ships DRAFT workflows: create, look, configure, plan, delete. There is
//! no Start button — the engine arrives with the next PR.

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
    workflow_cycle_note, workflow_node_caption, workflow_node_title, workflow_node_tone,
    workflow_shape_line, CaptionNode, EdgeNode, EdgeRelation,
    WorkflowNodeTone, DELETE_WORKFLOW_LABEL, PLAN_WORKFLOW_LABEL,
};

use crate::actions_view::page_scaffold_with;
use crate::icons::registry;
use crate::issue_graph::{grid_view, GridEdge, GridNode};
use crate::navigation::{nav_for_window, ChatSeed, Navigation, Screen};
use crate::queries;

/// The page column's cap — wide enough for the graph beside its panel.
const WORKFLOW_COLUMN_W: f32 = 1024.;
/// The graph's viewport; past it the grid scrolls.
const GRAPH_VIEW_W: f32 = 640.;
/// A workflow node's box: two lines (the title over its ONE caption).
const NODE_H: f32 = 42.;

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
        let edge_nodes: Vec<EdgeNode<'_>> = nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| {
                Some(EdgeNode {
                    id: node.id.as_str(),
                    issue_id: node.issue_id.as_deref()?,
                    member_issue_ids: members[index].iter().map(String::as_str).collect(),
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
        domain::workflow_view::workflow_edges(&edge_nodes, &edge_relations, &row.cycle_edges())
            .into_iter()
            .map(|edge| GridEdge {
                from: edge.from,
                to: edge.to,
                cycle: edge.cycle,
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
        let grid_nodes: Vec<GridNode> = nodes
            .iter()
            .map(|node| GridNode {
                key: node.id.clone(),
                wave: node.wave_index(),
                lane: node.lane_index(),
            })
            .collect();
        // Everything a box needs, keyed by node id — the render closure gets
        // only a `&App`, so the joins happen here.
        let facts: HashMap<String, NodeFacts> = nodes
            .iter()
            .map(|node| (node.id.clone(), NodeFacts::derive(node, row, cx)))
            .collect();
        let picked = self.picked.clone();
        let view = cx.entity().downgrade();
        let on_pick: Rc<dyn Fn(&str, &mut App)> = Rc::new(move |node_id: &str, cx: &mut App| {
            let Some(view) = view.upgrade() else {
                return;
            };
            let node_id = node_id.to_string();
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
        grid_view(
            &grid_nodes,
            &edges,
            GRAPH_VIEW_W,
            NODE_H,
            &notes,
            &render,
            cx,
        )
        .into_any_element()
    }

    /// The picked node's panel: its issue, its members, Kind / Risk, the
    /// `touches` globs and a way into the issue.
    fn render_node_panel(
        &self,
        row: &domain::rows::WorkflowRow,
        nodes: &[domain::rows::WorkflowNodeRow],
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let picked = self.picked.as_deref()?;
        let node = nodes.iter().find(|node| node.id == picked)?;
        let issue_id = node.issue_id.clone()?;
        let facts = NodeFacts::derive(node, row, cx);
        let muted = cx.theme().muted_foreground;
        let workflow_id = self.workflow_id.clone();
        let draft = row.status_wire() == domain::contract::WF_STATUS_DRAFT;

        let members: Vec<gpui::AnyElement> = node
            .member_ids()
            .iter()
            .map(|member| issue_chip_for(member, cx))
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

        let open_issue = issue_id.clone();
        Some(
            v_flex()
                .w(gpui::px(280.))
                .flex_shrink_0()
                .min_w_0()
                .gap_3()
                .child(issue_chip_for(&issue_id, cx))
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from(facts.caption.clone())),
                )
                .when(!members.is_empty(), |this| {
                    this.child(v_flex().min_w_0().gap_1().children(members))
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
                .child(
                    Button::new("workflow-node-open")
                        .ghost()
                        .small()
                        .icon(Icon::from(registry::UI_EXTERNAL_LINK))
                        .label("Open issue")
                        .on_click(move |_, window, cx| {
                            crate::navigation::navigate(
                                window,
                                cx,
                                Screen::IssueDetail {
                                    issue_id: open_issue.clone(),
                                },
                            );
                        }),
                )
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
            vec![device_row(&workflow_id, row.device_id.as_deref(), devices, cx)];
        rows.push(pick_row(
            "workflow-agent",
            "Agent",
            &crate::coding_selects::AGENT_CHOICES,
            &agent,
            true,
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
            true,
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
        // EXP-981: claude only, hidden for every other agent (the composer's
        // `subagent_model_pin` rule).
        if claude {
            rows.push(pick_row(
                "workflow-subagent-model",
                "Subagent model",
                &crate::coding_selects::SUBAGENT_MODEL_CHOICES,
                launch.subagent_model.as_deref().unwrap_or_default(),
                true,
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
            true,
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
        rows.push(parallel_row(row.max_parallel(), {
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
            true,
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
        rows.push(pick_row(
            "workflow-start-on",
            "Start",
            &START_ON_CHOICES,
            row.start_on
                .as_deref()
                .unwrap_or(domain::contract::WF_START_ON_CONTRACT),
            true,
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
                    // EXP-981: Plan seeds the Agent composer with the hidden
                    // planner builtin AND this workflow — the run is
                    // meaningless without the id.
                    .child(
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
                    .child(
                        crate::controls::ghost_icon_button(
                            "workflow-menu",
                            Icon::from(registry::UI_MORE),
                            cx,
                        )
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
            });

        let graph = self.render_graph(&row, &nodes, cx);
        let panel = self.render_node_panel(&row, &nodes, cx);
        let body = h_flex()
            .w_full()
            .min_w_0()
            .items_start()
            .gap_4()
            .child(div().flex_1().min_w_0().child(graph))
            .children(panel);

        let how = self.render_how_it_runs(&row, cx);
        let _ = team_id;

        page_scaffold_with(
            "workflow-screen-scroll",
            &self.scroll,
            v_flex().gap_6().child(header).child(body).child(how),
            WORKFLOW_COLUMN_W,
        )
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
        }
    }
}

/// ONE node's box: the title over its caption, inside the ring its state
/// earns, with a second card edge behind a compound node.
fn render_node_box(
    node_id: &str,
    facts: Option<&NodeFacts>,
    cycled: bool,
    selected: bool,
    on_pick: Rc<dyn Fn(&str, &mut App)>,
    cx: &App,
) -> gpui::AnyElement {
    let theme = cx.theme();
    let border = if cycled {
        theme.danger
    } else if selected {
        theme.ring
    } else {
        theme::tokens::glass::STROKE_CARD.to_hsla()
    };
    let caption_color = match facts.map(|facts| facts.tone) {
        Some(WorkflowNodeTone::Amber) => theme.warning,
        Some(WorkflowNodeTone::Danger) => theme.danger,
        Some(WorkflowNodeTone::Success) => theme.success,
        Some(WorkflowNodeTone::Active) => theme.foreground,
        _ => theme.muted_foreground,
    };
    let title = facts.map(|facts| facts.title.clone()).unwrap_or_default();
    let caption = facts.map(|facts| facts.caption.clone()).unwrap_or_default();
    let compound = facts.is_some_and(|facts| facts.compound);
    let target = node_id.to_string();
    let card = div()
        .id(SharedString::from(format!("workflow-node-{node_id}")))
        .absolute()
        .inset_0()
        .flex()
        .flex_col()
        .justify_center()
        .px_2()
        .rounded(gpui::px(6.))
        .border_1()
        .border_color(border)
        .bg(theme::tokens::glass::FILL_CARD.to_hsla())
        .cursor_pointer()
        .on_click(move |_: &ClickEvent, _window, cx| on_pick(&target, cx))
        .when(!title.is_empty(), |this| {
            this.child(
                div()
                    .w_full()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .child(SharedString::from(title)),
            )
        })
        .child(
            div()
                .w_full()
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(caption_color)
                .child(SharedString::from(caption)),
        );
    div()
        .relative()
        .size_full()
        // The stacked card: a second edge peeking out behind a compound node.
        .when(compound, |this| {
            this.child(
                div()
                    .absolute()
                    .left(gpui::px(3.))
                    .top(gpui::px(-3.))
                    .right(gpui::px(-3.))
                    .bottom(gpui::px(3.))
                    .rounded(gpui::px(6.))
                    .border_1()
                    .border_color(theme::tokens::glass::STROKE_CARD.to_hsla()),
            )
        })
        .child(card)
        .into_any_element()
}

/// The shared issue chip for one issue id, degrading to the raw id while the
/// row has not synced.
fn issue_chip_for(issue_id: &str, cx: &App) -> gpui::AnyElement {
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
    chip.into_any_element()
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
    on_pick: impl Fn(usize, &mut App) + Clone + 'static,
    cx: &App,
) -> gpui::Div {
    let control = Button::new("workflow-max-parallel")
        .ghost()
        .small()
        .label(SharedString::from(picked.to_string()))
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
fn device_row(workflow_id: &str, picked: Option<&str>, devices: Vec<queries::LaunchDevice>, cx: &App) -> gpui::Div {
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
