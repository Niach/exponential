//! The ONE shared Start-coding dialog — the issue-detail Play button and the
//! bulk bar's "Start coding" action both land here (`open_for_issue` /
//! `open_for_selection`). One surface, two run modes decided by the checked
//! count:
//!
//! - **1 issue** → today's plain single-issue session (its own worktree +
//!   `exp/<IDENTIFIER>` branch, `PrepareRequest::Issue`).
//! - **2+ issues** → a BATCH run (EXP-106): ONE Claude session on ONE
//!   `exp/batch-<id8>` branch implementing every checked issue and opening
//!   ONE combined PR (`PrepareRequest::Batch`). Deliberately loose — no
//!   per-issue subagent definitions, no waves; Claude organizes the work.
//!
//! The multi-issue PICKER is always present: a searchable checklist scoped to
//! the pre-seeded issues' board(s), OPEN issues only (EXP-119 —
//! `done`/`cancelled`/`duplicate`/PR-merged rows are hidden; other boards'
//! issues too). Pre-seeded ids are exempt from both filters and force-check:
//! the explicit pick wins, which keeps the Play-button re-run of a done issue
//! working. Repo probes (`repositories.forIssue`, background executor,
//! generation-guarded) run LAZILY for checked issues only — ONE repository
//! per run is enforced.
//!
//! Options: Model + Effort [`ChoiceSelect`]s, the ULTRACODE switch
//! (`--effort ultracode`, model-independent; disables the Effort select) and
//! the native "Plan mode" checkbox. Defaults come from [`coding::Settings`]'
//! per-AGENT fields (EXP-206 — one set for single-issue and batch runs
//! alike); switching the agent tab re-seeds them. The agent tab strip offers
//! only the doctor-installed agents.
//!
//! EXP-257: the dialog is the ONE unified launch surface — a segmented
//! **Issues | Actions** subject strip sits at the top. The Actions tab
//! replaces the issue checklist with an action search + single-select list
//! (the server-defined "Create action" builtin pinned first) and the
//! selected action's typed input fields (text / repo / board); the SHARED
//! bottom half (agent tabs, model/effort, toggles, footer) applies to both.
//! An action launch goes through the trust-gated
//! [`crate::action_run::start_action_run`] instead of `coding::prepare`
//! directly.
//!
//! Launch = snapshot → [`coding::prepare`] on the background executor → the
//! shared `coding_flow::spawn_into_window` foreground spawn. A
//! `Prepared::Disabled` reason renders inline and keeps the dialog open.

use std::collections::{HashMap, HashSet};

use gpui::{
    div, prelude::FluentBuilder as _, px, size, AnyWindowHandle, App, AppContext as _, Entity,
    InteractiveElement as _, IntoElement, ParentElement, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    scroll::{Scrollbar, ScrollbarAxis},
    select::Select,
    tab::{Tab, TabBar, TabVariant},
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _, Size,
};
use sync::Store;

use api::repositories::IssueRepository;
use coding::{
    ActionInputValue, BatchIssueSpec, BatchLaunchRequest, CodingAgent, LaunchOptions,
    LaunchOrigin, Prepared, PrepareRequest, RepoGroup,
};
use domain::IssueStatus;

use crate::action_run::{self, ActionRepo, ActionRepoRow, StartActionArgs};
use crate::coding_flow::{self, CodingHub, SessionSubject};
use crate::coding_selects::{
    agent_icon, choice_select, effort_choices_for, model_choices_for, selected, ChoiceSelect,
};
use crate::icons::ExpIcon;
use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::queries;

/// Soft cost warning threshold: more checked issues than this shows the
/// "token-expensive" note (no hard gate — coding is unmetered).
const COST_NOTE_THRESHOLD: usize = 6;

/// Hard cap per run: every checked issue adds a prompt section, and a batch
/// beyond this size stops being one coherent session anyway.
const MAX_ISSUES_PER_RUN: usize = 30;

/// Unchecked search matches rendered at once — a team can hold hundreds
/// of issues, and the checklist is a plain (non-virtual) list.
const MAX_UNCHECKED_ROWS: usize = 50;

/// Open the dialog from an issue's Play button: pre-seed that issue checked.
/// A no-op when the issue row isn't synced (racing a delete).
pub fn open_for_issue(window: &mut Window, cx: &mut App, issue_id: String) {
    let Some(issue) = Store::global(cx)
        .collections()
        .issues
        .read(cx)
        .get(&issue_id)
        .cloned()
    else {
        log::warn!("[ui] start-coding dialog for unknown issue {issue_id}");
        return;
    };
    let Some(team_id) = Store::global(cx)
        .collections()
        .boards
        .read(cx)
        .get(&issue.board_id)
        .map(|board| board.team_id.clone())
    else {
        log::warn!("[ui] start-coding dialog: board not synced for {issue_id}");
        return;
    };
    open(window, cx, team_id, vec![issue.id], None);
}

/// Open the dialog from the bulk bar with the selection pre-checked.
pub fn open_for_selection(
    window: &mut Window,
    cx: &mut App,
    team_id: String,
    issue_ids: Vec<String>,
) {
    open(window, cx, team_id, issue_ids, None);
}

/// Open the dialog on the ACTIONS tab with `action_id` preselected (EXP-257
/// — the actions panel rows land here).
pub fn open_for_action(window: &mut Window, cx: &mut App, team_id: String, action_id: String) {
    open(window, cx, team_id, Vec::new(), Some(action_id));
}

fn open(
    window: &mut Window,
    cx: &mut App,
    team_id: String,
    preselected: Vec<String>,
    preselect_action: Option<String>,
) {
    // EXP-268: widescreen two-column layout (web `sm:max-w-3xl` parity —
    // picker left, options right); the launched terminal tab lands back in
    // the OPENER window (EXP-284: the dialog is its own native window).
    let opener = window.window_handle();
    let height = (window.viewport_size().height * 0.85).min(px(640.));
    let spec = DialogSpec::new("Start coding", size(px(760.), height));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| {
            StartCodingDialogView::new(team_id, preselected, preselect_action, opener, window, cx)
        });
        let busy = view.clone();
        DialogContent::new(view)
            .header("Start coding")
            .can_close(move |cx| !busy.read(cx).launching)
    });
}

/// The unified dialog's top-level subject (EXP-257).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SubjectTab {
    Issues,
    Actions,
}

/// Actions-tab list state (EXP-268: a LIVE read of the synced `actions`
/// shape — Loading only until the shape reaches readiness).
enum ActionsLoad {
    Loading,
    Ready,
}

/// One checklist row, snapshotted from the sync store at open (titles and
/// descriptions ride into the launch request verbatim — the launcher never
/// re-reads the collections).
struct IssueRow {
    issue_id: String,
    identifier: String,
    title: String,
    description: Option<String>,
    /// Status snapshot at open — the launcher's step 6.5 flips backlog/todo
    /// to `in_progress` at launch (EXP-194).
    status: IssueStatus,
    /// The closed-state note (`done`/`cancelled`/`duplicate`/PR-merged),
    /// shown muted next to the title. Only pre-seeded rows can carry one
    /// (EXP-119 filters closed rows out of the pool) — it flags a re-run.
    /// `None` = plain row.
    state_hint: Option<&'static str>,
}

/// One issue's `repositories.forIssue` probe state.
enum RepoState {
    Loading,
    /// `Ready(None)` = no repository linked (excluded from the run).
    Ready(Option<IssueRepository>),
    /// Transport failure — the issue can't resolve a repo, so it is
    /// excluded like a repo-less one (the message says why).
    Error(String),
}

/// The `(ultracode, plan_mode, skip_permissions)` settings defaults for
/// `agent`, capability-masked (EXP-201: ultracode/plan are Claude-only, skip
/// does not exist for pi). EXP-206: ONE set of defaults — a single-issue run
/// and a multi-issue batch run seed identically, and skip permissions is a
/// per-AGENT setting.
fn agent_defaults(settings: &coding::Settings, agent: CodingAgent) -> (bool, bool, bool) {
    (
        settings.claude_ultracode && agent.supports_ultracode(),
        settings.claude_plan_mode && agent.supports_plan_mode(),
        settings.skip_permissions_for(agent) && agent.supports_skip_permissions(),
    )
}

pub struct StartCodingDialogView {
    team_id: String,
    /// The window that opened this dialog — the launched terminal tab spawns
    /// into ITS dock (EXP-284: the dialog is its own native window).
    opener: AnyWindowHandle,
    /// EXP-257: which subject half is showing — Issues (the checklist) or
    /// Actions (the single-select action list + input fields).
    subject_tab: SubjectTab,
    /// The team's actions (builtin pinned first by flag) — refreshed live
    /// from the synced `actions` collection (EXP-268).
    actions: Vec<api::actions::Action>,
    actions_load: ActionsLoad,
    /// The single-selected action (the Actions tab has no multi-run).
    selected_action_id: Option<String>,
    /// `open_for_action`'s preselect, applied once the list lands.
    pending_action_preselect: Option<String>,
    action_search: Entity<InputState>,
    action_list_scroll: ScrollHandle,
    action_inputs_scroll: ScrollHandle,
    /// Per-TEXT-input editor states for the selected action, keyed by input
    /// key — built in [`Self::select_action`] (never in render).
    action_text_inputs: HashMap<String, Entity<InputState>>,
    /// Change subscriptions for the states above (footer gate re-evaluates
    /// while typing); replaced wholesale on re-selection.
    action_input_subscriptions: Vec<Subscription>,
    /// Picked repo per `repo` input key (absent = none picked).
    action_repo_picks: HashMap<String, ActionRepoRow>,
    /// Picked `(board id, board name)` per `board` input key.
    action_board_picks: HashMap<String, (String, String)>,
    /// Picked `(representative issue id, display label)` per `pr` input key
    /// (EXP-259 — the fix-conflicts builtin's open-PR target).
    action_pr_picks: HashMap<String, (String, String)>,
    /// `repositories.list` rows for the repo input pickers.
    team_repos: Vec<ActionRepoRow>,
    /// Every non-archived team issue, board→number ordered.
    rows: Vec<IssueRow>,
    /// issue id → probe state (LAZY: only checked issues probe).
    repos: HashMap<String, RepoState>,
    /// issue id → its persisted worktree's resume state (EXP-202 — computed
    /// alongside the repo probe; drives the Resume affordance for a
    /// single-checked issue, per-agent since EXP-210).
    worktrees: HashMap<String, coding_flow::WorktreeResume>,
    /// EXP-202: "Resume previous session" checkbox state. Only ACTIVE
    /// ([`Self::resume_candidate`]) when exactly one issue is checked and
    /// its worktree exists; default-on so a re-launch resumes by default.
    resume: bool,
    /// Stale-probe guard (old results must not land after a re-open).
    probe_generation: u64,
    checked: HashSet<String>,
    search: Entity<InputState>,
    /// Scroll position of the checklist (view state so it survives
    /// re-renders — the EXP-67 scroll-pane idiom, bounded by `max_h`).
    list_scroll: ScrollHandle,
    /// The selected agent CLI (EXP-201) — the tab strip above the pickers;
    /// seeded from `settings.default_agent`, overridable per launch.
    agent: CodingAgent,
    model: ChoiceSelect,
    effort: ChoiceSelect,
    /// Dynamic workflows (`--effort ultracode`) — Claude-only, any model.
    ultracode: bool,
    /// Native Claude plan mode (`--permission-mode plan`). Claude-only.
    plan_mode: bool,
    /// Full permission bypass (claude/codex; pi has no permission system).
    skip_permissions: bool,
    launching: bool,
    error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl StartCodingDialogView {
    fn new(
        team_id: String,
        preselected: Vec<String>,
        preselect_action: Option<String>,
        opener: AnyWindowHandle,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let hub = CodingHub::global(cx);
        let settings = hub.read(cx).settings.clone();

        // Snapshot the picker's candidate pool (EXP-119): the pre-seeded
        // issues' board(s) only, OPEN issues only — unrelated boards and
        // closed rows just buried the launchable ones. Pre-seeded ids are
        // exempt from both filters: the explicit pick wins (the Play-button
        // re-run of a done issue), and a checked id MUST keep its row —
        // `batch_request`/`launch_blocker` iterate `rows` and would silently
        // drop it from the run otherwise.
        let preselected: HashSet<String> = preselected.into_iter().collect();
        let mut issues = queries::team_issues(cx, &team_id);
        // `team_issues` hides ARCHIVED rows, but the Play button resolves
        // its seed from the raw collection — re-read any missing seed raw so
        // an archived pick shows up force-checked instead of silently
        // vanishing from the run (`batch_request` iterates `rows`).
        for seed in &preselected {
            if !issues.iter().any(|issue| &issue.id == seed) {
                if let Some(issue) = Store::global(cx).collections().issues.read(cx).get(seed) {
                    issues.push(issue.clone());
                }
            }
        }
        let scope_boards: HashSet<String> = issues
            .iter()
            .filter(|issue| preselected.contains(&issue.id))
            .map(|issue| issue.board_id.clone())
            .collect();
        issues.retain(|issue| {
            if preselected.contains(&issue.id) {
                return true;
            }
            // No resolvable seed (racing a delete) → keep the whole
            // team pool rather than an empty picker.
            if !scope_boards.is_empty() && !scope_boards.contains(&issue.board_id) {
                return false;
            }
            let closed = matches!(
                issue.status,
                IssueStatus::Done | IssueStatus::Cancelled | IssueStatus::Duplicate
            ) || issue.pr_state.as_deref() == Some("merged");
            !closed
        });
        issues.sort_by(|a, b| {
            a.board_id
                .cmp(&b.board_id)
                .then_with(|| a.number.cmp(&b.number))
        });
        let mut checked = HashSet::new();
        let rows: Vec<IssueRow> = issues
            .into_iter()
            .map(|issue| {
                let merged = issue.pr_state.as_deref() == Some("merged");
                let state_hint = if merged {
                    Some("PR merged")
                } else {
                    match issue.status {
                        IssueStatus::Done => Some("done"),
                        IssueStatus::Cancelled => Some("cancelled"),
                        IssueStatus::Duplicate => Some("duplicate"),
                        _ => None,
                    }
                };
                // Pre-seeded ids force-check regardless of the state hint —
                // the user explicitly picked them.
                if preselected.contains(&issue.id) {
                    checked.insert(issue.id.clone());
                }
                IssueRow {
                    issue_id: issue.id,
                    identifier: issue.identifier,
                    title: issue.title,
                    description: issue.description,
                    status: issue.status,
                    state_hint,
                }
            })
            .collect();

        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search issues…"));
        let action_search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search actions…"));
        let local_sessions = coding_flow::LocalSessions::global(cx);
        let synced_sessions = Store::global(cx).collections().coding_sessions.clone();
        let synced_actions = Store::global(cx).collections().actions.clone();
        let subscriptions = vec![
            // EXP-268: the Actions tab is a live read of the synced shape —
            // an MCP-authored action appears while the dialog is open.
            cx.observe_in(&synced_actions, window, |this: &mut Self, _, window, cx| {
                this.refresh_actions(window, cx);
                cx.notify();
            }),
            // Doctor lands / re-runs → the footer gate moves AND the agent
            // tab strip re-filters to the installed agents (EXP-206), so a
            // selection whose tab just vanished hops to an installed one.
            cx.observe_in(&hub, window, |this: &mut Self, _, window, cx| {
                this.reconcile_agent(window, cx);
                cx.notify();
            }),
            // EXP-202: the one-session-per-issue blocker tracks both the
            // local registry (this process) and the synced rows (other
            // devices) — re-render whenever either moves.
            cx.observe(&local_sessions, |_: &mut Self, _, cx| cx.notify()),
            cx.observe(&synced_sessions, |_: &mut Self, _, cx| cx.notify()),
            cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe(&action_search, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];

        let agent = settings.default_agent;
        let (ultracode, plan_mode, skip_permissions) = agent_defaults(&settings, agent);

        let mut this = Self {
            team_id,
            opener,
            subject_tab: if preselect_action.is_some() {
                SubjectTab::Actions
            } else {
                SubjectTab::Issues
            },
            actions: Vec::new(),
            actions_load: ActionsLoad::Loading,
            selected_action_id: None,
            pending_action_preselect: preselect_action,
            action_search,
            action_list_scroll: ScrollHandle::new(),
            action_inputs_scroll: ScrollHandle::new(),
            action_text_inputs: HashMap::new(),
            action_input_subscriptions: Vec::new(),
            action_repo_picks: HashMap::new(),
            action_board_picks: HashMap::new(),
            action_pr_picks: HashMap::new(),
            team_repos: Vec::new(),
            rows,
            repos: HashMap::new(),
            worktrees: HashMap::new(),
            resume: true,
            probe_generation: 0,
            checked,
            search,
            list_scroll: ScrollHandle::new(),
            agent,
            model: choice_select(
                model_choices_for(agent),
                settings.model_for(agent),
                window,
                cx,
            ),
            effort: choice_select(
                effort_choices_for(agent),
                settings.effort_for(agent),
                window,
                cx,
            ),
            ultracode,
            plan_mode,
            skip_permissions,
            launching: false,
            error: None,
            _subscriptions: subscriptions,
        };
        this.probe_generation += 1;
        let ids: Vec<String> = this.checked.iter().cloned().collect();
        for issue_id in ids {
            this.ensure_probe(issue_id, cx);
        }
        // EXP-268: the Actions tab reads the synced shape — seed it now (and
        // the observer above keeps it live); repos stay a tRPC prefetch.
        this.refresh_actions(window, cx);
        this.fetch_team_repos(cx);
        // The doctor usually ran long before the dialog opens — if the
        // settings default agent isn't installed, preselect one that is
        // (EXP-206: the tab strip only shows installed agents).
        this.reconcile_agent(window, cx);
        this
    }

    // -- Actions tab (EXP-257) ------------------------------------------------

    /// Refresh the Actions tab from the synced `actions` collection
    /// (EXP-268). Builtin pinned FIRST; an `open_for_action` preselect is
    /// applied once the shape is ready.
    fn refresh_actions(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let (actions, ready) = queries::team_actions(cx, &self.team_id);
        self.actions = actions;
        self.actions_load = if ready {
            ActionsLoad::Ready
        } else {
            ActionsLoad::Loading
        };
        if ready {
            if let Some(pending) = self.pending_action_preselect.take() {
                self.select_action(pending, window, cx);
            }
        }
    }

    /// Prefetch `repositories.list` for the repo input pickers. Best-effort:
    /// a failed fetch leaves the pickers empty (a required repo input then
    /// blocks with its fill message rather than a broken menu).
    fn fetch_team_repos(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let team_id = self.team_id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { action_run::fetch_repositories(&trpc, &team_id) })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(rows) => {
                    this.team_repos = rows;
                    cx.notify();
                }
                Err(err) => log::warn!("[ui] repositories.list failed: {err}"),
            });
        })
        .detach();
    }

    /// The single-selected action's row, if the list landed.
    fn selected_action(&self) -> Option<&api::actions::Action> {
        let id = self.selected_action_id.as_deref()?;
        self.actions.iter().find(|action| action.id == id)
    }

    /// Select an action row: reset the per-input state and build the TEXT
    /// inputs' editor states (here, never in render — gpui entities must not
    /// be created mid-frame).
    fn select_action(
        &mut self,
        action_id: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.selected_action_id.as_deref() == Some(action_id.as_str()) {
            return;
        }
        self.action_text_inputs.clear();
        self.action_input_subscriptions.clear();
        self.action_repo_picks.clear();
        self.action_board_picks.clear();
        self.action_pr_picks.clear();
        if let Some(action) = self.actions.iter().find(|action| action.id == action_id) {
            for input in &action.inputs {
                if input.input_type != "text" {
                    continue;
                }
                let placeholder = input.placeholder.clone();
                let state = cx.new(|cx| {
                    let mut state = InputState::new(window, cx);
                    if let Some(placeholder) = placeholder {
                        state = state.placeholder(placeholder);
                    }
                    state
                });
                // The footer gate ("Fill in …") must re-evaluate per keystroke.
                self.action_input_subscriptions.push(cx.subscribe(
                    &state,
                    |_, _, event: &InputEvent, cx| {
                        if matches!(event, InputEvent::Change) {
                            cx.notify();
                        }
                    },
                ));
                self.action_text_inputs.insert(input.key.clone(), state);
            }
        }
        self.selected_action_id = Some(action_id);
        cx.notify();
    }

    /// Whether `input` currently holds a usable value.
    fn action_input_filled(&self, input: &api::actions::ActionInput, cx: &App) -> bool {
        match input.input_type.as_str() {
            "text" => self
                .action_text_inputs
                .get(&input.key)
                .is_some_and(|state| !state.read(cx).value().trim().is_empty()),
            "repo" => self.action_repo_picks.contains_key(&input.key),
            "board" => self.action_board_picks.contains_key(&input.key),
            "pr" => self.action_pr_picks.contains_key(&input.key),
            _ => false,
        }
    }

    /// Snapshot the filled input values in DEFINITION order (empty optional
    /// inputs are omitted): text → value=display=text; repo → value=id,
    /// display=fullName; board → value=id, display=name.
    fn collect_action_inputs(
        &self,
        action: &api::actions::Action,
        cx: &App,
    ) -> Vec<ActionInputValue> {
        let mut values = Vec::new();
        for input in &action.inputs {
            match input.input_type.as_str() {
                "text" => {
                    let Some(text) = self
                        .action_text_inputs
                        .get(&input.key)
                        .map(|state| state.read(cx).value().trim().to_string())
                    else {
                        continue;
                    };
                    if text.is_empty() {
                        continue;
                    }
                    values.push(ActionInputValue {
                        key: input.key.clone(),
                        label: input.label.clone(),
                        input_type: input.input_type.clone(),
                        value: text.clone(),
                        display: Some(text),
                    });
                }
                "repo" => {
                    let Some(repo) = self.action_repo_picks.get(&input.key) else {
                        continue;
                    };
                    values.push(ActionInputValue {
                        key: input.key.clone(),
                        label: input.label.clone(),
                        input_type: input.input_type.clone(),
                        value: repo.id.clone(),
                        display: Some(repo.full_name.clone()),
                    });
                }
                "board" => {
                    let Some((board_id, name)) = self.action_board_picks.get(&input.key) else {
                        continue;
                    };
                    values.push(ActionInputValue {
                        key: input.key.clone(),
                        label: input.label.clone(),
                        input_type: input.input_type.clone(),
                        value: board_id.clone(),
                        display: Some(name.clone()),
                    });
                }
                "pr" => {
                    let Some((issue_id, label)) = self.action_pr_picks.get(&input.key) else {
                        continue;
                    };
                    values.push(ActionInputValue {
                        key: input.key.clone(),
                        label: input.label.clone(),
                        input_type: input.input_type.clone(),
                        value: issue_id.clone(),
                        display: Some(label.clone()),
                    });
                }
                // Unknown types never reach here — the launch blocker gates.
                _ => {}
            }
        }
        values
    }

    /// Kick ONE `repositories.forIssue` probe for `issue_id` if it never ran
    /// (background executor, generation-guarded like
    /// `StartCodingControl::ensure_probe`). Lazy by design: only checked
    /// issues probe — a whole-team eager fan-out would be hundreds of
    /// tRPC calls.
    fn ensure_probe(&mut self, issue_id: String, cx: &mut gpui::Context<Self>) {
        if self.repos.contains_key(&issue_id) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            self.repos
                .insert(issue_id, RepoState::Error("Not signed in.".to_string()));
            return;
        };
        self.repos.insert(issue_id.clone(), RepoState::Loading);
        let generation = self.probe_generation;
        let probe_id = issue_id.clone();
        // EXP-202: once the repo resolves, the same background hop stats the
        // issue's persisted worktree (the launcher's own path derivation) —
        // never a blocking fs call in render.
        let settings = CodingHub::global(cx).read(cx).settings.clone();
        let identifier = self
            .rows
            .iter()
            .find(|row| row.issue_id == issue_id)
            .map(|row| row.identifier.clone());
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let result = api::repositories::for_issue(&trpc, &probe_id);
                    let worktree_resume = match (&result, &identifier) {
                        (Ok(Some(repo)), Some(identifier)) => coding_flow::issue_worktree_resume(
                            &settings,
                            &repo.full_name,
                            identifier,
                        ),
                        _ => coding_flow::WorktreeResume::None,
                    };
                    (result, worktree_resume)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.probe_generation != generation {
                    return; // superseded
                }
                let (result, worktree_resume) = result;
                let state = match result {
                    Ok(repo) => RepoState::Ready(repo),
                    Err(err) => RepoState::Error(err.to_string()),
                };
                // Unresolvable issues can never launch — uncheck them.
                if !matches!(state, RepoState::Ready(Some(_))) {
                    this.checked.remove(&issue_id);
                }
                this.repos.insert(issue_id.clone(), state);
                this.worktrees.insert(issue_id.clone(), worktree_resume);
                cx.notify();
            });
        })
        .detach();
    }

    /// EXP-202: the single checked issue whose persisted worktree already
    /// exists — the only shape a resume can take (batch branches are random
    /// `exp/batch-<id8>` and are never resumable). EXP-210: per-agent — a
    /// worktree only another agent coded in offers nothing to continue for
    /// the selected one, so the Resume row hides instead of promising a
    /// `--continue` that would fail.
    fn resume_candidate(&self) -> Option<&IssueRow> {
        // Resume is an ISSUE concept — the Actions tab never offers it.
        if self.subject_tab != SubjectTab::Issues {
            return None;
        }
        if self.checked.len() != 1 {
            return None;
        }
        let issue_id = self.checked.iter().next()?;
        if !self
            .worktrees
            .get(issue_id)
            .is_some_and(|state| state.offers(self.agent))
        {
            return None;
        }
        self.rows.iter().find(|row| &row.issue_id == issue_id)
    }

    /// Whether the launch will actually RESUME (checkbox on + a candidate).
    fn resume_active(&self) -> bool {
        self.resume && self.resume_candidate().is_some()
    }

    /// The agents the tab strip offers (EXP-206): only the ones the doctor
    /// found installed. While the report is pending — or when NOTHING is
    /// installed — every agent stays visible, so the strip never goes empty
    /// and the footer blocker can name the selected agent's failure.
    fn pickable_agents(report: Option<&coding::DoctorReport>) -> Vec<CodingAgent> {
        let installed = report
            .map(|report| report.installed_agents())
            .unwrap_or_default();
        if installed.is_empty() {
            CodingAgent::ALL.to_vec()
        } else {
            installed
        }
    }

    /// Keep the selection inside the pickable set: when the doctor report
    /// (re)lands and the selected agent has no tab anymore, hop to the first
    /// installed agent (mirrors the remote pickers, which only offer the
    /// device's advertised agents).
    fn reconcile_agent(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let report = CodingHub::global(cx).read(cx).doctor.report.clone();
        let pickable = Self::pickable_agents(report.as_ref());
        if !pickable.contains(&self.agent) {
            if let Some(&first) = pickable.first() {
                self.set_agent(first, window, cx);
            }
        }
    }

    /// Switch the agent tab (EXP-201): rebuild the model/effort selects from
    /// the agent's own choice lists + settings defaults and re-seed the
    /// toggles for the current mode (capability-masked).
    fn set_agent(
        &mut self,
        agent: CodingAgent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.agent == agent {
            return;
        }
        self.agent = agent;
        let settings = CodingHub::global(cx).read(cx).settings.clone();
        self.model = choice_select(
            model_choices_for(agent),
            settings.model_for(agent),
            window,
            cx,
        );
        self.effort = choice_select(
            effort_choices_for(agent),
            settings.effort_for(agent),
            window,
            cx,
        );
        (self.ultracode, self.plan_mode, self.skip_permissions) =
            agent_defaults(&settings, agent);
        cx.notify();
    }

    fn toggle_checked(&mut self, issue_id: String, on: bool, cx: &mut gpui::Context<Self>) {
        if on {
            self.checked.insert(issue_id.clone());
            self.ensure_probe(issue_id, cx);
        } else {
            self.checked.remove(&issue_id);
        }
        cx.notify();
    }

    /// Why the launch button is disabled right now; `None` = launchable.
    fn launch_blocker(&self, cx: &mut gpui::Context<Self>) -> Option<SharedString> {
        if self.launching {
            return Some("Starting…".into());
        }
        let hub = CodingHub::global(cx);
        match hub.read(cx).doctor.report.as_ref() {
            None => return Some("Checking local tools…".into()),
            // Per-agent gate (EXP-201): only git + the SELECTED agent block.
            Some(report) => {
                if let Some(failed) = report.first_failure_for(self.agent) {
                    return Some(
                        failed
                            .error
                            .clone()
                            .unwrap_or_else(|| format!("{} is not available", failed.tool))
                            .into(),
                    );
                }
            }
        }
        // EXP-257: the Actions tab has its own, much shorter gate — the
        // issue checklist/session/repo blockers below don't apply to it.
        if self.subject_tab == SubjectTab::Actions {
            match &self.actions_load {
                ActionsLoad::Loading => return Some("Loading actions…".into()),
                ActionsLoad::Ready => {}
            }
            let Some(action) = self.selected_action() else {
                return Some("Select an action.".into());
            };
            for input in &action.inputs {
                // Unknown input type = a schema this build predates — block
                // hard, never silently degrade to a text field.
                if !matches!(input.input_type.as_str(), "text" | "repo" | "board" | "pr") {
                    return Some("This action needs a newer app version.".into());
                }
            }
            for input in &action.inputs {
                if input.required && !self.action_input_filled(input, cx) {
                    return Some(format!("Fill in {}.", input.label).into());
                }
            }
            return None;
        }
        if self.checked.is_empty() {
            return Some("Select at least one issue.".into());
        }
        if self.checked.len() > MAX_ISSUES_PER_RUN {
            return Some(
                format!("At most {MAX_ISSUES_PER_RUN} issues per run — split the batch.").into(),
            );
        }
        // EXP-202: only ONE session per issue — a second agent spawned into
        // the same `exp/<ID>` worktree would orphan the first. Blocked
        // against both the local registry (this process; the bulk-bar path
        // had no guard) and the live synced rows (another device or a
        // pre-restart session still inside the staleness window).
        let sessions = coding_flow::LocalSessions::global(cx);
        let store = Store::global(cx);
        let now = chrono::Utc::now().timestamp();
        for row in &self.rows {
            if !self.checked.contains(&row.issue_id) {
                continue;
            }
            if sessions.read(cx).get(&row.issue_id).is_some() {
                return Some(
                    format!("Already coding {} — stop that session first.", row.identifier)
                        .into(),
                );
            }
            let synced = store.collections().coding_sessions.read(cx);
            if let Some(session) = synced.iter().find(|session| {
                session.issue_id.as_deref() == Some(row.issue_id.as_str())
                    && queries::coding_session_is_live(session, now)
            }) {
                let device = session
                    .device_label
                    .clone()
                    .unwrap_or_else(|| "another device".to_string());
                return Some(
                    format!(
                        "{} already has a live session on {device} — only one session per issue.",
                        row.identifier
                    )
                    .into(),
                );
            }
        }
        let mut repo: Option<&str> = None;
        for row in &self.rows {
            if !self.checked.contains(&row.issue_id) {
                continue;
            }
            match self.repos.get(&row.issue_id) {
                Some(RepoState::Ready(Some(resolved))) => match repo {
                    None => repo = Some(&resolved.repository_id),
                    Some(existing) if existing == resolved.repository_id => {}
                    Some(_) => {
                        return Some("One repository per run — deselect the others.".into())
                    }
                },
                // Still resolving (or unresolvable-but-checked — transient).
                _ => return Some("Checking linked repositories…".into()),
            }
        }
        None
    }

    /// The dialog's agent/model/effort/mode choices as launch options.
    fn options(&self, cx: &App) -> LaunchOptions {
        LaunchOptions {
            agent: self.agent,
            model: selected(&self.model, cx),
            // Ignored by the argv while ultracode is on (ultracode IS the
            // effort level); blank = omit the flag.
            effort: selected(&self.effort, cx),
            // Capability-clamped so a stale toggle can never leak onto an
            // agent that doesn't support it.
            ultracode: self.ultracode && self.agent.supports_ultracode(),
            // A RESUME never re-enters plan mode (EXP-202): the plan already
            // happened in the conversation being continued.
            plan_mode: self.plan_mode
                && self.agent.supports_plan_mode()
                && !self.resume_active(),
            skip_permissions: self.skip_permissions
                && self.agent.supports_skip_permissions(),
        }
    }

    /// Snapshot the checked set into a [`BatchLaunchRequest`] (2+ checked).
    /// `None` on a racing probe (the blocker just re-checked) — bail quietly.
    fn batch_request(&self, cx: &App) -> Option<BatchLaunchRequest> {
        let mut repo: Option<RepoGroup> = None;
        let mut issues: Vec<BatchIssueSpec> = Vec::new();
        for row in &self.rows {
            if !self.checked.contains(&row.issue_id) {
                continue;
            }
            let Some(RepoState::Ready(Some(resolved))) = self.repos.get(&row.issue_id) else {
                return None;
            };
            if repo.is_none() {
                repo = Some(RepoGroup {
                    repository_id: resolved.repository_id.clone(),
                    full_name: resolved.full_name.clone(),
                    default_branch: resolved.default_branch.clone(),
                });
            }
            issues.push(BatchIssueSpec {
                issue_id: row.issue_id.clone(),
                issue_identifier: row.identifier.clone(),
                title: row.title.clone(),
                description: row.description.clone(),
                status: row.status,
            });
        }
        Some(BatchLaunchRequest {
            batch_id: coding::new_batch_id(),
            team_id: self.team_id.clone(),
            repo: repo?,
            issues,
            device_label: coding::default_device_label(),
            origin: LaunchOrigin::Local,
            options: self.options(cx),
        })
    }

    /// The launch: 1 checked issue = the plain single-issue path, 2+ = a
    /// batch run. Prepare on the background executor, spawn on the foreground
    /// (the shared path).
    fn launch(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.launching || self.launch_blocker(cx).is_some() {
            return;
        }
        // EXP-257: an ACTION launch rides the trust-gated runner (which owns
        // fetch-fresh + trust dialog + prepare/spawn) — close the dialog and
        // hand off; the runner surfaces failures on the window itself.
        if self.subject_tab == SubjectTab::Actions {
            let Some(action) = self.selected_action().cloned() else {
                return;
            };
            let inputs = self.collect_action_inputs(&action, cx);
            let options = self.options(cx);
            // The runner's terminal tab targets the OPENER window — this
            // dialog window is about to be gone.
            let handle = self.opener;
            native_dialog::close_dialog_window(window, cx);
            action_run::start_action_run(
                StartActionArgs {
                    action_id: action.id,
                    team_id: self.team_id.clone(),
                    repo: ActionRepo::Resolve,
                    options,
                    origin: LaunchOrigin::Local,
                    inputs,
                    target: Some(handle),
                    activate_app: false,
                },
                cx,
            );
            return;
        }
        if self.checked.len() == 1 {
            let issue_id = self.checked.iter().next().cloned().expect("one checked");
            let options = self.options(cx);
            let resume = self.resume_active();
            let Some((request, deps)) =
                coding_flow::build_launch(&issue_id, LaunchOrigin::Local, options, resume, cx)
            else {
                self.error = Some("Sign in and wait for sync before starting a session.".into());
                cx.notify();
                return;
            };
            return self.run_prepare(
                PrepareRequest::Issue(request),
                deps,
                SessionSubject::Issue(issue_id),
                window,
                cx,
            );
        }
        let Some(request) = self.batch_request(cx) else {
            return;
        };
        let batch_id = request.batch_id.clone();
        let Some(deps) = coding_flow::build_batch_deps(cx) else {
            self.error = Some("Sign in and wait for sync before starting a session.".into());
            cx.notify();
            return;
        };
        self.run_prepare(
            PrepareRequest::Batch(request),
            deps,
            SessionSubject::Batch(batch_id),
            window,
            cx,
        );
    }

    /// Shared prepare→spawn tail for both modes: background
    /// [`coding::prepare`], then `coding_flow::spawn_into_window` on the
    /// foreground; a `Disabled` reason (or spawn error) renders inline.
    fn run_prepare(
        &mut self,
        request: PrepareRequest,
        deps: coding::CodingDeps,
        subject: SessionSubject,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.launching = true;
        self.error = None;
        cx.notify();

        let hooks = crate::steer_wiring::hook_setup(cx);
        let opener = self.opener;
        cx.spawn_in(window, async move |this, window| {
            let prepared = window
                .background_executor()
                .spawn(async move { coding::prepare_with_hooks(&request, &deps, hooks.as_ref()) })
                .await;
            // The terminal tab spawns into the OPENER window's dock
            // (EXP-284) — a fresh cross-window update from the async
            // context, never from inside this window's update.
            let outcome: Result<(), SharedString> = match prepared {
                Ok(Prepared::Ready(prepared)) => {
                    match opener.update(window, |_, window, cx| {
                        coding_flow::spawn_into_window(prepared, subject, window, cx)
                    }) {
                        Ok(Ok(())) => Ok(()),
                        Ok(Err(message)) => Err(message.into()),
                        Err(_) => Err("The window that opened this dialog was closed.".into()),
                    }
                }
                // Explain inline, never crash — the exact §7 copy.
                Ok(Prepared::Disabled(reason)) => Err(reason.message().into()),
                Err(err) => Err(format!("Could not start the coding session: {err}").into()),
            };
            let _ = this.update_in(window, |this, window, cx| {
                this.launching = false;
                match outcome {
                    Ok(()) => native_dialog::close_dialog_window(window, cx),
                    Err(message) => this.error = Some(message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    // -- render pieces --------------------------------------------------------

    /// One checklist row: checkbox + identifier + title (+ state hint or the
    /// probe's exclusion note).
    fn issue_row(&self, ix: usize, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let row = &self.rows[ix];
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let is_checked = self.checked.contains(&row.issue_id);
        // A checked row whose probe failed/excluded it renders the reason
        // (the probe auto-unchecks; this covers the transient frame).
        let probe_note: Option<SharedString> = match self.repos.get(&row.issue_id) {
            Some(RepoState::Ready(None)) => Some("no repository linked".into()),
            Some(RepoState::Error(err)) => {
                Some(format!("repository check failed: {err}").into())
            }
            Some(RepoState::Loading) if is_checked => Some("resolving repository…".into()),
            _ => None,
        };
        let toggle_id = row.issue_id.clone();

        h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .child(
                Checkbox::new(SharedString::from(format!("sc-check-{}", row.issue_id)))
                    .checked(is_checked)
                    .on_click(cx.listener(move |this, on: &bool, _, cx| {
                        this.toggle_checked(toggle_id.clone(), *on, cx);
                    })),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .font_family(theme::terminal::FONT_FAMILY)
                    .child(SharedString::from(row.identifier.clone())),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .truncate()
                    .text_color(theme.foreground)
                    .child(SharedString::from(row.title.clone())),
            )
            .when_some(
                probe_note.or_else(|| row.state_hint.map(SharedString::from)),
                |this, note| {
                    this.child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .child(note),
                    )
                },
            )
            .into_any_element()
    }

    /// A labeled field column with an optional muted hint under the control.
    fn labeled_field(
        label: &'static str,
        field: gpui::AnyElement,
        hint: Option<&'static str>,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        v_flex()
            .flex_1()
            .gap_1()
            .child(div().text_xs().text_color(muted).child(label))
            .child(field)
            .when_some(hint, |this, hint| {
                this.child(div().text_xs().text_color(muted.opacity(0.7)).child(hint))
            })
    }

    /// The "Dynamic workflows (ultracode)" checkbox — uniform with the other
    /// option rows (EXP-213 replaced the lone Switch).
    fn ultracode_row(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        Checkbox::new("sc-ultracode")
            .label("Dynamic workflows (ultracode)")
            .checked(self.ultracode)
            .on_click(cx.listener(|this, on: &bool, _, cx| {
                this.ultracode = *on;
                cx.notify();
            }))
    }

    /// The shared "Plan mode" checkbox (hint-free — EXP-206).
    fn plan_mode_row(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        Checkbox::new("sc-plan-mode")
            .label("Plan mode")
            .checked(self.plan_mode)
            .on_click(cx.listener(|this, on: &bool, _, cx| {
                this.plan_mode = *on;
                cx.notify();
            }))
    }

    /// EXP-202: the "Resume previous session" notice + checkbox — rendered
    /// only while a single checked issue's worktree already exists on disk
    /// ([`Self::resume_candidate`]).
    fn resume_row(&self, identifier: &str, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let settings = CodingHub::global(cx).read(cx).settings.clone();
        let branch = coding::branch_name(&settings.branch_prefix, identifier);
        let hint: SharedString = match self.agent {
            // codex has no cwd-scoped --continue; the launcher recovers the
            // worktree's exact recorded session id instead.
            CodingAgent::Codex => "Resumes the worktree's recorded Codex session \
                                   (codex resume <session-id>); if none is recorded, starts a \
                                   new session instructed to continue the work."
                .into(),
            agent => format!(
                "Continues the last {} conversation in this worktree (--continue).",
                agent.label()
            )
            .into(),
        };
        v_flex()
            .gap_0p5()
            .child(
                Checkbox::new("sc-resume")
                    .label("Resume previous session")
                    .checked(self.resume)
                    .on_click(cx.listener(|this, on: &bool, _, cx| {
                        this.resume = *on;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .pl_6()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(format!(
                        "A worktree for {identifier} already exists ({branch})."
                    ))),
            )
            .child(
                div()
                    .pl_6()
                    .text_xs()
                    .text_color(muted.opacity(0.7))
                    .child(hint),
            )
    }

    /// The "Skip permissions" checkbox (EXP-201) — full bypass instead of the
    /// agent's guarded auto mode. Hidden for pi (no permission system).
    fn skip_permissions_row(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        Checkbox::new("sc-skip-permissions")
            .label("Skip permissions")
            .checked(self.skip_permissions)
            .on_click(cx.listener(|this, on: &bool, _, cx| {
                this.skip_permissions = *on;
                cx.notify();
            }))
    }

    /// The agent tab strip (EXP-201) — compact, above the pickers. Offers
    /// only the installed agents (EXP-206; all of them while the doctor is
    /// still probing or found none).
    fn agent_tabs(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let report = CodingHub::global(cx).read(cx).doctor.report.clone();
        let pickable = Self::pickable_agents(report.as_ref());
        let active_ix = pickable
            .iter()
            .position(|agent| *agent == self.agent)
            .unwrap_or(0);
        let click_agents = pickable.clone();
        // Centered pill tabs with each agent's brand mark + name (EXP-206;
        // icon and text ride one custom child — `Tab::icon` drops the label).
        h_flex().w_full().justify_center().child(
            TabBar::new("sc-agent-tabs")
                .with_variant(TabVariant::Pill)
                .with_size(Size::Small)
                .selected_index(active_ix)
                .on_click(cx.listener(move |this, ix: &usize, window, cx| {
                    if let Some(agent) = click_agents.get(*ix).copied() {
                        this.set_agent(agent, window, cx);
                    }
                }))
                .children(pickable.iter().map(|agent| {
                    Tab::new().child(
                        h_flex()
                            .gap_1p5()
                            .items_center()
                            .child(Icon::from(agent_icon(*agent)).size_3p5())
                            .child(SharedString::from(agent.label())),
                    )
                })),
        )
    }

    /// The top-level Issues | Actions subject strip (EXP-257) — segmented,
    /// distinct from the Pill agent strip below.
    fn subject_tabs(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let active_ix = match self.subject_tab {
            SubjectTab::Issues => 0,
            SubjectTab::Actions => 1,
        };
        h_flex().w_full().justify_center().child(
            TabBar::new("sc-subject-tabs")
                .with_variant(TabVariant::Segmented)
                .with_size(Size::Small)
                .selected_index(active_ix)
                .on_click(cx.listener(|this, ix: &usize, _window, cx| {
                    let tab = if *ix == 0 {
                        SubjectTab::Issues
                    } else {
                        SubjectTab::Actions
                    };
                    if this.subject_tab != tab {
                        this.subject_tab = tab;
                        cx.notify();
                    }
                }))
                .child(Tab::new().child("Issues"))
                .child(Tab::new().child("Actions")),
        )
    }

    /// One Actions-tab list row: icon + name + repo badge + selection check.
    fn action_row(
        &self,
        action: &api::actions::Action,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let is_selected = self.selected_action_id.as_deref() == Some(action.id.as_str());
        let select_id = action.id.clone();
        h_flex()
            .id(SharedString::from(format!("sc-action-{}", action.id)))
            .w_full()
            .items_center()
            .gap_2()
            .px_1p5()
            .py_1()
            .rounded(theme.radius)
            .when(is_selected, |this| this.bg(theme.accent.opacity(0.4)))
            .hover(|this| this.bg(theme.accent.opacity(0.3)))
            .on_click(cx.listener(move |this, _: &gpui::ClickEvent, window, cx| {
                this.select_action(select_id.clone(), window, cx);
            }))
            .child(if action.builtin {
                // The builtin creator's distinct mark.
                Icon::new(gpui_component::IconName::Plus).xsmall().text_color(muted)
            } else {
                Icon::from(ExpIcon::Zap).xsmall().text_color(muted)
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .truncate()
                    .text_color(theme.foreground)
                    .child(SharedString::from(action.name.clone())),
            )
            .when(action.repository_id.is_some(), |this| {
                this.child(Icon::from(ExpIcon::GitMerge).xsmall().text_color(muted))
            })
            .when(is_selected, |this| {
                this.child(
                    Icon::new(gpui_component::IconName::Check)
                        .xsmall()
                        .text_color(theme.primary),
                )
            })
            .into_any_element()
    }

    /// One typed input field for the selected action (EXP-257): text →
    /// [`Input`] (state pre-built in [`Self::select_action`]), repo/board →
    /// dropdown-menu buttons over the team's repos / synced boards.
    fn action_input_field(
        &self,
        ix: usize,
        input: &api::actions::ActionInput,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let label: SharedString = if input.required {
            input.label.clone().into()
        } else {
            format!("{} (optional)", input.label).into()
        };
        let field: gpui::AnyElement = match input.input_type.as_str() {
            "text" => match self.action_text_inputs.get(&input.key) {
                Some(state) => Input::new(state).small().into_any_element(),
                None => div().into_any_element(), // transient re-selection frame
            },
            "repo" => {
                let pick_label: SharedString = match self.action_repo_picks.get(&input.key) {
                    Some(repo) => repo.full_name.clone().into(),
                    None => "Select repository…".into(),
                };
                let repos = self.team_repos.clone();
                let key = input.key.clone();
                let optional = !input.required;
                let view = cx.entity().downgrade();
                Button::new(("sc-input-repo", ix))
                    .outline()
                    .small()
                    .label(pick_label)
                    .dropdown_menu(move |mut menu, _window, _cx| {
                        if optional {
                            let view = view.clone();
                            let key = key.clone();
                            menu = menu.item(PopupMenuItem::new("None").on_click(
                                move |_, _, cx| {
                                    if let Some(view) = view.upgrade() {
                                        view.update(cx, |view, cx| {
                                            view.action_repo_picks.remove(&key);
                                            cx.notify();
                                        });
                                    }
                                },
                            ));
                        }
                        for repo in &repos {
                            let view = view.clone();
                            let key = key.clone();
                            let repo = repo.clone();
                            menu = menu.item(
                                PopupMenuItem::new(SharedString::from(repo.full_name.clone()))
                                    .on_click(move |_, _, cx| {
                                        if let Some(view) = view.upgrade() {
                                            view.update(cx, |view, cx| {
                                                view.action_repo_picks
                                                    .insert(key.clone(), repo.clone());
                                                cx.notify();
                                            });
                                        }
                                    }),
                            );
                        }
                        menu
                    })
                    .into_any_element()
            }
            "board" => {
                let pick_label: SharedString = match self.action_board_picks.get(&input.key) {
                    Some((_, name)) => name.clone().into(),
                    None => "Select board…".into(),
                };
                let boards: Vec<(String, String)> = Store::global(cx)
                    .collections()
                    .boards_in_team(&self.team_id, cx)
                    .into_iter()
                    .map(|board| (board.id, board.name))
                    .collect();
                let key = input.key.clone();
                let optional = !input.required;
                let view = cx.entity().downgrade();
                Button::new(("sc-input-board", ix))
                    .outline()
                    .small()
                    .label(pick_label)
                    .dropdown_menu(move |mut menu, _window, _cx| {
                        if optional {
                            let view = view.clone();
                            let key = key.clone();
                            menu = menu.item(PopupMenuItem::new("None").on_click(
                                move |_, _, cx| {
                                    if let Some(view) = view.upgrade() {
                                        view.update(cx, |view, cx| {
                                            view.action_board_picks.remove(&key);
                                            cx.notify();
                                        });
                                    }
                                },
                            ));
                        }
                        for (board_id, name) in &boards {
                            let view = view.clone();
                            let key = key.clone();
                            let board_id = board_id.clone();
                            let name = name.clone();
                            menu = menu.item(
                                PopupMenuItem::new(SharedString::from(name.clone())).on_click(
                                    move |_, _, cx| {
                                        if let Some(view) = view.upgrade() {
                                            view.update(cx, |view, cx| {
                                                view.action_board_picks.insert(
                                                    key.clone(),
                                                    (board_id.clone(), name.clone()),
                                                );
                                                cx.notify();
                                            });
                                        }
                                    },
                                ),
                            );
                        }
                        menu
                    })
                    .into_any_element()
            }
            "pr" => {
                // EXP-259: the team's OPEN issue-linked pull requests,
                // deduped by prUrl (a batch PR shows once; its value is the
                // representative issue's id).
                let pick_label: SharedString = match self.action_pr_picks.get(&input.key) {
                    Some((_, label)) => label.clone().into(),
                    None => "Select pull request…".into(),
                };
                let pulls: Vec<(String, String)> = crate::queries::review_groups(cx, &self.team_id)
                    .iter()
                    .flat_map(|group| group.entries.iter())
                    .map(|entry| {
                        let issue = entry.representative();
                        let idents = entry
                            .issues
                            .iter()
                            .map(|issue| issue.identifier.clone())
                            .collect::<Vec<_>>()
                            .join(", ");
                        let label = match (issue.pr_number, entry.is_batch()) {
                            (Some(number), true) => format!("#{number} · {idents}"),
                            (Some(number), false) => {
                                format!("#{number} · {idents} {}", issue.title)
                            }
                            (None, _) => idents,
                        };
                        (issue.id.clone(), label)
                    })
                    .collect();
                let key = input.key.clone();
                let optional = !input.required;
                let view = cx.entity().downgrade();
                Button::new(("sc-input-pr", ix))
                    .outline()
                    .small()
                    .label(pick_label)
                    .dropdown_menu(move |mut menu, _window, _cx| {
                        if optional {
                            let view = view.clone();
                            let key = key.clone();
                            menu = menu.item(PopupMenuItem::new("None").on_click(
                                move |_, _, cx| {
                                    if let Some(view) = view.upgrade() {
                                        view.update(cx, |view, cx| {
                                            view.action_pr_picks.remove(&key);
                                            cx.notify();
                                        });
                                    }
                                },
                            ));
                        }
                        if pulls.is_empty() {
                            menu = menu
                                .item(PopupMenuItem::new("No open pull requests").disabled(true));
                        }
                        for (issue_id, label) in &pulls {
                            let view = view.clone();
                            let key = key.clone();
                            let issue_id = issue_id.clone();
                            let label = label.clone();
                            menu = menu.item(
                                PopupMenuItem::new(SharedString::from(label.clone())).on_click(
                                    move |_, _, cx| {
                                        if let Some(view) = view.upgrade() {
                                            view.update(cx, |view, cx| {
                                                view.action_pr_picks.insert(
                                                    key.clone(),
                                                    (issue_id.clone(), label.clone()),
                                                );
                                                cx.notify();
                                            });
                                        }
                                    },
                                ),
                            );
                        }
                        menu
                    })
                    .into_any_element()
            }
            // Unknown type (newer server): named, never a fake text field —
            // the launch blocker holds the run.
            _ => div()
                .text_xs()
                .text_color(muted)
                .child("Unsupported input type — update the app.")
                .into_any_element(),
        };
        v_flex()
            .gap_1()
            .child(div().text_xs().text_color(muted).child(label))
            .child(field)
            .into_any_element()
    }

    /// A bounded scroll pane (the EXP-119/EXP-67 idiom this dialog already
    /// uses for the issue checklist): `max_h`-capped, boxed, overlay
    /// scrollbar. `overflow_y_scrollbar` would drop the `max_h`.
    fn bounded_pane(
        &self,
        id: &'static str,
        handle: &ScrollHandle,
        max_h: f32,
        content: gpui::AnyElement,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        div()
            .relative()
            .max_h(px(max_h))
            .border_1()
            .border_color(cx.theme().border)
            .rounded(cx.theme().radius)
            .overflow_hidden()
            .child(
                div()
                    .id(id)
                    .max_h(px(max_h))
                    .overflow_y_scroll()
                    .track_scroll(handle)
                    .child(content),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .child(Scrollbar::new(handle).axis(ScrollbarAxis::Vertical)),
            )
    }

    /// Footer: blocker copy + Cancel + Start.
    fn footer(
        &self,
        blocker: Option<SharedString>,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let mut footer = h_flex().items_center().gap_2().pt_1();
        if let Some(reason) = &blocker {
            if !self.launching {
                footer = footer.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .truncate()
                        .text_color(cx.theme().muted_foreground)
                        .child(reason.clone()),
                );
            }
        }
        footer
            .child(div().flex_1())
            .child(
                Button::new("sc-cancel")
                    .outline()
                    .small()
                    .label("Cancel")
                    .disabled(self.launching)
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.launching {
                            return;
                        }
                        native_dialog::close_dialog_window(window, cx);
                    })),
            )
            .child(
                Button::new("sc-start")
                    .primary()
                    .small()
                    .label(if self.launching {
                        "Starting…"
                    } else if self.subject_tab == SubjectTab::Actions {
                        // EXP-257: the builtin's run IS creation.
                        if self.selected_action().is_some_and(|action| action.builtin) {
                            "Create action"
                        } else {
                            "Run action"
                        }
                    } else if self.resume_active() {
                        "Resume coding"
                    } else {
                        "Start coding"
                    })
                    .loading(self.launching)
                    .disabled(blocker.is_some())
                    .on_click(cx.listener(|this, _, window, cx| this.launch(window, cx))),
            )
    }
}

impl Render for StartCodingDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme_muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let warning = cx.theme().warning;
        let checked_count = self.checked.len();
        let ultracode = self.ultracode;

        // ---- searchable checklist: checked rows pinned first, then the
        //      unchecked search matches (capped — see MAX_UNCHECKED_ROWS) ----
        let query = self.search.read(cx).value().trim().to_lowercase();
        let mut checked_ixs: Vec<usize> = Vec::new();
        let mut match_ixs: Vec<usize> = Vec::new();
        for (ix, row) in self.rows.iter().enumerate() {
            if self.checked.contains(&row.issue_id) {
                checked_ixs.push(ix);
            } else if query.is_empty()
                || row.identifier.to_lowercase().contains(&query)
                || row.title.to_lowercase().contains(&query)
            {
                match_ixs.push(ix);
            }
        }
        let hidden = match_ixs.len().saturating_sub(MAX_UNCHECKED_ROWS);
        let no_matches = !query.is_empty() && match_ixs.is_empty() && !self.rows.is_empty();
        match_ixs.truncate(MAX_UNCHECKED_ROWS);

        let mut checklist = v_flex().gap_1().p_2();
        if self.rows.is_empty() {
            checklist = checklist.child(
                div()
                    .text_sm()
                    .text_color(theme_muted)
                    .child("No open issues in this board."),
            );
        }
        for ix in checked_ixs {
            checklist = checklist.child(self.issue_row(ix, cx));
        }
        for ix in match_ixs {
            checklist = checklist.child(self.issue_row(ix, cx));
        }
        if no_matches {
            // Without this the scoped pool renders a silently blank list —
            // the filter (open issues, this board only) is invisible.
            checklist = checklist.child(
                div()
                    .text_xs()
                    .text_color(theme_muted)
                    .child("No matches — only open issues from this board are shown."),
            );
        }
        if hidden > 0 {
            checklist = checklist.child(
                div()
                    .text_xs()
                    .text_color(theme_muted)
                    .child(SharedString::from(format!(
                        "+{hidden} more — refine your search."
                    ))),
            );
        }

        // ---- model/effort selects (per-agent lists — EXP-201) ----
        let agent = self.agent;
        let effort_hint = (ultracode && agent.supports_ultracode())
            .then_some("ultracode sets effort");
        let main_row = h_flex()
            .gap_3()
            .w_full()
            // Top-align (h_flex centers): the Effort column grows an
            // "ultracode sets effort" hint line, which would otherwise sink
            // the Model label below the shared baseline.
            .items_start()
            .child(Self::labeled_field(
                "Model",
                Select::new(&self.model).small().into_any_element(),
                None,
                cx,
            ))
            .child(Self::labeled_field(
                agent.effort_label(),
                Select::new(&self.effort)
                    .small()
                    .disabled(ultracode && agent.supports_ultracode())
                    .into_any_element(),
                effort_hint,
                cx,
            ));

        // ---- resume (EXP-202): single checked issue with an existing
        //      worktree offers "Resume previous session" ----
        let resume_active = self.resume_active();
        let resume_row = self
            .resume_candidate()
            .map(|row| row.identifier.clone())
            .map(|identifier| self.resume_row(&identifier, cx).into_any_element());

        // ---- toggles (capability-gated per agent — EXP-201; hint-free,
        //      EXP-206) ----
        let has_toggles = agent.supports_ultracode()
            || (agent.supports_plan_mode() && !resume_active)
            || agent.supports_skip_permissions();
        let mut toggles = v_flex().gap_2();
        if agent.supports_ultracode() {
            toggles = toggles.child(self.ultracode_row(cx).into_any_element());
        }
        // A resume never re-enters plan mode — hide the row while the resume
        // checkbox is on (options() clamps it off regardless).
        if agent.supports_plan_mode() && !resume_active {
            toggles = toggles.child(self.plan_mode_row(cx).into_any_element());
        }
        if agent.supports_skip_permissions() {
            toggles = toggles.child(self.skip_permissions_row(cx).into_any_element());
        }

        let agent_label = agent.label();
        let intro: SharedString = if checked_count >= 2 {
            format!(
                "One {agent_label} session implements the {checked_count} checked issues on \
                 one branch and opens one combined PR."
            )
            .into()
        } else {
            format!(
                "{agent_label} works on the checked issue in its own worktree and opens the \
                 pull request when done. Check more issues for a batch run."
            )
            .into()
        };

        let blocker = self.launch_blocker(cx);
        // EXP-268: two-column widescreen layout (web `launch-dialog.tsx`
        // parity) — subject tabs full-width on top, then picker LEFT /
        // options RIGHT, error + footer full-width below.
        let mut left = v_flex().flex_1().min_w_0().gap_3();
        match self.subject_tab {
            SubjectTab::Issues => {
                left = left
                    .child(div().text_xs().text_color(theme_muted).child(intro))
                    .child(Input::new(&self.search).small())
                    // Bounded, actually-scrollable checklist (EXP-119):
                    // compose the EXP-67 scroll-pane primitives directly —
                    // gpui-component's `overflow_y_scrollbar` wrapper drops
                    // the wrapped element's `max_h`, so the bound never
                    // constrained the list and it pushed the dialog body
                    // instead of scrolling. (EXP-213: boxed like the web
                    // picker.)
                    .child(self.bounded_pane(
                        "sc-issues-scroll",
                        &self.list_scroll.clone(),
                        360.,
                        checklist.into_any_element(),
                        cx,
                    ));
            }
            SubjectTab::Actions => {
                // The single-select action list (builtin pinned first) + the
                // selected action's typed input fields (EXP-257).
                let query = self.action_search.read(cx).value().trim().to_lowercase();
                let visible: Vec<usize> = self
                    .actions
                    .iter()
                    .enumerate()
                    .filter(|(_, action)| {
                        query.is_empty()
                            || action.name.to_lowercase().contains(&query)
                            || action
                                .description
                                .as_deref()
                                .is_some_and(|text| text.to_lowercase().contains(&query))
                    })
                    .map(|(ix, _)| ix)
                    .collect();
                let mut list = v_flex().gap_0p5().p_1();
                match &self.actions_load {
                    ActionsLoad::Loading => {
                        list = list.child(
                            div()
                                .p_2()
                                .text_xs()
                                .text_color(theme_muted)
                                .child("Loading actions…"),
                        );
                    }
                    ActionsLoad::Ready => {
                        if visible.is_empty() {
                            list = list.child(
                                div()
                                    .p_2()
                                    .text_xs()
                                    .text_color(theme_muted)
                                    .child("No matching actions."),
                            );
                        }
                        for ix in visible {
                            let action = self.actions[ix].clone();
                            list = list.child(self.action_row(&action, cx));
                        }
                    }
                }
                left = left
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme_muted)
                            .child("Run a reusable team action on this device — pick one and \
fill in its inputs. \"Create action\" authors a new one from your description."),
                    )
                    .child(Input::new(&self.action_search).small())
                    .child(self.bounded_pane(
                        "sc-actions-scroll",
                        &self.action_list_scroll.clone(),
                        200.,
                        list.into_any_element(),
                        cx,
                    ));
                if let Some(action) = self.selected_action().cloned() {
                    if !action.inputs.is_empty() {
                        let mut fields = v_flex().gap_2().p_2();
                        for (ix, input) in action.inputs.iter().enumerate() {
                            fields = fields.child(self.action_input_field(ix, input, cx));
                        }
                        left = left.child(self.bounded_pane(
                            "sc-action-inputs-scroll",
                            &self.action_inputs_scroll.clone(),
                            220.,
                            fields.into_any_element(),
                            cx,
                        ));
                    }
                }
            }
        }
        // Web parity: the selection-size notes ride the picker column.
        if self.subject_tab == SubjectTab::Issues {
            if checked_count > MAX_ISSUES_PER_RUN {
                left = left.child(div().text_xs().text_color(warning).child(
                    SharedString::from(format!(
                        "At most {MAX_ISSUES_PER_RUN} issues per run — split the batch."
                    )),
                ));
            } else if checked_count > COST_NOTE_THRESHOLD {
                left = left.child(
                    div()
                        .text_xs()
                        .text_color(warning)
                        .child("Large batches can be token-expensive."),
                );
            }
        }

        // Right column: the shared options cluster (agent tabs, model/effort,
        // resume, toggles) — the web `LaunchOptionsPane` twin.
        let mut right = v_flex()
            .flex_1()
            .min_w_0()
            .gap_3()
            .child(self.agent_tabs(cx))
            .child(main_row);
        if let Some(resume_row) = resume_row {
            right = right.child(resume_row);
        }
        // pi has no option rows — an empty toggles child would still eat a
        // gap_3 slot.
        if has_toggles {
            right = right.child(toggles);
        }

        let mut body = v_flex().gap_3().child(self.subject_tabs(cx)).child(
            h_flex()
                .w_full()
                .gap_5()
                .items_start()
                .child(left)
                .child(right),
        );
        if let Some(error) = &self.error {
            body = body.child(div().text_sm().text_color(danger).child(error.clone()));
        }

        body.child(self.footer(blocker, cx)).into_any_element()
    }
}
