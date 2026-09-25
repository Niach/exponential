//! One workflow's page (EXP-981 → EXP-1085): the ONE place for every issue,
//! run, change and result of a workflow.
//!
//! The page is a PICKER over a FACE TOGGLE: the header (name, status glyph,
//! the ONE primary button, an overflow with Stop / Delete, one caption line),
//! the node strip (`All` first, then the chips in DAG order off
//! `domain::workflow_view::workflow_node_strip`), the work face toggle
//! (`crate::work_header::face_toggle`), and a body that is selection × face:
//!
//! - All × Issue: the workflow's issues, sub-issues under their compound
//!   node, and the decisions log folded away.
//! - All × Runs: the session tree of the workflow's runs; a run opens IN
//!   PLACE.
//! - All × Changes: the final pull request (the ONE human review).
//! - All × Results: every run's pictures by topic.
//! - One node × face: that issue's own views embedded here — the issue
//!   detail, the run's steer view, the PR files, the run's results.
//! - Several nodes: the All faces filtered to them.
//!
//! Every word the page says comes from `domain::workflow_view` (×4, fixture
//! locked); every button is one `workflows.*` call and the server owns the
//! rule. An open question of a run (`domain::workflow_questions`) is a
//! banner at the top until someone answers it — in the run's own composer.

use std::collections::HashSet;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, AnyElement, App, AppContext as _, ClickEvent, Entity, FocusHandle,
    InteractiveElement as _, IntoElement, KeyDownEvent, Modifiers, ParentElement, Render,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState},
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
};
use sync::Store;

use domain::rows::{CodingSession, WorkflowNodeRow, WorkflowRow};
use domain::workflow_questions::{workflow_open_questions, WorkflowOpenQuestion};
use domain::workflow_view::{
    node_chip_menu, workflow_final_pr_caption, workflow_header_caption, workflow_node_strip,
    workflow_node_title, workflow_primary_action, workflow_start_blocker, HeaderNode, NodeChip,
    NodeChipAction, StartableWorkflow, StripNodeInput, StripWave, WorkflowNodeDisplayState,
    WorkflowPrimaryAction, CANCEL_WORKFLOW_CONFIRM, DELETE_WORKFLOW_LABEL, FINAL_PR_TITLE,
    MERGE_FINAL_PR_CONFIRM, MERGE_FINAL_PR_LABEL, NEEDS_YOU_LABEL, NODE_UNSYNCED_TITLE,
    PAUSE_WORKFLOW_LABEL, PLAN_WORKFLOW_LABEL, RESUME_WORKFLOW_LABEL, SKIP_NODE_CONFIRM,
    SKIP_NODE_LABEL, START_WORKFLOW_LABEL, ALL_NODES_LABEL, DECISIONS_LABEL, PICK_DEVICE_LABEL,
    REVIEW_FINAL_PR_LABEL, RUNS_ON_LABEL, STOP_WORKFLOW_LABEL, workflow_overflow_menu,
    WorkflowOverflowItem,
};

use crate::icons::registry;
use crate::navigation::{nav_for_window, ChatSeed, Navigation, Screen};
use crate::queries;
use crate::work_header::{face_toggle, Face, FaceToggle, RunEntry};

/// The page column's cap — the work column the embedded views read at.
const WORKFLOW_COLUMN_W: f32 = 1024.;
/// A chip's title cap inside the strip: the identifier says which issue, the
/// title only reminds.
const STRIP_TITLE_W: f32 = 140.;
/// The folded audit trail under All × Issue (while it has rows).
const EVENTS_TITLE: &str = "Activity";
/// The banner's one button: the run's own composer answers it.
const ANSWER_LABEL: &str = "Answer";

// ---------------------------------------------------------------------------
// Selection — pure, so the picker's rules are unit tests
// ---------------------------------------------------------------------------

/// What the strip has picked: nothing (= `All`, position 0) or a set of
/// node ids. `anchor` is where a shift-click range and a step start from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Selection {
    picked: Vec<String>,
    anchor: Option<String>,
}

impl Selection {
    pub(crate) fn is_all(&self) -> bool {
        self.picked.is_empty()
    }

    pub(crate) fn contains(&self, id: &str) -> bool {
        self.picked.iter().any(|picked| picked == id)
    }

    /// The picked ids, in the order given (the strip's DAG order).
    pub(crate) fn picked_in<'a>(&self, order: &'a [String]) -> Vec<&'a String> {
        order.iter().filter(|id| self.contains(id)).collect()
    }

    /// The one picked node, when exactly one is.
    pub(crate) fn single(&self) -> Option<&str> {
        (self.picked.len() == 1).then(|| self.picked[0].as_str())
    }

    /// Back to `All`.
    pub(crate) fn select_all(&mut self) {
        self.picked.clear();
        self.anchor = None;
    }

    /// A plain click: this node and nothing else.
    pub(crate) fn select(&mut self, id: &str) {
        self.picked = vec![id.to_string()];
        self.anchor = Some(id.to_string());
    }

    /// Cmd-click: in or out of the set. Emptying it is `All` again.
    pub(crate) fn toggle(&mut self, id: &str) {
        if let Some(index) = self.picked.iter().position(|picked| picked == id) {
            self.picked.remove(index);
            if self.anchor.as_deref() == Some(id) {
                self.anchor = self.picked.last().cloned();
            }
        } else {
            self.picked.push(id.to_string());
            self.anchor = Some(id.to_string());
        }
    }

    /// Shift-click: every node from the anchor to `id` in DAG order. With no
    /// anchor it is a plain select.
    pub(crate) fn extend(&mut self, order: &[String], id: &str) {
        let Some(to) = order.iter().position(|node| node == id) else {
            return;
        };
        let Some(from) = self
            .anchor
            .as_deref()
            .and_then(|anchor| order.iter().position(|node| node == anchor))
        else {
            self.select(id);
            return;
        };
        let (low, high) = (from.min(to), from.max(to));
        self.picked = order[low..=high].to_vec();
    }

    /// Where the selection stands in the strip: `All` = 0, node `i` = `i+1`
    /// (the anchor's, for a set).
    pub(crate) fn position(&self, order: &[String]) -> usize {
        if self.is_all() {
            return 0;
        }
        self.anchor
            .as_deref()
            .or_else(|| self.picked.first().map(String::as_str))
            .and_then(|id| order.iter().position(|node| node == id))
            .map_or(0, |index| index + 1)
    }

    /// ←/→ (and k/j): one step over `All` + the DAG order, clamped at both
    /// ends. A step always lands on ONE chip.
    pub(crate) fn step(&mut self, order: &[String], delta: i64) {
        let current = self.position(order) as i64;
        let next = (current + delta).clamp(0, order.len() as i64) as usize;
        if next == 0 {
            self.select_all();
        } else {
            self.select(&order[next - 1]);
        }
    }

    /// Drop the ids that left the workflow.
    pub(crate) fn retain(&mut self, order: &[String]) {
        self.picked.retain(|id| order.contains(id));
        if self
            .anchor
            .as_deref()
            .is_some_and(|anchor| !self.picked.iter().any(|id| id == anchor))
        {
            self.anchor = self.picked.last().cloned();
        }
    }
}

/// A chip click, by its modifiers: cmd toggles, shift extends, plain picks.
fn apply_click(selection: &mut Selection, order: &[String], id: &str, modifiers: &Modifiers) {
    if modifiers.secondary() {
        selection.toggle(id);
    } else if modifiers.shift {
        selection.extend(order, id);
    } else {
        selection.select(id);
    }
}

/// The strip's step for a key, `None` for every other key.
fn step_for_key(key: &str) -> Option<i64> {
    match key {
        "left" | "k" => Some(-1),
        "right" | "j" => Some(1),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// The page
// ---------------------------------------------------------------------------

pub struct WorkflowView {
    #[allow(dead_code)] // held for the team-switch re-render subscription
    nav: Entity<Navigation>,
    workflow_id: String,
    name_input: Entity<InputState>,
    /// The name last pushed into the input, so a remote rename repaints it
    /// while a local edit in flight does not bounce.
    name_seeded: String,
    selection: Selection,
    face: Face,
    /// A run opened IN PLACE from a list (All/several × Runs, the banner).
    open_run: Option<String>,
    decisions_open: bool,
    events_open: bool,
    /// The session tree's folded rows.
    collapsed_runs: HashSet<String>,
    /// The strip's keys (←/→, j/k) — only while the strip holds focus, so
    /// typing in an embedded view never steps.
    strip_focus: FocusHandle,
    scroll: ScrollHandle,
    /// The ONE node's views, embedded (lazily built, re-pointed).
    issue_view: Option<Entity<crate::issue_detail::IssueDetailView>>,
    pr_view: Option<Entity<crate::pr_diff::PrDiffView>>,
    run_view: Option<(String, Entity<crate::session_screen::SessionScreenView>)>,
    images: Entity<crate::markdown::ImageCache>,
    _subscriptions: Vec<Subscription>,
}

impl crate::sessions_section::Collapsible for WorkflowView {
    fn collapsed_mut(&mut self) -> &mut HashSet<String> {
        &mut self.collapsed_runs
    }
}

/// One node joined with its issue and its run, once per render.
struct NodeInfo {
    row: WorkflowNodeRow,
    identifier: String,
    title: String,
    status: Option<domain::statuses::ResolvedStatus>,
    run: Option<CodingSession>,
    /// The run's dot tone while the agent is mid-turn.
    busy_tone: Option<gpui::Hsla>,
    has_pr: bool,
}

/// What one chip draws: the domain's chip plus the joins the strip needs.
#[derive(Clone)]
struct ChipFacts {
    chip: NodeChip,
    issue_title: String,
    /// The node's issue and sub-issues: the mini-graph's subjects.
    issue_ids: Vec<String>,
    status: Option<domain::statuses::ResolvedStatus>,
    busy_tone: Option<gpui::Hsla>,
    proposed: bool,
    menu: Vec<NodeChipAction>,
}

/// The strip's callbacks; the styleguide draws it without any.
#[derive(Clone)]
struct StripHandlers {
    on_all: Rc<dyn Fn(&mut Window, &mut App)>,
    on_pick: Rc<dyn Fn(&str, Modifiers, &mut Window, &mut App)>,
    on_menu: Rc<dyn Fn(&str, NodeChipAction, &mut Window, &mut App)>,
}

impl WorkflowView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Workflow name"));
        let mut subscriptions = vec![cx.subscribe_in(
            &name_input,
            window,
            |this, _, event: &InputEvent, _window, cx| {
                // Saves on blur when changed; Enter commits (the issue title
                // rule).
                if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    this.save_name(cx);
                }
            },
        )];
        let nav = nav_for_window(window, cx);
        subscriptions.push(cx.observe(&nav, |_, _, cx| cx.notify()));
        if let Some(store) = Store::try_global(cx) {
            let collections = store.collections().clone();
            subscriptions.push(cx.observe_in(
                &collections.workflows,
                window,
                |this, _, window, cx| {
                    this.sync_name(window, cx);
                    cx.notify();
                },
            ));
            subscriptions.push(cx.observe(&collections.workflow_nodes, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&collections.workflow_events, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&collections.issues, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&collections.coding_sessions, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&collections.devices, |_, _, cx| cx.notify()));
        }
        let transport = queries::attachment_transport(cx);
        let images = cx.new(|_| crate::markdown::ImageCache::new(transport));
        Self {
            nav,
            workflow_id: String::new(),
            name_input,
            name_seeded: String::new(),
            selection: Selection::default(),
            face: Face::Issue,
            open_run: None,
            decisions_open: false,
            events_open: false,
            collapsed_runs: HashSet::new(),
            strip_focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            issue_view: None,
            pr_view: None,
            run_view: None,
            images,
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
        self.selection = Selection::default();
        self.face = Face::Issue;
        self.open_run = None;
        self.decisions_open = false;
        self.events_open = false;
        self.drop_run_view(cx);
        // Swap the name UNCONDITIONALLY, or the next blur would write the
        // previous workflow's name onto this one.
        self.name_seeded = String::new();
        self.sync_name(window, cx);
        cx.notify();
    }

    fn row(&self, cx: &App) -> Option<WorkflowRow> {
        Store::try_global(cx)?
            .collections()
            .workflows
            .read(cx)
            .get(&self.workflow_id)
            .cloned()
    }

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
        // A blank name is a mis-edit, not a rename.
        if name.is_empty() || name == self.name_seeded {
            return;
        }
        self.name_seeded = name.clone();
        let mut input = api::workflows::WorkflowUpdate::new(self.workflow_id.clone());
        input.name = Some(name);
        spawn_update(input, cx);
    }

    fn drop_run_view(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some((_, view)) = self.run_view.take() {
            view.update(cx, |view, cx| view.shutdown(cx));
        }
    }

    /// A strip pick: the selection moves, the face stays.
    fn pick(&mut self, order: &[String], id: &str, modifiers: Modifiers, cx: &mut gpui::Context<Self>) {
        apply_click(&mut self.selection, order, id, &modifiers);
        self.open_run = None;
        cx.notify();
    }

    fn pick_all(&mut self, cx: &mut gpui::Context<Self>) {
        self.selection.select_all();
        self.open_run = None;
        cx.notify();
    }

    /// The banner's Answer: the run that asked, on its Run face — its own
    /// composer is the answer path.
    fn answer(&mut self, question: &WorkflowOpenQuestion, cx: &mut gpui::Context<Self>) {
        match question.node_id.as_deref() {
            Some(node_id) => {
                self.selection.select(node_id);
                self.open_run = None;
            }
            None => {
                self.selection.select_all();
                self.open_run = Some(question.session_id.clone());
            }
        }
        self.face = Face::Run;
        cx.notify();
    }

    /// The workflow's nodes joined with their issues and runs.
    fn node_infos(&self, sessions: &[CodingSession], cx: &App) -> Vec<NodeInfo> {
        let Some(store) = Store::try_global(cx) else {
            return Vec::new();
        };
        let collections = store.collections();
        let issues = collections.issues.read(cx);
        let now = chrono::Utc::now().timestamp();
        let muted = cx.theme().muted_foreground;
        queries::workflow_nodes(cx, &self.workflow_id)
            .into_iter()
            .map(|row| {
                let issue = row.issue_id.as_deref().and_then(|id| issues.get(id)).cloned();
                let run = node_run(&row, sessions);
                let busy_tone = run.as_ref().and_then(|session| {
                    let pr_state = issue.as_ref().and_then(|issue| issue.pr_state.as_deref());
                    let display = queries::coding_session_display(session, pr_state);
                    node_run_busy(session, display, now).then(|| {
                        queries::session_dot_tone(
                            queries::SessionDotFacts::from_display(display, false, false),
                            muted,
                        )
                    })
                });
                let identifier = issue
                    .as_ref()
                    .map(|issue| issue.identifier.clone())
                    .filter(|identifier| !identifier.is_empty())
                    .unwrap_or_else(|| {
                        row.issue_id.as_deref().unwrap_or_default().chars().take(8).collect()
                    });
                NodeInfo {
                    identifier,
                    title: issue
                        .as_ref()
                        .map(|issue| issue.title.clone())
                        .unwrap_or_else(|| NODE_UNSYNCED_TITLE.to_string()),
                    status: issue.as_ref().map(|issue| queries::resolve_issue_status(cx, issue)),
                    has_pr: issue.as_ref().is_some_and(|issue| issue.pr_url.is_some()),
                    run,
                    busy_tone,
                    row,
                }
            })
            .collect()
    }
}

/// A node's run: the one the node row names, else its newest run.
fn node_run(node: &WorkflowNodeRow, sessions: &[CodingSession]) -> Option<CodingSession> {
    if let Some(id) = node.session_id.as_deref() {
        if let Some(session) = sessions.iter().find(|session| session.id == id) {
            return Some(session.clone());
        }
    }
    sessions
        .iter()
        .filter(|session| session.workflow_node_id.as_deref() == Some(node.id.as_str()))
        .max_by(|a, b| a.started_at.cmp(&b.started_at))
        .cloned()
}

/// Does a node's dot ping? The ONE turn rule every session list keys on,
/// and only while the row DISPLAYS as running.
fn node_run_busy(
    session: &CodingSession,
    display: queries::CodingSessionDisplay,
    now_epoch: i64,
) -> bool {
    display == queries::CodingSessionDisplay::Running
        && queries::session_agent_busy(session, None, now_epoch)
}

/// The workflow's runs (`coding_sessions.workflow_id`).
fn workflow_sessions(workflow_id: &str, cx: &App) -> Vec<CodingSession> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    store
        .collections()
        .coding_sessions
        .read(cx)
        .iter()
        .filter(|session| session.workflow_id.as_deref() == Some(workflow_id))
        .cloned()
        .collect()
}

/// EXP-1085 — the rail's Workflows dot: does any workflow of `team_id` wait
/// on a person?
pub(crate) fn team_has_open_question(team_id: &str, cx: &App) -> bool {
    let Some(store) = Store::try_global(cx) else {
        return false;
    };
    let collections = store.collections();
    let workflows: Vec<String> = collections
        .workflows
        .read(cx)
        .iter()
        .filter(|row| row.team_id.as_deref() == Some(team_id))
        .map(|row| row.id.clone())
        .collect();
    if workflows.is_empty() {
        return false;
    }
    let sessions: Vec<CodingSession> = collections
        .coding_sessions
        .read(cx)
        .iter()
        .filter(|session| {
            session
                .workflow_id
                .as_deref()
                .is_some_and(|id| workflows.iter().any(|workflow| workflow == id))
        })
        .cloned()
        .collect();
    workflows
        .iter()
        .any(|id| !workflow_open_questions(&sessions, id).is_empty())
}

/// The runner row's name, for the caption.
fn device_label(device_id: Option<&str>, cx: &App) -> Option<String> {
    let device_id = device_id?;
    let store = Store::try_global(cx)?;
    store
        .collections()
        .devices
        .read(cx)
        .iter()
        .find(|row| row.device_id.as_deref() == Some(device_id))
        .and_then(|row| row.label.clone())
        .filter(|label| !label.is_empty())
}

impl Render for WorkflowView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(row) = self.row(cx) else {
            return crate::actions_view::page_scaffold_with(
                "workflow-screen-scroll",
                &self.scroll,
                v_flex().child(crate::controls::empty_state(
                    Icon::from(registry::NAV_WORKFLOWS),
                    "Workflow not found",
                    "It may have been deleted, or it has not synced to this machine yet.",
                    cx,
                )),
                WORKFLOW_COLUMN_W,
            )
            .into_any_element();
        };
        // The merge's synced echo releases the in-flight guard.
        if row.final_pr_state.as_deref() != Some("open") {
            MergingFinalPrs::remove(&row.id, cx);
        }
        let sessions = workflow_sessions(&self.workflow_id, cx);
        let questions = workflow_open_questions(&sessions, &self.workflow_id);
        let infos = self.node_infos(&sessions, cx);
        let strip = strip_waves(&infos, &questions);
        let order: Vec<String> = strip
            .iter()
            .flat_map(|wave| wave.nodes.iter().map(|chip| chip.id.clone()))
            .collect();
        self.selection.retain(&order);

        let header = self.render_header(&row, &infos, window, cx);
        let banner = render_question_banner(&questions, &infos, cx.entity().downgrade(), cx);
        let chip_facts = chip_facts(&strip, &infos);
        let handlers = self.strip_handlers(order.clone(), cx);
        let strip_element = render_strip(&chip_facts, &self.selection, Some(handlers), cx);

        // The scope the body shows: every node, or the picked ones.
        let scope: Vec<&NodeInfo> = if self.selection.is_all() {
            order
                .iter()
                .filter_map(|id| infos.iter().find(|info| &info.row.id == id))
                .collect()
        } else {
            self.selection
                .picked_in(&order)
                .into_iter()
                .filter_map(|id| infos.iter().find(|info| &info.row.id == id))
                .collect()
        };
        let single = self
            .selection
            .single()
            .and_then(|id| infos.iter().find(|info| info.row.id == id));
        let scope_sessions: Vec<CodingSession> = if self.selection.is_all() {
            sessions.clone()
        } else {
            sessions
                .iter()
                .filter(|session| {
                    scope.iter().any(|info| {
                        session.workflow_node_id.as_deref() == Some(info.row.id.as_str())
                            || info.row.session_id.as_deref() == Some(session.id.as_str())
                    })
                })
                .cloned()
                .collect()
        };
        let results = collect_results(&scope_sessions);
        let has_changes = if self.selection.is_all() {
            row.final_pr_number.is_some() || row.final_pr_url.is_some()
        } else {
            scope.iter().any(|info| info.has_pr)
        };
        let run_entries = run_entries(&scope_sessions, &infos);
        let toggle_spec = FaceToggle {
            issue: true,
            run: match single {
                Some(info) => info.run.as_ref().map(|run| run.id.clone()),
                None => scope_sessions.first().map(|run| run.id.clone()),
            },
            diff: None,
            pr_changes: has_changes,
            results: !results.is_empty(),
            active: self.face,
            runs: run_entries,
            checked_run: self.open_run.clone(),
        };
        let items = toggle_spec.items();
        let face = if items.contains(&self.face) {
            self.face
        } else {
            Face::Issue
        };
        let view = cx.entity().downgrade();
        let on_face: crate::work_header::OnPickFace = Rc::new({
            let view = view.clone();
            move |face, _window, cx| {
                if let Some(view) = view.upgrade() {
                    view.update(cx, |this, cx| {
                        this.face = face;
                        this.open_run = None;
                        cx.notify();
                    });
                }
            }
        });
        let on_run: crate::work_header::OnPickRun = Rc::new({
            let view = view.clone();
            move |session_id, _window, cx| {
                if let Some(view) = view.upgrade() {
                    let session_id = session_id.to_string();
                    view.update(cx, |this, cx| {
                        this.face = Face::Run;
                        this.open_run = Some(session_id);
                        cx.notify();
                    });
                }
            }
        });
        let toggle = face_toggle(
            FaceToggle {
                active: face,
                ..toggle_spec
            },
            on_face,
            on_run,
            cx,
        );

        // The run a Run face shows in place, if any.
        let shown_run = match face {
            Face::Run => self
                .open_run
                .clone()
                .filter(|id| scope_sessions.iter().any(|session| &session.id == id))
                .or_else(|| single.and_then(|info| info.run.as_ref().map(|run| run.id.clone()))),
            _ => None,
        };
        if shown_run.as_deref() != self.run_view.as_ref().map(|(id, _)| id.as_str()) {
            self.drop_run_view(cx);
        }

        let body: AnyElement = match (face, single, shown_run) {
            (Face::Run, _, Some(session_id)) => {
                let screen = match self.run_view.as_ref() {
                    Some((_, view)) => view.clone(),
                    None => {
                        let screen = cx.new(|cx| {
                            crate::session_screen::SessionScreenView::new(
                                session_id.clone(),
                                window,
                                cx,
                            )
                        });
                        cx.observe(&screen, |_, _, cx| cx.notify()).detach();
                        self.run_view = Some((session_id.clone(), screen.clone()));
                        screen
                    }
                };
                let inner = screen.read(cx).inner().clone();
                embedded(inner.into_any_element())
            }
            (Face::Issue, Some(info), _) if info.row.issue_id.is_some() => {
                let issue_id = info.row.issue_id.clone().unwrap_or_default();
                let detail = self
                    .issue_view
                    .get_or_insert_with(|| {
                        cx.new(|cx| crate::issue_detail::IssueDetailView::new(window, cx))
                    })
                    .clone();
                detail.update(cx, |detail, cx| detail.set_issue(issue_id, window, cx));
                embedded(detail.into_any_element())
            }
            (Face::Diff, Some(info), _) if info.has_pr => {
                let issue_id = info.row.issue_id.clone().unwrap_or_default();
                let diff = self
                    .pr_view
                    .get_or_insert_with(|| {
                        cx.new(|cx| {
                            let mut view = crate::pr_diff::PrDiffView::new(window, cx);
                            view.embedded = true;
                            view
                        })
                    })
                    .clone();
                diff.update(cx, |diff, cx| diff.set_issue(issue_id, cx));
                embedded(diff.into_any_element())
            }
            (Face::Results, _, _) => {
                let groups = domain::session_results::group_session_results(&results);
                let width = (f32::from(window.viewport_size().width)
                    - crate::shell::window_left_column_width(window, cx))
                .min(crate::work_header::WORK_COLUMN_W)
                    - 2. * crate::issue_detail::DETAIL_GUTTER;
                let images = self.images.clone();
                let page = crate::session_results::render(&groups, width.max(200.), &images, cx);
                self.scrolled(page)
            }
            (Face::Diff, _, _) => {
                let list = if self.selection.is_all() {
                    render_final_pr(&row, cx)
                } else {
                    render_pr_rows(&scope, cx)
                };
                self.scrolled(list)
            }
            (Face::Run, _, None) => {
                let tree = self.render_run_tree(&scope_sessions, cx);
                self.scrolled(tree)
            }
            _ => {
                let list = self.render_issue_list(&row, &scope, cx);
                self.scrolled(list)
            }
        };

        let strip_focus = self.strip_focus.clone();
        let key_order = order.clone();
        let top = v_flex()
            .w_full()
            .min_w_0()
            .max_w(px(WORKFLOW_COLUMN_W))
            .mx_auto()
            .px_4()
            .pt_4()
            .pb_2()
            .gap_3()
            .child(header)
            .children(banner)
            .child(
                div()
                    .id("workflow-strip")
                    .track_focus(&strip_focus)
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _window, cx| {
                        let modifiers = &event.keystroke.modifiers;
                        if modifiers.control || modifiers.alt || modifiers.platform {
                            return;
                        }
                        if let Some(delta) = step_for_key(event.keystroke.key.as_str()) {
                            cx.stop_propagation();
                            this.selection.step(&key_order, delta);
                            this.open_run = None;
                            cx.notify();
                        }
                    }))
                    .child(strip_element),
            )
            .children(toggle);

        v_flex()
            .size_full()
            .min_h_0()
            .min_w_0()
            .child(div().w_full().min_w_0().flex_shrink_0().child(top))
            .child(div().flex_1().min_h_0().min_w_0().flex().flex_col().child(body))
            .into_any_element()
    }
}

/// An embedded entity view takes the rest of the page's height.
fn embedded(view: AnyElement) -> AnyElement {
    div()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .size_full()
        .child(view)
        .into_any_element()
}

impl WorkflowView {
    /// A list face scrolls under the fixed top, in the page's column.
    fn scrolled(&self, content: AnyElement) -> AnyElement {
        crate::scroll_pane::v_scroll_pane(
            "workflow-body-scroll",
            &self.scroll,
            div().w_full().min_w_0().child(
                div()
                    .w_full()
                    .min_w_0()
                    .max_w(px(WORKFLOW_COLUMN_W))
                    .mx_auto()
                    .px_4()
                    .pt_2()
                    .pb_6()
                    .child(content),
            ),
        )
        .size_full()
        .into_any_element()
    }

    fn strip_handlers(&self, order: Vec<String>, cx: &mut gpui::Context<Self>) -> StripHandlers {
        let view = cx.entity().downgrade();
        let focus = self.strip_focus.clone();
        StripHandlers {
            on_all: Rc::new({
                let view = view.clone();
                let focus = focus.clone();
                move |window, cx| {
                    window.focus(&focus, cx);
                    if let Some(view) = view.upgrade() {
                        view.update(cx, |this, cx| this.pick_all(cx));
                    }
                }
            }),
            on_pick: Rc::new({
                let view = view.clone();
                move |id, modifiers, window, cx| {
                    window.focus(&focus, cx);
                    if let Some(view) = view.upgrade() {
                        let id = id.to_string();
                        let order = order.clone();
                        view.update(cx, |this, cx| this.pick(&order, &id, modifiers, cx));
                    }
                }
            }),
            on_menu: Rc::new(|node_id, action, window, cx| {
                let node_id = node_id.to_string();
                match action {
                    NodeChipAction::Retry => spawn_resolve(node_id, "retry", cx),
                    NodeChipAction::Skip => crate::native_dialog::open_alert(
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
                    ),
                    NodeChipAction::Admit => spawn_admit(node_id, true, cx),
                    NodeChipAction::Dismiss => spawn_admit(node_id, false, cx),
                }
            }),
        }
    }

    /// Name, status glyph, the ONE primary button, the overflow; the caption
    /// line; the draft's start blocker while it blocks Start.
    fn render_header(
        &self,
        row: &WorkflowRow,
        infos: &[NodeInfo],
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let status = row.status_wire().to_string();
        let draft = status == domain::contract::WF_STATUS_DRAFT;
        let label = device_label(row.device_id.as_deref(), cx);
        let header_nodes: Vec<HeaderNode> = infos
            .iter()
            .map(|info| HeaderNode {
                state: info.row.state_wire().to_string(),
                members: info.row.member_ids().len(),
            })
            .collect();
        let caption = workflow_header_caption(&status, &header_nodes, label.as_deref());
        // A runner whose row has not synced still counts as picked.
        let picked = label.clone().or_else(|| row.device_id.clone());
        let action = workflow_primary_action(&status, picked.as_deref());
        let blocker = draft
            .then(|| {
                workflow_start_blocker(
                    StartableWorkflow {
                        status: row.status_wire(),
                        device_id: row.device_id.as_deref(),
                        repository_id: row.repository_id.as_deref(),
                    },
                    &row.shape(),
                )
            })
            .flatten();
        // The picker IS the answer to the "pick a device" blocker.
        let notice = blocker
            .clone()
            .filter(|_| action != Some(WorkflowPrimaryAction::PickDevice));
        let (glyph, tint) = status_glyph(&status, cx);
        let primary = action.map(|action| primary_button(action, row, blocker.is_some(), cx));

        v_flex()
            .w_full()
            .min_w_0()
            .gap_1()
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_2()
                    .child(Icon::from(glyph).small().text_color(tint))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(crate::controls::glass_input(&self.name_input, window, cx)),
                    )
                    .children(primary)
                    .child(overflow_menu(row, cx)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(caption)),
            )
            .when_some(notice, |this, note| {
                this.child(
                    crate::controls::alert(
                        crate::controls::AlertVariant::Default,
                        Some(Icon::from(registry::UI_INFO)),
                        cx,
                    )
                    .child(div().min_w_0().child(SharedString::from(note))),
                )
            })
            .into_any_element()
    }

    /// All/several × Issue: the nodes' issues, sub-issues under their
    /// compound node; All adds the folded decisions log (and the audit trail
    /// once it has rows).
    fn render_issue_list(
        &self,
        row: &WorkflowRow,
        scope: &[&NodeInfo],
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let mut column = v_flex().w_full().min_w_0();
        if scope.is_empty() {
            column = column.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("This workflow has no issues yet."),
            );
        }
        let view = cx.entity().downgrade();
        let mut index = 0;
        for info in scope {
            let node_id = info.row.id.clone();
            let display = domain::workflow_view::workflow_node_display_state(info.row.state_wire());
            let mut chip = crate::issue_chip::issue_chip(
                SharedString::from(format!("workflow-issue-node-{}", info.row.id)),
                info.identifier.clone(),
                info.title.clone(),
            )
            .flexible()
            .note(display.label(), display_color(display, cx));
            if let Some(status) = info.status.clone() {
                chip = chip.status(status);
            }
            let view = view.clone();
            chip = chip.on_click(move |_: &ClickEvent, _window, cx| {
                if let Some(view) = view.upgrade() {
                    let node_id = node_id.clone();
                    view.update(cx, |this, cx| {
                        this.selection.select(&node_id);
                        this.face = Face::Issue;
                        this.open_run = None;
                        cx.notify();
                    });
                }
            });
            column = column.child(crate::surface::list_row(
                div().w_full().min_w_0().px_2().py_1().child(chip),
                index,
            ));
            index += 1;
            for member in info.row.member_ids() {
                column = column.child(crate::surface::list_row(
                    div()
                        .w_full()
                        .min_w_0()
                        .pl_8()
                        .pr_2()
                        .py_1()
                        .child(issue_chip_link(&member, cx)),
                    index,
                ));
                index += 1;
            }
        }
        if self.selection.is_all() {
            let decisions = row
                .decisions
                .clone()
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty());
            if let Some(text) = decisions {
                let open = self.decisions_open;
                column = column.child(
                    v_flex()
                        .w_full()
                        .min_w_0()
                        .pt_4()
                        .gap_2()
                        .child(
                            crate::controls::disclosure_header(
                                "workflow-decisions",
                                open,
                                crate::controls::ChevronSide::Leading,
                                div().text_sm().child(DECISIONS_LABEL),
                                cx,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.decisions_open = !this.decisions_open;
                                cx.notify();
                            })),
                        )
                        .when(open, |this| {
                            this.child(
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .pl_6()
                                    .text_sm()
                                    .text_color(muted)
                                    .children(text.lines().map(|line| {
                                        div().min_w_0().child(SharedString::from(line.to_string()))
                                    })),
                            )
                        }),
                );
            }
            let events: Vec<domain::rows::WorkflowEventRow> = Store::try_global(cx)
                .map(|store| {
                    store
                        .collections()
                        .workflow_events
                        .read(cx)
                        .iter()
                        .filter(|event| event.workflow_id.as_deref() == Some(row.id.as_str()))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            if !events.is_empty() {
                let open = self.events_open;
                column = column.child(
                    v_flex()
                        .w_full()
                        .min_w_0()
                        .pt_4()
                        .gap_2()
                        .child(
                            crate::controls::disclosure_header(
                                "workflow-events",
                                open,
                                crate::controls::ChevronSide::Leading,
                                div().text_sm().child(EVENTS_TITLE),
                                cx,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.events_open = !this.events_open;
                                cx.notify();
                            })),
                        )
                        .when(open, |this| {
                            this.child(crate::workflow_events::WorkflowEventList::new(&events))
                        }),
                );
            }
        }
        column.into_any_element()
    }

    /// All/several × Runs: the session tree of the runs in scope. The
    /// workflow's own group row is structure the page already is, so its
    /// children are the roots. A row opens its run IN PLACE.
    fn render_run_tree(
        &mut self,
        sessions: &[CodingSession],
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        enum Kind {
            Run(crate::run_rows::RunListFacts),
            Group(crate::run_rows::SessionGroupFacts),
        }
        let now = chrono::Utc::now().timestamp();
        let rows: Vec<(String, usize, bool, Kind)> = {
            let refs: Vec<&CodingSession> = sessions.iter().collect();
            let inputs = queries::session_tree_inputs(cx, &refs);
            let tree = domain::session_tree::session_tree(
                refs,
                |session: &&CodingSession| domain::session_tree::coding_session_facts(session),
                &inputs.context(),
            );
            let roots: &[domain::session_tree::SessionTreeNode<&CodingSession>] =
                match tree.as_slice() {
                    [domain::session_tree::SessionTreeNode::Workflow(group)]
                        if group.workflow_id == self.workflow_id =>
                    {
                        &group.children
                    }
                    _ => &tree,
                };
            domain::session_tree::visible_session_tree_rows(roots, &self.collapsed_runs)
                .into_iter()
                .map(|flat| {
                    let kind = match flat.node.session() {
                        Some(session) => {
                            Kind::Run(crate::run_rows::RunListFacts::derive(session, now, cx))
                        }
                        None => Kind::Group(
                            crate::run_rows::SessionGroupFacts::from_node(flat.node)
                                .expect("a node that is not a session is a group"),
                        ),
                    };
                    (flat.key, flat.depth, flat.has_children, kind)
                })
                .collect()
        };
        if rows.is_empty() {
            return div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Nothing has run yet.")
                .into_any_element();
        }
        let guides = domain::tree_guides::guides_for(
            &rows.iter().map(|(_, depth, _, _)| *depth).collect::<Vec<_>>(),
        );
        let view = cx.entity().downgrade();
        let mut column = v_flex().w_full().min_w_0();
        for (index, (key, _, has_children, kind)) in rows.into_iter().enumerate() {
            let fold = crate::sessions_section::fold_for(
                key.clone(),
                has_children,
                &self.collapsed_runs,
                cx,
            );
            let guides = guides.get(index).cloned().unwrap_or_default();
            let element = match kind {
                Kind::Group(facts) => crate::run_rows::render_group_row(
                    crate::run_rows::GroupRowSpec {
                        id_prefix: "workflow-run",
                        index,
                        guides,
                        fold,
                        on_open: crate::sessions_section::group_row_open(&facts),
                        facts,
                    },
                    cx,
                ),
                Kind::Run(facts) => {
                    let open_id = facts.session_id().to_string();
                    let view = view.clone();
                    crate::run_rows::render_run_list_row(
                        "workflow-run",
                        index,
                        guides,
                        fold,
                        facts,
                        false,
                        Box::new(move |_, _window, cx| {
                            if let Some(view) = view.upgrade() {
                                let open_id = open_id.clone();
                                view.update(cx, |this, cx| {
                                    this.face = Face::Run;
                                    this.open_run = Some(open_id);
                                    cx.notify();
                                });
                            }
                        }),
                        cx,
                    )
                }
            };
            column = column.child(element);
        }
        column.into_any_element()
    }
}

/// The strip's input off the joined nodes.
fn strip_waves(infos: &[NodeInfo], questions: &[WorkflowOpenQuestion]) -> Vec<StripWave> {
    let inputs: Vec<StripNodeInput> = infos
        .iter()
        .map(|info| StripNodeInput {
            id: info.row.id.clone(),
            identifier: info.identifier.clone(),
            state: info.row.state_wire().to_string(),
            wave: info.row.wave.unwrap_or(0),
            lane: info.row.lane.unwrap_or(0),
            members: info.row.member_ids().len(),
            live: info.busy_tone.is_some(),
            needs_you: questions
                .iter()
                .any(|question| question.node_id.as_deref() == Some(info.row.id.as_str())),
            note: info.row.note.clone(),
        })
        .collect();
    workflow_node_strip(&inputs, &[])
}

fn chip_facts(strip: &[StripWave], infos: &[NodeInfo]) -> Vec<Vec<ChipFacts>> {
    strip
        .iter()
        .map(|wave| {
            wave.nodes
                .iter()
                .filter_map(|chip| {
                    let info = infos.iter().find(|info| info.row.id == chip.id)?;
                    let mut issue_ids: Vec<String> = info.row.issue_id.iter().cloned().collect();
                    issue_ids.extend(info.row.member_ids());
                    Some(ChipFacts {
                        chip: chip.clone(),
                        issue_title: info.title.clone(),
                        issue_ids,
                        status: info.status.clone(),
                        busy_tone: info.busy_tone,
                        proposed: info.row.state_wire() == domain::contract::WF_NODE_STATE_PROPOSED,
                        menu: node_chip_menu(info.row.state_wire()),
                    })
                })
                .collect()
        })
        .collect()
}

/// The ONE runs' results, in scope order.
fn collect_results(sessions: &[CodingSession]) -> Vec<domain::session_results::SessionResultEntry> {
    sessions
        .iter()
        .flat_map(|session| {
            domain::session_results::parse_session_results(session.results.as_ref())
        })
        .collect()
}

/// The Run item's menu: one entry per run in scope, named after its node.
fn run_entries(sessions: &[CodingSession], infos: &[NodeInfo]) -> Vec<RunEntry> {
    let now = chrono::Utc::now().timestamp();
    sessions
        .iter()
        .map(|session| {
            let name = session
                .workflow_node_id
                .as_deref()
                .and_then(|id| infos.iter().find(|info| info.row.id == id))
                .map(|info| workflow_node_title(&info.identifier, info.row.member_ids().len()))
                .unwrap_or_else(|| crate::work_header::RUN_FACE_LABEL.to_string());
            let when = crate::run_rows::issue_run_when(session, now);
            RunEntry {
                id: session.id.clone(),
                label: if when.is_empty() {
                    name
                } else {
                    format!("{name} · {when}")
                },
                live: queries::is_live_run_status(session),
            }
        })
        .collect()
}

/// A display state's colour.
fn display_color(display: WorkflowNodeDisplayState, cx: &App) -> gpui::Hsla {
    let theme = cx.theme();
    match display {
        WorkflowNodeDisplayState::Running => theme.foreground,
        WorkflowNodeDisplayState::Done => theme.success,
        WorkflowNodeDisplayState::Failed => theme.danger,
        WorkflowNodeDisplayState::Queued | WorkflowNodeDisplayState::Skipped => {
            theme.muted_foreground
        }
    }
}

/// A display state's glyph; `Queued` keeps the issue's own status glyph.
fn display_glyph(display: WorkflowNodeDisplayState) -> Option<crate::icons::ExpIcon> {
    match display {
        WorkflowNodeDisplayState::Queued => None,
        WorkflowNodeDisplayState::Running => Some(registry::CODING_RUNNING),
        WorkflowNodeDisplayState::Done => Some(registry::PR_MERGED),
        WorkflowNodeDisplayState::Failed => Some(registry::UI_ERROR),
        WorkflowNodeDisplayState::Skipped => Some(registry::STATUS_CANCELLED),
    }
}

/// The workflow status beside its name.
fn status_glyph(status: &str, cx: &App) -> (crate::icons::ExpIcon, gpui::Hsla) {
    let theme = cx.theme();
    match status {
        "draft" => (registry::PR_DRAFT, theme.muted_foreground),
        "running" => (registry::CODING_RUNNING, theme.foreground),
        "paused" => (registry::CODING_STOP, theme.muted_foreground),
        "done" => (registry::STATUS_DONE, theme.success),
        "failed" => (registry::UI_ERROR, theme.danger),
        "cancelled" => (registry::STATUS_CANCELLED, theme.muted_foreground),
        _ => (registry::NAV_WORKFLOWS, theme.muted_foreground),
    }
}

/// The node strip: `All`, then the waves left to right, a wave's lanes top
/// to bottom. `handlers` is `None` in the styleguide.
fn render_strip(
    waves: &[Vec<ChipFacts>],
    selection: &Selection,
    handlers: Option<StripHandlers>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let mut all = crate::surface::glass_pill_button(
        "workflow-strip-all",
        crate::surface::PillSize::Sm,
        cx,
    )
    .label(ALL_NODES_LABEL);
    if selection.is_all() {
        all = all.border_color(theme.ring);
    }
    if let Some(handlers) = handlers.clone() {
        all = all.on_click(move |_, window, cx| (handlers.on_all)(window, cx));
    }
    let mut row = h_flex()
        .w_full()
        .min_w_0()
        .flex_wrap()
        .items_center()
        .gap_2()
        .child(all);
    for wave in waves.iter().filter(|wave| !wave.is_empty()) {
        row = row
            .child(Icon::from(registry::UI_CHEVRON_RIGHT).xsmall().text_color(muted))
            .child(
                v_flex().items_start().gap_1().children(
                    wave.iter()
                        .map(|facts| strip_chip(facts, selection.contains(&facts.chip.id), handlers.clone(), cx)),
                ),
            );
    }
    row.into_any_element()
}

/// ONE chip: the issue chip with the node's state glyph (or the live dot
/// while its agent is mid-turn), the caption inside it, the `needs you` dot
/// after it; hover = the mini-graph; a failed / proposed node's menu on the
/// right click.
fn strip_chip(
    facts: &ChipFacts,
    selected: bool,
    handlers: Option<StripHandlers>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let chip_facts = &facts.chip;
    let tint = display_color(chip_facts.display, cx);
    let mut chip = crate::issue_chip::issue_chip(
        SharedString::from(format!("workflow-chip-{}", chip_facts.id)),
        chip_facts.title.clone(),
        facts.issue_title.clone(),
    )
    .max_title_width(px(STRIP_TITLE_W))
    .note(chip_facts.caption.clone(), tint);
    if let Some(status) = facts.status.clone() {
        chip = chip.status(status);
    }
    if let Some(tone) = facts.busy_tone {
        chip = chip.slot(crate::surface::live_dot(tone, true));
    } else if let Some(glyph) = display_glyph(chip_facts.display) {
        chip = chip.slot(Icon::from(glyph).xsmall().text_color(tint));
    }
    if chip_facts.needs_you {
        chip = chip.trailing(crate::surface::pill_dot(theme.warning));
    }
    if chip_facts.stacked {
        chip = chip.stacked();
    }
    if selected {
        chip = chip.outline(theme.ring, false);
    } else if facts.proposed {
        chip = chip.outline(muted_outline(cx), true);
    }
    if let Some(handlers) = handlers.clone() {
        let id = chip_facts.id.clone();
        let on_pick = handlers.on_pick.clone();
        chip = chip.on_click(move |event: &ClickEvent, window, cx| {
            on_pick(&id, event.modifiers(), window, cx)
        });
    }
    // The hover: the chip's caption through the app's own tooltip; a node
    // with blocks edges adds the mini-graph under that caption.
    let subjects = facts.issue_ids.clone();
    let caption = chip_facts.caption.clone();
    let cell = div()
        .id(SharedString::from(format!("workflow-chip-cell-{}", chip_facts.id)))
        .child(chip)
        .hoverable_tooltip(move |window, cx| {
            let ids: Vec<&str> = subjects.iter().map(String::as_str).collect();
            let graph = crate::issue_graph::graph_for(&ids, cx);
            if graph.nodes.len() > 1 {
                let caption = caption.clone();
                cx.new(|_| NodeGraphTip { graph, caption }).into()
            } else {
                gpui_component::tooltip::Tooltip::new(caption.clone()).build(window, cx)
            }
        });
    if let Some(handlers) = handlers {
        if !facts.menu.is_empty() {
            let menu_items = facts.menu.clone();
            let node_id = chip_facts.id.clone();
            let on_menu = handlers.on_menu.clone();
            return cell.context_menu(move |mut menu, _window, _cx| {
                for action in menu_items.clone() {
                    let node_id = node_id.clone();
                    let on_menu = on_menu.clone();
                    menu = menu.item(
                        PopupMenuItem::new(action.label())
                            .on_click(move |_, window, cx| on_menu(&node_id, action, window, cx)),
                    );
                }
                menu
            })
            .into_any_element();
        }
    }
    cell.into_any_element()
}

fn muted_outline(cx: &App) -> gpui::Hsla {
    cx.theme().muted_foreground
}

/// The hover over a chip whose issue has blocks edges: its caption over the
/// frozen EXP-980 mini-graph.
struct NodeGraphTip {
    graph: domain::issue_graph::IssueGraph,
    caption: String,
}

impl Render for NodeGraphTip {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        v_flex()
            .gap_1()
            .p_2()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .child(div().text_xs().child(SharedString::from(self.caption.clone())))
            .child(crate::issue_graph::graph_overlay(
                &self.graph,
                crate::issue_graph::VIEW_W,
                cx,
            ))
    }
}

/// The banner at the top while a run of the workflow waits on a person:
/// the question, the node that asks, and Answer (the run's own composer).
fn render_question_banner(
    questions: &[WorkflowOpenQuestion],
    infos: &[NodeInfo],
    view: gpui::WeakEntity<WorkflowView>,
    cx: &App,
) -> Option<AnyElement> {
    if questions.is_empty() {
        return None;
    }
    let warning = cx.theme().warning;
    let mut column = v_flex().w_full().min_w_0().gap_2();
    for (index, question) in questions.iter().enumerate() {
        let who = question
            .node_id
            .as_deref()
            .and_then(|id| infos.iter().find(|info| info.row.id == id))
            .map(|info| workflow_node_title(&info.identifier, info.row.member_ids().len()));
        let view = view.clone();
        let question_owned = question.clone();
        column = column.child(
            crate::controls::alert(
                crate::controls::AlertVariant::Default,
                Some(Icon::from(registry::CODING_NEEDS_INPUT).text_color(warning)),
                cx,
            )
            .border_color(warning)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(crate::controls::alert_title(match who {
                        Some(who) => format!("{who} · {NEEDS_YOU_LABEL}"),
                        None => NEEDS_YOU_LABEL.to_string(),
                    }))
                    .child(div().min_w_0().child(SharedString::from(question.question.clone()))),
            )
            .child(
                Button::new(SharedString::from(format!("workflow-answer-{index}")))
                    .primary()
                    .small()
                    .label(ANSWER_LABEL)
                    .on_click(move |_, _window, cx| {
                        if let Some(view) = view.upgrade() {
                            let question = question_owned.clone();
                            view.update(cx, |this, cx| this.answer(&question, cx));
                        }
                    }),
            ),
        );
    }
    Some(column.into_any_element())
}

/// All × Changes: the final pull request — its number and state, Merge while
/// it is open (the run's ONE human review), a click opens it on GitHub.
fn render_final_pr(row: &WorkflowRow, cx: &App) -> AnyElement {
    let states: Vec<String> = queries::workflow_nodes(cx, &row.id)
        .iter()
        .map(|node| node.state_wire().to_string())
        .collect();
    let states: Vec<&str> = states.iter().map(String::as_str).collect();
    let caption = workflow_final_pr_caption(
        &states,
        row.final_pr_state.as_deref(),
        row.final_pr_number,
    )
    .unwrap_or_default();
    let tint = match row.final_pr_state.as_deref() {
        Some("merged") => cx.theme().success,
        Some("closed") => cx.theme().muted_foreground,
        _ => cx.theme().foreground,
    };
    let mut chip = crate::issue_chip::issue_chip(
        "workflow-final-pr",
        row.final_pr_number
            .map(|number| format!("#{number}"))
            .unwrap_or_default(),
        FINAL_PR_TITLE,
    )
    .flexible()
    .slot(Icon::from(registry::PR_OPEN).xsmall().text_color(tint))
    .note(caption, tint);
    if row.final_pr_state.as_deref() == Some("open") {
        chip = chip.trailing(merge_final_pr_button(row.id.clone(), cx));
    }
    if let Some(url) = row.final_pr_url.clone() {
        chip = chip.on_click(move |_: &ClickEvent, _window, cx| cx.open_url(&url));
    }
    crate::surface::list_row(div().w_full().min_w_0().px_2().py_1().child(chip), 0)
        .into_any_element()
}

/// Several × Changes: the picked nodes' pull requests, each opening its
/// files.
fn render_pr_rows(scope: &[&NodeInfo], cx: &App) -> AnyElement {
    let mut column = v_flex().w_full().min_w_0();
    for (index, info) in scope.iter().filter(|info| info.has_pr).enumerate() {
        let Some(issue_id) = info.row.issue_id.clone() else {
            continue;
        };
        let target = issue_id.clone();
        let chip = issue_chip_element(&issue_id, cx).on_click(move |_, window, cx| {
            crate::navigation::navigate(
                window,
                cx,
                Screen::PrDiff {
                    issue_id: target.clone(),
                },
            );
        });
        column = column.child(crate::surface::list_row(
            div().w_full().min_w_0().px_2().py_1().child(chip),
            index,
        ));
    }
    column.into_any_element()
}

/// The header's ONE primary button.
fn primary_button(
    action: WorkflowPrimaryAction,
    row: &WorkflowRow,
    blocked: bool,
    cx: &App,
) -> AnyElement {
    let id = row.id.clone();
    match action {
        WorkflowPrimaryAction::PickDevice => {
            let trigger = Button::new("workflow-primary")
                .primary()
                .small()
                .icon(Icon::from(registry::NAV_DEVICES))
                .label(PICK_DEVICE_LABEL)
                .into_any_element();
            device_picker(&id, row.device_id.as_deref(), trigger, cx)
        }
        WorkflowPrimaryAction::Start => Button::new("workflow-primary")
            .primary()
            .small()
            .icon(Icon::from(registry::ACTION_RUN))
            .label(START_WORKFLOW_LABEL)
            .disabled(blocked)
            .on_click(move |_, _window, cx| spawn_command(Command::Start, id.clone(), cx))
            .into_any_element(),
        WorkflowPrimaryAction::Pause => Button::new("workflow-primary")
            .primary()
            .small()
            .icon(Icon::from(registry::CODING_STOP))
            .label(PAUSE_WORKFLOW_LABEL)
            .on_click(move |_, _window, cx| spawn_command(Command::Pause, id.clone(), cx))
            .into_any_element(),
        WorkflowPrimaryAction::Resume => Button::new("workflow-primary")
            .primary()
            .small()
            .icon(Icon::from(registry::ACTION_RUN))
            .label(RESUME_WORKFLOW_LABEL)
            .on_click(move |_, _window, cx| spawn_command(Command::Resume, id.clone(), cx))
            .into_any_element(),
        WorkflowPrimaryAction::ReviewFinalPr => Button::new("workflow-primary")
            .primary()
            .small()
            .icon(Icon::from(registry::NAV_REVIEWS))
            .label(REVIEW_FINAL_PR_LABEL)
            .on_click(|_, window, cx| {
                crate::navigation::navigate(window, cx, Screen::Reviews);
            })
            .into_any_element(),
    }
}

/// The overflow, exactly `workflow_overflow_menu`: Plan, Runs on and
/// Delete on a draft; Stop while it runs; Delete once it is over. Runs on
/// holds THE device picker's rows (`launch_device_rows`: this machine and
/// the team's shared online runners) as its submenu.
fn overflow_menu(row: &WorkflowRow, cx: &App) -> AnyElement {
    let status = row.status_wire().to_string();
    let id = row.id.clone();
    let name = row.name.clone().unwrap_or_default();
    let picked = row.device_id.clone();
    crate::controls::ghost_icon_button("workflow-menu", Icon::from(registry::UI_MORE), cx)
        .dropdown_menu(move |mut menu, window, cx| {
            for item in workflow_overflow_menu(&status) {
                let id = id.clone();
                menu = match item {
                    WorkflowOverflowItem::Plan => menu.item(
                        PopupMenuItem::new(PLAN_WORKFLOW_LABEL)
                            .icon(Icon::from(registry::ACTION_RUN))
                            .on_click(move |_, window, cx| {
                                crate::navigation::navigate_to_chat(
                                    window,
                                    cx,
                                    ChatSeed::plan_workflow(&id),
                                );
                            }),
                    ),
                    WorkflowOverflowItem::RunsOn => {
                        let picked = picked.clone();
                        menu.submenu_with_icon(
                            Some(Icon::from(registry::NAV_DEVICES)),
                            RUNS_ON_LABEL,
                            window,
                            cx,
                            move |mut sub, _window, cx| {
                                let devices = queries::launch_devices(cx);
                                let rows = crate::launch_options::launch_device_rows(&devices, cx);
                                for device in rows {
                                    let workflow_id = id.clone();
                                    let device_id = device.id.clone();
                                    sub = sub.item(
                                        PopupMenuItem::new(device.name.clone())
                                            .icon(Icon::from(crate::icons::device_icon(
                                                device.icon.as_deref(),
                                                device.server,
                                            )))
                                            .checked(picked.as_deref() == Some(device.id.as_str()))
                                            .disabled(device.disabled)
                                            .on_click(move |_, _window, cx| {
                                                let mut input = api::workflows::WorkflowUpdate::new(
                                                    workflow_id.clone(),
                                                );
                                                input.device_id =
                                                    api::Patch::Set(device_id.clone());
                                                spawn_update(input, cx);
                                            }),
                                    );
                                }
                                sub
                            },
                        )
                    }
                    WorkflowOverflowItem::Stop => {
                        let name = name.clone();
                        menu.item(
                            crate::controls::danger_menu_item(
                                STOP_WORKFLOW_LABEL,
                                Icon::from(registry::CODING_STOP),
                                cx,
                            )
                            .on_click(move |_, window, cx| {
                                let id = id.clone();
                                let title = if name.is_empty() {
                                    STOP_WORKFLOW_LABEL.to_string()
                                } else {
                                    format!("{STOP_WORKFLOW_LABEL} {name}?")
                                };
                                crate::native_dialog::open_alert(
                                    window,
                                    cx,
                                    crate::native_dialog::AlertSpec::new(
                                        title,
                                        CANCEL_WORKFLOW_CONFIRM,
                                        STOP_WORKFLOW_LABEL,
                                    )
                                    .ok_variant(ButtonVariant::Danger)
                                    .on_ok(move |_window, cx| {
                                        spawn_command(Command::Cancel, id.clone(), cx);
                                        true
                                    }),
                                );
                            }),
                        )
                    }
                    WorkflowOverflowItem::Delete => menu.item(
                        crate::controls::danger_menu_item(
                            DELETE_WORKFLOW_LABEL,
                            Icon::from(registry::UI_DELETE),
                            cx,
                        )
                        .on_click(move |_, window, cx| prompt_delete(id.clone(), window, cx)),
                    ),
                };
            }
            menu
        })
        .into_any_element()
}

/// The runner pick over THE device picker: this machine and the team's
/// shared ONLINE runners (`queries::launch_devices`).
fn device_picker(
    workflow_id: &str,
    picked: Option<&str>,
    trigger: AnyElement,
    _cx: &App,
) -> AnyElement {
    let workflow_id = workflow_id.to_string();
    let picked = picked.map(str::to_string);
    crate::picker::deferred(move |window, cx| {
        let devices = queries::launch_devices(cx);
        let rows = crate::launch_options::launch_device_rows(&devices, cx);
        let workflow_id = workflow_id.clone();
        crate::picker::device_picker::device_picker(
            &rows,
            picked.clone(),
            trigger,
            Rc::new(move |values: Vec<String>, _window, cx: &mut App| {
                let Some(device_id) = values.into_iter().next() else {
                    return;
                };
                let mut input = api::workflows::WorkflowUpdate::new(workflow_id.clone());
                input.device_id = api::Patch::Set(device_id);
                spawn_update(input, cx);
            }),
        )
        .empty_text("No machines")
        .render(window, cx)
    })
    .into_any_element()
}

/// The issue chip for one issue, opening it.
fn issue_chip_link(issue_id: &str, cx: &App) -> AnyElement {
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

fn issue_chip_element(issue_id: &str, cx: &App) -> crate::issue_chip::IssueChip {
    let row = Store::try_global(cx)
        .and_then(|store| store.collections().issues.read(cx).get(issue_id).cloned());
    // A row that has not synced reads as the id's first 8 characters and the
    // ONE line that says why there is no title — never a bare uuid (×4).
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

// ---------------------------------------------------------------------------
// Final PR merge
// ---------------------------------------------------------------------------

/// The workflows whose final PR merge is IN FLIGHT: from the confirm until
/// the synced echo repaints (or the call is refused). Two quick clicks
/// cannot become two merges.
#[derive(Default)]
struct MergingFinalPrs(HashSet<String>);

impl gpui::Global for MergingFinalPrs {}

impl MergingFinalPrs {
    fn contains(workflow_id: &str, cx: &App) -> bool {
        cx.try_global::<Self>()
            .is_some_and(|merging| merging.0.contains(workflow_id))
    }

    fn insert(workflow_id: &str, cx: &mut App) {
        cx.default_global::<Self>().0.insert(workflow_id.to_string());
    }

    fn remove(workflow_id: &str, cx: &mut App) {
        if let Some(merging) = cx.try_global::<Self>() {
            if merging.0.contains(workflow_id) {
                cx.default_global::<Self>().0.remove(workflow_id);
            }
        }
    }
}

/// EXP-1032 — Merge on the final pull request: squash-merging it is the
/// whole run's single human review. Confirms first; inert while in flight.
fn merge_final_pr_button(workflow_id: String, cx: &App) -> AnyElement {
    if MergingFinalPrs::contains(&workflow_id, cx) {
        return div()
            .id("workflow-final-pr-merge")
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .opacity(0.5)
            .cursor_default()
            .child(MERGE_FINAL_PR_LABEL)
            .on_click(|_, _, cx| cx.stop_propagation())
            .into_any_element();
    }
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

// ---------------------------------------------------------------------------
// tRPC calls
// ---------------------------------------------------------------------------

/// Destructive actions confirm first.
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

/// The four status flips, one tRPC call each; the server owns every
/// precondition.
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

/// `workflows.mergeFinalPr`; a refusal is a sentence worth reading, so it
/// lands as an error notification.
fn spawn_merge_final_pr(workflow_id: String, window: &mut Window, cx: &mut App) {
    if MergingFinalPrs::contains(&workflow_id, cx) {
        return;
    }
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    MergingFinalPrs::insert(&workflow_id, cx);
    window.refresh();
    let handle = window.window_handle();
    let id = workflow_id.clone();
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::workflows::merge_final_pr(&trpc, &workflow_id) })
            .await;
        let _ = handle.update(cx, |_, window, cx| {
            if let Err(err) = result {
                MergingFinalPrs::remove(&id, cx);
                window.refresh();
                window.push_notification(
                    Notification::error(SharedString::from(err.user_message())),
                    cx,
                );
            }
        });
    })
    .detach();
}

/// A member's call on a `proposed` node: admit it, or dismiss it.
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

/// Unsticking a failed node: `retry` or `skip`.
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
            if let Err(err) = result {
                log::warn!("workflows: updating the workflow failed: {err}");
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

// ---------------------------------------------------------------------------
// Styleguide
// ---------------------------------------------------------------------------

/// The IDE styleguide's `workflow-graph` entry: ONE fixed sample of the
/// page's top — the caption line and the node strip — drawn by the page's
/// own code. No store, no workflow row: the facts are spelled out here.
pub(crate) fn styleguide_sample_graph(cx: &App) -> AnyElement {
    use domain::statuses::constructed_default;
    use domain::IssueStatus;

    let node = |id: &str, identifier: &str, state: &str, wave: i64, lane: i64, members: usize| {
        StripNodeInput {
            id: id.to_string(),
            identifier: identifier.to_string(),
            state: state.to_string(),
            wave,
            lane,
            members,
            live: state == "running",
            needs_you: state == "waiting",
            note: None,
        }
    };
    let inputs = vec![
        node("contract", "EXP-1029", "landed", 0, 0, 0),
        node("deck", "EXP-1032", "ready", 1, 0, 2),
        node("running", "EXP-1034", "running", 1, 1, 0),
        node("asks", "EXP-1035", "waiting", 1, 2, 0),
        node("failed", "EXP-1036", "failed", 2, 0, 0),
    ];
    let strip = workflow_node_strip(&inputs, &[]);
    let titles = [
        ("contract", "Workflow contract", IssueStatus::Done),
        ("deck", "Workflow screen (web)", IssueStatus::Backlog),
        ("running", "Workflow screen (IDE)", IssueStatus::InProgress),
        ("asks", "Workflow screen (iOS)", IssueStatus::InProgress),
        ("failed", "Workflow screen (Android)", IssueStatus::InProgress),
    ];
    let live_tone = cx.theme().success;
    let waves: Vec<Vec<ChipFacts>> = strip
        .iter()
        .map(|wave| {
            wave.nodes
                .iter()
                .map(|chip| {
                    let (_, title, status) = titles
                        .iter()
                        .find(|(id, _, _)| *id == chip.id)
                        .copied()
                        .unwrap_or(("", "", IssueStatus::Backlog));
                    ChipFacts {
                        chip: chip.clone(),
                        issue_title: title.to_string(),
                        issue_ids: Vec::new(),
                        status: Some(constructed_default(status)),
                        busy_tone: chip.live.then_some(live_tone),
                        proposed: false,
                        menu: Vec::new(),
                    }
                })
                .collect()
        })
        .collect();
    let mut selection = Selection::default();
    selection.select("running");
    let header_nodes: Vec<HeaderNode> = inputs
        .iter()
        .map(|node| HeaderNode {
            state: node.state.clone(),
            members: node.members,
        })
        .collect();
    let caption = workflow_header_caption("running", &header_nodes, Some("MacBook"));
    v_flex()
        .w_full()
        .gap_3()
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(SharedString::from(caption)),
        )
        .child(render_strip(&waves, &selection, None, cx))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order() -> Vec<String> {
        ["a", "b", "c", "d"].iter().map(|id| id.to_string()).collect()
    }

    fn picked(selection: &Selection) -> Vec<String> {
        selection
            .picked_in(&order())
            .into_iter()
            .cloned()
            .collect()
    }

    #[test]
    fn a_workflow_opens_on_all() {
        let selection = Selection::default();
        assert!(selection.is_all());
        assert_eq!(selection.position(&order()), 0);
        assert_eq!(selection.single(), None);
    }

    #[test]
    fn a_plain_click_picks_one_node() {
        let mut selection = Selection::default();
        apply_click(&mut selection, &order(), "c", &Modifiers::default());
        assert_eq!(picked(&selection), vec!["c"]);
        assert_eq!(selection.single(), Some("c"));
        apply_click(&mut selection, &order(), "a", &Modifiers::default());
        assert_eq!(picked(&selection), vec!["a"]);
    }

    #[test]
    fn cmd_toggles_and_emptying_the_set_is_all_again() {
        let mut selection = Selection::default();
        let cmd = Modifiers::secondary_key();
        apply_click(&mut selection, &order(), "b", &cmd);
        apply_click(&mut selection, &order(), "d", &cmd);
        assert_eq!(picked(&selection), vec!["b", "d"]);
        assert_eq!(selection.single(), None);
        apply_click(&mut selection, &order(), "b", &cmd);
        assert_eq!(picked(&selection), vec!["d"]);
        apply_click(&mut selection, &order(), "d", &cmd);
        assert!(selection.is_all());
    }

    #[test]
    fn shift_extends_a_range_in_dag_order() {
        let mut selection = Selection::default();
        selection.select("d");
        let shift = Modifiers::shift();
        apply_click(&mut selection, &order(), "b", &shift);
        assert_eq!(picked(&selection), vec!["b", "c", "d"]);
        // The anchor stays where the range started.
        apply_click(&mut selection, &order(), "c", &shift);
        assert_eq!(picked(&selection), vec!["c", "d"]);
        // No anchor yet: a shift-click is a plain pick.
        let mut fresh = Selection::default();
        fresh.extend(&order(), "b");
        assert_eq!(picked(&fresh), vec!["b"]);
    }

    #[test]
    fn stepping_walks_all_then_the_dag_order_and_clamps() {
        let mut selection = Selection::default();
        selection.step(&order(), -1);
        assert!(selection.is_all(), "All is the left end");
        selection.step(&order(), 1);
        assert_eq!(selection.single(), Some("a"));
        selection.step(&order(), 1);
        selection.step(&order(), 1);
        assert_eq!(selection.single(), Some("c"));
        selection.step(&order(), 5);
        assert_eq!(selection.single(), Some("d"), "the last node is the right end");
        selection.step(&order(), -4);
        assert!(selection.is_all());
    }

    #[test]
    fn a_step_from_a_set_starts_at_its_anchor_and_picks_one() {
        let mut selection = Selection::default();
        selection.select("a");
        selection.toggle("c");
        selection.step(&order(), 1);
        assert_eq!(picked(&selection), vec!["d"]);
    }

    #[test]
    fn a_node_that_left_the_workflow_leaves_the_selection() {
        let mut selection = Selection::default();
        selection.select("a");
        selection.toggle("c");
        selection.retain(&["a".to_string(), "b".to_string()]);
        assert_eq!(picked(&selection), vec!["a"]);
        assert_eq!(selection.position(&order()), 1);
        selection.retain(&[]);
        assert!(selection.is_all());
    }

    #[test]
    fn arrows_and_j_k_step_and_nothing_else_does() {
        assert_eq!(step_for_key("left"), Some(-1));
        assert_eq!(step_for_key("k"), Some(-1));
        assert_eq!(step_for_key("right"), Some(1));
        assert_eq!(step_for_key("j"), Some(1));
        assert_eq!(step_for_key("enter"), None);
        assert_eq!(step_for_key("up"), None);
    }

    fn session(status: &str, agent_busy: Option<bool>) -> CodingSession {
        serde_json::from_value(serde_json::json!({
            "id": "s1",
            "status": status,
            "agent_busy": agent_busy,
            "last_heartbeat_at": chrono::Utc::now().to_rfc3339(),
            "updated_at": chrono::Utc::now().to_rfc3339(),
        }))
        .expect("a coding session row")
    }

    #[test]
    fn a_chip_pings_only_for_a_running_row_mid_turn() {
        let now = chrono::Utc::now().timestamp();
        let busy = session("running", Some(true));
        assert!(!node_run_busy(
            &busy,
            queries::CodingSessionDisplay::NeedsInput,
            now
        ));
        let idle = session("running", Some(false));
        assert!(!node_run_busy(&idle, queries::CodingSessionDisplay::Running, now));
    }

    #[test]
    fn a_node_run_is_the_named_one_else_the_newest() {
        let node: WorkflowNodeRow = serde_json::from_value(serde_json::json!({
            "id": "n1",
        }))
        .unwrap();
        let run = |id: &str, started: &str| -> CodingSession {
            serde_json::from_value(serde_json::json!({
                "id": id,
                "workflow_node_id": "n1",
                "started_at": started,
            }))
            .unwrap()
        };
        let sessions = vec![run("old", "2026-09-01T00:00:00Z"), run("new", "2026-09-02T00:00:00Z")];
        assert_eq!(node_run(&node, &sessions).map(|run| run.id), Some("new".to_string()));
        let named = WorkflowNodeRow {
            session_id: Some("old".to_string()),
            ..node
        };
        assert_eq!(node_run(&named, &sessions).map(|run| run.id), Some("old".to_string()));
    }
}
