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
//! **Issues | Actions | Chat** subject strip sits at the top. The Actions tab
//! replaces the issue checklist with an action search + single-select list
//! (the fix-conflicts builtin pinned first; "Create action" is not offered —
//! authoring lives in [`crate::create_action_dialog`]) and the selected
//! action's typed input fields (text / repo / board). EXP-615 adds the
//! **Chat** tab: a free prompt on a picked repository's trunk clone, running
//! as the hidden `builtin:chat` action over the same action rails — no issue,
//! no branch, no PR. The SHARED right half is the ONE
//! [`crate::launch_options::LaunchOptionsSection`] (agent pills, model/effort,
//! toggles) every launch surface renders. An action launch goes through
//! [`crate::action_run::start_action_run`] instead of `coding::prepare`
//! directly.
//!
//! EXP-696: WHERE the run happens is its own **Device** group ABOVE that
//! cluster (web/mobile parity), rendered only when there is more than one
//! candidate — this machine plus every ONLINE synced device advertising a
//! runnable agent, and only while `steer.config` says the instance runs a
//! relay. Picking another machine re-seeds the whole options cluster off
//! ITS advertisement and swaps the launch for one `steer.startSession` with
//! the same subject; the local doctor gate does not apply to it (that
//! machine's own advertisement does, and the server re-checks it). An
//! EXPLICIT pick — the row's, or a caller's ▶ preselect — is STICKY: when its
//! machine falls out of the candidates the dialog keeps pointing at it, keeps
//! the Device row on screen and BLOCKS the launch, instead of silently
//! retargeting the run at this machine with this machine's options.
//!
//! EXP-291: the dialog renders as a full-height column — only the BODY
//! (subject strip + the two columns) scrolls; the Cancel / Start action bar
//! is pinned to the window's bottom edge and is always reachable, however
//! small the window is resized. That needs [`DialogContent::self_scrolling`]:
//! the shell's default wrapper scrolls the whole content view, footer and all.
//!
//! Launch = snapshot → [`coding::prepare`] on the background executor → the
//! shared `coding_flow::spawn_into_window` foreground spawn. A
//! `Prepared::Disabled` reason renders inline and keeps the dialog open.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui::{
    div, prelude::FluentBuilder as _, px, size, AnyWindowHandle, App, AppContext as _, ClickEvent,
    Entity, InteractiveElement as _, IntoElement, ParentElement, Render, ScrollHandle,
    SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
};

use crate::controls::WebControl as _;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    scroll::{Scrollbar, ScrollbarAxis},
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
};
use sync::Store;

use api::repositories::IssueRepository;
use coding::{
    run_registry::RunRecord, ActionInputValue, BatchIssueSpec, BatchLaunchRequest, LaunchOptions,
    LaunchOrigin, Prepared, PrepareRequest, RepoGroup, ResumeRunRequest,
};
use domain::IssueStatus;

use crate::action_run::{self, ActionRepo, ActionRepoRow, StartActionArgs};
use crate::coding_flow::{self, CodingHub, SessionSubject};
use crate::launch_options::{self, LaunchOptionsSection};
use crate::icons::registry;
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

/// The `pr` input's pick list (EXP-259): the team's OPEN issue-linked pull
/// requests, deduped by prUrl (a batch PR shows once; its value is the
/// representative issue's id). Shared by the field's dropdown and the
/// `open_for_fix_conflicts` preselect (EXP-313).
fn pr_pick_options(cx: &App, team_id: &str) -> Vec<(String, String)> {
    crate::queries::review_groups(cx, team_id)
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
        .collect()
}

/// Fired once after a successful ISSUE-subject launch (EXP-439) — the bulk
/// bar passes a callback that clears its multiselect, so starting a batch
/// session leaves batch selection behind.
pub type OnLaunched = Rc<dyn Fn(&mut App)>;

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
    open(
        window,
        cx,
        team_id,
        vec![issue.id],
        None,
        None,
        None,
        SubjectTab::Issues,
        None,
    );
}

/// Open the dialog from the bulk bar with the selection pre-checked.
/// `on_launched` fires once on a successful launch (EXP-439 — the bulk bar
/// clears its selection there). EXP-696: `preselected_device_id` preselects
/// the machine the run starts on (the machines list's ▶ passes its own row);
/// `None` = this machine, as every other entry point.
pub fn open_for_selection(
    window: &mut Window,
    cx: &mut App,
    team_id: String,
    issue_ids: Vec<String>,
    on_launched: Option<OnLaunched>,
    preselected_device_id: Option<String>,
) {
    open(
        window,
        cx,
        team_id,
        issue_ids,
        None,
        None,
        on_launched,
        SubjectTab::Issues,
        preselected_device_id,
    );
}

/// Open the dialog on the ACTIONS tab with `action_id` preselected (EXP-257
/// — the actions panel rows land here).
pub fn open_for_action(window: &mut Window, cx: &mut App, team_id: String, action_id: String) {
    open(
        window,
        cx,
        team_id,
        Vec::new(),
        Some(action_id),
        None,
        None,
        SubjectTab::Actions,
        None,
    );
}

/// Open the launcher straight on the Chat tab (EXP-615): a free prompt on a
/// repository's default branch, no issue and no saved action attached.
pub fn open_for_chat(window: &mut Window, cx: &mut App, team_id: String) {
    open(
        window,
        cx,
        team_id,
        Vec::new(),
        None,
        None,
        None,
        SubjectTab::Chat,
        None,
    );
}

/// Open the dialog on the ACTIONS tab with the builtin "Fix merge conflicts"
/// action AND its `pr` input preselected (EXP-313 — the Reviews-list /
/// issue-detail "Fix conflicts" buttons land here so agent/model/effort stay
/// choosable instead of firing the run with settings defaults).
pub fn open_for_fix_conflicts(
    window: &mut Window,
    cx: &mut App,
    team_id: String,
    issue_id: String,
) {
    open(
        window,
        cx,
        team_id,
        Vec::new(),
        Some(api::actions::BUILTIN_FIX_CONFLICTS_ID.to_string()),
        Some(issue_id),
        None,
        SubjectTab::Actions,
        None,
    );
}

#[allow(clippy::too_many_arguments)] // the one shared entry every open_* rides
fn open(
    window: &mut Window,
    cx: &mut App,
    team_id: String,
    preselected: Vec<String>,
    preselect_action: Option<String>,
    preselect_pr: Option<String>,
    on_launched: Option<OnLaunched>,
    // The subject the caller seeded — each `open_*` entry names its own tab
    // (an action preselect only ever makes sense on Actions).
    tab: SubjectTab,
    // EXP-696: the machine to preselect in the Device row (`None` = this one).
    preselect_device: Option<String>,
) {
    // EXP-268: widescreen two-column layout (web `sm:max-w-3xl` parity —
    // picker left, options right); the launched terminal tab lands back in
    // the OPENER window (EXP-284: the dialog is its own native window).
    let opener = window.window_handle();
    // EXP-285: trimmed 640 → 560 and user-resizable — the two-column layout
    // tolerates it (both lists are max_h-capped). EXP-635: 520 → 540 — with
    // the picker column at its tallest (a capped issue list plus the batch
    // cost note) the body overflowed by ~20px and grew a sliver of a
    // scrollbar; the two columns fit outright now.
    let height = (window.viewport_size().height * 0.85).min(px(540.));
    let spec =
        DialogSpec::new("Start coding", size(px(760.), height)).resizable(size(px(640.), px(480.)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| {
            StartCodingDialogView::new(
                team_id,
                preselected,
                preselect_action,
                preselect_pr,
                on_launched,
                tab,
                preselect_device.clone(),
                opener,
                window,
                cx,
            )
        });
        let busy = view.clone();
        DialogContent::new(view)
            // EXP-291: the view pins its own action bar and scrolls only the
            // body — the shell's wrapper would scroll the buttons away.
            .self_scrolling()
            .can_close(move |cx| !busy.read(cx).launching)
    });
}

/// The unified dialog's top-level subject (EXP-257).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SubjectTab {
    Issues,
    Actions,
    /// EXP-615: a free prompt on a repository's trunk clone, run as the
    /// hidden `builtin:chat` action.
    Chat,
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
    /// The issue's board — EXP-712: its `default_branch` is the base every
    /// launch for this issue cuts from, and a batch may only mix boards that
    /// agree on it.
    board_id: String,
    identifier: String,
    title: String,
    description: Option<String>,
    /// Status snapshot at open — the launcher's step 6.5 flips backlog
    /// to `in_progress` at launch (EXP-194).
    status: IssueStatus,
    /// The closed-state note (`done`/`cancelled`/`duplicate`/PR-merged),
    /// shown muted next to the title. Only pre-seeded rows can carry one
    /// (EXP-119 filters closed rows out of the pool) — it flags a re-run.
    /// `None` = plain row.
    state_hint: Option<&'static str>,
}

/// EXP-696: what the REMOTE resume row shows — the target machine's synced
/// worktree for the checked issue.
struct RemoteResume {
    /// The worktree's branch (`exp/<IDENTIFIER>`), empty on an old report.
    branch: String,
    /// When the machine last reported the worktree (ISO), for the "…ended"
    /// line's relative time.
    reported_at: Option<String>,
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
    /// `open_for_fix_conflicts`' PR preselect (a representative issue id,
    /// EXP-313) — applied right after the action preselect, which clears the
    /// pick maps.
    pending_pr_preselect: Option<String>,
    /// EXP-615 — the Chat tab: the free prompt the agent receives verbatim…
    chat_prompt: Entity<TextareaState>,
    /// …and the repository whose trunk clone it runs in (auto-picked when the
    /// team has exactly one; the run is refused without it).
    chat_repo: Option<ActionRepoRow>,
    action_search: Entity<InputState>,
    action_list_scroll: ScrollHandle,
    action_inputs_scroll: ScrollHandle,
    /// Per-TEXT-input editor states for the selected action, keyed by input
    /// key — built in [`Self::select_action`] (never in render).
    action_text_inputs: HashMap<String, Entity<InputState>>,
    /// EXP-530: the same, for `textarea` inputs — a separate map because the
    /// two editors are different entity types (multi-line vs one-line).
    action_textarea_inputs: HashMap<String, Entity<TextareaState>>,
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
    /// Picked curated icon NAME per `icon` input key (EXP-273). The only
    /// pick that is not an id — the value goes to the server verbatim.
    action_icon_picks: HashMap<String, String>,
    /// Last action id whose `repo` inputs were seeded from the action's own
    /// bound repository (EXP-349) — the latch keeps a manual re-pick
    /// (including clearing to "None") from being re-seeded.
    seeded_repo_action_id: Option<String>,
    /// `repositories.list` rows for the repo input pickers.
    team_repos: Vec<ActionRepoRow>,
    /// Every team issue, board→number ordered.
    rows: Vec<IssueRow>,
    /// issue id → probe state (LAZY: only checked issues probe).
    repos: HashMap<String, RepoState>,
    /// issue id → the newest resumable run record for it (EXP-662 — probed
    /// alongside the repo, `None` = nothing to resume). Drives the Resume
    /// affordance for a single-checked issue; the record names the agent,
    /// branch and transcript the resume relaunches.
    resumables: HashMap<String, Option<RunRecord>>,
    /// EXP-202: "Resume previous session" checkbox state. Only ACTIVE
    /// ([`Self::resume_candidate`]) when exactly one issue is checked and
    /// a record exists for it; default-on so a re-launch resumes by default.
    resume: bool,
    /// Stale-probe guard (old results must not land after a re-open).
    probe_generation: u64,
    checked: HashSet<String>,
    search: Entity<InputState>,
    /// Scroll position of the checklist (view state so it survives
    /// re-renders — the EXP-67 scroll-pane idiom, bounded by `max_h`).
    list_scroll: ScrollHandle,
    /// EXP-291: scroll position of the dialog BODY (tabs + the two columns).
    /// The body is the only scrolling region — the action bar below it is
    /// pinned to the dialog's bottom edge and never scrolls away.
    body_scroll: ScrollHandle,
    /// EXP-615: the ONE shared options cluster (agent pills, model/effort,
    /// toggles) — the same component the create-action dialog renders.
    launch: LaunchOptionsSection,
    /// EXP-696: the machine the run starts on (`None` before the first
    /// settle). The routing switch is its candidate's `is_own` flag: this
    /// machine takes the LOCAL launch paths, anything else goes out as one
    /// `steer.startSession`.
    device_id: Option<String>,
    /// Whether [`Self::device_id`] is a pick the USER made (the Device row, or
    /// a caller's ▶ preselect) rather than a settle's fallback. An explicit
    /// pick is STICKY — see [`queries::settled_device`].
    device_explicit: bool,
    /// Whether the pick currently resolves to a candidate. `false` = its
    /// machine has dropped out (offline / no runnable agent): the launch is
    /// blocked instead of quietly re-pointing at this one.
    device_resolved: bool,
    /// The pick's last known label — the candidate row is gone while its
    /// machine is offline, and the blocker still has to name it.
    device_label: Option<String>,
    /// The caller's preselect (a machines-row ▶), adopted by the first
    /// [`Self::settle_device`].
    pending_device_preselect: Option<String>,
    /// The candidate list the picker last settled against — recomputed on
    /// every `devices` delta so a machine coming online mid-dialog appears.
    devices: Vec<queries::LaunchDevice>,
    launching: bool,
    error: Option<SharedString>,
    /// EXP-439: fired ONCE after a successful issue-subject launch — the bulk
    /// bar's opener passes a selection-clearing callback (`take()`n on fire).
    on_launched: Option<OnLaunched>,
    _subscriptions: Vec<Subscription>,
}

impl StartCodingDialogView {
    #[allow(clippy::too_many_arguments)] // mirrors `open`'s parameter list
    fn new(
        team_id: String,
        preselected: Vec<String>,
        preselect_action: Option<String>,
        preselect_pr: Option<String>,
        on_launched: Option<OnLaunched>,
        tab: SubjectTab,
        preselect_device: Option<String>,
        opener: AnyWindowHandle,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let hub = CodingHub::global(cx);

        // Snapshot the picker's candidate pool (EXP-119): the pre-seeded
        // issues' board(s) only, OPEN issues only — unrelated boards and
        // closed rows just buried the launchable ones. Pre-seeded ids are
        // exempt from both filters: the explicit pick wins (the Play-button
        // re-run of a done issue), and a checked id MUST keep its row —
        // `batch_request`/`launch_blocker` iterate `rows` and would silently
        // drop it from the run otherwise.
        let preselected: HashSet<String> = preselected.into_iter().collect();
        let mut issues = queries::team_issues(cx, &team_id);
        // `team_issues` joins through the boards collection, but the Play
        // button resolves its seed from the raw issues collection — re-read
        // any seed the join dropped (a board row that hasn't synced yet) so
        // the pick shows up force-checked instead of silently vanishing from
        // the run (`batch_request` iterates `rows`).
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
            // EXP-314: ANCHOR-based on purpose — every closed-ish CATEGORY
            // writes one of these three anchors, so a custom completed /
            // cancelled status is classified correctly with no status-row
            // join (this picker spans boards).
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
                    board_id: issue.board_id,
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
        let synced_devices = Store::global(cx).collections().devices.clone();
        let synced_worktrees = Store::global(cx).collections().device_worktrees.clone();
        // EXP-696: remote start exists only behind a relay — until
        // `steer.config` answers, this machine is the only candidate.
        let steer_config = queries::steer_config(cx);
        let subscriptions = vec![
            cx.observe_in(&steer_config, window, |this: &mut Self, _, window, cx| {
                this.settle_device(window, cx);
                cx.notify();
            }),
            // EXP-696: the Device row is a live read of the `devices` shape —
            // a machine going offline (or coming back) re-settles the pick.
            cx.observe_in(&synced_devices, window, |this: &mut Self, _, window, cx| {
                this.settle_device(window, cx);
                cx.notify();
            }),
            // EXP-696: the REMOTE resume row reads `device_worktrees`.
            cx.observe(&synced_worktrees, |_: &mut Self, _, cx| cx.notify()),
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
                this.launch.reconcile_agent(window, cx);
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

        // EXP-615: the Chat tab's prompt editor (its placeholder comes from
        // the hidden builtin's own input definition, so it can never drift
        // from the other three clients).
        let chat_placeholder = api::actions::builtin_chat_action(&team_id)
            .inputs
            .iter()
            .find(|input| input.key == "prompt")
            .and_then(|input| input.placeholder.clone())
            .unwrap_or_default();
        let chat_prompt = cx.new(|cx| {
            TextareaState::new(window, cx).placeholder(SharedString::from(chat_placeholder))
        });

        let mut this = Self {
            team_id,
            opener,
            subject_tab: tab,
            actions: Vec::new(),
            actions_load: ActionsLoad::Loading,
            selected_action_id: None,
            pending_action_preselect: preselect_action,
            pending_pr_preselect: preselect_pr,
            chat_prompt,
            chat_repo: None,
            action_search,
            action_list_scroll: ScrollHandle::new(),
            action_inputs_scroll: ScrollHandle::new(),
            action_text_inputs: HashMap::new(),
            action_textarea_inputs: HashMap::new(),
            action_input_subscriptions: Vec::new(),
            action_repo_picks: HashMap::new(),
            action_board_picks: HashMap::new(),
            action_pr_picks: HashMap::new(),
            action_icon_picks: HashMap::new(),
            seeded_repo_action_id: None,
            team_repos: Vec::new(),
            rows,
            repos: HashMap::new(),
            resumables: HashMap::new(),
            resume: true,
            probe_generation: 0,
            checked,
            search,
            list_scroll: ScrollHandle::new(),
            body_scroll: ScrollHandle::new(),
            launch: LaunchOptionsSection::new(window, cx),
            device_id: None,
            device_explicit: false,
            device_resolved: false,
            device_label: None,
            pending_device_preselect: preselect_device,
            devices: Vec::new(),
            launching: false,
            error: None,
            on_launched,
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
        // EXP-696: settle the machine BEFORE the agent reconcile — a remote
        // preselect re-seeds the whole options cluster off that machine's
        // advertisement, and the local reconcile below must not undo it.
        this.settle_device(window, cx);
        // The doctor usually ran long before the dialog opens — if the
        // settings default agent isn't installed, preselect one that is
        // (EXP-206: the tab strip only shows installed agents).
        this.launch.reconcile_agent(window, cx);
        this
    }

    // -- device picker (EXP-696) ----------------------------------------------

    /// Recompute the candidate machines and settle the pick
    /// ([`queries::settled_device`] holds the chain, web
    /// `use-launch-options.ts`' settle effect). A pick that newly RESOLVES to
    /// a candidate re-points the options cluster at that machine; one whose
    /// machine dropped out keeps everything exactly as it was — reseeding
    /// there is the silent retarget this guards against.
    fn settle_device(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.devices = queries::launch_devices(cx);
        let preselect = self.pending_device_preselect.take();
        if preselect.is_some() {
            self.device_explicit = true;
        }
        let next = queries::settled_device(
            &self.devices,
            self.device_id.as_deref(),
            self.device_explicit,
            preselect.as_deref(),
        );
        let label = next.as_deref().and_then(|id| {
            self.devices
                .iter()
                .find(|device| device.device_id == id)
                .map(|device| device.label.clone())
        });
        let resolved = label.is_some();
        if let Some(label) = label {
            self.device_label = Some(label);
        }
        let changed = next != self.device_id;
        self.device_id = next;
        let reseed = resolved && (changed || !self.device_resolved);
        self.device_resolved = resolved;
        if reseed {
            self.apply_device_defaults(window, cx);
        }
    }

    /// Explicit pick from the Device row.
    fn set_device(&mut self, device_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.device_explicit = true;
        self.pending_device_preselect = None;
        if self.device_id.as_deref() == Some(device_id.as_str()) && self.device_resolved {
            return;
        }
        let label = self
            .devices
            .iter()
            .find(|device| device.device_id == device_id)
            .map(|device| device.label.clone());
        self.device_resolved = label.is_some();
        if let Some(label) = label {
            self.device_label = Some(label);
        }
        self.device_id = Some(device_id);
        if self.device_resolved {
            self.apply_device_defaults(window, cx);
        }
    }

    /// EXP-696: the sticky pick whose machine has left the candidate list —
    /// its heartbeat lapsed (or it stopped advertising a runnable agent).
    /// Some(label) blocks the launch and keeps the Device row on screen so
    /// the run can be re-pointed instead of landing here by surprise.
    fn offline_pick(&self) -> Option<&str> {
        if self.device_resolved || self.device_id.is_none() {
            return None;
        }
        Some(self.device_label.as_deref().unwrap_or("The selected device"))
    }

    /// Re-point the shared options cluster at the settled machine: a remote
    /// target hands it that machine's advertised agents + published launch
    /// defaults, this machine hands it back to the doctor + `CodingHub`.
    fn apply_device_defaults(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let remote = self
            .remote_device()
            .map(|device| launch_options::RemoteDefaults {
                agents: device.agents.clone(),
                settings: device.defaults.clone(),
            });
        self.launch.set_remote(remote, window, cx);
    }

    /// The settled machine, or `None` while nothing has settled.
    fn selected_device(&self) -> Option<&queries::LaunchDevice> {
        let device_id = self.device_id.as_deref()?;
        self.devices
            .iter()
            .find(|device| device.device_id == device_id)
    }

    /// The settled machine when it is NOT this one — the remote-start route.
    fn remote_device(&self) -> Option<&queries::LaunchDevice> {
        self.selected_device().filter(|device| !device.is_own)
    }

    /// EXP-696: the Device row — its own glass group above the agent options,
    /// exactly where web and mobile put it. Rendered only with MORE THAN ONE
    /// candidate: with a single machine there is nothing to choose, and the
    /// dialog reads exactly as it did before remote start existed. A sticky
    /// pick whose machine went offline always keeps the row — that is the
    /// only way to re-point the run.
    fn device_picker(&self, cx: &mut gpui::Context<Self>) -> Option<gpui::AnyElement> {
        let offline = self.offline_pick();
        if self.devices.len() < 2 && offline.is_none() {
            return None;
        }
        let foreground = cx.theme().foreground;
        let picked: SharedString = match offline {
            Some(label) => format!("{label} — offline"),
            None => self
                .selected_device()
                .map(|device| device.label.clone())
                .unwrap_or_else(|| "Select device…".to_string()),
        }
        .into();
        let view = cx.entity().downgrade();
        let candidates = self.devices.clone();
        let bound = self.device_id.clone();
        let trigger = Button::new("sc-device")
            .ghost()
            .cursor_pointer()
            .h_auto()
            .px_0()
            .py_0()
            .text_color(foreground.opacity(0.7))
            .dropdown_caret(true)
            // NOT `.label()` — upstream draws that in a `flex_none` box, so a
            // long machine name wraps onto a second line (EXP-697).
            .child(crate::surface::picker_value_label(picked))
            .dropdown_menu(move |mut menu, _window, _cx| {
                for device in &candidates {
                    let view = view.clone();
                    let device_id = device.device_id.clone();
                    menu = menu.item(
                        PopupMenuItem::new(SharedString::from(device.label.clone()))
                            .checked(bound.as_deref() == Some(device_id.as_str()))
                            .on_click(move |_, window, cx| {
                                let Some(view) = view.upgrade() else {
                                    return;
                                };
                                let device_id = device_id.clone();
                                view.update(cx, |this, cx| {
                                    this.set_device(device_id, window, cx);
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            });
        Some(
            crate::surface::glass_group_rows(vec![crate::surface::glass_picker_row(
                "Device",
                None,
                trigger.into_any_element(),
                cx,
            )])
            .into_any_element(),
        )
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
        // A live edit to the SELECTED action can change its input set —
        // reconcile the text-input editor states (typed values survive).
        self.build_action_text_inputs(window, cx);
        if ready {
            if let Some(pending) = self.pending_action_preselect.take() {
                self.select_action(pending, window, cx);
                // EXP-313: the fix-conflicts entry points preselect their PR
                // too — applied AFTER `select_action`, which clears the pick
                // maps. A PR that isn't in the open-reviews list (racing a
                // merge) is dropped; the user just picks manually.
                if let Some(issue_id) = self.pending_pr_preselect.take() {
                    let pick = pr_pick_options(cx, &self.team_id)
                        .into_iter()
                        .find(|(id, _)| *id == issue_id);
                    if let (Some(pick), Some(key)) = (
                        pick,
                        self.selected_action().and_then(|action| {
                            action
                                .inputs
                                .iter()
                                .find(|input| input.input_type == "pr")
                                .map(|input| input.key.clone())
                        }),
                    ) {
                        self.action_pr_picks.insert(key, pick);
                    }
                }
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
                    // An action selected before the fetch landed couldn't
                    // seed its repo inputs yet (EXP-349) — retry now.
                    this.seed_action_repo_inputs();
                    // EXP-615: chat REQUIRES a repo — with exactly one there
                    // is nothing to pick (web parity).
                    if this.chat_repo.is_none() {
                        if let [only] = &this.team_repos[..] {
                            this.chat_repo = Some(only.clone());
                        }
                    }
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
        self.action_textarea_inputs.clear();
        self.action_input_subscriptions.clear();
        self.action_repo_picks.clear();
        self.action_board_picks.clear();
        self.action_pr_picks.clear();
        self.action_icon_picks.clear();
        self.selected_action_id = Some(action_id);
        self.build_action_text_inputs(window, cx);
        self.seed_action_repo_inputs();
        cx.notify();
    }

    /// Build editor states for the SELECTED action's text inputs. Idempotent:
    /// states for keys that still exist are kept (typed values survive a live
    /// shape refresh), missing keys get fresh states, removed keys are
    /// dropped — so [`Self::refresh_actions`] can call this when a teammate
    /// edits the selected action's inputs while the dialog is open, and a
    /// newly added required input gets a field instead of an unfillable
    /// "Fill in X." block.
    fn build_action_text_inputs(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(action) = self.selected_action() else {
            return;
        };
        let text_inputs: Vec<(String, Option<String>)> = action
            .inputs
            .iter()
            .filter(|input| input.input_type == "text")
            .map(|input| (input.key.clone(), input.placeholder.clone()))
            .collect();
        self.action_text_inputs
            .retain(|key, _| text_inputs.iter().any(|(kept, _)| kept == key));
        for (key, placeholder) in text_inputs {
            if self.action_text_inputs.contains_key(&key) {
                continue;
            }
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
            self.action_text_inputs.insert(key, state);
        }

        // EXP-530: the same reconcile for `textarea` inputs — a multi-line
        // editor for the prompts and briefs a one-line field mangled.
        let Some(action) = self.selected_action() else {
            return;
        };
        let textarea_inputs: Vec<(String, Option<String>)> = action
            .inputs
            .iter()
            .filter(|input| input.input_type == "textarea")
            .map(|input| (input.key.clone(), input.placeholder.clone()))
            .collect();
        self.action_textarea_inputs
            .retain(|key, _| textarea_inputs.iter().any(|(kept, _)| kept == key));
        for (key, placeholder) in textarea_inputs {
            if self.action_textarea_inputs.contains_key(&key) {
                continue;
            }
            let state = cx.new(|cx| {
                let mut state = TextareaState::new(window, cx);
                if let Some(placeholder) = placeholder {
                    state = state.placeholder(placeholder);
                }
                state
            });
            self.action_input_subscriptions.push(cx.subscribe(
                &state,
                |_, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                },
            ));
            self.action_textarea_inputs.insert(key, state);
        }
    }

    /// Pre-fill `repo` inputs with the selected action's bound repository
    /// (EXP-349) — a picker reading "None" while the run targets the bound
    /// repo anyway looked misconfigured. Runs from [`Self::select_action`]
    /// and again when `repositories.list` lands (whichever comes last); the
    /// id latch keeps a manual re-pick from being stomped.
    fn seed_action_repo_inputs(&mut self) {
        if self.selected_action_id == self.seeded_repo_action_id {
            return;
        }
        let Some(action) = self.selected_action() else {
            return;
        };
        let repository_id = action.repository_id.clone();
        let repo_keys: Vec<String> = action
            .inputs
            .iter()
            .filter(|input| input.input_type == "repo")
            .map(|input| input.key.clone())
            .collect();
        let Some(repository_id) = repository_id else {
            self.seeded_repo_action_id = self.selected_action_id.clone();
            return;
        };
        // Repos not fetched yet — leave the latch unset so the fetch's
        // completion arm retries the seed.
        let Some(repo) = self
            .team_repos
            .iter()
            .find(|repo| repo.id == repository_id)
            .cloned()
        else {
            return;
        };
        for key in repo_keys {
            self.action_repo_picks.insert(key, repo.clone());
        }
        self.seeded_repo_action_id = self.selected_action_id.clone();
    }

    /// Whether `input` currently holds a usable value.
    fn action_input_filled(&self, input: &api::actions::ActionInput, cx: &App) -> bool {
        match input.input_type.as_str() {
            "text" => self
                .action_text_inputs
                .get(&input.key)
                .is_some_and(|state| !state.read(cx).value().trim().is_empty()),
            "textarea" => self
                .action_textarea_inputs
                .get(&input.key)
                .is_some_and(|state| !state.read(cx).value().trim().is_empty()),
            "repo" => self.action_repo_picks.contains_key(&input.key),
            "board" => self.action_board_picks.contains_key(&input.key),
            "pr" => self.action_pr_picks.contains_key(&input.key),
            "icon" => self.action_icon_picks.contains_key(&input.key),
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
                // EXP-530: identical to `text` on the wire — the type only
                // changes the editor, and the server resolves both the same.
                "textarea" => {
                    let Some(text) = self
                        .action_textarea_inputs
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
                "icon" => {
                    let Some(icon) = self.action_icon_picks.get(&input.key) else {
                        continue;
                    };
                    values.push(ActionInputValue {
                        key: input.key.clone(),
                        label: input.label.clone(),
                        input_type: input.input_type.clone(),
                        value: icon.clone(),
                        display: Some(icon.clone()),
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
        // EXP-662: once the repo resolves, the same background hop reads the
        // run registry for this issue's newest resumable record (a file read
        // plus a `.git` stat — never a blocking fs call in render). Both
        // inputs are snapshotted here: the background thread reaches no
        // globals.
        let data_dir = coding_flow::coding_data_dir(cx);
        let account_id = queries::active_account(cx).map(|account| account.id);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let result = api::repositories::for_issue(&trpc, &probe_id);
                    // Only a repo-backed issue can launch at all, so a
                    // record for an unresolvable one would offer a Resume
                    // the blocker refuses anyway.
                    let resumable = match (&result, &account_id) {
                        (Ok(Some(_)), Some(account_id)) => coding::run_registry::latest_for_issue(
                            &data_dir,
                            account_id,
                            &probe_id,
                        ),
                        _ => None,
                    };
                    (result, resumable)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.probe_generation != generation {
                    return; // superseded
                }
                let (result, resumable) = result;
                let state = match result {
                    Ok(repo) => RepoState::Ready(repo),
                    Err(err) => RepoState::Error(err.to_string()),
                };
                // Unresolvable issues can never launch — uncheck them.
                if !matches!(state, RepoState::Ready(Some(_))) {
                    this.checked.remove(&issue_id);
                }
                this.repos.insert(issue_id.clone(), state);
                this.resumables.insert(issue_id.clone(), resumable);
                cx.notify();
            });
        })
        .detach();
    }

    /// EXP-202/EXP-662: the single checked issue that has a resumable run
    /// record, with that record — the only shape a resume can take (a batch
    /// session's issues are never singly checked here). NOT gated on the
    /// picked agent: a resume relaunches the RECORDED agent's transcript
    /// (D2), the row just says so.
    fn resume_candidate(&self) -> Option<(&IssueRow, &RunRecord)> {
        // EXP-696: the local run registry describes THIS machine only — a
        // remote target resumes off its own synced worktree row instead (and
        // an offline pick is a remote target whose row is out of reach).
        if self.remote_device().is_some() || self.offline_pick().is_some() {
            return None;
        }
        let row = self.resume_issue()?;
        let record = self.resumables.get(&row.issue_id)?.as_ref()?;
        Some((row, record))
    }

    /// The one checked issue a resume could apply to at all — resume is an
    /// ISSUE concept (the Actions and Chat tabs never offer it) and a batch
    /// has no per-issue worktree.
    fn resume_issue(&self) -> Option<&IssueRow> {
        if self.subject_tab != SubjectTab::Issues {
            return None;
        }
        if self.checked.len() != 1 {
            return None;
        }
        let issue_id = self.checked.iter().next()?;
        self.rows.iter().find(|row| &row.issue_id == issue_id)
    }

    /// EXP-696 (web `resumeWorktree`): the REMOTE resume offer — the target
    /// machine's synced `device_worktrees` row for the single checked issue,
    /// resumable by the picked agent. `steer.startSession`'s `resume: true`
    /// continues that worktree's session there.
    fn remote_resume(&self, cx: &App) -> Option<RemoteResume> {
        let device = self.remote_device()?;
        let row = self.resume_issue()?;
        let collections = Store::global(cx).collections();
        let worktrees = collections.device_worktrees.read(cx);
        let worktree = queries::resume_worktree(
            worktrees.iter(),
            &device.row_id,
            &row.identifier,
            self.launch.agent.id(),
        )?;
        Some(RemoteResume {
            branch: worktree.branch.clone().unwrap_or_default(),
            reported_at: worktree.reported_at.clone(),
        })
    }

    /// Whether the launch will actually RESUME (checkbox on + a candidate —
    /// this machine's run registry, or the target machine's worktree row).
    fn resume_active(&self, cx: &App) -> bool {
        self.resume && (self.resume_candidate().is_some() || self.remote_resume(cx).is_some())
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
        // EXP-696: the picked machine dropped out of the candidates while the
        // dialog was open. Starting anyway would run HERE, with this
        // machine's agent/model/effort — so it blocks until the machine is
        // back or the user re-points the Device row.
        if let Some(label) = self.offline_pick() {
            return Some(
                format!("{label} is offline — reconnect it or pick another device.").into(),
            );
        }
        // EXP-696: the LOCAL tooling gate applies to a local run only — the
        // doctor probes THIS machine's CLIs and clone root, and neither
        // decides anything about another machine. A remote start is gated on
        // the target's own advertisement (which `steer.startSession`
        // re-checks server-side).
        match self.remote_device() {
            Some(device) => {
                if !device.agents.contains(&self.launch.agent) {
                    return Some(
                        format!(
                            "{} can't run {}.",
                            device.label,
                            self.launch.agent.label()
                        )
                        .into(),
                    );
                }
            }
            None => {
                let hub = CodingHub::global(cx);
                // EXP-662: a resume runs the RECORDED agent, not the picked
                // one — so that is the one whose tooling has to be there.
                let gated_agent = match self.resume_active(cx) {
                    true => self
                        .resume_candidate()
                        .map(|(_, record)| record.agent)
                        .unwrap_or(self.launch.agent),
                    false => self.launch.agent,
                };
                match hub.read(cx).doctor.report.as_ref() {
                    None => return Some("Checking local tools…".into()),
                    // Per-agent gate (EXP-201): only git + the SELECTED agent block.
                    Some(report) => {
                        if let Some(failed) = report.first_failure_for(gated_agent) {
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
            }
        }
        // EXP-615: the Chat tab's gate is its builtin's two required inputs,
        // named exactly like the Actions tab names an unfilled input.
        if self.subject_tab == SubjectTab::Chat {
            if self.chat_prompt.read(cx).value().trim().is_empty() {
                return Some("Fill in Prompt.".into());
            }
            if self.chat_repo.is_none() {
                return Some("Fill in Repository.".into());
            }
            return None;
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
                if !matches!(
                    input.input_type.as_str(),
                    "text" | "textarea" | "repo" | "board" | "pr" | "icon"
                ) {
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
                format!("At most {MAX_ISSUES_PER_RUN} issues per run. Split the batch.").into(),
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
                    format!("Already coding {}. Stop that session first.", row.identifier)
                        .into(),
                );
            }
            let synced = store.collections().coding_sessions.read(cx);
            if let Some(session) = synced.iter().find(|session| {
                session.issue_id.as_deref() == Some(row.issue_id.as_str())
                    && queries::coding_session_is_live(session, now)
            }) {
                // EXP-549: name the machine as it is called TODAY.
                let device = queries::session_device_presentation(
                    session,
                    store.collections().devices.read(cx).iter(),
                    now * 1_000,
                )
                .label
                .unwrap_or_else(|| "another device".to_string());
                return Some(
                    format!(
                        "{} already has a live session on {device} (only one session per issue).",
                        row.identifier
                    )
                    .into(),
                );
            }
        }
        let mut repo: Option<&str> = None;
        // EXP-712: a batch cuts ONE `exp/batch-<id8>` branch, so every issue
        // in it must resolve to the same BASE branch too — boards on one repo
        // can now develop on different ones. Same refusal as the server's.
        let mut bases: Vec<Option<String>> = Vec::new();
        for row in &self.rows {
            if !self.checked.contains(&row.issue_id) {
                continue;
            }
            match self.repos.get(&row.issue_id) {
                Some(RepoState::Ready(Some(resolved))) => {
                    match repo {
                        None => repo = Some(&resolved.repository_id),
                        Some(existing) if existing == resolved.repository_id => {}
                        Some(_) => {
                            return Some("One repository per run. Deselect the others.".into())
                        }
                    }
                    // `repositories.forIssue` resolves the branch through the
                    // issue's BOARD since EXP-712, so this IS the base the
                    // launch would cut from.
                    bases.push(
                        Some(resolved.default_branch.clone())
                            .filter(|branch| !branch.trim().is_empty()),
                    );
                }
                // Still resolving (or unresolvable-but-checked — transient).
                _ => return Some("Checking linked repositories…".into()),
            }
        }
        if let Some((first, second)) = crate::repo_resolver::batch_branch_conflict(&bases) {
            return Some(
                format!(
                    "All issues in a batch must share one base branch ({first} vs {second})."
                )
                .into(),
            );
        }
        None
    }


    /// The dialog's agent/model/effort/mode choices as launch options. A
    /// RESUME never re-enters plan mode (EXP-202): the plan already happened
    /// in the conversation being continued.
    fn options(&self, cx: &App) -> LaunchOptions {
        self.launch.options(self.resume_active(cx), cx)
    }

    /// Snapshot the checked set into a [`BatchLaunchRequest`] (2+ checked).
    /// `None` on a racing probe (the blocker just re-checked) — bail quietly.
    fn batch_request(&self, cx: &App) -> Option<BatchLaunchRequest> {
        let mut repo: Option<RepoGroup> = None;
        let mut issues: Vec<BatchIssueSpec> = Vec::new();
        // EXP-712: the board whose branch the batch cuts from. The blocker
        // already refused a set whose boards resolve to different branches,
        // so the FIRST board's branch is every checked issue's branch.
        let mut board_id: Option<String> = None;
        for row in &self.rows {
            if !self.checked.contains(&row.issue_id) {
                continue;
            }
            board_id.get_or_insert_with(|| row.board_id.clone());
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
            board_id,
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
        // EXP-696: another machine runs it — one `steer.startSession` with the
        // same subject, and the target's own launcher does the rest.
        if self.remote_device().is_some() {
            return self.launch_remote(window, cx);
        }
        // EXP-615: a CHAT launch is an action launch — the hidden builtin,
        // its two inputs, and the same runner (which resolves the repo and
        // spawns into the opener window).
        if self.subject_tab == SubjectTab::Chat {
            let Some(repo) = self.chat_repo.clone() else {
                return;
            };
            let prompt = self.chat_prompt.read(cx).value().trim().to_string();
            let action = api::actions::builtin_chat_action(&self.team_id);
            let inputs: Vec<ActionInputValue> = action
                .inputs
                .iter()
                .map(|input| {
                    let (value, display) = if input.key == "prompt" {
                        (prompt.clone(), prompt.clone())
                    } else {
                        (repo.id.clone(), repo.full_name.clone())
                    };
                    ActionInputValue {
                        key: input.key.clone(),
                        label: input.label.clone(),
                        input_type: input.input_type.clone(),
                        value,
                        display: Some(display),
                    }
                })
                .collect();
            let options = self.options(cx);
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
                    reservation: None,
                    trigger: None,
                    automation_id: None,
                    on_failed: None,
                },
                cx,
            );
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
                    reservation: None,
                    // A person clicked Start — never an automation firing
                    // (no trigger prompt section, no `started_reason`), so
                    // there is no poison-pill state to back off either.
                    trigger: None,
                    automation_id: None,
                    on_failed: None,
                },
                cx,
            );
            return;
        }
        if self.checked.len() == 1 {
            let issue_id = self.checked.iter().next().cloned().expect("one checked");
            let options = self.options(cx);
            // EXP-662: an active resume relaunches the RECORDED run exactly
            // (its agent, its worktree, its transcript). Only model/effort may
            // be nudged, and only while the picker is on that same agent (D2)
            // — otherwise the picker is describing a different program.
            let record = self
                .resume_active(cx)
                .then(|| self.resume_candidate().map(|(_, record)| record.clone()))
                .flatten();
            if let Some(record) = record {
                let same_agent = options.agent == record.agent;
                let Some(deps) = coding_flow::build_resume_deps(&record, cx) else {
                    self.error =
                        Some("Sign in and wait for sync before starting a session.".into());
                    cx.notify();
                    return;
                };
                let request = ResumeRunRequest {
                    record,
                    device_label: coding::default_device_label(),
                    origin: LaunchOrigin::Local,
                    model: same_agent.then(|| options.model.clone()),
                    effort: same_agent.then(|| options.effort.clone()),
                };
                return self.run_prepare(
                    PrepareRequest::ResumeRun(request),
                    deps,
                    SessionSubject::Issue(issue_id),
                    window,
                    cx,
                );
            }
            let Some((request, deps)) =
                coding_flow::build_launch(&issue_id, LaunchOrigin::Local, options, false, cx)
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

    /// EXP-696: build the `steer.startSession` payload for the current tab —
    /// the SAME subject the local paths launch, addressed at another machine.
    /// Exactly one subject: `issue_id` for a single checked issue,
    /// `issue_ids` for a batch, `action_id` (+ `team_id` for a builtin,
    /// + `inputs`) for the Actions and Chat tabs. `None` = nothing to send
    /// (a race the blocker just re-checked).
    fn remote_start_input(&self, device_id: String, cx: &App) -> Option<api::steer::StartSessionInput> {
        let options = self.options(cx);
        let mut input = api::steer::StartSessionInput {
            device_id,
            agent: Some(options.agent.id().to_string()),
            // Blank IS the "CLI default" sentinel here; the server's
            // per-agent vocabulary has no empty member, so omit it.
            model: Some(options.model.clone()).filter(|model| !model.is_empty()),
            effort: Some(options.effort.clone()).filter(|effort| !effort.is_empty()),
            ultracode: Some(options.ultracode),
            plan_mode: Some(options.plan_mode),
            ..Default::default()
        };
        match self.subject_tab {
            SubjectTab::Chat => {
                let repo = self.chat_repo.clone()?;
                let prompt = self.chat_prompt.read(cx).value().trim().to_string();
                let action = api::actions::builtin_chat_action(&self.team_id);
                let mut inputs = std::collections::BTreeMap::new();
                for definition in &action.inputs {
                    let value = if definition.key == "prompt" {
                        prompt.clone()
                    } else {
                        repo.id.clone()
                    };
                    inputs.insert(definition.key.clone(), value);
                }
                input.action_id = Some(action.id);
                // A builtin has no DB row to derive the team from.
                input.team_id = Some(self.team_id.clone());
                input.inputs = Some(inputs);
            }
            SubjectTab::Actions => {
                let action = self.selected_action()?.clone();
                let inputs: std::collections::BTreeMap<String, String> = self
                    .collect_action_inputs(&action, cx)
                    .into_iter()
                    .map(|filled| (filled.key, filled.value))
                    .collect();
                let builtin = api::actions::is_builtin_action_id(&action.id);
                input.team_id = builtin.then(|| self.team_id.clone());
                input.inputs = (!inputs.is_empty()).then_some(inputs);
                input.action_id = Some(action.id);
            }
            SubjectTab::Issues => {
                let mut checked: Vec<String> = self
                    .rows
                    .iter()
                    .filter(|row| self.checked.contains(&row.issue_id))
                    .map(|row| row.issue_id.clone())
                    .collect();
                match checked.len() {
                    0 => return None,
                    1 => {
                        input.issue_id = checked.pop();
                        // EXP-481: `resume` is a single-issue flag — it
                        // continues the machine's existing worktree.
                        if self.resume_active(cx) {
                            input.resume = Some(true);
                        }
                    }
                    _ => input.issue_ids = Some(checked),
                }
            }
        }
        Some(input)
    }

    /// EXP-696: hand the run to another machine. The mutation is the whole
    /// launch — the target's own launcher resolves the repo, mints its token
    /// and spawns the agent — so success just closes the dialog with a note
    /// on the opener window; a refusal renders in the dialog's error slot.
    fn launch_remote(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(device) = self.remote_device() else {
            return;
        };
        let device_label = device.label.clone();
        let device_id = device.device_id.clone();
        let Some(input) = self.remote_start_input(device_id, cx) else {
            return;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            self.error = Some("Sign in and wait for sync before starting a session.".into());
            cx.notify();
            return;
        };
        self.launching = true;
        self.error = None;
        cx.notify();
        let opener = self.opener;
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::steer::start_session(&trpc, &input) })
                .await;
            if result.is_ok() {
                // The agent tab opens on the OTHER machine, so say where the
                // run went — on the window that asked for it (EXP-284: this
                // dialog is its own native window, and is about to close).
                let note = Notification::success(SharedString::from(format!(
                    "Start sent to {device_label}."
                )));
                let _ = opener.update(window, |_, window, cx| {
                    window.push_notification(note, cx);
                });
            }
            let _ = this.update_in(window, |this, window, cx| {
                this.launching = false;
                match result {
                    Ok(()) => {
                        if let Some(on_launched) = this.on_launched.take() {
                            on_launched(cx);
                        }
                        native_dialog::close_dialog_window(window, cx);
                    }
                    Err(err) => this.error = Some(err.user_message().into()),
                }
                cx.notify();
            });
        })
        .detach();
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
        let observer = crate::steer_wiring::observer_setup(cx);
        let opener = self.opener;
        cx.spawn_in(window, async move |this, window| {
            let prepared = window
                .background_executor()
                .spawn(async move { coding::prepare_with_hooks(&request, &deps, hooks.as_ref(), observer.as_ref()) })
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
                    Ok(()) => {
                        // EXP-439: the session took the selection over — the
                        // bulk bar's callback drops the multiselect.
                        if let Some(on_launched) = this.on_launched.take() {
                            on_launched(cx);
                        }
                        native_dialog::close_dialog_window(window, cx)
                    }
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

    /// EXP-202/EXP-662: the "Resume previous session" notice + checkbox —
    /// rendered only while a single checked issue has a resumable run record
    /// ([`Self::resume_candidate`]). The copy names the RECORDED agent, since
    /// that is the one the resume relaunches whatever the picker says (D2).
    fn resume_row(
        &self,
        row: &IssueRow,
        record: &RunRecord,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let settings = CodingHub::global(cx).read(cx).settings.clone();
        let branch = record
            .branch
            .clone()
            .unwrap_or_else(|| coding::branch_name(&settings.branch_prefix, &row.identifier));
        let when = crate::comments::relative_time_epoch(
            record.recorded_at as i64,
            chrono::Utc::now().timestamp(),
        );
        let hint: SharedString = format!(
            "Resumes the {} session exactly (its own transcript), and falls back to a fresh \
             session seeded with the resume prompt if the transcript is gone.",
            record.agent.label()
        )
        .into();
        // The picker's agent is IGNORED by a resume — say so instead of
        // silently launching a different program than the pills show.
        let agent_note: Option<SharedString> = (self.launch.agent != record.agent).then(|| {
            format!(
                "Runs {}, not {}: a resume keeps the session's own agent.",
                record.agent.label(),
                self.launch.agent.label()
            )
            .into()
        });
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
                        "A session on {branch} ended {when}."
                    ))),
            )
            .child(
                div()
                    .pl_6()
                    .text_xs()
                    .text_color(muted.opacity(0.7))
                    .child(hint),
            )
            .when_some(agent_note, |this, note| {
                this.child(
                    div()
                        .pl_6()
                        .text_xs()
                        .text_color(muted.opacity(0.7))
                        .child(note),
                )
            })
    }

    /// EXP-696: the REMOTE machine's resume row. There is no local run record
    /// to name an agent or a transcript from — the offer is the target's
    /// synced worktree (web parity), so the copy says exactly that.
    fn remote_resume_row(
        &self,
        resume: &RemoteResume,
        device_label: &str,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let where_line: SharedString = match resume.branch.trim() {
            "" => format!("A worktree for this issue is still on {device_label}."),
            branch => format!("A {branch} worktree is still on {device_label}."),
        }
        .into();
        let when: Option<SharedString> = resume
            .reported_at
            .as_deref()
            .and_then(crate::comments::parse_epoch)
            .map(|seen| {
                format!(
                    "Last reported {}.",
                    crate::comments::relative_time_epoch(seen, chrono::Utc::now().timestamp())
                )
                .into()
            });
        v_flex()
            .gap_0p5()
            .child(
                Checkbox::new("sc-resume-remote")
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
                    .child(where_line),
            )
            .when_some(when, |this, when| {
                this.child(
                    div()
                        .pl_6()
                        .text_xs()
                        .text_color(muted.opacity(0.7))
                        .child(when),
                )
            })
    }

    /// The top-level Issues | Actions subject strip (EXP-257). EXP-525: the
    /// web launch dialog's FULL-WIDTH segmented capsule (`ui/tabs.tsx`)
    /// instead of the small centered `TabBar`.
    fn subject_tabs(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let segment = |label: &'static str, tab: SubjectTab, active: bool| {
            crate::controls::segmented_item(active, cx)
                .id(label)
                .child(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    if this.subject_tab != tab {
                        this.subject_tab = tab;
                        cx.notify();
                    }
                }))
        };
        crate::controls::segmented(cx)
            .child(segment(
                "Issues",
                SubjectTab::Issues,
                self.subject_tab == SubjectTab::Issues,
            ))
            .child(segment(
                "Actions",
                SubjectTab::Actions,
                self.subject_tab == SubjectTab::Actions,
            ))
            // EXP-615: the third subject — a free prompt on a repository.
            .child(segment(
                "Chat",
                SubjectTab::Chat,
                self.subject_tab == SubjectTab::Chat,
            ))
    }

    /// The Chat pane (EXP-615): the prompt editor + the repository picker.
    /// Both fields come from the hidden builtin's own input definitions, so
    /// their labels/placeholder cannot drift from the other three clients.
    fn chat_pane(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let muted = cx.theme().muted_foreground;
        let repos = self.team_repos.clone();
        let repo_field: gpui::AnyElement = if repos.is_empty() {
            div()
                .text_xs()
                .text_color(muted)
                .child("Connect a repository to this team to chat.")
                .into_any_element()
        } else {
            action_run::repo_dropdown(
                "sc-chat-repo".into(),
                self.chat_repo.as_ref(),
                repos,
                false,
                |this: &mut Self, repo, cx| {
                    this.chat_repo = repo;
                    cx.notify();
                },
                cx,
            )
            .into_any_element()
        };
        v_flex()
            .w_full()
            .gap_3()
            .child(launch_options::labeled_field(
                "Prompt",
                Textarea::new(&self.chat_prompt).h(px(180.)).into_any_element(),
                None,
                cx,
            ))
            .child(launch_options::labeled_field(
                "Repository",
                repo_field,
                None,
                cx,
            ))
            .into_any_element()
    }

    /// One Actions-tab list row: icon + name + selection check.
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
            // EXP-273: each action draws its own curated glyph (the builtins
            // set one explicitly), so the row reads the same as the web list.
            .child(crate::icons::action_icon(action.icon.as_deref()).xsmall().text_color(muted))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .truncate()
                    .text_color(theme.foreground)
                    .child(SharedString::from(action.name.clone())),
            )
            .when(is_selected, |this| {
                this.child(
                    Icon::new(registry::UI_CHECK)
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
                Some(state) => Input::new(state).web_input_sm().into_any_element(),
                None => div().into_any_element(), // transient re-selection frame
            },
            // EXP-530: the multi-line twin of `text` — same value on the
            // wire, a taller editor in the form.
            "textarea" => match self.action_textarea_inputs.get(&input.key) {
                Some(state) => Textarea::new(state).h(px(80.)).into_any_element(),
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
                    .outline().cursor_pointer()
                    .web_input_sm()
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
                    .outline().cursor_pointer()
                    .web_input_sm()
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
                let pick_label: SharedString = match self.action_pr_picks.get(&input.key) {
                    Some((_, label)) => label.clone().into(),
                    None => "Select pull request…".into(),
                };
                let pulls = pr_pick_options(cx, &self.team_id);
                let key = input.key.clone();
                let optional = !input.required;
                let view = cx.entity().downgrade();
                Button::new(("sc-input-pr", ix))
                    .outline().cursor_pointer()
                    .web_input_sm()
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
            // EXP-273: the curated icon set. Unlike the other pickers the
            // value is a NAME, not an id. EXP-575: the shared swatch-and-popover
            // picker (`board_form::icon_picker`), as everywhere else.
            "icon" => {
                let picked = self.action_icon_picks.get(&input.key).cloned();
                let key = input.key.clone();
                let view = cx.entity().downgrade();
                crate::board_form::icon_picker(
                    format!("sc-input-icon-{ix}"),
                    picked.as_deref(),
                    !input.required,
                    move |name, _, cx| {
                        if let Some(view) = view.upgrade() {
                            view.update(cx, |view, cx| {
                                match name {
                                    Some(name) => {
                                        view.action_icon_picks
                                            .insert(key.clone(), name.to_string());
                                    }
                                    None => {
                                        view.action_icon_picks.remove(&key);
                                    }
                                }
                                cx.notify();
                            });
                        }
                    },
                    cx,
                )
                .into_any_element()
            }
            // Unknown type (newer server): named, never a fake text field —
            // the launch blocker holds the run.
            _ => div()
                .text_xs()
                .text_color(muted)
                .child("This input type isn't supported. Update the app.")
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
    ///
    /// EXP-291: PINNED at the dialog's bottom edge (see [`Render`]) — it sits
    /// outside the scrolling body, so it reads as a bar: a hairline above it
    /// and `pt_3` breathing room instead of the old in-flow `pt_1`.
    fn footer(
        &self,
        blocker: Option<SharedString>,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let mut footer = h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .pt_3()
            .border_t_1()
            .border_color(cx.theme().border);
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
                    .outline().cursor_pointer()
                    .web_sm()
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
                    .primary().cursor_pointer()
                    .web_sm()
                    .label(if self.launching {
                        "Starting…"
                    } else if self.subject_tab == SubjectTab::Chat {
                        "Start chat"
                    } else if self.subject_tab == SubjectTab::Actions {
                        "Run action"
                    } else if self.resume_active(cx) {
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
                    .child("No matches. Only open issues from this board are shown."),
            );
        }
        if hidden > 0 {
            checklist = checklist.child(
                div()
                    .text_xs()
                    .text_color(theme_muted)
                    .child(SharedString::from(format!(
                        "+{hidden} more. Refine your search."
                    ))),
            );
        }

        // ---- resume (EXP-202/EXP-662): single checked issue with a recorded
        //      run offers "Resume previous session" ----
        let resume_active = self.resume_active(cx);
        // EXP-696: local runs offer this install's own run record; a remote
        // target offers ITS synced worktree instead.
        let remote_resume = self.remote_resume(cx).map(|resume| {
            let label = self
                .remote_device()
                .map(|device| device.label.clone())
                .unwrap_or_default();
            (resume, label)
        });
        let resume_row = match &remote_resume {
            Some((resume, label)) => {
                Some(self.remote_resume_row(resume, label, cx).into_any_element())
            }
            None => self
                .resume_candidate()
                .map(|(row, record)| self.resume_row(row, record, cx).into_any_element()),
        };

        let blocker = self.launch_blocker(cx);
        // EXP-268: two-column widescreen layout (web `launch-dialog.tsx`
        // parity) — subject tabs full-width on top, then picker LEFT /
        // options RIGHT, error + footer full-width below. EXP-525: the
        // explainer paragraphs are gone (web has none either).
        let mut left = v_flex().flex_1().min_w_0().gap_3();
        match self.subject_tab {
            SubjectTab::Issues => {
                left = left
                    .child(Input::new(&self.search).web_input_sm())
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
            SubjectTab::Chat => {
                // EXP-615: the free prompt + its repository — no picker, no
                // list; the pane IS the form.
                left = left.child(self.chat_pane(cx));
            }
            SubjectTab::Actions => {
                // The single-select action list (the fix-conflicts builtin
                // pinned first) + the selected action's typed input fields
                // (EXP-257). "Create action" is deliberately not offered here
                // — authoring lives in `crate::create_action_dialog`.
                let query = self.action_search.read(cx).value().trim().to_lowercase();
                let visible: Vec<usize> = self
                    .actions
                    .iter()
                    .enumerate()
                    .filter(|(_, action)| action.id != api::actions::BUILTIN_CREATE_ACTION_ID)
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
                    .child(Input::new(&self.action_search).web_input_sm())
                    .child(self.bounded_pane(
                        "sc-actions-scroll",
                        &self.action_list_scroll.clone(),
                        200.,
                        list.into_any_element(),
                        cx,
                    ));
            }
        }
        // The selected action's typed input fields — shared by both Actions
        // presentations (the picker and EXP-431 create mode, where they ARE
        // the form).
        if self.subject_tab == SubjectTab::Actions {
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
        // Web parity: the selection-size notes ride the picker column.
        if self.subject_tab == SubjectTab::Issues {
            if checked_count > MAX_ISSUES_PER_RUN {
                left = left.child(div().text_xs().text_color(warning).child(
                    SharedString::from(format!(
                        "At most {MAX_ISSUES_PER_RUN} issues per run. Split the batch."
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

        // Right column: the ONE shared options cluster (agent pills,
        // model/effort, toggles) — the web `LaunchOptionsPane` twin. A resume
        // never re-enters plan mode, so its row is hidden while the resume
        // checkbox is on (`options()` clamps it off regardless).
        // EXP-696: WHERE it runs is its own group ABOVE what it runs with —
        // the web `LaunchOptionsPane` order, and only with a choice to make.
        let device_picker = self.device_picker(cx);
        let right = v_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .children(device_picker)
            .child(self.launch.render(
                "sc-launch",
                |this: &mut Self| &mut this.launch,
                resume_row,
                resume_active,
                cx,
            ));

        let mut body = v_flex().w_full().gap_3().child(self.subject_tabs(cx));
        body = body.child(
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

        // EXP-291: full-height column — the BODY scrolls, the action bar is
        // pinned to the dialog's bottom edge and always reachable. The dialog
        // opts out of the shell's own scroller
        // ([`DialogContent::self_scrolling`]), which hands this root a
        // definite-height box for `size_full` to resolve against; without it
        // the root would collapse to its content height and the whole view —
        // footer included — would scroll again.
        //
        // The body pane is the EXP-67 scroll-pane idiom without a `max_h`
        // (`flex_1` + `min_h_0` bounds it against the window instead), so at
        // the 640×480 resize floor the two columns scroll here while the
        // inner `bounded_pane` lists keep their own caps and scroll
        // independently.
        let body_scroll = self.body_scroll.clone();
        v_flex()
            .size_full()
            .gap_3()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        v_flex()
                            .id("sc-body-scroll")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&body_scroll)
                            .child(body),
                    )
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .child(Scrollbar::new(&body_scroll).axis(ScrollbarAxis::Vertical)),
                    ),
            )
            .child(self.footer(blocker, cx))
            .into_any_element()
    }
}
