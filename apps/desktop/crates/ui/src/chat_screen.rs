//! EXP-772/EXP-825 — the Agent page (`Screen::Chat`), the desktop twin of the
//! web `t/$teamSlug/agent` route. Since EXP-825 its composer is THE launcher:
//! the one place a chat, a single-issue run, a batch or an action starts, on
//! this machine or another. The three-tab Start-coding dialog and the
//! Create-action dialog are gone; every play button navigates here with a
//! [`navigation::ChatSeed`] instead.
//!
//! One wide rounded composer, vertically centred:
//!
//! ```text
//! ┌───────────────────────────────────────────────┐
//! │ [EXP-42 ✕] [EXP-43 ✕]      ← subject chips     │  OR one action chip
//! │ [the action's pick inputs]                     │
//! │ the mention field (@ members, # issues, :emoji)│
//! │ [pending images]                               │
//! │ #  ▶  🖼                       ( Start batch · 2 ) │
//! └───────────────────────────────────────────────┘
//!  Device ▾ · Agent ▾ · Model ▾ · Plan ○ · Resume ○ · Repository ▾ · ⋯
//! ```
//!
//! **Subject by swap** (decision 2026-09-10): issue chips OR one action chip.
//! Picking an issue while an action is picked replaces it, and the other way
//! round; the chips make it visible, no control is ever disabled. **Free
//! text** is the chat prompt with no subject, the Create-action builtin's
//! request, and optional additional instructions beside a subject
//! ([`chat_launch::text_required`]). **Images** ride the text as steer
//! embeds, uploaded to the team route before the session exists.
//!
//! **Options row B** under the card: Device, Agent, Model, Plan on one muted
//! line (+ Resume while a single issue has a resumable worktree, + Repository
//! while no subject is picked), and `⋯` unfolds Effort, Ultracode, MCP
//! servers and Account. The options are the ONE launch model every desktop
//! surface uses ([`crate::launch_options::LaunchOptionsSection`]); plan mode
//! reseeds when the subject flips (a chat is a conversation, OFF; a picked
//! subject takes the agent's default).
//!
//! EXP-696: WHERE the run happens is the Device pick — this machine first,
//! then every ONLINE synced device advertising a runnable agent, only while
//! `steer.config` says the instance runs a relay. Another machine swaps the
//! launch for one `steer.startSession` with the same subject; an explicit
//! pick is STICKY and blocks the launch while its machine is offline.
//!
//! EXP-790/EXP-820: suggestion chips over the EMPTY, subject-less field —
//! four drawn once per page from a pool byte-identical to the web's
//! `CHAT_SUGGESTIONS` (locked below).

use std::collections::{HashMap, HashSet};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, AnyElement, App, AppContext as _, ClickEvent, Entity, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::input::{InputEvent, InputState, TextareaState};
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_component::notification::Notification;
use gpui_component::popover::Popover;
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _};
use sync::Store;

use coding::{
    run_registry::RunRecord, LaunchOptions, LaunchOrigin, Prepared, PrepareRequest,
    ResumeRunRequest,
};

use crate::action_inputs::ActionInputPicks;
use crate::action_run::{self, ActionRepo, ActionRepoRow, StartActionArgs};
use crate::chat_launch::{self, RemoteSubject, RepoState, SubjectKind};
use crate::coding_flow::{self, CodingHub, SessionSubject};
use crate::composer_images::{self, PendingImages};
use crate::icons::registry;
use crate::issue_picker::{self, IssueRow};
use crate::launch_options::{self, inline_pin_trigger, LaunchOptionsSection};
use crate::mention_input::MentionInput;
use crate::navigation::{self, ChatSeed, Navigation};
use crate::queries;
use crate::surface::{glass_pill, PillMode, PillSize};

/// The page's one field: wide, rounded, Enter sends and Shift+Enter breaks a
/// line — the steer composer's rhythm, on a page with nothing else on it.
const PROMPT_MAX_W: f32 = 640.;

/// EXP-822: the repo-less entry of the Repository pick, byte-identical to the
/// web chat page's `NO_REPO_LABEL` (`lib/chat-repo.ts`).
const NO_REPO_LABEL: &str = "No repository";

/// EXP-790/EXP-820: the suggestion POOL over an empty prompt — the desktop
/// twin of the web page's `CHAT_SUGGESTIONS` (`lib/chat-suggestions.ts`,
/// rendered by `routes/t/$teamSlug/agent.tsx`), byte-identical and in the
/// same order. A suggestion carrying a `#` opens the issue picker the moment
/// it lands (the caret parks right after that `#`); the others are plain text.
/// The page shows [`CHAT_SUGGESTION_COUNT`]
/// of them, picked once per page ([`pick_chat_suggestions`]).
pub(crate) const CHAT_SUGGESTIONS: [&str; 16] = [
    "Fix #",
    "Explain #",
    "Review #",
    "Split # into sub-issues",
    "Label every issue in the backlog",
    "Set a priority on every unprioritized issue",
    "Find duplicate issues and link them",
    "Do a code review of the open PRs and file the findings on a new board",
    "Create an automation that labels new issues",
    "Set up a weekly standup digest automation",
    "Draft release notes from the issues completed this month",
    "Summarize what changed across the boards this week",
    "Start a session for # on my other machine",
    "Move stale in-progress issues back to the backlog",
    "Comment a plan on #",
    "Which issues are blocked, and by what?",
];

/// How many of the pool a page shows.
pub(crate) const CHAT_SUGGESTION_COUNT: usize = 4;

/// EXP-820: `CHAT_SUGGESTION_COUNT` DISTINCT indices into
/// [`CHAT_SUGGESTIONS`] — a partial Fisher-Yates over the pool driven by a
/// tiny xorshift on `seed`, so the page needs no `rand` dependency. Pure, so
/// the distinctness is testable; the caller seeds it once per page (the chips
/// must not reshuffle on every frame).
pub(crate) fn pick_chat_suggestions(seed: u64) -> Vec<usize> {
    let mut state = seed | 1; // xorshift needs a non-zero state
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut indices: Vec<usize> = (0..CHAT_SUGGESTIONS.len()).collect();
    let count = CHAT_SUGGESTION_COUNT.min(indices.len());
    for at in 0..count {
        let swap = at + (next() as usize) % (indices.len() - at);
        indices.swap(at, swap);
    }
    indices.truncate(count);
    indices
}

/// The seed a page picks its chips with: the clock's nanoseconds.
fn suggestion_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15)
}

/// EXP-484/747 B7: a machine's `agent_accounts` payload off its SYNCED row —
/// which login each agent CLI runs as there, and its account profiles. Empty
/// for a row that never reported (an older build, or one that has not beaten
/// yet): the Account pin then has nothing to offer and hides.
fn device_agent_accounts(row_id: &str, cx: &App) -> coding::agent_accounts::AgentAccounts {
    if row_id.is_empty() {
        return Default::default();
    }
    let collections = Store::global(cx).collections();
    let devices = collections.devices.read(cx);
    let Some(row) = devices.iter().find(|row| row.id == row_id) else {
        return Default::default();
    };
    crate::device_settings::parse_agent_map::<coding::agent_accounts::AgentAccount>(
        row.agent_accounts.as_ref(),
    )
}

/// EXP-862 — a machine's KIND glyph for the device picker: the headless CLI
/// daemon is a SERVER (`ui-server`), everything else a desktop (`ui-device`),
/// the same pair the Devices list and Getting started wear. An unsynced row
/// reads as a desktop, which is what this IDE is.
fn device_kind_icon(device_id: &str, cx: &App) -> crate::icons::ExpIcon {
    let server = Store::try_global(cx).is_some_and(|store| {
        store
            .collections()
            .devices
            .read(cx)
            .iter()
            .any(|row| row.device_id.as_deref() == Some(device_id) && row.is_server())
    });
    if server {
        registry::UI_SERVER
    } else {
        registry::UI_DEVICE
    }
}

/// The checked issues and everything their launch needs.
struct IssueSubject {
    /// The picker's pool (open team issues + the seeded ones).
    rows: Vec<IssueRow>,
    checked: HashSet<String>,
    /// issue id → probe state (LAZY: only checked issues probe).
    repos: HashMap<String, RepoState>,
    /// issue id → the newest resumable run record for it (EXP-662 — probed
    /// alongside the repo, `None` = nothing to resume).
    resumables: HashMap<String, Option<RunRecord>>,
    /// "Resume previous session" (EXP-202): only ACTIVE with exactly one
    /// checked issue and a candidate; default-on so a re-launch resumes.
    resume: bool,
}

/// The picked action and its input picks.
struct ActionSubject {
    action_id: String,
    picks: ActionInputPicks,
}

/// What the composer is about to start — chips OR chip, never both.
enum Subject {
    None,
    Issues(IssueSubject),
    Action(ActionSubject),
}

/// EXP-696: the machine the run starts on.
#[derive(Default)]
struct DevicePick {
    /// `None` before the first settle. The routing switch is its candidate's
    /// `is_own` flag: this machine takes the LOCAL launch paths, anything
    /// else goes out as one `steer.startSession`.
    device_id: Option<String>,
    /// EXP-836: the machine a ▶ REQUESTED (`?device=`, `ChatSeed.device_id`).
    /// It outranks everything the moment it is a candidate — and keeps
    /// outranking the already-settled default until then. One-shot: a person's
    /// pick drops it, and it is never persisted as a default.
    requested: Option<String>,
    /// The machine the PERSON picked in the Device pin. STICKY: it survives its
    /// machine dropping out of the candidate list.
    picked: Option<String>,
    /// Whether the settled pick currently resolves to a candidate. `false` =
    /// its machine dropped out: the launch is blocked instead of re-pointing.
    resolved: bool,
    /// The pick's last known label — the blocker names an offline machine.
    label: Option<String>,
    /// The candidate list the picker last settled against.
    devices: Vec<queries::LaunchDevice>,
}

/// EXP-825: the field's hint follows the subject — byte-identical to the web
/// `composerPlaceholder` (components/launch-composer.tsx): a chat asks, a
/// picked subject takes optional extra instructions, and a picked action
/// with a non-blank `prompt_placeholder` (the synced column; the
/// Create-action builtin carries its own) says what to type instead.
const CHAT_PLACEHOLDER: &str = "Ask the agent…";
const SUBJECT_PLACEHOLDER: &str = "Additional instructions (optional)…";

/// The hint for `subject`, given the picked action's row (if the list holds
/// it). The web `composerPlaceholder` rule, one for one.
fn placeholder_for_subject(
    subject: &Subject,
    action: Option<&api::actions::Action>,
) -> SharedString {
    if matches!(subject, Subject::None) {
        return CHAT_PLACEHOLDER.into();
    }
    if matches!(subject, Subject::Action(_)) {
        let hint = action
            .and_then(|action| action.prompt_placeholder.as_deref())
            .map(str::trim)
            .filter(|hint| !hint.is_empty());
        if let Some(hint) = hint {
            return SharedString::from(hint.to_string());
        }
    }
    SUBJECT_PLACEHOLDER.into()
}

pub(crate) struct ChatScreenView {
    nav: Entity<Navigation>,
    /// The team the page is scoped to; a switch resets every pick.
    team_id: Option<String>,
    input: Entity<TextareaState>,
    /// The hint the field currently shows (see [`placeholder_for_subject`]).
    placeholder: SharedString,
    /// EXP-790: the completion overlay (`@` / `#` / `:`) over `input`; the
    /// composer card draws the chrome, so the widget draws none of its own.
    mention: Entity<MentionInput>,
    mention_team: Option<String>,
    subject: Subject,
    /// Stale-probe guard (old results must not land after a subject swap).
    probe_generation: u64,
    /// The team's actions (builtins pinned first) — live off the synced
    /// `actions` shape; `actions_ready` = the shape reached readiness.
    actions: Vec<api::actions::Action>,
    actions_ready: bool,
    /// A seed's action preselect, applied once the shape is ready, with its
    /// PR (EXP-313) and icon (a suggestion's glyph) riding along.
    pending_action: Option<String>,
    pending_pr: Option<String>,
    pending_icon: Option<String>,
    issue_search: Entity<InputState>,
    /// The team's connected repos (`repositories.list`, one fetch per team).
    team_repos: Vec<ActionRepoRow>,
    repos_team: Option<String>,
    /// EXP-822: the chat's optional repository anchor (no-subject only).
    chat_repo: Option<ActionRepoRow>,
    /// The ONE launch cluster's state (agent, model, effort, toggles, MCP,
    /// account). Built once the coding hub exists (the page can render
    /// before it does).
    launch: Option<LaunchOptionsSection>,
    device: DevicePick,
    /// EXP-792: the team's MCP servers + THIS machine's readiness, one fetch
    /// per team; `None` while the fetch is out.
    mcp: Option<(
        Vec<api::mcp_servers::McpServerListEntry>,
        Vec<api::mcp_servers::McpReadinessReport>,
    )>,
    mcp_team: Option<String>,
    images: PendingImages,
    /// The image strip's notice (too many / too big).
    notice: Option<SharedString>,
    /// Whether the `⋯` line (Effort, Ultracode, MCP, Account) is unfolded.
    more_open: bool,
    /// Images are uploading (the send is in flight).
    sending: bool,
    /// The launch is preparing / the remote start is in flight.
    launching: bool,
    error: Option<SharedString>,
    /// EXP-820: the chips this page shows — indices into
    /// [`CHAT_SUGGESTIONS`], drawn once when the page was built.
    suggestions: Vec<usize>,
    /// A parked accessor target for the pick renderers while no action is
    /// picked (a menu can outlive the chip it was opened from).
    spare_picks: ActionInputPicks,
    focus_handle: FocusHandle,
    /// EXP-851: the Agent page's OWN session list — the Running and Past
    /// sections that sat in the retired tool column beside it. They stack
    /// UNDER the composer now, in the page's one scroll.
    sessions_running: Entity<crate::sessions_section::RunningSessionsSection>,
    sessions_past: Entity<crate::sessions_section::PastSessionsSection>,
    /// That one scroll (composer + both sections).
    page_scroll: gpui::ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl ChatScreenView {
    pub(crate) fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = navigation::nav_for_window(window, cx);
        let input = cx.new(|cx| {
            crate::controls::web_textarea(2, 8, window, cx)
                .submit_on_enter(true)
                .placeholder(CHAT_PLACEHOLDER)
        });
        let mention = cx.new(|cx| {
            let mut mention = MentionInput::new(input.clone(), cx);
            mention.set_appearance(false);
            mention
        });
        let issue_search = cx.new(|cx| InputState::new(window, cx).placeholder("Search issues…"));
        let mut subscriptions = vec![
            cx.subscribe_in(
                &input,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::PressEnter { shift: false, .. } => this.send(window, cx),
                    // The suggestion chips and the blocker are functions of
                    // the draft.
                    InputEvent::Change => cx.notify(),
                    _ => {}
                },
            ),
            cx.observe(&nav, |_, _, cx| cx.notify()),
            cx.subscribe(&issue_search, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];
        let collections = Store::global(cx).collections();
        let synced_devices = collections.devices.clone();
        let synced_actions = collections.actions.clone();
        let synced_sessions = collections.coding_sessions.clone();
        let synced_worktrees = collections.device_worktrees.clone();
        let steer_config = queries::steer_config(cx);
        subscriptions.push(cx.observe_in(&steer_config, window, |this: &mut Self, _, window, cx| {
            this.settle_device(window, cx);
            cx.notify();
        }));
        // EXP-696: the Device pick is a live read of the `devices` shape — a
        // machine going offline (or coming back) re-settles the pick.
        subscriptions.push(cx.observe_in(&synced_devices, window, |this: &mut Self, _, window, cx| {
            this.settle_device(window, cx);
            cx.notify();
        }));
        subscriptions.push(cx.observe(&synced_worktrees, |_: &mut Self, _, cx| cx.notify()));
        // EXP-268: the actions list is a live read of the synced shape.
        subscriptions.push(cx.observe_in(&synced_actions, window, |this: &mut Self, _, window, cx| {
            this.refresh_actions(window, cx);
            cx.notify();
        }));
        // EXP-202: the one-session-per-issue blocker tracks both the local
        // registry and the synced rows — re-render whenever either moves.
        let local_sessions = coding_flow::LocalSessions::global(cx);
        subscriptions.push(cx.observe(&local_sessions, |_: &mut Self, _, cx| cx.notify()));
        subscriptions.push(cx.observe(&synced_sessions, |_: &mut Self, _, cx| cx.notify()));
        // The doctor report lands after the window does — re-seed the agent
        // pick when it does, or the page would sit on "no agent" at first.
        if let Some(hub) = CodingHub::global_ref(cx) {
            subscriptions.push(cx.observe_in(&hub, window, |this: &mut Self, _, window, cx| {
                if let Some(launch) = this.launch.as_mut() {
                    launch.reconcile_agent(window, cx);
                }
                cx.notify();
            }));
        }
        let mut this = Self {
            nav,
            team_id: None,
            input,
            placeholder: CHAT_PLACEHOLDER.into(),
            mention,
            mention_team: None,
            subject: Subject::None,
            probe_generation: 0,
            actions: Vec::new(),
            actions_ready: false,
            pending_action: None,
            pending_pr: None,
            pending_icon: None,
            issue_search,
            team_repos: Vec::new(),
            repos_team: None,
            chat_repo: None,
            launch: None,
            device: DevicePick::default(),
            mcp: None,
            mcp_team: None,
            images: PendingImages::default(),
            notice: None,
            more_open: false,
            sending: false,
            launching: false,
            error: None,
            suggestions: pick_chat_suggestions(suggestion_seed()),
            spare_picks: ActionInputPicks::default(),
            focus_handle: cx.focus_handle(),
            // EXP-862: the Agent page's Running band is ALWAYS on screen —
            // empty it says so ("No agents running right now."), which is the
            // answer the page exists to give.
            sessions_running: cx.new(crate::sessions_section::RunningSessionsSection::always),
            sessions_past: cx
                .new(|cx| crate::sessions_section::PastSessionsSection::new(window, cx)),
            page_scroll: gpui::ScrollHandle::new(),
            _subscriptions: subscriptions,
        };
        this.ensure_launch(window, cx);
        this
    }

    // ── team scope ────────────────────────────────────────────────────────

    /// Point every per-team read at the active team; a switch resets the
    /// subject and the picks (screens are team-scoped).
    fn sync_team(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let team_id = navigation::active_team_id(&self.nav, cx);
        if team_id == self.team_id {
            return;
        }
        self.team_id = team_id.clone();
        self.subject = Subject::None;
        self.probe_generation += 1;
        self.pending_action = None;
        self.pending_pr = None;
        self.pending_icon = None;
        self.error = None;
        self.mention_team = team_id.clone();
        self.mention.update(cx, |mention, _| {
            mention.set_source(team_id.clone().map(crate::markdown::store_completion_source));
        });
        self.refresh_actions(window, cx);
        self.ensure_repos_loaded(cx);
        self.ensure_mcp_loaded(cx);
        if let Some(launch) = self.launch.as_mut() {
            launch.reseed_plan_for_subject(false, cx);
            // The new team's `enabled_by_default` servers seed afresh once
            // its list lands; the old team's ticks mean nothing here.
            launch.reset_mcp_seed();
        }
    }

    /// The launch cluster, built once the coding hub exists.
    fn ensure_launch(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.launch.is_some() || CodingHub::global_ref(cx).is_none() {
            return;
        }
        let mut launch = LaunchOptionsSection::new(window, cx);
        launch.reconcile_agent(window, cx);
        // A chat starts in build mode, always (EXP-772).
        launch.reseed_plan_for_subject(false, cx);
        self.launch = Some(launch);
        // EXP-696: settle the machine, which may re-seed the cluster off a
        // remote advertisement.
        self.settle_device(window, cx);
    }

    fn launch_ref(&self) -> &LaunchOptionsSection {
        self.launch.as_ref().expect("the launch cluster is built before it is read")
    }

    fn launch_access(this: &mut Self) -> &mut LaunchOptionsSection {
        this.launch.as_mut().expect("the launch cluster is built before it is edited")
    }

    fn picks_access(this: &mut Self) -> &mut ActionInputPicks {
        match &mut this.subject {
            Subject::Action(action) => &mut action.picks,
            _ => &mut this.spare_picks,
        }
    }

    // ── the seed (every play button) ──────────────────────────────────────

    /// Apply a [`ChatSeed`]: an action wins over issues (the web rule),
    /// a device seed is a sticky explicit pick, text lands on an EMPTY draft.
    fn apply_seed(&mut self, seed: ChatSeed, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if let Some(action_id) = seed.action_id {
            self.pending_action = Some(action_id);
            self.pending_pr = seed.pr_issue_id;
            self.pending_icon = seed.icon;
            self.refresh_actions(window, cx);
        } else if !seed.issue_ids.is_empty() {
            self.set_issue_subject(seed.issue_ids.into_iter().collect(), cx);
        }
        if let Some(device_id) = seed.device_id {
            // EXP-836: a REQUEST, not a settle input — this screen is
            // long-lived, so the default machine has already settled by now and
            // the request has to outrank it.
            self.device.requested = Some(device_id);
            self.settle_device(window, cx);
        }
        if let Some(text) = seed.text {
            if self.input.read(cx).value().trim().is_empty() {
                self.mention
                    .update(cx, |mention, cx| mention.insert_text(&text, window, cx));
            }
        }
        cx.notify();
    }

    // ── subject: issues ───────────────────────────────────────────────────

    /// Replace the subject with these checked issues (a seed, or the first
    /// pick while an action was the subject) and probe them.
    fn set_issue_subject(&mut self, checked: HashSet<String>, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.team_id.clone() else {
            return;
        };
        let rows = issue_picker::snapshot_rows(cx, &team_id, &checked);
        let checked: HashSet<String> = rows
            .iter()
            .filter(|row| checked.contains(&row.issue_id))
            .map(|row| row.issue_id.clone())
            .collect();
        self.probe_generation += 1;
        let had_subject = !matches!(self.subject, Subject::None);
        self.subject = Subject::Issues(IssueSubject {
            rows,
            checked: checked.clone(),
            repos: HashMap::new(),
            resumables: HashMap::new(),
            resume: true,
        });
        if !had_subject {
            if let Some(launch) = self.launch.as_mut() {
                launch.reseed_plan_for_subject(true, cx);
            }
        }
        for issue_id in checked {
            self.ensure_probe(issue_id, cx);
        }
    }

    /// The `#` picker's toggle: check/uncheck an issue, swapping an action
    /// subject out for issues on the first check, and back to no subject on
    /// the last uncheck (the label then reads "Start chat" again).
    fn toggle_issue(&mut self, issue_id: String, on: bool, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        match &mut self.subject {
            Subject::Issues(issues) => {
                if on {
                    issues.checked.insert(issue_id.clone());
                    self.ensure_probe(issue_id, cx);
                } else {
                    issues.checked.remove(&issue_id);
                    if issues.checked.is_empty() {
                        self.clear_subject(cx);
                    }
                }
            }
            _ if on => self.set_issue_subject([issue_id].into_iter().collect(), cx),
            _ => {}
        }
        cx.notify();
    }

    /// Back to a plain chat: no chips, plan mode off.
    fn clear_subject(&mut self, cx: &mut gpui::Context<Self>) {
        self.subject = Subject::None;
        self.probe_generation += 1;
        if let Some(launch) = self.launch.as_mut() {
            launch.reseed_plan_for_subject(false, cx);
        }
        cx.notify();
    }

    /// Kick ONE `repositories.forIssue` probe for `issue_id` if it never ran
    /// (background executor, generation-guarded). Lazy by design: only
    /// checked issues probe. EXP-662: the same hop reads the run registry
    /// for the issue's newest resumable record.
    fn ensure_probe(&mut self, issue_id: String, cx: &mut gpui::Context<Self>) {
        let Subject::Issues(issues) = &mut self.subject else {
            return;
        };
        if issues.repos.contains_key(&issue_id) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            issues
                .repos
                .insert(issue_id, RepoState::Error("Not signed in.".to_string()));
            return;
        };
        issues.repos.insert(issue_id.clone(), RepoState::Loading);
        let generation = self.probe_generation;
        let probe_id = issue_id.clone();
        let data_dir = coding_flow::coding_data_dir(cx);
        let account_id = queries::active_account(cx).map(|account| account.id);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let result = api::repositories::for_issue(&trpc, &probe_id);
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
                let Subject::Issues(issues) = &mut this.subject else {
                    return;
                };
                let (result, resumable) = result;
                let state = match result {
                    Ok(repo) => RepoState::Ready(repo),
                    Err(err) => RepoState::Error(err.to_string()),
                };
                // Unresolvable issues can never launch — uncheck them.
                if !matches!(state, RepoState::Ready(Some(_))) {
                    issues.checked.remove(&issue_id);
                }
                issues.repos.insert(issue_id.clone(), state);
                issues.resumables.insert(issue_id, resumable);
                cx.notify();
            });
        })
        .detach();
    }

    /// The one checked issue a resume could apply to at all.
    fn resume_issue(&self) -> Option<&IssueRow> {
        let Subject::Issues(issues) = &self.subject else {
            return None;
        };
        if issues.checked.len() != 1 {
            return None;
        }
        let issue_id = issues.checked.iter().next()?;
        issues.rows.iter().find(|row| &row.issue_id == issue_id)
    }

    /// EXP-202/EXP-662: the single checked issue with a LOCAL resumable run
    /// record. A remote target resumes off its own synced worktree instead.
    fn resume_candidate(&self) -> Option<(&IssueRow, &RunRecord)> {
        if self.remote_device().is_some() || self.offline_pick().is_some() {
            return None;
        }
        let Subject::Issues(issues) = &self.subject else {
            return None;
        };
        let row = self.resume_issue()?;
        let record = issues.resumables.get(&row.issue_id)?.as_ref()?;
        Some((row, record))
    }

    /// EXP-696 (web `resumeWorktree`): the REMOTE resume offer — the target
    /// machine's synced `device_worktrees` row for the single checked issue.
    fn remote_resume(&self, cx: &App) -> bool {
        let Some(device) = self.remote_device() else {
            return false;
        };
        let Some(row) = self.resume_issue() else {
            return false;
        };
        let collections = Store::global(cx).collections();
        let worktrees = collections.device_worktrees.read(cx);
        queries::resume_worktree(
            worktrees.iter(),
            &device.row_id,
            &row.identifier,
            self.launch_ref().agent.id(),
        )
        .is_some()
    }

    fn resume_offered(&self, cx: &App) -> bool {
        self.resume_candidate().is_some() || self.remote_resume(cx)
    }

    /// Whether the launch will actually RESUME (switch on + a candidate).
    fn resume_active(&self, cx: &App) -> bool {
        let Subject::Issues(issues) = &self.subject else {
            return false;
        };
        issues.resume && self.resume_offered(cx)
    }

    // ── subject: action ───────────────────────────────────────────────────

    /// Refresh the actions list from the synced shape (EXP-268) and apply a
    /// pending seed once the shape is ready.
    fn refresh_actions(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.team_id.clone() else {
            self.actions = Vec::new();
            self.actions_ready = false;
            return;
        };
        let (actions, ready) = queries::team_actions(cx, &team_id);
        self.actions = actions;
        self.actions_ready = ready;
        if ready {
            if let Some(pending) = self.pending_action.take() {
                self.select_action(pending, cx);
            }
        }
        // A live edit to the selected action's binding may seed its repo.
        self.seed_action_repo_inputs();
    }

    /// EXP-862 — the action this composer is SEEDED with, for the lists that
    /// mark their own row: a pinned action row highlights while its run is
    /// being composed, and the rail's Agent entry then reads as not-active
    /// (web does the same off `?action=`). `None` for a chat or an issue
    /// subject.
    ///
    /// The pending seed counts: a play button navigates here with an action
    /// id that is only resolved once the `actions` shape has synced, and the
    /// row must light up on the click, not a beat later.
    pub(crate) fn active_action_id(&self) -> Option<&str> {
        match &self.subject {
            Subject::Action(subject) => Some(subject.action_id.as_str()),
            _ => self.pending_action.as_deref(),
        }
    }

    /// Pick an action: the subject becomes that ONE chip (issues, if any,
    /// are dropped — the swap rule), its picks reset, the seed's PR and
    /// icon applied.
    fn select_action(&mut self, action_id: String, cx: &mut gpui::Context<Self>) {
        let had_subject = !matches!(self.subject, Subject::None);
        self.probe_generation += 1;
        self.subject = Subject::Action(ActionSubject {
            action_id: action_id.clone(),
            picks: ActionInputPicks::default(),
        });
        if !had_subject {
            if let Some(launch) = self.launch.as_mut() {
                launch.reseed_plan_for_subject(true, cx);
            }
        }
        self.seed_action_repo_inputs();
        let pending_pr = self.pending_pr.take();
        let pending_icon = self.pending_icon.take();
        let Some(action) = self.selected_action().cloned() else {
            return;
        };
        let Subject::Action(subject) = &mut self.subject else {
            return;
        };
        if let Some(issue_id) = pending_pr {
            let options = crate::action_inputs::pr_pick_options(cx, &action.team_id);
            subject.picks.preselect_pr(&action, &issue_id, &options);
        }
        if let Some(icon) = pending_icon {
            subject.picks.preselect_icon(&action, &icon);
        }
        cx.notify();
    }

    /// The picked action's row, if the list holds it.
    fn selected_action(&self) -> Option<&api::actions::Action> {
        let Subject::Action(subject) = &self.subject else {
            return None;
        };
        self.actions
            .iter()
            .find(|action| action.id == subject.action_id)
    }

    /// EXP-349: seed the picked action's `repo` inputs from its binding.
    fn seed_action_repo_inputs(&mut self) {
        let Some(action) = self.selected_action().cloned() else {
            return;
        };
        let team_repos = self.team_repos.clone();
        if let Subject::Action(subject) = &mut self.subject {
            subject.picks.seed_repo_inputs(&action, &team_repos);
        }
    }

    // ── fetches: repos, MCP ───────────────────────────────────────────────

    /// EXP-822: the team's repositories for the Repository pick and the
    /// action repo inputs. One fetch per team; best-effort.
    fn ensure_repos_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        if self.team_id == self.repos_team {
            return;
        }
        self.repos_team = self.team_id.clone();
        self.team_repos = Vec::new();
        self.chat_repo = None;
        let (Some(team), Some(trpc)) = (self.team_id.clone(), queries::trpc_client(cx)) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move {
                    let rows = action_run::fetch_repositories(&trpc, &team)
                        .inspect_err(|err| log::debug!("[ui] repositories.list for chat: {err}"))
                        .ok()?;
                    Some((team, rows))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let Some((team, rows)) = loaded else {
                    return;
                };
                // A team switch under the fetch wins.
                if this.repos_team.as_deref() != Some(team.as_str()) {
                    return;
                }
                this.chat_repo = action_run::preselect_repo(&rows);
                this.team_repos = rows;
                this.seed_action_repo_inputs();
                cx.notify();
            });
        })
        .detach();
    }

    /// EXP-792: one `mcpServers.list` per team plus THIS machine's readiness.
    fn ensure_mcp_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        if self.team_id == self.mcp_team {
            return;
        }
        self.mcp_team = self.team_id.clone();
        self.mcp = None;
        let (Some(team), Some(trpc), Some(account)) = (
            self.team_id.clone(),
            queries::trpc_client(cx),
            queries::active_account(cx),
        ) else {
            return;
        };
        let data_dir = crate::session::AuthContext::global(cx).data_dir.clone();
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move {
                    crate::settings::mcp_servers::list_with_local_readiness(
                        &trpc, &team, &data_dir, &account.id,
                    )
                    .inspect_err(|err| log::debug!("[ui] mcpServers.list for chat: {err}"))
                    .ok()
                    .map(|loaded| (team, loaded))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let Some((team, loaded)) = loaded else {
                    return;
                };
                if this.mcp_team.as_deref() != Some(team.as_str()) {
                    return;
                }
                this.mcp = Some(loaded);
                cx.notify();
            });
        })
        .detach();
    }

    /// EXP-792: the team's servers as the pick offers them, resolved against
    /// the machine the composer currently targets.
    fn mcp_options(&self) -> Vec<launch_options::McpServerOption> {
        let Some((entries, local)) = self.mcp.as_ref() else {
            return Vec::new();
        };
        let now = chrono::Utc::now();
        let remote = self.remote_device();
        entries
            .iter()
            .map(|entry| {
                let (readiness, label) = match remote {
                    Some(device) => (
                        entry
                            .readiness
                            .iter()
                            .find(|row| row.device_id.as_deref() == Some(device.device_id.as_str()))
                            .map(crate::settings::mcp_servers::Readiness::from),
                        Some(device.label.as_str()),
                    ),
                    None => (
                        local
                            .iter()
                            .find(|row| row.server_id == entry.config.id)
                            .map(crate::settings::mcp_servers::Readiness::from),
                        None,
                    ),
                };
                launch_options::McpServerOption {
                    id: entry.config.id.clone(),
                    name: entry.config.name.clone(),
                    blocked: launch_options::mcp_block_reason(
                        &entry.config.auth,
                        readiness,
                        label,
                        now,
                    ),
                    enabled_by_default: entry.config.enabled_by_default,
                }
            })
            .collect()
    }

    /// EXP-792: the first PICKED server the target machine cannot satisfy.
    fn mcp_blocker(&self) -> Option<coding::McpBlocker> {
        let launch = self.launch_ref();
        let picked = launch.mcp_server_ids();
        launch
            .mcp_servers()
            .iter()
            .filter(|server| picked.iter().any(|id| id == &server.id))
            .find_map(|server| {
                server.blocked.clone().map(|reason| coding::McpBlocker {
                    server: server.name.clone(),
                    reason,
                })
            })
    }

    // ── device (EXP-696) ──────────────────────────────────────────────────

    /// Recompute the candidate machines and settle the pick
    /// ([`queries::settled_device`]). A pick that newly RESOLVES re-points
    /// the options cluster at that machine; one whose machine dropped out
    /// keeps everything as it was.
    fn settle_device(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.launch.is_none() {
            return;
        }
        self.device.devices = if self.team_id.is_some() {
            queries::launch_devices(cx)
        } else {
            Vec::new()
        };
        let next = queries::settled_device(
            &self.device.devices,
            self.device.requested.as_deref(),
            self.device.picked.as_deref(),
        );
        let label = next.as_deref().and_then(|id| {
            self.device
                .devices
                .iter()
                .find(|device| device.device_id == id)
                .map(|device| device.label.clone())
        });
        let resolved = label.is_some();
        if let Some(label) = label {
            self.device.label = Some(label);
        }
        let changed = next != self.device.device_id;
        self.device.device_id = next;
        let reseed = resolved && (changed || !self.device.resolved);
        self.device.resolved = resolved;
        if reseed {
            self.apply_device_defaults(window, cx);
        }
    }

    /// The PERSON's pick from the Device pin. It drops any pending ▶ request
    /// (EXP-836: a request is one-shot and never fights a human choice).
    fn set_device(&mut self, device_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.device.picked = Some(device_id.clone());
        self.device.requested = None;
        if self.device.device_id.as_deref() == Some(device_id.as_str()) && self.device.resolved {
            return;
        }
        let label = self
            .device
            .devices
            .iter()
            .find(|device| device.device_id == device_id)
            .map(|device| device.label.clone());
        self.device.resolved = label.is_some();
        if let Some(label) = label {
            self.device.label = Some(label);
        }
        self.device.device_id = Some(device_id);
        if self.device.resolved {
            self.apply_device_defaults(window, cx);
        }
    }

    /// Re-point the options cluster at the settled machine.
    fn apply_device_defaults(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let remote = self
            .remote_device()
            .map(|device| launch_options::RemoteDefaults {
                agents: device.agents.clone(),
                acp_agents: device.acp_agents.clone(),
                settings: device.defaults.clone(),
                accounts: device_agent_accounts(&device.row_id, cx),
            });
        let has_subject = !matches!(self.subject, Subject::None);
        let Some(launch) = self.launch.as_mut() else {
            return;
        };
        launch.set_remote(remote, window, cx);
        // A reseed off a machine's defaults must not re-enter plan mode
        // for a chat (EXP-772).
        launch.reseed_plan_for_subject(has_subject, cx);
    }

    /// EXP-836: why the machine a ▶ REQUESTED cannot take this run, for the
    /// options line — the run goes to the fallback machine, so saying which and
    /// why beats silently starting somewhere else
    /// ([`queries::requested_device_note`], web `deviceRequestNote`).
    fn device_request_note(&self, cx: &App) -> Option<SharedString> {
        let collections = Store::try_global(cx)?.collections().clone();
        let rows = collections.devices.read(cx);
        queries::requested_device_note(
            &self.device.devices,
            self.device.requested.as_deref(),
            rows.iter(),
            navigation::shapes_ready(cx),
            chrono::Utc::now().timestamp_millis(),
        )
        .map(SharedString::from)
    }

    /// EXP-696: the sticky pick whose machine has left the candidate list.
    fn offline_pick(&self) -> Option<&str> {
        if self.device.resolved || self.device.device_id.is_none() {
            return None;
        }
        Some(self.device.label.as_deref().unwrap_or("The selected device"))
    }

    fn selected_device(&self) -> Option<&queries::LaunchDevice> {
        let device_id = self.device.device_id.as_deref()?;
        self.device
            .devices
            .iter()
            .find(|device| device.device_id == device_id)
    }

    /// The settled machine when it is NOT this one — the remote-start route.
    fn remote_device(&self) -> Option<&queries::LaunchDevice> {
        self.selected_device().filter(|device| !device.is_own)
    }

    /// EXP-749/EXP-773: the target has the agent but cannot speak ACP with
    /// it — a muted line beside the blocker, never a blocker on its own.
    fn no_session_note(&self) -> Option<SharedString> {
        let device = self.remote_device()?;
        let agent = self.launch_ref().agent;
        launch_options::cannot_run_session(&device.acp_agents, agent).then(|| {
            format!("{} can't run {} sessions.", device.label, agent.label()).into()
        })
    }

    // ── the gate ──────────────────────────────────────────────────────────

    fn composer_placeholder(&self) -> SharedString {
        placeholder_for_subject(&self.subject, self.selected_action())
    }

    /// Re-hint the field when the subject (or the picked action's synced
    /// hint) changed — set only on a change so the input is not notified on
    /// every paint.
    fn sync_placeholder(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let next = self.composer_placeholder();
        if self.placeholder != next {
            self.placeholder = next.clone();
            self.input
                .update(cx, |state, cx| state.set_placeholder(next, window, cx));
        }
    }

    fn subject_kind(&self) -> SubjectKind {
        match &self.subject {
            Subject::None => SubjectKind::Chat,
            Subject::Issues(issues) => SubjectKind::Issues {
                count: issues.checked.len(),
            },
            Subject::Action(action) => SubjectKind::Action {
                id: action.action_id.clone(),
            },
        }
    }

    /// Why the submit is disabled right now; `None` = launchable. The order
    /// is the deleted dialog's: the machine, then the tooling, then the
    /// picked MCP servers, then the subject's own rules.
    fn launch_blocker(&self, cx: &mut App) -> Option<SharedString> {
        if self.launching {
            return Some("Starting…".into());
        }
        if self.sending {
            return Some("Uploading images…".into());
        }
        if self.team_id.is_none() {
            return Some("Sign in and wait for sync before starting a session.".into());
        }
        let Some(launch) = self.launch.as_ref() else {
            return Some("Checking local tools…".into());
        };
        if let Some(label) = self.offline_pick() {
            return Some(
                format!("{label} is offline — reconnect it or pick another device.").into(),
            );
        }
        // EXP-696: the LOCAL tooling gate applies to a local run only.
        match self.remote_device() {
            Some(device) => {
                if !device.agents.contains(&launch.agent) {
                    return Some(
                        format!("{} can't run {}.", device.label, launch.agent.label()).into(),
                    );
                }
            }
            None => {
                let gated_agent = match self.resume_active(cx) {
                    true => self
                        .resume_candidate()
                        .map(|(_, record)| record.agent)
                        .unwrap_or(launch.agent),
                    false => launch.agent,
                };
                let report = CodingHub::global_ref(cx).and_then(|hub| hub.read(cx).doctor.report.clone());
                match report.as_ref() {
                    None => return Some("Checking local tools…".into()),
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
        if let Some(blocker) = self.mcp_blocker() {
            return Some(blocker.to_string().into());
        }
        let text = self.input.read(cx).value().to_string();
        let kind = self.subject_kind();
        match &self.subject {
            Subject::None => chat_launch::text_blocker(&kind, &text, self.images.len()).map(Into::into),
            Subject::Action(subject) => {
                if !self.actions_ready {
                    return Some("Loading actions…".into());
                }
                let Some(action) = self.selected_action() else {
                    return Some("Select an action.".into());
                };
                if let Some(reason) = crate::action_inputs::unsupported_reason(action) {
                    return Some(reason.into());
                }
                if let Some(input) = subject.picks.missing_required(action) {
                    return Some(format!("Fill in {}.", input.label).into());
                }
                chat_launch::text_blocker(&kind, &text, self.images.len()).map(Into::into)
            }
            Subject::Issues(issues) => {
                if let Some(reason) = chat_launch::issue_count_blocker(issues.checked.len()) {
                    return Some(reason.into());
                }
                // EXP-202: only ONE session per issue — local registry and
                // live synced rows alike.
                let sessions = coding_flow::LocalSessions::global(cx);
                let store = Store::global(cx);
                let now = chrono::Utc::now().timestamp();
                for row in &issues.rows {
                    if !issues.checked.contains(&row.issue_id) {
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
                chat_launch::repo_blocker(&issues.rows, &issues.checked, &issues.repos).map(Into::into)
            }
        }
    }

    /// The launch options as picked. A RESUME never re-enters plan mode.
    fn options(&self, cx: &App) -> LaunchOptions {
        self.launch_ref().options(self.resume_active(cx), cx)
    }

    // ── send ──────────────────────────────────────────────────────────────

    /// The submit: upload the staged images to the TEAM route (sequential,
    /// idempotent), fold them into the steer message shape, then start.
    fn send(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.launch_blocker(cx).is_some() {
            return;
        }
        let text = self.input.read(cx).value().to_string();
        if self.images.is_empty() {
            self.start(text, window, cx);
            return;
        }
        let Some(team_id) = self.team_id.clone() else {
            return;
        };
        let Some(transport) = queries::attachment_transport(cx) else {
            self.notice = Some("Couldn't upload image".into());
            cx.notify();
            return;
        };
        let jobs = self.images.jobs();
        self.sending = true;
        self.notice = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move {
                    composer_images::upload_all(jobs, |filename, content_type, bytes| {
                        transport.upload_team_session_file(&team_id, filename, content_type, bytes)
                    })
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.sending = false;
                match outcome {
                    Ok(resolved) => {
                        this.images.note_uploaded(&resolved);
                        // The subject may have moved under the upload (an
                        // action pick cleared, a 31st issue ticked, a probe
                        // failed): the gate runs again, or the blocker is
                        // bypassed. The uploads stay noted for the retry.
                        if let Some(blocker) = this.launch_blocker(cx) {
                            this.notice = Some(blocker);
                            cx.notify();
                            return;
                        }
                        let ids: Vec<String> = resolved.into_iter().map(|(_, id)| id).collect();
                        let message = steer::build_steer_image_message(&text, &ids);
                        this.start(message, window, cx);
                    }
                    Err((resolved, error)) => {
                        // Keep what landed so a retry uploads only the rest.
                        this.images.note_uploaded(&resolved);
                        log::warn!("[ui] chat composer upload failed: {error}");
                        this.notice = Some("Couldn't upload image".into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Start the run for the composed `message` (text + embeds). Local
    /// subjects take the same rails the deleted dialog took; a remote target
    /// sends ONE `steer.startSession`.
    fn start(&mut self, message: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.team_id.clone() else {
            return;
        };
        let prompt = chat_launch::prompt_of(&message);
        let options = self.options(cx);
        if let Some(device) = self.remote_device() {
            let device_id = device.device_id.clone();
            let label = device.label.clone();
            let input = match &self.subject {
                Subject::None => chat_launch::remote_start_input(
                    &device_id,
                    &options,
                    RemoteSubject::Chat {
                        team_id: &team_id,
                        repository_id: self.chat_repo.as_ref().map(|repo| repo.id.as_str()),
                    },
                    prompt,
                ),
                Subject::Action(subject) => {
                    let Some(action) = self.selected_action() else {
                        return;
                    };
                    let inputs = subject.picks.collect(action);
                    chat_launch::remote_start_input(
                        &device_id,
                        &options,
                        RemoteSubject::Action {
                            action_id: &subject.action_id,
                            team_id: &team_id,
                            inputs: &inputs,
                        },
                        prompt,
                    )
                }
                Subject::Issues(issues) => {
                    let mut checked: Vec<String> = issues
                        .rows
                        .iter()
                        .filter(|row| issues.checked.contains(&row.issue_id))
                        .map(|row| row.issue_id.clone())
                        .collect();
                    let subject = match checked.len() {
                        0 => return,
                        1 => RemoteSubject::Issue {
                            issue_id: &checked.pop().expect("one checked"),
                            resume: self.resume_active(cx),
                        }
                        .into_owned(),
                        _ => RemoteSubject::Batch { issue_ids: checked }.into_owned(),
                    };
                    chat_launch::remote_start_input(&device_id, &options, subject.borrow(), prompt)
                }
            };
            return self.launch_remote(input, label, window, cx);
        }
        match &self.subject {
            Subject::None => {
                let Some(host) = crate::session_bar::host_for_window(window, cx) else {
                    self.error = Some("Open a team window to start a chat.".into());
                    cx.notify();
                    return;
                };
                let repo = self
                    .chat_repo
                    .as_ref()
                    .map(|repo| (repo.id.clone(), repo.full_name.clone()));
                host.update(cx, |host, cx| {
                    host.launch_chat_run(options, repo, prompt, window, cx);
                });
                self.after_started(window, cx);
            }
            Subject::Action(subject) => {
                let Some(action) = self.selected_action().cloned() else {
                    return;
                };
                let inputs = subject.picks.collect(&action);
                action_run::start_action_run(
                    StartActionArgs {
                        action_id: action.id,
                        team_id,
                        repo: ActionRepo::Resolve,
                        options,
                        origin: LaunchOrigin::Local,
                        inputs,
                        target: Some(window.window_handle()),
                        activate_app: false,
                        reservation: None,
                        // A person pressed Run — never an automation firing.
                        trigger: None,
                        automation_id: None,
                        on_settled: None,
                        prompt,
                    },
                    cx,
                );
                self.after_started(window, cx);
            }
            Subject::Issues(issues) => {
                if issues.checked.len() == 1 {
                    let issue_id = issues.checked.iter().next().cloned().expect("one checked");
                    // EXP-662: an active resume relaunches the RECORDED run
                    // exactly; only model/effort may be nudged, and only
                    // while the pick sits on that same agent (D2).
                    let record = self
                        .resume_active(cx)
                        .then(|| self.resume_candidate().map(|(_, record)| record.clone()))
                        .flatten();
                    if let Some(record) = record {
                        let same_agent = options.agent == record.agent;
                        let Some(deps) = coding_flow::build_resume_deps(&record, cx) else {
                            self.error = Some("Sign in and wait for sync before starting a session.".into());
                            cx.notify();
                            return;
                        };
                        let request = ResumeRunRequest {
                            record,
                            device_label: coding::default_device_label(),
                            origin: LaunchOrigin::Local,
                            model: same_agent.then(|| options.model.clone()),
                            effort: same_agent.then(|| options.effort.clone()),
                            prompt,
                            // EXP-849: the composer's account pick reaches a
                            // resume too — picking another account on a
                            // resumable issue continues it there.
                            account: same_agent.then(|| options.account.clone()).flatten(),
                        };
                        return self.run_prepare(
                            PrepareRequest::ResumeRun(request),
                            deps,
                            SessionSubject::Issue(issue_id),
                            window,
                            cx,
                        );
                    }
                    let Some((request, deps)) = coding_flow::build_launch(
                        &issue_id,
                        LaunchOrigin::Local,
                        options,
                        false,
                        prompt,
                        cx,
                    ) else {
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
                let Some(request) = chat_launch::batch_request(
                    &team_id,
                    &issues.rows,
                    &issues.checked,
                    &issues.repos,
                    options,
                    prompt,
                ) else {
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
        }
    }

    /// EXP-696: hand the run to another machine. Success clears the composer
    /// and says where the run went; a refusal renders in the error slot.
    ///
    /// EXP-818: it then FOLLOWS the run in — a remote start lands in the
    /// session screen exactly as a local one does
    /// ([`coding_flow::follow_remote_start`], which waits for the row the
    /// other machine writes). The toast stays: it names the machine, which is
    /// the one thing the screen itself does not announce.
    fn launch_remote(
        &mut self,
        input: api::steer::StartSessionInput,
        device_label: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(trpc) = queries::trpc_client(cx) else {
            self.error = Some("Sign in and wait for sync before starting a session.".into());
            cx.notify();
            return;
        };
        let device_id = input.device_id.clone();
        let subject = coding_flow::RemoteRunSubject::of(&input);
        self.launching = true;
        self.error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::steer::start_session(&trpc, &input) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.launching = false;
                match result {
                    Ok(()) => {
                        window.push_notification(
                            Notification::success(SharedString::from(format!(
                                "Start sent to {device_label}."
                            ))),
                            cx,
                        );
                        coding_flow::follow_remote_start(device_id, subject, window, cx);
                        this.after_started(window, cx);
                    }
                    Err(err) => this.error = Some(err.user_message().into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Shared prepare→spawn tail: background [`coding::prepare`], then
    /// `coding_flow::spawn_into_window` on THIS window; a `Disabled` reason
    /// (or spawn error) renders inline and keeps the draft.
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
        cx.spawn_in(window, async move |this, cx| {
            let prepared = cx
                .background_executor()
                .spawn(async move { coding::prepare(&request, &deps) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.launching = false;
                let outcome: Result<(), SharedString> = match prepared {
                    Ok(Prepared::Ready(prepared)) => {
                        coding_flow::spawn_into_window(prepared, subject, window, cx)
                            .map_err(SharedString::from)
                    }
                    Ok(Prepared::Disabled(reason)) => Err(reason.message().into()),
                    Err(err) => Err(format!("Could not start the coding session: {err}").into()),
                };
                match outcome {
                    Ok(()) => this.after_started(window, cx),
                    Err(message) => this.error = Some(message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The run is on its way: clear the draft, the images and the subject.
    /// Navigating into the run is NOT this function's job — a local launch
    /// lands there from `coding_flow::spawn_into_window`, a remote one from
    /// `coding_flow::follow_remote_start` once the other machine's row syncs
    /// (EXP-818: both paths open the session screen).
    fn after_started(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.images.clear();
        self.notice = None;
        self.error = None;
        self.clear_subject(cx);
    }

    // ── images ────────────────────────────────────────────────────────────

    fn stage_images(
        &mut self,
        images: Vec<composer_images::StagedFile>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.notice = self.images.stage(images, &self.input, window, cx);
        cx.notify();
    }

    fn remove_image(&mut self, key: u64, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.images.remove(key, &self.input, window, cx);
        cx.notify();
    }

    fn on_paste(
        &mut self,
        _: &gpui_component::input::Paste,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let images = composer_images::clipboard_images(cx);
        if images.is_empty() {
            return;
        }
        cx.stop_propagation();
        self.stage_images(images, window, cx);
    }

    // ── render pieces ─────────────────────────────────────────────────────

    /// The subject chips: one per checked issue, or the action's one.
    fn render_chips(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let muted = cx.theme().muted_foreground;
        let mut chips: Vec<AnyElement> = Vec::new();
        match &self.subject {
            Subject::None => return None,
            Subject::Issues(issues) => {
                for (ix, row) in issues
                    .rows
                    .iter()
                    .filter(|row| issues.checked.contains(&row.issue_id))
                    .enumerate()
                {
                    let issue_id = row.issue_id.clone();
                    chips.push(
                        glass_pill(("chat-chip-issue", ix), PillSize::Sm, PillMode::Readonly, cx)
                            .child(
                                div()
                                    .text_xs()
                                    .font_family(theme::terminal::FONT_FAMILY)
                                    .child(SharedString::from(row.identifier.clone())),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .max_w(px(220.))
                                    .truncate()
                                    .text_color(muted)
                                    .child(SharedString::from(row.title.clone())),
                            )
                            .child(
                                crate::composer::composer_tool(
                                    ("chat-chip-issue-remove", ix),
                                    registry::UI_CLOSE,
                                    cx,
                                )
                                .tooltip("Remove")
                                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                    this.toggle_issue(issue_id.clone(), false, window, cx);
                                })),
                            )
                            .into_any_element(),
                    );
                }
            }
            Subject::Action(subject) => {
                let (name, icon) = match self.selected_action() {
                    Some(action) => (action.name.clone(), action.icon.clone()),
                    None => (
                        api::actions::builtin_action_name(&subject.action_id)
                            .unwrap_or("Action")
                            .to_string(),
                        api::actions::builtin_action_icon(&subject.action_id).map(str::to_string),
                    ),
                };
                chips.push(
                    glass_pill("chat-chip-action", PillSize::Sm, PillMode::Readonly, cx)
                        .child(crate::icons::action_icon(icon.as_deref()).xsmall().text_color(muted))
                        .child(div().text_xs().child(SharedString::from(name)))
                        .child(
                            crate::composer::composer_tool(
                                "chat-chip-action-remove",
                                registry::UI_CLOSE,
                                cx,
                            )
                            .tooltip("Remove")
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.clear_subject(cx);
                            })),
                        )
                        .into_any_element(),
                );
            }
        }
        Some(
            h_flex()
                .w_full()
                .min_w_0()
                .flex_wrap()
                .gap_1()
                .px_1()
                .children(chips)
                .into_any_element(),
        )
    }

    /// The picked action's remaining typed inputs (repo/board/pr/icon).
    fn render_action_fields(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let Subject::Action(subject) = &self.subject else {
            return None;
        };
        let action = self.selected_action()?.clone();
        if action.inputs.is_empty() {
            return None;
        }
        let team_id = action.team_id.clone();
        let mut fields = v_flex().w_full().gap_2().px_1().py_1();
        for (ix, input) in action.inputs.iter().enumerate() {
            fields = fields.child(subject.picks.render_field(
                "chat-input",
                ix,
                input,
                &team_id,
                &self.team_repos,
                Self::picks_access,
                cx,
            ));
        }
        Some(fields.into_any_element())
    }

    /// The `#` tool: the issue picker popover.
    fn issue_tool(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let (rows, checked, notes): (Vec<IssueRow>, HashSet<String>, Vec<(String, SharedString)>) =
            match &self.subject {
                Subject::Issues(issues) => (
                    issues.rows.clone(),
                    issues.checked.clone(),
                    issues
                        .rows
                        .iter()
                        .filter_map(|row| {
                            let note: SharedString = match issues.repos.get(&row.issue_id)? {
                                RepoState::Ready(None) => "no repository linked".into(),
                                RepoState::Error(err) => {
                                    format!("repository check failed: {err}").into()
                                }
                                RepoState::Loading if issues.checked.contains(&row.issue_id) => {
                                    "resolving repository…".into()
                                }
                                _ => return None,
                            };
                            Some((row.issue_id.clone(), note))
                        })
                        .collect(),
                ),
                _ => (
                    self.team_id
                        .as_deref()
                        .map(|team| issue_picker::snapshot_rows(cx, team, &HashSet::new()))
                        .unwrap_or_default(),
                    HashSet::new(),
                    Vec::new(),
                ),
            };
        let trigger = crate::composer::composer_tool("chat-tool-issues", registry::EDITOR_ISSUE_REF, cx)
            .tooltip("Pick issues");
        issue_picker::issue_picker_popover(
            trigger,
            &rows,
            &checked,
            &self.issue_search,
            notes,
            Self::toggle_issue,
            cx,
        )
        .into_any_element()
    }

    /// The ▶ tool: the actions popover (builtins pinned first, Create
    /// action included; Chat is never listed).
    fn action_tool(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let actions: Vec<(String, String, Option<String>, Option<String>)> = self
            .actions
            .iter()
            .filter(|action| action.id != api::actions::BUILTIN_CHAT_ID)
            .map(|action| {
                (
                    action.id.clone(),
                    action.name.clone(),
                    action.icon.clone(),
                    action
                        .description
                        .as_deref()
                        .map(str::trim)
                        .filter(|text| !text.is_empty())
                        .map(str::to_string),
                )
            })
            .collect();
        let ready = self.actions_ready;
        let picked = match &self.subject {
            Subject::Action(subject) => Some(subject.action_id.clone()),
            _ => None,
        };
        let view = cx.entity().downgrade();
        let trigger = crate::composer::composer_tool("chat-tool-actions", registry::ACTION_RUN, cx)
            .tooltip("Run an action");
        Popover::new("chat-action-picker")
            .p_1()
            .trigger(trigger)
            .content(move |_, _window, cx| {
                let muted = cx.theme().muted_foreground;
                let mut rows = v_flex()
                    .id("chat-action-picker-rows")
                    .w(px(360.))
                    .max_h(px(360.))
                    .overflow_y_scroll();
                if !ready {
                    rows = rows.child(issue_picker::list_note("Loading actions…", cx));
                } else if actions.is_empty() {
                    rows = rows.child(issue_picker::list_note("No actions yet.", cx));
                }
                for (id, name, icon, description) in &actions {
                    let is_picked = picked.as_deref() == Some(id.as_str());
                    let view = view.clone();
                    let id = id.clone();
                    rows = rows.child(
                        crate::pickers::picker_row(SharedString::from(format!("chat-action-{id}")), cx)
                            .child(
                                Icon::new(if is_picked {
                                    registry::UI_SELECTED
                                } else {
                                    registry::UI_UNSELECTED
                                })
                                .small()
                                .text_color(muted),
                            )
                            .child(crate::icons::action_icon(icon.as_deref()).xsmall().text_color(muted))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .child(div().text_sm().truncate().child(SharedString::from(name.clone())))
                                    .children(description.clone().map(|text| {
                                        div().text_xs().truncate().text_color(muted).child(SharedString::from(text))
                                    })),
                            )
                            .on_click(move |_, _, cx| {
                                if let Some(view) = view.upgrade() {
                                    let id = id.clone();
                                    view.update(cx, |this, cx| this.select_action(id, cx));
                                }
                            }),
                    );
                }
                rows
            })
            .into_any_element()
    }

    /// The Device pin: this machine first, then the online remote ones. A
    /// single candidate reads as a label; an offline sticky pick keeps the
    /// menu so the run can be re-pointed.
    fn device_pin(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let offline = self.offline_pick();
        let label = match offline {
            Some(label) => format!("{label} — offline"),
            None => self
                .selected_device()
                .map(|device| device.label.clone())
                .unwrap_or_else(|| "This device".to_string()),
        };
        let candidates = self.device.devices.clone();
        if candidates.len() < 2 && offline.is_none() {
            // EXP-862: no menu with one device, but the kind glyph still
            // leads the value (web `InlinePicker`'s one-option arm).
            let kind = self
                .device
                .device_id
                .as_deref()
                .map(|id| device_kind_icon(id, cx))
                .unwrap_or(registry::UI_DEVICE);
            return h_flex()
                .px_1()
                .gap_1()
                .items_center()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(Icon::new(kind).size(gpui::px(12.)))
                .child(SharedString::from(label))
                .into_any_element();
        }
        let bound = self.device.device_id.clone();
        // EXP-862: the machine's KIND leads the trigger AND every row — a
        // picker whose value wears an icon offers that icon on its items.
        let kinds: HashMap<String, crate::icons::ExpIcon> = candidates
            .iter()
            .map(|device| (device.device_id.clone(), device_kind_icon(&device.device_id, cx)))
            .collect();
        let selected_kind = self
            .device
            .device_id
            .as_deref()
            .and_then(|id| kinds.get(id).cloned())
            .unwrap_or(registry::UI_DEVICE);
        let view = cx.entity().downgrade();
        crate::launch_options::inline_pin_trigger_with(
            "chat-pin-device".into(),
            Some(selected_kind),
            label,
            cx,
        )
        .dropdown_menu(move |mut menu, _window, _cx| {
            for device in &candidates {
                let view = view.clone();
                let device_id = device.device_id.clone();
                let kind = kinds
                    .get(&device_id)
                    .cloned()
                    .unwrap_or(registry::UI_DEVICE);
                menu = menu.item(
                    PopupMenuItem::new(SharedString::from(device.label.clone()))
                        .icon(Icon::new(kind))
                        .checked(bound.as_deref() == Some(device_id.as_str()))
                        .on_click(move |_, window, cx| {
                            if let Some(view) = view.upgrade() {
                                let device_id = device_id.clone();
                                view.update(cx, |this, cx| {
                                    this.set_device(device_id, window, cx);
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

    /// EXP-822: the Repository pin (no-subject chats only). "No repository"
    /// is a real entry, so a pick can always be walked back.
    fn repo_pin(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let repos = self.team_repos.clone();
        let label = match &self.chat_repo {
            Some(repo) => repo.full_name.clone(),
            None => NO_REPO_LABEL.to_string(),
        };
        let picked = self.chat_repo.as_ref().map(|repo| repo.id.clone());
        let view = cx.entity().downgrade();
        inline_pin_trigger("chat-pin-repo".into(), label, cx)
            .dropdown_menu(move |mut menu, _window, _cx| {
                let none_view = view.clone();
                menu = menu.item(
                    PopupMenuItem::new(NO_REPO_LABEL)
                        .checked(picked.is_none())
                        .on_click(move |_, _, cx| {
                            if let Some(view) = none_view.upgrade() {
                                view.update(cx, |view, cx| {
                                    view.chat_repo = None;
                                    cx.notify();
                                });
                            }
                        }),
                );
                for repo in &repos {
                    let view = view.clone();
                    let repo = repo.clone();
                    let checked = picked.as_deref() == Some(repo.id.as_str());
                    menu = menu.item(
                        PopupMenuItem::new(repo.full_name.clone())
                            .checked(checked)
                            .on_click(move |_, _, cx| {
                                let repo = repo.clone();
                                if let Some(view) = view.upgrade() {
                                    view.update(cx, |view, cx| {
                                        view.chat_repo = Some(repo);
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

    /// EXP-202/EXP-662: the Resume switch, inline while a single checked
    /// issue has a resumable run here or a worktree on the target machine.
    fn resume_switch(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let Subject::Issues(issues) = &self.subject else {
            return None;
        };
        if !self.resume_offered(cx) {
            return None;
        }
        let muted = cx.theme().muted_foreground;
        let hint: SharedString = match self.resume_candidate() {
            Some((_, record)) => format!(
                "Resumes the {} session exactly (its own transcript); a resume keeps the session's own agent.",
                record.agent.label()
            )
            .into(),
            None => "Resumes the worktree still on that machine.".into(),
        };
        Some(
            h_flex()
                .gap_1p5()
                .items_center()
                .px_1()
                .text_xs()
                .text_color(muted)
                .child("Resume")
                .child(
                    crate::controls::web_switch("chat-resume")
                        .checked(issues.resume)
                        .tooltip(hint)
                        .on_click(cx.listener(|this, on: &bool, _, cx| {
                            if let Subject::Issues(issues) = &mut this.subject {
                                issues.resume = *on;
                            }
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    /// Options row B: Device · Agent · Model (· Account) · Plan (· Resume ·
    /// Repository) · ⋯, then the unfolded Effort · Ultracode · MCP line.
    fn render_options_row(&mut self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let has_launch = self.launch.is_some();
        let mut row = h_flex()
            .w_full()
            .min_w_0()
            .flex_wrap()
            .gap_1()
            .items_center()
            .px_1()
            .text_xs()
            .text_color(muted)
            .child(self.device_pin(cx));
        if has_launch {
            row = row
                .child(self.launch_ref().agent_pin("chat", Self::launch_access, cx))
                .child(self.launch_ref().model_pin("chat", Self::launch_access, cx))
                // EXP-862: WHICH ACCOUNT a run spends is a first-row decision
                // wherever there is a decision to make — the pin renders only
                // when the selected machine reports two or more profiles for
                // the selected agent, so it is never a dead row.
                .children(self.launch_ref().account_pin("chat", Self::launch_access, cx))
                .children(self.launch_ref().plan_toggle("chat", Self::launch_access, cx));
        }
        row = row.children(self.resume_switch(cx));
        if matches!(self.subject, Subject::None) && !self.team_repos.is_empty() {
            row = row.child(self.repo_pin(cx));
        }
        if has_launch {
            let more = self.more_open;
            row = row.child(
                launch_options::inline_icon_trigger("chat-pin-more".into(), registry::UI_MORE, cx)
                    .tooltip(if more { "Fewer options" } else { "More options" })
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.more_open = !this.more_open;
                        cx.notify();
                    })),
            );
        }
        let mut column = v_flex().w_full().min_w_0().gap_1().child(row);
        if has_launch && self.more_open {
            let launch = self.launch_ref();
            column = column.child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .flex_wrap()
                    .gap_1()
                    .items_center()
                    .px_1()
                    .text_xs()
                    .text_color(muted)
                    .child(div().px_1().child("Effort"))
                    .child(launch.effort_pin("chat", Self::launch_access, cx))
                    .children(launch.ultracode_toggle("chat", Self::launch_access, cx))
                    .children(launch.mcp_pin("chat", Self::launch_access, cx)),
            );
        }
        column.into_any_element()
    }

    /// EXP-790: the suggestion chips, shown over the EMPTY, subject-less
    /// field only. A click inserts the text through the mention widget and
    /// parks the caret after the suggestion's first `#`
    /// ([`MentionInput::insert_suggestion`]), so the issue picker opens
    /// exactly as typing the token would — wherever in the sentence it sits.
    fn render_suggestions(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        if !matches!(self.subject, Subject::None) || !self.input.read(cx).value().trim().is_empty() {
            return None;
        }
        let chips = self.suggestions.iter().enumerate().map(|(index, &pick)| {
            let text: &'static str = CHAT_SUGGESTIONS[pick];
            glass_pill(("chat-suggestion", index), PillSize::Sm, PillMode::Action, cx)
                .cursor_pointer()
                .child(div().text_xs().child(text))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.mention
                        .update(cx, |mention, cx| mention.insert_suggestion(text, window, cx));
                    cx.notify();
                }))
        });
        Some(
            h_flex()
                .w_full()
                .min_w_0()
                .flex_wrap()
                .gap_1()
                .px_1()
                .children(chips)
                .into_any_element(),
        )
    }
}

impl Focusable for ChatScreenView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ChatScreenView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.sync_team(window, cx);
        self.ensure_launch(window, cx);
        self.sync_placeholder(window, cx);
        // The seed: a play button (or the dev route) may have navigated here
        // with one. It is consumed only once the team is known and the
        // shapes have synced — a seed taken on the first paint of a cold
        // start would resolve no issue rows and be lost.
        if self.team_id.is_some() && navigation::shapes_ready(cx) {
            if let Some(seed) = navigation::take_pending_chat_seed(&self.nav, cx) {
                self.apply_seed(seed, window, cx);
            }
        }
        let mcp = self.mcp_options();
        if let Some(launch) = self.launch.as_mut() {
            launch.set_mcp_servers(mcp);
        }

        let blocker = self.launch_blocker(cx);
        let no_session_note = self.no_session_note();
        let request_note = self.device_request_note(cx);
        // EXP-827: ICON-ONLY — the ONE round send of `composer::glass_composer`
        // (the steer composer's button, same ring, same 32px hit box). The
        // label the web `submitLabel` mirrors is the TOOLTIP now: the composer
        // card is the page's only control, so a word beside the arrow only
        // repeated what the chips above it already say.
        let label = chat_launch::submit_label(&self.subject_kind());
        let submit = crate::composer::composer_submit(
            "chat-send",
            registry::UI_SUBMIT,
            blocker.is_some(),
            cx,
        )
        .tooltip(SharedString::from(label))
        .loading(self.launching || self.sending)
        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.send(window, cx)));
        let chips = self.render_chips(cx);
        let fields = self.render_action_fields(cx);
        let leading = match (chips, fields) {
            (None, None) => None,
            (chips, fields) => Some(
                v_flex()
                    .w_full()
                    .min_w_0()
                    .gap_1()
                    .children(chips)
                    .children(fields)
                    .into_any_element(),
            ),
        };
        let strip = (!self.images.is_empty()).then(|| {
            self.images
                .render_strip("chat-pending-remove", self.sending, Self::remove_image, cx)
        });
        let mut composer = crate::composer::GlassComposer::new(
            div()
                .w_full()
                .min_w_0()
                .child(self.mention.clone())
                .into_any_element(),
        )
        .strip(strip)
        .tool(self.issue_tool(cx))
        .tool(self.action_tool(cx))
        .tool(
            // EXP-850 §13: the steer composers attach with the `ui-add` plus
            // ×4 — `editor-image` stays the comment/description editors'.
            crate::composer::composer_tool("chat-tool-attach", registry::UI_ADD, cx)
                .tooltip("Attach images")
                .disabled(self.sending)
                .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                    composer_images::pick_image_files(window, cx, |this, read, window, cx| {
                        this.stage_images(read, window, cx)
                    });
                })),
        )
        .submit(submit);
        if let Some(leading) = leading {
            composer = composer.leading(leading);
        }
        let suggestions = self.render_suggestions(cx);
        let options = self.render_options_row(cx);
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let mut notes = v_flex().w_full().min_w_0().gap_0p5().px_1().text_xs();
        if let Some(notice) = &self.notice {
            notes = notes.child(div().text_color(muted).child(notice.clone()));
        }
        // EXP-836: the ▶ named a machine this run cannot go to. It is a WARNING,
        // not a blocker — the fallback machine takes the run (web parity).
        if let Some(note) = request_note {
            notes = notes.child(div().text_color(cx.theme().warning).child(note));
        }
        // EXP-862: the blocker still disables the send — only the NOTE is
        // filtered, and the one it drops is the empty composer telling the
        // reader to type into the composer they are looking at.
        let blocker_note = blocker
            .filter(|_| !self.launching && !self.sending)
            .filter(|reason| chat_launch::note_for_blocker(Some(reason.as_ref())).is_some());
        if let Some(reason) = blocker_note {
            // EXP-862: the blocker is a sentence and nothing else — the
            // "Sign in to <agent>" pill it used to carry is gone ×4; a login
            // is offered ONCE, on the account chip that owns it (Devices).
            notes = notes.child(div().w_full().min_w_0().text_color(muted).child(reason));
        }
        if let Some(note) = no_session_note {
            notes = notes.child(div().text_color(muted).child(note));
        }
        if let Some(error) = &self.error {
            notes = notes.child(div().text_color(danger).child(error.clone()));
        }
        // EXP-851: composer on top, the Running/Past session rows beneath it,
        // ONE scroll. The composer keeps its centred max-width column; the
        // sections share it so the page reads as one stack rather than the
        // retired centre-plus-list split.
        // EXP-862: with nothing running and nothing past, the composer is the
        // whole page — so it sits in the MIDDLE of it (web `justify-center`),
        // not pinned under the top edge above two empty bands.
        let only_composer =
            self.sessions_running.read(cx).is_empty() && self.sessions_past.read(cx).is_empty();
        v_flex()
            .size_full()
            .min_h_0()
            .track_focus(&self.focus_handle)
            .child(crate::scroll_pane::v_scroll_pane(
                "chat-page-scroll",
                &self.page_scroll,
                v_flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .p_6()
                    .gap_6()
                    .when(only_composer, |column| {
                        column.min_h_full().justify_center()
                    })
                    // Both stacks must NOT shrink: inside the scroll column a
                    // flex-shrinkable child gets squeezed to the viewport and
                    // its trailing rows (options, the blocker note) painted
                    // under the Running band that follows.
                    .child(
                        v_flex()
                            .w_full()
                            .max_w(px(PROMPT_MAX_W))
                            .min_w_0()
                            .flex_shrink_0()
                            .gap_2()
                            .children(suggestions)
                            .child(
                                crate::composer::glass_composer(composer)
                                    .capture_action(cx.listener(Self::on_paste)),
                            )
                            .child(options)
                            .child(notes),
                    )
                    .child(
                        v_flex()
                            .w_full()
                            .max_w(px(PROMPT_MAX_W))
                            .min_w_0()
                            .flex_shrink_0()
                            .child(self.sessions_running.clone())
                            .child(self.sessions_past.clone()),
                    ),
            ))
    }
}

/// [`RemoteSubject`] borrows the ids it names; the issue arms need an owned
/// carrier so the checked list can be built inside `start` and borrowed
/// afterwards.
enum OwnedRemoteSubject {
    Issue { issue_id: String, resume: bool },
    Batch { issue_ids: Vec<String> },
}

impl OwnedRemoteSubject {
    fn borrow(&self) -> RemoteSubject<'_> {
        match self {
            OwnedRemoteSubject::Issue { issue_id, resume } => RemoteSubject::Issue {
                issue_id,
                resume: *resume,
            },
            OwnedRemoteSubject::Batch { issue_ids } => RemoteSubject::Batch {
                issue_ids: issue_ids.clone(),
            },
        }
    }
}

impl RemoteSubject<'_> {
    fn into_owned(self) -> OwnedRemoteSubject {
        match self {
            RemoteSubject::Issue { issue_id, resume } => OwnedRemoteSubject::Issue {
                issue_id: issue_id.to_string(),
                resume,
            },
            RemoteSubject::Batch { issue_ids } => OwnedRemoteSubject::Batch { issue_ids },
            _ => unreachable!("only the issue arms are built here"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-790/EXP-820: the pool is the web page's `CHAT_SUGGESTIONS`
    /// (`lib/chat-suggestions.ts`), byte for byte and in the same order.
    #[test]
    fn chat_suggestions_mirror_the_web_page() {
        assert_eq!(
            CHAT_SUGGESTIONS,
            [
                "Fix #",
                "Explain #",
                "Review #",
                "Split # into sub-issues",
                "Label every issue in the backlog",
                "Set a priority on every unprioritized issue",
                "Find duplicate issues and link them",
                "Do a code review of the open PRs and file the findings on a new board",
                "Create an automation that labels new issues",
                "Set up a weekly standup digest automation",
                "Draft release notes from the issues completed this month",
                "Summarize what changed across the boards this week",
                "Start a session for # on my other machine",
                "Move stale in-progress issues back to the backlog",
                "Comment a plan on #",
                "Which issues are blocked, and by what?",
            ]
        );
    }

    /// EXP-820: a page's draw is four DISTINCT pool entries, whatever the
    /// seed (including the degenerate zero).
    #[test]
    fn a_page_draws_four_distinct_suggestions() {
        for seed in [0u64, 1, 42, u64::MAX, suggestion_seed()] {
            let picks = pick_chat_suggestions(seed);
            assert_eq!(picks.len(), CHAT_SUGGESTION_COUNT, "seed {seed}");
            let mut sorted = picks.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), CHAT_SUGGESTION_COUNT, "seed {seed}: {picks:?}");
            for pick in picks {
                assert!(
                    pick < CHAT_SUGGESTIONS.len(),
                    "seed {seed}: index {pick} outside the pool"
                );
            }
        }
        // Different seeds do reach different draws — it is a shuffle, not a
        // fixed prefix.
        let draws: std::collections::HashSet<Vec<usize>> =
            (1..64u64).map(|seed| pick_chat_suggestions(seed * 7919)).collect();
        assert!(draws.len() > 1);
    }

    /// EXP-822: the Repository pin seeds itself. One connected repo is not a
    /// choice, so it lands pre-picked and the chat gets a worktree; with
    /// several the page stays repo-less on purpose and the run's prompt makes
    /// the agent ASK which one instead of hunting for a clone on disk.
    #[test]
    fn one_repo_preselects_and_several_stay_repo_less() {
        let row = |id: &str, full_name: &str| ActionRepoRow {
            id: id.to_string(),
            full_name: full_name.to_string(),
            default_branch: None,
        };
        let only = action_run::preselect_repo(&[row("repo-1", "niach/exponential")]);
        assert_eq!(only.map(|repo| repo.id), Some("repo-1".to_string()));
        assert!(action_run::preselect_repo(&[]).is_none());
        assert!(action_run::preselect_repo(&[
            row("repo-1", "niach/exponential"),
            row("repo-2", "niach/other"),
        ])
        .is_none());
        // The pin's repo-less entry reads the same as the web page's.
        assert_eq!(NO_REPO_LABEL, "No repository");
    }

    /// EXP-825: the field's hint follows the subject like the web
    /// `composerPlaceholder`: a chat asks, a picked subject takes extra
    /// instructions, and a picked action with a non-blank
    /// `prompt_placeholder` shows that instead — the Create-action builtin's
    /// own hint included; a blank hint, an unlisted action or an issue
    /// subject fall back.
    #[test]
    fn composer_placeholder_follows_the_subject_and_the_actions_hint() {
        assert_eq!(CHAT_PLACEHOLDER, "Ask the agent…");
        assert_eq!(SUBJECT_PLACEHOLDER, "Additional instructions (optional)…");
        let action_subject = |id: &str| {
            Subject::Action(ActionSubject {
                action_id: id.to_string(),
                picks: ActionInputPicks::default(),
            })
        };
        let mut action = api::actions::builtin_fix_conflicts_action("team-1");
        action.id = "act-1".to_string();

        // No subject: the chat hint, whatever row is offered.
        assert_eq!(
            placeholder_for_subject(&Subject::None, Some(&action)),
            CHAT_PLACEHOLDER
        );
        // A picked action without a hint: the generic subject hint.
        assert_eq!(
            placeholder_for_subject(&action_subject("act-1"), Some(&action)),
            SUBJECT_PLACEHOLDER
        );
        // The action's hint wins, trimmed.
        action.prompt_placeholder = Some("  Scope: which platforms, which version  ".to_string());
        assert_eq!(
            placeholder_for_subject(&action_subject("act-1"), Some(&action)),
            "Scope: which platforms, which version"
        );
        // A blank hint is no hint.
        action.prompt_placeholder = Some("   ".to_string());
        assert_eq!(
            placeholder_for_subject(&action_subject("act-1"), Some(&action)),
            SUBJECT_PLACEHOLDER
        );
        // An action the list doesn't hold (no row) falls back too.
        assert_eq!(
            placeholder_for_subject(&action_subject("act-1"), None),
            SUBJECT_PLACEHOLDER
        );
        // The Create-action builtin's hint now comes from its OWN field —
        // the old special case, byte for byte.
        let create = api::actions::builtin_create_action("team-1");
        assert_eq!(
            placeholder_for_subject(
                &action_subject(api::actions::BUILTIN_CREATE_ACTION_ID),
                Some(&create)
            ),
            "Describe the action — what it should do, and its name if you have one…"
        );
        // An issue subject never reads an action hint.
        let issues = Subject::Issues(IssueSubject {
            rows: Vec::new(),
            checked: HashSet::new(),
            repos: HashMap::new(),
            resumables: HashMap::new(),
            resume: false,
        });
        assert_eq!(
            placeholder_for_subject(&issues, Some(&action)),
            SUBJECT_PLACEHOLDER
        );
    }

    /// The issue arms of the remote subject round-trip through the owned
    /// carrier the start path needs.
    #[test]
    fn owned_remote_subject_round_trips() {
        let issue = RemoteSubject::Issue {
            issue_id: "i-1",
            resume: true,
        }
        .into_owned();
        assert_eq!(
            issue.borrow(),
            RemoteSubject::Issue {
                issue_id: "i-1",
                resume: true
            }
        );
        let batch = RemoteSubject::Batch {
            issue_ids: vec!["a".into()],
        }
        .into_owned();
        assert_eq!(
            batch.borrow(),
            RemoteSubject::Batch {
                issue_ids: vec!["a".into()]
            }
        );
    }
}
