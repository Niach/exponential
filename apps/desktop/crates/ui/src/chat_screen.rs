//! EXP-772/EXP-825 — HOME (`Screen::Home`), the desktop twin of the web
//! `t/$teamSlug/agent` route. Its composer is THE launcher: the one place a
//! session starts, on this machine or another. Every play button opens it
//! with a [`navigation::ChatSeed`] (a seed with a subject in a dialog).
//!
//! One wide rounded composer, vertically centred:
//!
//! ```text
//! ┌───────────────────────────────────────────────┐
//! │ [EXP-42 ✕] [EXP-43 ✕]      ← subject chips     │  OR one action chip
//! │ [the action's pick inputs]                     │
//! │ the mention field (@ members, # issues, :emoji)│
//! │ [pending images]                               │
//! │ +                                          (↑) │
//! └───────────────────────────────────────────────┘
//!  Device ▾ · Account ▾ · Model ▾ · Plan ○ · Repository ▾
//! ```
//!
//! EXP-1249: the "+" is the composer's ONE tool — a menu (Implement issue ›,
//! Add file or image | Effort ›, Ultracode |
//! MCP servers ›, Computer use). NEW-39: no Subagents › row, like web
//! (`sessions.send` carries no subagent model).
//!
//! **Subject by swap** (decision 2026-09-10): issue chips OR one action chip
//! OR (NEW-3) one agent chip. Picking one kind while another is picked
//! replaces it. An AGENT subject (Message an agent › in the "+") hides the
//! options line, the submit reads `Send`, and the send goes into the
//! caller's DM with the agent, whose conversation then opens. **Free text** is the prompt with no subject and optional additional
//! instructions beside one. **Images** ride as attachments, uploaded to the
//! team route before the session exists.
//!
//! EXP-1294: a start is ONE `sessions.send` — a new root in the ACTIVE team
//! on the picked device (this machine or another), linking 1..N issues or
//! naming an action. The hosting device picks the row up from its shapes
//! ([`crate::session_pickup`]); the page lands on the new root
//! (`Screen::Thread`) the moment the send answers, a dialog closes and lands
//! its opener there.
//!
//! FEED-100: the page wears the DAWN — the faint brand mark's top half over
//! the composer — and nothing else of its own: no greeting, no history
//! button. While the team has no board the Getting started block follows
//! the composer.
//!
//! EXP-1271: after Start the send goes pending and the card's options line
//! becomes the STARTING row (the mark, "Starting on <device>", elapsed)
//! until the send answers, or the 30 s timeout toast offers Retry.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::picker::search_list;
use std::rc::Rc;

use gpui::{
    div, px, AnyElement, App, AppContext as _, ClickEvent, Entity,
    FocusHandle, Focusable, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription,
    Window,
};
use gpui_component::input::{self as input, InputEvent, TextareaState};
use gpui_component::menu::PopupMenuItem;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{
    h_flex, v_flex, ActiveTheme as _, Disableable as _, ElementExt as _, Icon, Sizable as _,
};
use sync::Store;

use coding::LaunchOptions;

use crate::action_run::{self, ActionRepoRow};
use crate::chat_launch::{self, RepoState, StartSubject, SubjectKind};
use crate::coding_flow::{self, CodingHub};
use crate::composer_images::{self, PendingImages};
use crate::device_readiness;
use crate::icons::registry;
use crate::picker::issue_picker::{self, IssueRow};
use crate::launch_options::{self, LaunchOptionsSection};
use crate::mention_input::MentionInput;
use crate::navigation::{self, ChatSeed, Navigation, Screen};
use crate::queries;
use crate::home_greeting;
use crate::surface::{glass_pill, PillMode, PillSize};

/// The page's one field: wide, rounded, Enter sends and Shift+Enter breaks a
/// line — the steer composer's rhythm, on a page with nothing else on it.
const PROMPT_MAX_W: f32 = 640.;

/// EXP-1155: the subject band's cap — about three rows of issue chips; past
/// it the band scrolls on its own.
const CHIPS_MAX_H: f32 = 88.;

/// FEED-100 (EXP-1249's mark, restored): the DAWN above the page's composer
/// column — the brand mark's edge, the half of it that shows (a clip of half
/// its height, the logo aligned to the clip's top) and its ink (the
/// foreground at ~2.5%). It stands on the card, no gap (EXP-1314); every
/// client draws the same (web `HomeDawn`, iOS `HomeDawn`, Android
/// `AgentHomeDawn`).
const PAGE_MARK_SIZE: f32 = 440.;
const PAGE_MARK_ALPHA: f32 = 0.025;
/// How many slices the half mark's fade is painted in (gpui has no mask).
const PAGE_MARK_FADE_STEPS: usize = 12;

/// EXP-1249: the composer "+" menu's words (web `launch-composer.tsx`'s
/// menu, the styleguide `menu` entry's composer specimen).
pub(crate) const PLUS_TOOLTIP: &str = "Add";
pub(crate) const MENU_IMPLEMENT_ISSUE: &str = "Issue";
pub(crate) const MENU_ADD_FILE: &str = "Photo or file";
pub(crate) const MENU_DEVICE: &str = "Device";
pub(crate) const MENU_ACCOUNT: &str = "Account";
pub(crate) const MENU_MODEL: &str = "Model";
pub(crate) const MENU_REPOSITORY: &str = "Repository";
pub(crate) const MENU_SUBAGENTS: &str = "Subagents";
pub(crate) const MENU_ULTRACODE: &str = "Ultracode";
pub(crate) const MENU_MCP_SERVERS: &str = "MCP servers";
pub(crate) const MENU_COMPUTER_USE: &str = "Computer use";

/// The issue picker footer's submit (fixture `testIds.implementSubmit`).
pub(crate) const IMPLEMENT_SUBMIT_ID: &str = "agent-composer-implement";

/// The fixture's `implementButton`: "Implement 1 issue" / "Implement {n}
/// issues". It only closes the picker — the picks already are the subject.
pub(crate) fn implement_button_label(count: usize) -> String {
    if count == 1 {
        "Implement 1 issue".to_string()
    } else {
        format!("Implement {count} issues")
    }
}

/// EXP-1249: how tall the "+" menu may grow, for the side-with-room fit.
const PLUS_MENU_WANTED_HEIGHT: f32 = 360.;

/// EXP-1249: the searchable picker a "+" menu row opens, anchored to the +
/// (SLOP-28: Implement issue › is a submenu now, THE search
/// list inside the menu).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComposerPicker {
    McpServers,
}

/// EXP-484/747 B7: a machine's `agent_accounts` payload off its SYNCED row —
/// which login each agent CLI runs as there, and its account profiles. Empty
/// for a row that never reported (an older build, or one that has not beaten
/// yet): the Account pin then has nothing to offer and hides.
fn device_agent_status(
    row_id: &str,
    cx: &App,
) -> (
    coding::agent_accounts::AgentAccounts,
    coding::agent_usage::AgentUsageMap,
) {
    if row_id.is_empty() {
        return Default::default();
    }
    let collections = Store::global(cx).collections();
    let devices = collections.devices.read(cx);
    let Some(row) = devices.iter().find(|row| row.id == row_id) else {
        return Default::default();
    };
    (
        crate::device_settings::parse_agent_map::<coding::agent_accounts::AgentAccount>(
            row.agent_accounts.as_ref(),
        ),
        // EXP-992: the ACTIVE login's numbers still ride the pre-profile
        // slot, which is what the account picker's bars fall back to.
        crate::device_settings::parse_agent_map::<coding::agent_usage::AgentUsage>(
            row.agent_usage.as_ref(),
        ),
    )
}

/// The checked issues and everything their launch needs.
struct IssueSubject {
    /// The picker's pool (open team issues + the seeded ones).
    rows: Rc<Vec<IssueRow>>,
    checked: HashSet<String>,
    /// issue id → probe state (LAZY: only checked issues probe).
    repos: HashMap<String, RepoState>,
}

/// The picked saved prompt (NEW-2: prompts replaced actions; no inputs).
struct ActionSubject {
    action_id: String,
}

/// EXP-868: what [`ChatScreenView::team_pool`] is valid for.
#[derive(Clone, PartialEq, Eq)]
struct TeamPoolKey {
    team_id: String,
    issues: u64,
    boards: u64,
    issue_statuses: u64,
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
    /// `None` before the first settle. EXP-1294: this machine or another,
    /// the start is the same `sessions.send` naming it.
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
/// picked subject takes optional extra instructions (NEW-2: prompts carry
/// no composer hint).
/// EXP-1019: both hints are the shared contract's, never local literals.
/// EXP-1253: with no subject the field asks the home's question.
const CHAT_PLACEHOLDER: &str = home_greeting::HOME_PLACEHOLDER;
const SUBJECT_PLACEHOLDER: &str = domain::contract::COMPOSER_UI_INSTRUCTIONS_PLACEHOLDER;

/// The hint for `subject`. The web `composerPlaceholder` rule, one for one.
fn placeholder_for_subject(subject: &Subject) -> SharedString {
    if matches!(subject, Subject::None) {
        return CHAT_PLACEHOLDER.into();
    }
    SUBJECT_PLACEHOLDER.into()
}

/// EXP-1037 — WHERE this composer is drawn. One view, one set of state, one
/// `send`/`start`; only the chrome around the launcher differs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Presentation {
    /// HOME (`Screen::Home`): the dawn over the centred column.
    Page,
    /// A dialog window opened by a play button ([`crate::composer_dialog`]):
    /// the launcher alone — headline, card, options, notes — and nothing of
    /// the page's chrome.
    Dialog,
}

pub(crate) struct ChatScreenView {
    nav: Entity<Navigation>,
    presentation: Presentation,
    /// EXP-1037: the dialog's seed, applied on the first render that has a
    /// team AND synced shapes (the page takes the same seed off the nav).
    dialog_seed: Option<ChatSeed>,
    /// The team the page is scoped to; a switch resets every pick.
    team_id: Option<String>,
    input: Entity<TextareaState>,
    /// The hint the field currently shows (see [`placeholder_for_subject`]).
    placeholder: SharedString,
    /// EXP-790: the completion overlay (`@` / `#` / `:`) over `input`; the
    /// composer card draws the chrome, so the widget draws none of its own.
    mention: Entity<MentionInput>,
    subject: Subject,
    /// S-3: the agent row's last tap (`None` = the default: the Assistant,
    /// else Temporary session; a page mount starts on it again).
    agent_pick: Option<String>,
    /// Stale-probe guard (old results must not land after a subject swap).
    probe_generation: u64,
    /// The team's saved prompts — live off the synced `prompts` shape;
    /// `actions_ready` = the shape reached readiness.
    actions: Vec<api::prompts::Prompt>,
    actions_ready: bool,
    /// Release review R5: the `#` picker's ranked list, memoised so a busy
    /// repaint never re-ranks the pool. EXP-1030: the picker's QUERY and its
    /// keyboard cursor are the primitive's now — this memo is all the host
    /// still holds.
    issue_pick_memo: RefCell<issue_picker::VisibleRowsMemo>,
    /// EXP-868: the issue picker's pool while nothing is picked, keyed by the
    /// team and the revisions of every collection it reads.
    team_pool: RefCell<Option<(TeamPoolKey, Rc<Vec<IssueRow>>)>>,
    /// EXP-946/EXP-1249: where the "+" tool actually painted, captured at
    /// prepaint (the `Popup` recipe). Its menu and the pickers it opens pick
    /// the side with room and cap themselves to it.
    plus_tool_bounds: Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    /// EXP-1249: the picker a "+" menu row opened (anchored to the +), until
    /// it closes itself.
    open_picker: Option<ComposerPicker>,
    /// The team's connected repos (`repositories.list`, one fetch per team).
    team_repos: Vec<ActionRepoRow>,
    repos_team: Option<String>,
    /// EXP-822: the session's optional repository anchor (no-subject only).
    chat_repo: Option<ActionRepoRow>,
    /// The ONE launch cluster's state (agent, model, effort, toggles, MCP,
    /// account). Built once the coding hub exists (the page can render
    /// before it does).
    launch: Option<LaunchOptionsSection>,
    device: DevicePick,
    /// EXP-792: the team's MCP servers with the person's own connection to
    /// each, one fetch per team; `None` while the fetch is out.
    mcp: Option<Vec<api::mcp_servers::McpServerListEntry>>,
    mcp_team: Option<String>,
    images: PendingImages,
    /// The image strip's notice (too many / too big).
    notice: Option<SharedString>,
    /// Images are uploading (the send is in flight).
    sending: bool,
    /// EXP-1271: the send button's pending face and the card's STARTING row
    /// (`crate::feedback`), from Start until the send answers.
    start_pending: crate::feedback::Pending,
    starting: Option<Starting>,
    starting_generation: u64,
    /// Review #3: a start whose 30 s deadline passed before its send
    /// answered: the composer is idle again, and a late answer still lands
    /// ([`Self::start_landed`]).
    late: Option<LateStart>,
    /// Bumped whenever the draft is consumed ([`Self::after_started`]): an
    /// upload still running for that draft drops its result instead of
    /// starting the draft a second time.
    send_epoch: u64,
    focus_handle: FocusHandle,
    /// EXP-923: the page's one scroll. It holds the composer and NOTHING
    /// else.
    page_scroll: gpui::ScrollHandle,
    /// EXP-1155: the subject chips' band scrolls on its own once it passes
    /// its cap, so the rest of the launcher never has to.
    chips_scroll: gpui::ScrollHandle,
    /// EXP-1155: the dialog window follows its content (`None` on the page).
    dialog_fit: Option<DialogFit>,
    /// NEW-2: THE `/` menu over the draft — the team's prompts alone (no
    /// session yet, so no builtins); `None` = closed.
    slash: Option<(Vec<crate::slash_commands::SlashItem>, usize)>,
    slash_scroll: gpui::ScrollHandle,
    /// The draft the menu was dismissed (or picked) on; it stays shut until
    /// the draft changes.
    slash_dismissed_for: Option<String>,
    _subscriptions: Vec<Subscription>,
}

/// EXP-1155: the composer dialog sizes its WINDOW to the launcher instead of
/// opening at a fixed height and scrolling the rest away (the
/// `search_sheet::fit_to_content` precedent). Both heights are measured at
/// prepaint; the window is refit only when the content height moves, so a
/// manual drag holds until something in the dialog changes.
struct DialogFit {
    /// The content-height cap (the opener viewport share the dialog opened
    /// against); past it the dialog's own scroll is the fallback.
    max_height: gpui::Pixels,
    /// The launcher's natural height, last frame.
    content_h: Rc<std::cell::Cell<gpui::Pixels>>,
    /// The scroll body's box, last frame: the window viewport minus it is the
    /// chrome around the body (titlebar strip + shell padding).
    body_h: Rc<std::cell::Cell<gpui::Pixels>>,
    /// The content height the window was last fitted to.
    fitted: Option<gpui::Pixels>,
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
        let mut subscriptions = vec![
            cx.subscribe_in(
                &input,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::PressEnter { shift: false, .. } => this.send(window, cx),
                    // The blocker is a function of the draft, the `/`
                    // menu too.
                    InputEvent::Change => {
                        this.refresh_slash(cx);
                        cx.notify();
                    }
                    _ => {}
                },
            ),
            cx.observe(&nav, |_, _, cx| cx.notify()),
        ];
        let collections = Store::global(cx).collections();
        let synced_devices = collections.devices.clone();
        let synced_actions = collections.prompts.clone();
        let synced_sessions = collections.sessions.clone();
        let synced_worktrees = collections.device_worktrees.clone();
        let synced_agents = collections.agents.clone();
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
        // NEW-3: the "+" menu's Message an agent › rows and the agent chip
        // read the synced `agents` rows.
        subscriptions.push(cx.observe(&synced_agents, |_: &mut Self, _, cx| cx.notify()));
        // NEW-2: the prompts list is a live read of the synced shape.
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
            presentation: Presentation::Page,
            dialog_seed: None,
            team_id: None,
            input,
            placeholder: CHAT_PLACEHOLDER.into(),
            mention,
            subject: Subject::None,
            agent_pick: None,
            probe_generation: 0,
            actions: Vec::new(),
            actions_ready: false,
            issue_pick_memo: RefCell::new(issue_picker::VisibleRowsMemo::default()),
            team_pool: RefCell::new(None),
            plus_tool_bounds: Rc::new(std::cell::Cell::new(gpui::Bounds::default())),
            open_picker: None,
            team_repos: Vec::new(),
            repos_team: None,
            chat_repo: None,
            launch: None,
            device: DevicePick::default(),
            mcp: None,
            mcp_team: None,
            images: PendingImages::default(),
            notice: None,
            sending: false,
            start_pending: crate::feedback::Pending::default(),
            starting: None,
            starting_generation: 0,
            late: None,
            send_epoch: 0,
            focus_handle: cx.focus_handle(),
            page_scroll: gpui::ScrollHandle::new(),
            chips_scroll: gpui::ScrollHandle::new(),
            dialog_fit: None,
            slash: None,
            slash_scroll: gpui::ScrollHandle::new(),
            slash_dismissed_for: None,
            _subscriptions: subscriptions,
        };
        this.ensure_launch(window, cx);
        this
    }

    /// EXP-1037 — the SAME composer, built for a dialog window and prefilled
    /// with `seed`. The seed is applied on the first render that has both a
    /// team and synced shapes, exactly like the page's pending seed.
    ///
    /// EXP-1155: `max_height` caps the content height the window grows to.
    pub(crate) fn dialog(
        seed: ChatSeed,
        max_height: gpui::Pixels,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let mut this = Self::new(window, cx);
        this.presentation = Presentation::Dialog;
        this.dialog_seed = Some(seed);
        this.dialog_fit = Some(DialogFit {
            max_height,
            content_h: Rc::new(std::cell::Cell::new(px(0.))),
            body_h: Rc::new(std::cell::Cell::new(px(0.))),
            fitted: None,
        });
        // The dialog is opened to be typed into (or sent straight away): the
        // field takes focus the moment the window is up.
        let field = this.input.read(cx).focus_handle(cx);
        window.focus(&field, cx);
        this
    }

    /// Whether a start is in flight — the dialog's close gate.
    pub(crate) fn starting(&self) -> bool {
        self.sending || self.starting.is_some()
    }

    // ── starting (EXP-1271) ───────────────────────────────────────────────

    /// The device a start goes to, by its picker label (NEW-3: a message
    /// to an agent names the agent).
    fn start_device_label(&self, cx: &App) -> String {
        if self.to_agent(cx) {
            return self
                .selected_agent(cx)
                .and_then(|agent| agent.name)
                .unwrap_or_else(|| "the agent".to_string());
        }
        self.selected_device()
            .map(|device| device.label.clone())
            .unwrap_or_else(coding::default_device_label)
    }

    /// Start pressed: the send goes pending and the card shows the STARTING
    /// row, with the [`crate::feedback::STARTING_TIMEOUT`] deadline armed.
    /// Returns the start's generation.
    fn begin_starting(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> u64 {
        // A new start takes over: a late one, should it answer now, no
        // longer lands.
        if let Some(late) = self.late.take() {
            late.dismiss_toast(cx);
        }
        self.starting_generation += 1;
        let generation = self.starting_generation;
        let device = self.start_device_label(cx);
        self.starting = Some(Starting {
            device,
            since: std::time::Instant::now(),
            generation,
        });
        self.start_pending.start(cx);
        self.arm_deadline(generation, window, cx);
        cx.notify();
        generation
    }

    /// Start `generation`'s deadline counts from now.
    fn arm_deadline(&mut self, generation: u64, window: &mut Window, cx: &mut gpui::Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(crate::feedback::STARTING_TIMEOUT)
                .await;
            let _ = this.update_in(cx, |this, window, cx| this.time_out_start(generation, window, cx));
        })
        .detach();
    }

    /// Start `generation`'s send has not answered by its deadline: back to
    /// rest, and the timeout toast (`feedback-states.json`
    /// `starting.timeoutToast`) whose Retry sends the draft again. The send
    /// is not aborted: a late answer still lands ([`Self::start_landed`]).
    fn time_out_start(&mut self, generation: u64, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(starting) = self
            .starting
            .as_ref()
            .filter(|starting| starting.generation == generation)
        else {
            return;
        };
        let device = starting.device.clone();
        self.end_starting(generation, cx);
        let (title, description) = crate::feedback::starting_timeout_copy(&device);
        let toast = crate::toast::Toast::error(title)
            .description(description)
            .action(crate::feedback::RETRY, self.retry_send(cx));
        let toast = crate::toast::show_dismissible(toast, window, cx);
        let draft = self.input.read(cx).value().to_string();
        if let Some(previous) = self.late.replace(LateStart {
            generation,
            draft,
            toast: Some(toast),
        }) {
            previous.dismiss_toast(cx);
        }
    }

    /// A Retry: send the draft as it is NOW.
    fn retry_send(&self, cx: &gpui::Context<Self>) -> impl Fn(&mut Window, &mut App) + 'static {
        let weak = cx.entity().downgrade();
        move |window, cx| {
            let _ = weak.update(cx, |this, cx| this.send(window, cx));
        }
    }

    /// Start `generation`'s send answered with `session_id`. Returns whether
    /// the composer lands on it.
    ///
    /// - In time: the start settles, the draft clears, the root opens.
    /// - LATE, draft unchanged since its send and no other start in flight:
    ///   the error clears, the draft clears, the root opens, as if in time.
    /// - LATE otherwise: the error clears and nothing else happens (the
    ///   session shows in Running).
    fn start_landed(&mut self, generation: u64, window: &mut Window, cx: &mut gpui::Context<Self>) -> bool {
        let in_time = self
            .starting
            .as_ref()
            .is_some_and(|starting| starting.generation == generation);
        let late = self.late.take_if(|late| late.generation == generation);
        let open = if in_time {
            self.end_starting(generation, cx);
            true
        } else if let Some(late) = late {
            late.dismiss_toast(cx);
            self.starting.is_none() && self.input.read(cx).value().as_ref() == late.draft
        } else {
            false
        };
        if open {
            self.after_started(window, cx);
        }
        cx.notify();
        open
    }

    /// FEED-79: a start that failed (refused, unreachable, nothing to run
    /// on) is the error toast; Retry sends the kept draft again.
    fn start_failed(&mut self, message: SharedString, cx: &mut gpui::Context<Self>) {
        crate::action_error::failed_action_with_retry(
            "start",
            "the session",
            Some(message.to_string()),
            self.retry_send(cx),
            cx,
        );
        cx.notify();
    }

    /// The start `generation` settled (landed, failed or timed out): the
    /// send and the card return to rest. A later start's state survives.
    fn end_starting(&mut self, generation: u64, cx: &mut gpui::Context<Self>) {
        if self
            .starting
            .take_if(|starting| starting.generation == generation)
            .is_none()
        {
            return;
        }
        self.start_pending.finish();
        cx.notify();
    }

    fn in_dialog(&self) -> bool {
        matches!(self.presentation, Presentation::Dialog)
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
        self.agent_pick = None;
        // S-3: the viewer's Assistant is created on first use (once per team
        // per app run).
        if let Some(team_id) = team_id.as_deref() {
            crate::session_data::ensure_assistant(team_id, cx);
        }
        self.probe_generation += 1;
        self.mention.update(cx, |mention, _| {
            mention.set_source(team_id.clone().map(crate::markdown::store_completion_source));
        });
        self.refresh_actions(window, cx);
        self.ensure_repos_loaded(cx);
        self.ensure_mcp_loaded(cx);
        if let Some(launch) = self.launch.as_mut() {
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

    // ── the seed (every play button) ──────────────────────────────────────

    /// Apply a [`ChatSeed`]: its issues become the subject, a device seed
    /// is a sticky explicit pick, text lands on an EMPTY draft.
    fn apply_seed(&mut self, seed: ChatSeed, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if !seed.issue_ids.is_empty() {
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
        self.subject = Subject::Issues(IssueSubject {
            rows: Rc::new(rows),
            checked: checked.clone(),
            repos: HashMap::new(),
        });
        for issue_id in checked {
            self.ensure_probe(issue_id, cx);
        }
    }

    /// The issues the `#` picker currently has checked — the primitive's
    /// `value`, and what a reported SET is diffed against to name the one row
    /// that moved.
    fn checked_issue_ids(&self) -> HashSet<String> {
        match &self.subject {
            Subject::Issues(issues) => issues.checked.clone(),
            _ => HashSet::new(),
        }
    }

    /// The `#` picker's toggle: check/uncheck an issue, swapping an action
    /// subject out for issues on the first check, and back to no subject on
    /// the last uncheck.
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

    /// Back to no subject: no chips, plan mode off.
    fn clear_subject(&mut self, cx: &mut gpui::Context<Self>) {
        self.subject = Subject::None;
        self.probe_generation += 1;
        cx.notify();
    }

    /// Kick ONE `repositories.forIssue` probe for `issue_id` if it never ran
    /// (background executor, generation-guarded). Lazy by design: only
    /// checked issues probe.
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
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::repositories::for_issue(&trpc, &probe_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.probe_generation != generation {
                    return; // superseded
                }
                let Subject::Issues(issues) = &mut this.subject else {
                    return;
                };
                let state = match result {
                    Ok(repo) => RepoState::Ready(repo),
                    Err(err) => RepoState::Error(err.to_string()),
                };
                // #N-15: an unresolvable issue STAYS checked — it is still
                // the subject the headline names — and the repository gate
                // (`chat_launch::repo_blocker`) says why it cannot start.
                // Unchecking it silently left the chip up and the send
                // blocked on "Select at least one issue.".
                issues.repos.insert(issue_id, state);
                cx.notify();
            });
        })
        .detach();
    }

    // ── subject: action ───────────────────────────────────────────────────

    /// Refresh the team's prompts from the synced `prompts` shape.
    fn refresh_actions(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.team_id.clone() else {
            self.actions = Vec::new();
            self.actions_ready = false;
            return;
        };
        let (actions, ready) = queries::team_prompts(cx, &team_id);
        self.actions = actions;
        self.actions_ready = ready;
    }

    /// Pick an action: the subject becomes that ONE chip (issues, if any,
    /// are dropped — the swap rule), its picks reset.
    fn select_action(&mut self, action_id: String, cx: &mut gpui::Context<Self>) {
        self.probe_generation += 1;
        self.subject = Subject::Action(ActionSubject { action_id });
        cx.notify();
    }

    /// The picked prompt's row, if the list holds it.
    fn selected_action(&self) -> Option<&api::prompts::Prompt> {
        let Subject::Action(subject) = &self.subject else {
            return None;
        };
        self.actions
            .iter()
            .find(|action| action.id == subject.action_id)
    }

    // ── the `/` menu (NEW-2) ──────────────────────────────────────────────

    /// Re-derive the `/` menu from the draft (web `launch-composer`: the
    /// team's prompts, no builtins — there is no session yet).
    fn refresh_slash(&mut self, cx: &mut gpui::Context<Self>) {
        let draft = self.input.read(cx).value().to_string();
        if self.slash_dismissed_for.as_deref() == Some(draft.as_str()) {
            self.slash = None;
            return;
        }
        self.slash_dismissed_for = None;
        let prompts: Vec<crate::slash_commands::SlashPrompt> = self
            .actions
            .iter()
            .map(crate::slash_commands::SlashPrompt::from)
            .collect();
        let items = crate::slash_commands::slash_items(&draft, None, &prompts);
        self.slash = if items.is_empty() {
            None
        } else {
            let selected = self
                .slash
                .as_ref()
                .map_or(0, |(_, selected)| *selected)
                .min(items.len() - 1);
            Some((items, selected))
        };
    }

    fn move_slash(&mut self, delta: isize, cx: &mut gpui::Context<Self>) {
        if let Some((items, selected)) = self.slash.as_mut() {
            let len = items.len() as isize;
            if len > 0 {
                *selected = (*selected as isize + delta).rem_euclid(len) as usize;
                self.slash_scroll.scroll_to_item(*selected);
                cx.notify();
            }
        }
    }

    /// A pick makes the prompt the SUBJECT (its chip) and clears the `/name`
    /// it was typed as; the send then carries `promptId`.
    fn accept_slash(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some((items, selected)) = self.slash.take() else {
            return;
        };
        if let Some(crate::slash_commands::SlashItem::Prompt(prompt)) = items.get(selected).cloned() {
            self.slash_dismissed_for = Some(String::new());
            self.select_action(prompt.id, cx);
            self.input
                .update(cx, |state, cx| state.set_value("", window, cx));
        }
        cx.notify();
    }

    fn on_slash_up(&mut self, _: &input::MoveUp, _: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.slash.is_some() {
            self.move_slash(-1, cx);
            cx.stop_propagation();
        }
    }

    fn on_slash_down(&mut self, _: &input::MoveDown, _: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.slash.is_some() {
            self.move_slash(1, cx);
            cx.stop_propagation();
        }
    }

    fn on_slash_escape(&mut self, _: &input::Escape, _: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.slash.take().is_some() {
            self.slash_dismissed_for = Some(self.input.read(cx).value().to_string());
            cx.stop_propagation();
            cx.notify();
        }
    }

    /// With the menu open Enter ACCEPTS, and the field never emits the
    /// `PressEnter` that starts.
    fn on_slash_enter(&mut self, action: &input::Enter, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.slash.is_some() && !action.shift {
            self.accept_slash(window, cx);
            cx.stop_propagation();
        }
    }

    fn on_slash_tab(&mut self, _: &input::IndentInline, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.slash.is_some() {
            self.accept_slash(window, cx);
            cx.stop_propagation();
        }
    }

    /// THE `/` menu inside the card, above the field (the session
    /// composer's [`crate::composer::session_slash_menu`]).
    fn render_slash(&self, window: &Window, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let (items, selected) = self.slash.as_ref()?;
        Some(crate::composer::session_slash_menu(
            "chat-slash-menu",
            "chat-slash-row",
            items,
            *selected,
            &self.slash_scroll,
            |this: &mut Self, index, cx| {
                if let Some((_, selected)) = this.slash.as_mut() {
                    if *selected != index {
                        *selected = index;
                        cx.notify();
                    }
                }
            },
            |this: &mut Self, index, window, cx| {
                if let Some((_, selected)) = this.slash.as_mut() {
                    *selected = index;
                }
                this.accept_slash(window, cx);
            },
            window,
            cx,
        ))
    }

    // ── subject: agent (NEW-3) ────────────────────────────────────────────

    /// S-3: the agent row above the card (composer-home.json `agentRow`):
    /// the viewer's Assistant first, the team agents they see, then the
    /// dotted Temporary session; `agent_pick` = the last tap.
    fn team_agent_defs(&self, cx: &App) -> Vec<domain::rows::AgentDefRow> {
        let Some(team_id) = self.team_id.as_deref() else {
            return Vec::new();
        };
        Store::global(cx)
            .collections()
            .agents
            .read(cx)
            .iter()
            .filter(|agent| agent.team_id.as_deref() == Some(team_id))
            .cloned()
            .collect()
    }

    /// The agent the row picks (`None` = Temporary session).
    fn selected_agent(&self, cx: &App) -> Option<domain::rows::AgentDefRow> {
        let defs = self.team_agent_defs(cx);
        let me = crate::queries::active_account(cx).map(|account| account.user_id);
        let state = domain::composer_agent_row::composer_agent_row(&defs, me.as_deref(), self.agent_pick.as_deref());
        state.agent.cloned()
    }

    /// Whether the send goes to an agent (no device, no runner rows).
    fn to_agent(&self, cx: &App) -> bool {
        self.selected_agent(cx).is_some()
    }

    /// A tap on the agent row (tapping the selected circle keeps it).
    pub(crate) fn pick_agent(&mut self, pick: String, cx: &mut gpui::Context<Self>) {
        self.agent_pick = Some(pick);
        cx.notify();
    }

    // ── fetches: repos, MCP ───────────────────────────────────────────────

    /// EXP-822: the team's repositories for the Repository pick and the
    /// action repo inputs. One fetch per team; best-effort.
    fn ensure_repos_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        if self.team_id == self.repos_team {
            return;
        }
        self.team_repos = Vec::new();
        self.chat_repo = None;
        // NEW-39: the marker lands only once the fetch can run, so a team
        // that syncs before the auth context retries on the next render.
        let (Some(team), Some(trpc)) = (self.team_id.clone(), queries::trpc_client(cx)) else {
            self.repos_team = None;
            return;
        };
        self.repos_team = Some(team.clone());
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
                cx.notify();
            });
        })
        .detach();
    }

    /// EXP-792: one `mcpServers.list` per team (it carries the person's own
    /// connection to each server).
    fn ensure_mcp_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        if self.team_id == self.mcp_team {
            return;
        }
        self.mcp = None;
        // NEW-39: the marker lands only once the fetch can run, so a team
        // that syncs before the auth context retries on the next render
        // (the "+" menu's MCP servers row).
        let (Some(team), Some(trpc)) = (self.team_id.clone(), queries::trpc_client(cx)) else {
            self.mcp_team = None;
            return;
        };
        self.mcp_team = Some(team.clone());
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move {
                    api::mcp_servers::list(&trpc, &team)
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

    /// EXP-792: the team's servers as the pick offers them, greyed where the
    /// person has not connected (the server holds the credential, so the
    /// target machine does not matter).
    fn mcp_options(&self) -> Vec<launch_options::McpServerOption> {
        let Some(entries) = self.mcp.as_ref() else {
            return Vec::new();
        };
        entries
            .iter()
            .map(|entry| launch_options::McpServerOption {
                id: entry.config.id.clone(),
                name: entry.config.name.clone(),
                blocked: launch_options::mcp_block_reason(&entry.connection),
                enabled_by_default: entry.config.enabled_by_default,
                url: entry.config.url.clone(),
                command: entry.config.command.clone(),
            })
            .collect()
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
            .map(|device| {
                let (accounts, usage) = device_agent_status(&device.row_id, cx);
                launch_options::RemoteDefaults {
                    agents: device.agents.clone(),
                    acp_agents: device.acp_agents.clone(),
                    settings: device.defaults.clone(),
                    accounts,
                    usage,
                }
            });
        let Some(launch) = self.launch.as_mut() else {
            return;
        };
        launch.set_remote(remote, window, cx);
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

    /// The settled machine when it is NOT this one (its synced row answers
    /// the readiness questions this process cannot).
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

    /// EXP-1196: the picked agent's run gate on the picked machine as ONE
    /// readiness row (`device_readiness::blocking_row`: Git when Git fails,
    /// else that agent's row), with the remote target when the machine is
    /// not this one. `None` = no block for it (an older build's NULL doctor,
    /// the first local run not landed) or nothing against the agent there.
    fn readiness_blocker(
        &self,
        cx: &App,
    ) -> Option<(device_readiness::ReadinessRow, Option<(String, SharedString)>)> {
        let launch = self.launch.as_ref()?;
        match self.remote_device() {
            Some(device) => {
                let store = Store::try_global(cx)?;
                let rows = store.collections().devices.read(cx);
                let row = rows.get(&device.row_id)?;
                let doctor = device_readiness::parse(row.doctor.as_ref())?;
                let row = device_readiness::blocking_row(&doctor, launch.agent, false)?;
                Some((row, Some((device.device_id.clone(), device.label.clone().into()))))
            }
            None => {
                let hub = CodingHub::global_ref(cx)?;
                let hub = hub.read(cx);
                let doctor = hub.doctor.device.as_ref()?;
                Some((device_readiness::blocking_row(doctor, launch.agent, true)?, None))
            }
        }
    }

    /// EXP-1375: the picked machine's low-disk row when nothing blocks the
    /// start there (`device_readiness::warning_row`), with the remote target
    /// like [`Self::readiness_blocker`]. Never a blocker.
    fn readiness_warning(
        &self,
        cx: &App,
    ) -> Option<(device_readiness::ReadinessRow, Option<(String, SharedString)>)> {
        self.launch.as_ref()?;
        match self.remote_device() {
            Some(device) => {
                let store = Store::try_global(cx)?;
                let rows = store.collections().devices.read(cx);
                let row = rows.get(&device.row_id)?;
                let doctor = device_readiness::parse(row.doctor.as_ref())?;
                let row = device_readiness::warning_row(&doctor, false)?;
                Some((row, Some((device.device_id.clone(), device.label.clone().into()))))
            }
            None => {
                let hub = CodingHub::global_ref(cx)?;
                let hub = hub.read(cx);
                let doctor = hub.doctor.device.as_ref()?;
                Some((device_readiness::warning_row(doctor, true)?, None))
            }
        }
    }

    /// The readiness row under the composer, wired to its action (THIS
    /// device: every action; another: the remote ones only).
    fn render_readiness_row(
        row: &device_readiness::ReadinessRow,
        target: Option<(String, SharedString)>,
        cx: &App,
    ) -> gpui::Div {
        let props = match target {
            None => device_readiness::local_props("chat-readiness", cx),
            Some((device_id, label)) => {
                device_readiness::remote_props("chat-readiness", device_id, label, cx)
            }
        };
        device_readiness::render_single(row, props, cx)
    }

    // ── the gate ──────────────────────────────────────────────────────────

    fn composer_placeholder(&self) -> SharedString {
        placeholder_for_subject(&self.subject)
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
            Subject::Action(_) => SubjectKind::Action,
        }
    }

    /// Why the submit is disabled right now;    /// Why the submit is disabled right now; `None` = launchable. The order
    /// is the deleted dialog's: the machine, then the tooling, then the
    /// picked MCP servers, then the subject's own rules.
    fn launch_blocker(&self, cx: &mut App) -> Option<SharedString> {
        if self.sending {
            return Some("Uploading images…".into());
        }
        if self.team_id.is_none() {
            return Some("Sign in and wait for sync before starting a session.".into());
        }
        // NEW-3: a DM runs on the agent's own device and login: the
        // composer's machine and its readiness never gate it.
        if self.to_agent(cx) {
            let text = self.input.read(cx).value().to_string();
            return chat_launch::text_blocker(&SubjectKind::Agent, &text, self.images.len())
                .map(Into::into);
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
                    // EXP-1196: the machine's own row says why, when it
                    // sent one.
                    if let Some((row, _)) = self.readiness_blocker(cx) {
                        return Some(device_readiness::summary(&row).into());
                    }
                    return Some(
                        format!("{} can't run {}.", device.label, launch.agent.label()).into(),
                    );
                }
            }
            None => {
                let gated_agent = launch.agent;
                let report = CodingHub::global_ref(cx).and_then(|hub| hub.read(cx).doctor.report.clone());
                match report.as_ref() {
                    None => return Some("Checking local tools…".into()),
                    Some(report) => {
                        if let Some(failed) = report.first_failure_for(gated_agent) {
                            if let Some((row, _)) = self.readiness_blocker(cx) {
                                return Some(device_readiness::summary(&row).into());
                            }
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
        let text = self.input.read(cx).value().to_string();
        let kind = self.subject_kind();
        match &self.subject {
            Subject::None => chat_launch::text_blocker(&kind, &text, self.images.len()).map(Into::into),
            Subject::Action(_) => {
                if !self.actions_ready {
                    return Some(chat_launch::ACTIONS_LOADING.into());
                }
                if self.selected_action().is_none() {
                    return Some("Select a prompt.".into());
                }
                chat_launch::text_blocker(&kind, &text, self.images.len()).map(Into::into)
            }
            Subject::Issues(issues) => {
                if let Some(reason) = chat_launch::issue_count_blocker(issues.checked.len()) {
                    return Some(reason.into());
                }
                chat_launch::repo_blocker(&issues.rows, &issues.checked, &issues.repos).map(Into::into)
            }
        }
    }

    /// The launch options as picked. EXP-1294: every subject (an action
    /// too: actions are saved prompts) takes the composer's MCP pick.
    fn options(&self, cx: &App) -> LaunchOptions {
        self.launch_ref().options(cx)
    }

    // ── send ──────────────────────────────────────────────────────────────

    /// The submit: upload the staged images to the TEAM route (sequential,
    /// idempotent), then start.
    fn send(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        // EXP-1271: pending = inert (an Enter mid-start sends nothing).
        if self.launch_blocker(cx).is_some() || self.sending || self.starting.is_some() {
            return;
        }
        let text = self.input.read(cx).value().to_string();
        if self.images.is_empty() {
            self.start(text, Vec::new(), window, cx);
            return;
        }
        let Some(team_id) = self.team_id.clone() else {
            return;
        };
        let Some(transport) = queries::attachment_transport(cx) else {
            crate::action_error::failed_action("upload", "the attachment", None, cx);
            return;
        };
        let jobs = self.images.jobs();
        let epoch = self.send_epoch;
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
                // The draft this upload was for already landed (a late
                // answer of its earlier send): it is consumed.
                if this.send_epoch != epoch {
                    cx.notify();
                    return;
                }
                match outcome {
                    Ok(resolved) => {
                        this.images.note_uploaded(&resolved);
                        // The subject may have moved under the upload: the
                        // gate runs again. The uploads stay noted for the
                        // retry.
                        if let Some(blocker) = this.launch_blocker(cx) {
                            this.notice = Some(blocker);
                            cx.notify();
                            return;
                        }
                        // The embeds name the uploads; the send carries the
                        // text and the ids apart.
                        let parsed = domain::image_message::parse_steer_message(
                            &this.images.message(&text),
                        );
                        let mut attachment_ids = parsed.attachment_ids;
                        attachment_ids.extend(parsed.files.into_iter().map(|file| file.id));
                        this.start(parsed.text, attachment_ids, window, cx);
                    }
                    Err((resolved, error)) => {
                        // Keep what landed so a retry uploads only the rest.
                        this.images.note_uploaded(&resolved);
                        log::warn!("[ui] home composer upload failed: {error}");
                        // FEED-79: a failed upload is the error toast; Retry
                        // sends again (only the rest uploads).
                        let composer = cx.entity().downgrade();
                        crate::action_error::failed_action_with_retry(
                            "upload",
                            "the attachment",
                            None,
                            move |window, cx| {
                                let _ = composer.update(cx, |this, cx| this.send(window, cx));
                            },
                            cx,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// EXP-1294: start the session — ONE `sessions.send` for a new root in
    /// the ACTIVE team on the picked device, with the subject, the text, the
    /// uploads and the options. The page lands on the root the send answers;
    /// a dialog closes and lands its opener there.
    fn start(
        &mut self,
        text: String,
        attachment_ids: Vec<String>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // EXP-1271: a start in flight keeps the send inert.
        if self.starting.is_some() {
            return;
        }
        let Some(team_id) = self.team_id.clone() else {
            return;
        };
        // S-3: an agent picked in the row = ONE sessions.send({agentId})
        // into the viewer's conversation with it; picked issues ride as
        // `#IDENT` references.
        if let Some(agent) = self.selected_agent(cx) {
            let mut text = text;
            if let Subject::Issues(issues) = &self.subject {
                let refs: Vec<String> = issues
                    .rows
                    .iter()
                    .filter(|row| issues.checked.contains(&row.issue_id))
                    .map(|row| format!("#{}", row.identifier))
                    .collect();
                if !refs.is_empty() {
                    if !text.trim().is_empty() {
                        text.push_str("\n\n");
                    }
                    text.push_str(&refs.join(" "));
                }
            }
            self.send_to_agent(team_id, agent.id, text, attachment_ids, window, cx);
            return;
        }
        let Some(device_id) = self.selected_device().map(|device| device.device_id.clone()) else {
            self.start_failed("Pick a device to start the session on.".into(), cx);
            return;
        };
        let subject = match &self.subject {
            Subject::None => StartSubject {
                repository_id: self.chat_repo.as_ref().map(|repo| repo.id.clone()),
                ..Default::default()
            },
            Subject::Issues(issues) => StartSubject {
                // The picker's order: the session lists them as picked.
                issue_ids: issues
                    .rows
                    .iter()
                    .filter(|row| issues.checked.contains(&row.issue_id))
                    .map(|row| row.issue_id.clone())
                    .collect(),
                repository_id: chat_launch::issues_repository(&issues.checked, &issues.repos),
                ..Default::default()
            },
            Subject::Action(subject) => {
                if self.selected_action().is_none() {
                    return;
                }
                StartSubject {
                    prompt_id: Some(subject.action_id.clone()),
                    ..Default::default()
                }
            }
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            self.start_failed("Sign in and wait for sync before starting a session.".into(), cx);
            return;
        };
        let input = chat_launch::send_input(
            &team_id,
            &device_id,
            &self.options(cx),
            subject,
            &text,
            attachment_ids,
        );
        let generation = self.begin_starting(window, cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::sessions::send(&trpc, &input) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(output) => {
                    // Read before `start_landed` closes a dialog composer.
                    let in_dialog = this.in_dialog();
                    if this.start_landed(generation, window, cx) {
                        let session_id = output.session_id;
                        // A dialog has closed itself by now: the root opens
                        // in the window its play button was pressed in, ON
                        // TOP of what it showed (Back returns there); the
                        // Home page gives way to the root it started.
                        navigation::defer_in_result_window(window, cx, move |window, cx| {
                            let screen = Screen::Thread { session_id };
                            if in_dialog {
                                navigation::navigate(window, cx, screen);
                            } else {
                                navigation::navigate_replace(window, cx, screen);
                            }
                        });
                    }
                }
                Err(err) => {
                    // A late failure after the timeout toast says nothing
                    // more; an in-time one is the error toast.
                    let in_time = this
                        .starting
                        .as_ref()
                        .is_some_and(|starting| starting.generation == generation);
                    if let Some(late) = this.late.take_if(|late| late.generation == generation) {
                        late.dismiss_toast(cx);
                    }
                    if in_time {
                        this.end_starting(generation, cx);
                        this.start_failed(err.user_message().into(), cx);
                    }
                }
            });
        })
        .detach();
    }

    /// NEW-3 — a message to `agent_id`: ONE `sessions.send({teamId, agentId,
    /// text, attachmentIds})` into the caller's DM with it (created on
    /// first use), then the agent's conversation opens (`Screen::Agent`,
    /// what the rail's agent row opens). A failure = `Couldn't send the
    /// message` with Retry.
    fn send_to_agent(
        &mut self,
        team_id: String,
        agent_id: String,
        text: String,
        attachment_ids: Vec<String>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(trpc) = queries::trpc_client(cx) else {
            self.message_failed(Some("Sign in and wait for sync before sending.".to_string()), cx);
            return;
        };
        let input = chat_launch::agent_send_input(&team_id, &agent_id, &text, attachment_ids);
        let generation = self.begin_starting(window, cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::sessions::send(&trpc, &input) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(output) => {
                    let in_dialog = this.in_dialog();
                    if this.start_landed(generation, window, cx) {
                        // NEW-6: the agent's page shows the caller's
                        // conversation (the send created or reused it).
                        let _ = output.session_id;
                        navigation::defer_in_result_window(window, cx, move |window, cx| {
                            let screen = Screen::Agent { agent_id };
                            if in_dialog {
                                navigation::navigate(window, cx, screen);
                            } else {
                                navigation::navigate_replace(window, cx, screen);
                            }
                        });
                    }
                }
                Err(err) => {
                    let in_time = this
                        .starting
                        .as_ref()
                        .is_some_and(|starting| starting.generation == generation);
                    if let Some(late) = this.late.take_if(|late| late.generation == generation) {
                        late.dismiss_toast(cx);
                    }
                    if in_time {
                        this.end_starting(generation, cx);
                        this.message_failed(Some(err.user_message().to_string()), cx);
                    }
                }
            });
        })
        .detach();
    }

    /// NEW-3 (web `failedAction('send', 'the message')`): a DM that did not
    /// go out is the error toast; Retry sends the kept draft again.
    fn message_failed(&mut self, message: Option<String>, cx: &mut gpui::Context<Self>) {
        crate::action_error::failed_action_with_retry(
            "send",
            "the message",
            message,
            self.retry_send(cx),
            cx,
        );
        cx.notify();
    }

    /// The session is on its way: clear the draft, the images and the
    /// subject; a dialog composer closes itself.
    fn after_started(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.images.clear();
        self.send_epoch += 1;
        self.notice = None;
        self.clear_subject(cx);
        // EXP-1037: a dialog composer has done its one job — the root opens
        // in the window the play button was pressed in (the navigation hands
        // back through `navigation::owner_window_for`).
        if self.in_dialog() {
            clear_open_dialog(cx.entity().entity_id(), cx);
            crate::native_dialog::close_dialog_window(window, cx);
        }
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

    /// EXP-1019/EXP-1037 — the composer's HEADLINE: the launcher's main
    /// element, with the text field below it reading as the secondary one.
    /// `Run` beside the action chip, `Implement` beside the issue chips
    /// ([`chat_launch::headline`], the shared contract's words).
    ///
    /// With NO subject there is no headline at all: the contract's "Ask the
    /// agent" is already the field's own placeholder right underneath, and
    /// the subject-less page is deliberately quiet.
    fn render_headline(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let kind = self.subject_kind();
        if matches!(kind, SubjectKind::Chat) {
            return None;
        }
        let chips = self.render_chips(cx)?;
        // NEW-3: an agent's chip IS the headline (no verb).
        let verb = chat_launch::headline(&kind);
        Some(
            h_flex()
                .w_full()
                .min_w_0()
                .flex_wrap()
                .items_center()
                .gap_2()
                .px_1()
                .children((!verb.is_empty()).then(|| {
                    div()
                        .flex_shrink_0()
                        .text_lg()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(cx.theme().foreground)
                        .child(verb)
                }))
                .child(chips)
                .into_any_element(),
        )
    }

    /// The subject chips: one per checked issue, or the action's one. They
    /// sit in the HEADLINE (EXP-1019), not in the card's leading slot.
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
                    // EXP-885: the ONE issue badge — the markdown editor's
                    // `#IDENT` chip, status glyph included. It used to be a
                    // `glass_pill` capsule with no status at all, so the same
                    // issue wore two different badges two panels apart.
                    chips.push(
                        crate::issue_chip::issue_chip(
                            ("chat-chip-issue", ix),
                            row.identifier.clone(),
                            row.title.clone(),
                        )
                        .status(row.resolved.clone())
                        .max_title_width(px(220.))
                        .on_remove(
                            ("chat-chip-issue-remove", ix),
                            cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.toggle_issue(issue_id.clone(), false, window, cx);
                            }),
                        )
                        .into_any_element(),
                    );
                }
            }
            Subject::Action(_) => {
                let (name, icon) = match self.selected_action() {
                    Some(action) => (action.name.clone(), action.icon.clone()),
                    None => ("Action".to_string(), None),
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
        // EXP-1155: 30 issue chips wrap into many rows; past the cap
        // they scroll in their own band beside the verb, never the dialog.
        Some(
            crate::scroll_pane::capped_v_scroll(
                "chat-chips-scroll",
                &self.chips_scroll,
                px(CHIPS_MAX_H),
                h_flex()
                    .w_full()
                    .min_w_0()
                    .flex_wrap()
                    .items_center()
                    .gap_1()
                    .children(chips),
            )
            .flex_1()
            .into_any_element(),
        )
    }

    /// EXP-868: the open team pool behind the issue picker    /// EXP-868: the open team pool behind the issue picker, rebuilt only when
    /// the team or one of the collections it reads has moved.
    fn team_pool(&self, team_id: &str, cx: &App) -> Rc<Vec<IssueRow>> {
        let collections = Store::global(cx).collections();
        let key = TeamPoolKey {
            team_id: team_id.to_string(),
            issues: collections.issues.read(cx).revision(),
            boards: collections.boards.read(cx).revision(),
            issue_statuses: collections.issue_statuses.read(cx).revision(),
        };
        if let Some((cached_key, rows)) = self.team_pool.borrow().as_ref() {
            if *cached_key == key {
                return rows.clone();
            }
        }
        let rows = Rc::new(issue_picker::snapshot_rows(cx, team_id, &HashSet::new()));
        *self.team_pool.borrow_mut() = Some((key, rows.clone()));
        rows
    }

    /// The "+" → Implement issue › pool: the subject's rows (the picks and
    /// the repo-probe notes) or, with no issue subject yet, the team's open
    /// issues.
    fn implement_pool(
        &self,
        cx: &App,
    ) -> (Rc<Vec<IssueRow>>, HashSet<String>, HashMap<String, SharedString>) {
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
                            RepoState::Error(err) => format!("repository check failed: {err}").into(),
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
                    .map(|team| self.team_pool(team, cx))
                    .unwrap_or_default(),
                HashSet::new(),
                HashMap::new(),
            ),
        }
    }

    /// Back to the field after a "+" submenu closed the menu.
    fn refocus_input(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.input.read(cx).focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    /// "+" → MCP servers ›: the team's servers behind the shared multi
    /// picker (EXP-792), opened by the menu row and anchored to the +
    /// (EXP-1249). A blocked server reads its reason as the second line and
    /// DIMS, but still toggles (the launch starts without it — a warning,
    /// never a blocker).
    /// N-6: whether this composer's pickers open in its flow — the dialog
    /// presentation (the host rule, `native_dialog::is_dialog_window`).
    fn inline_pickers(&self) -> bool {
        matches!(self.presentation, Presentation::Dialog)
    }

    /// N-6: the "+" menu's picker, INLINE in the card (the dialog). `None`
    /// on the page, or while no picker is open.
    fn inline_picker(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        if !self.inline_pickers() {
            return None;
        }
        match self.open_picker {
            Some(ComposerPicker::McpServers) => {
                let bounds = self.plus_tool_bounds.get();
                let fit = issue_picker::popover_fit(
                    bounds,
                    gpui::size(px(0.), px(0.)),
                    issue_picker::POPOVER_WANTED_HEIGHT,
                );
                self.mcp_picker_surface(div().w(px(0.)).h(px(0.)).into_any_element(), fit, cx)
            }
            None => None,
        }
    }

    fn mcp_picker_surface(
        &self,
        trigger: AnyElement,
        fit: (gpui::Anchor, gpui::Pixels),
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let (servers, selected) = self.launch.as_ref()?.mcp_menu();
        if servers.is_empty() {
            return None;
        }
        let rows = launch_options::mcp_picker_servers(&servers);
        let dimmed: Vec<String> = servers
            .iter()
            .filter(|server| server.blocked.is_some())
            .map(|server| server.id.clone())
            .collect();
        let view = cx.entity().downgrade();
        let on_close = self.picker_on_close(cx);
        let inline = self.inline_pickers();
        Some(
            crate::picker::deferred(move |window, cx| {
                crate::picker::mcp_server_picker::mcp_server_picker(
                    &rows,
                    selected,
                    trigger,
                    Rc::new(move |ids: Vec<String>, _window: &mut Window, cx: &mut App| {
                        if let Some(view) = view.upgrade() {
                            view.update(cx, |this, cx| {
                                Self::launch_access(this).set_mcp_selected(ids);
                                cx.notify();
                            });
                        }
                    }),
                )
                .id("chat-mcp-picker")
                .open(on_close)
                .fit(fit)
                .inline_if(inline)
                .render_item(move |item, cx| {
                    let body = crate::picker::picker_item_body(item, cx);
                    if dimmed.iter().any(|id| id == &item.value) {
                        h_flex().flex_1().min_w_0().opacity(0.5).child(body).into_any_element()
                    } else {
                        body
                    }
                })
                .render(window, cx)
            })
            .into_any_element(),
        )
    }

    /// EXP-1249: what a "+"-opened picker runs as it closes — stop mounting
    /// it and hand the caret back to the field.
    fn picker_on_close(&self, cx: &mut gpui::Context<Self>) -> crate::picker::PickerOnClose {
        let view = cx.entity().downgrade();
        Rc::new(move |window: &mut Window, cx: &mut App| {
            if let Some(view) = view.upgrade() {
                view.update(cx, |this, cx| {
                    this.open_picker = None;
                    this.input.read(cx).focus_handle(cx).focus(window, cx);
                    cx.notify();
                });
            }
        })
    }

    /// EXP-1249: a "+" menu row opens `picker` (anchored to the +).
    fn open_composer_picker(&mut self, picker: ComposerPicker, cx: &mut gpui::Context<Self>) {
        self.open_picker = Some(picker);
        cx.notify();
    }

    /// EXP-1249 — the composer's ONE tool: the "+" and its menu, plus the
    /// picker one of its rows opened, hung off a zero-width anchor at the
    /// +'s left edge so it opens exactly where the menu did.
    fn plus_tool(&self, window: &Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        let bounds = self.plus_tool_bounds.get();
        let viewport = window.viewport_size();
        let menu_fit = issue_picker::popover_fit(bounds, viewport, PLUS_MENU_WANTED_HEIGHT);
        let picker_fit =
            issue_picker::popover_fit(bounds, viewport, issue_picker::POPOVER_WANTED_HEIGHT);
        let options = self.launch.as_ref().map(|launch| launch.composer_menu(cx));
        let temporary = !self.to_agent(cx);
        let runner = self.runner_menu(cx);
        // EXP-1259 (DEV-ONLY, `EXP_DEV_COMPOSER_MENU=implement-issue`): the
        // home page opens its "+" menu once, on the first render that has
        // the full menu (the launch cluster), synced issues, a measured +
        // and (NEW-39) the team's MCP servers loaded: the menu is built when
        // it opens, so a list landing later would photograph without its
        // MCP servers row (web's capture hovers after the page settled).
        // N-6: `dialog-implement-issue` is the dialog's turn (its menu opens
        // in a menu window over the dialog, `composer-dialog-menu`).
        let dev_open = options.is_some()
            && bounds.size.width > px(0.)
            && crate::navigation::shapes_ready(cx)
            && self.mcp.is_some()
            && crate::screens::take_dev_composer_menu(matches!(self.presentation, Presentation::Dialog));
        let view = cx.entity().downgrade();
        let trigger = crate::composer::composer_tool("chat-tool-plus", registry::UI_ADD, cx)
            .tooltip(PLUS_TOOLTIP)
            .disabled(self.sending);
        let menu = crate::controls::PointerMenu::new("chat-tool-plus-menu", trigger, {
            move |menu, window, cx| {
                plus_menu(menu, &view, options.as_ref(), &runner, temporary, window, cx)
            }
        })
        .anchor(menu_fit.0)
        .wanted_height(PLUS_MENU_WANTED_HEIGHT)
        .dev_open(dev_open);

        // The picker's anchor: as tall as the + (the fit measures the room
        // above from it), no width, nothing to hit. N-6: the dialog mounts
        // it INLINE in the card instead (`inline_picker`).
        let anchor = || div().w(px(0.)).h(bounds.size.height.max(px(24.))).into_any_element();
        let picker = match self.open_picker {
            Some(ComposerPicker::McpServers) if !self.inline_pickers() => {
                self.mcp_picker_surface(anchor(), picker_fit, cx)
            }
            _ => None,
        };
        let slot = self.plus_tool_bounds.clone();
        let view_id = cx.entity_id();
        h_flex()
            .items_center()
            .children(picker)
            .child(
                div()
                    .child(menu)
                    // A + that moved repaints once so the menu and the
                    // pickers measure from where it is now (release review
                    // R5). Steady state: no notify.
                    .on_prepaint(move |bounds, _, cx| {
                        if slot.get() != bounds {
                            slot.set(bounds);
                            cx.notify(view_id);
                        }
                    }),
            )
            .into_any_element()
    }

    /// S-3 — THE agent row (composer-home.json `agentRow`): the label line,
    /// then the single-select circles (the chats agent bubble at the row's
    /// size), the dotted Temporary-session circle last.
    fn render_agent_row(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let defs = self.team_agent_defs(cx);
        let me = crate::queries::active_account(cx).map(|account| account.user_id);
        let state = domain::composer_agent_row::composer_agent_row(&defs, me.as_deref(), self.agent_pick.as_deref());
        let view = cx.entity().downgrade();
        crate::chat_list::composer_agent_row(
            &state,
            std::rc::Rc::new(move |pick: String, _: &mut Window, cx: &mut App| {
                if let Some(view) = view.upgrade() {
                    view.update(cx, |this, cx| this.pick_agent(pick, cx));
                }
            }),
            cx,
        )
    }

    /// S-3: the Device / Account / Model / Repository submenus' state (the
    /// options line's pins moved into the "+").
    fn runner_menu(&self, cx: &mut gpui::Context<Self>) -> RunnerMenu {
        let mut runner = RunnerMenu {
            devices: self
                .device
                .devices
                .iter()
                .map(|device| (device.device_id.clone(), device.label.clone()))
                .collect(),
            device_picked: self.device.device_id.clone(),
            repos: self.team_repos.clone(),
            repo_picked: self.chat_repo.as_ref().map(|repo| repo.id.clone()),
            ..RunnerMenu::default()
        };
        if let Some(launch) = self.launch.as_ref() {
            runner.accounts = launch.account_options(cx);
            runner.account_picked = launch.account_key();
            let (models, picked) = launch.model_menu(cx);
            runner.models = models;
            runner.model_picked = picked;
        }
        runner
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
            // EXP-1037: a DIALOG composer carries its own seed (its window
            // has no pending-seed nav of its own); the page takes the nav's.
            let seed = self
                .dialog_seed
                .take()
                .or_else(|| navigation::take_pending_chat_seed(&self.nav, cx));
            if let Some(seed) = seed {
                self.apply_seed(seed, window, cx);
            }
        }
        match self.presentation {
            Presentation::Page => self.render_page(window, cx),
            Presentation::Dialog => self.render_dialog(window, cx),
        }
    }
}

impl ChatScreenView {
    /// EXP-1019/EXP-1037 — THE launcher: the headline, the composer card, the
    /// options row and the notes under it. Byte-identical in both
    /// presentations; only the chrome around it differs.
    fn render_launcher(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        // NEW-39: retry the per-team fetches that found no tRPC client yet;
        // both return at once when the team already matches.
        self.ensure_mcp_loaded(cx);
        self.ensure_repos_loaded(cx);
        let mcp = self.mcp_options();
        if let Some(launch) = self.launch.as_mut() {
            launch.set_mcp_servers(mcp);
        }

        let blocker = self.launch_blocker(cx);
        // EXP-1196: a device/agent failure renders as THE failing readiness
        // row (with its action) instead of a sentence.
        // NEW-3: a DM runs on the agent's own device: no machine notes.
        let to_agent = self.to_agent(cx);
        // EXP-1375: else the low-disk warning row (the start stays enabled).
        let readiness = self
            .readiness_blocker(cx)
            .or_else(|| self.readiness_warning(cx))
            .filter(|_| !to_agent);
        let readiness_summary = readiness
            .as_ref()
            .map(|(row, _)| SharedString::from(device_readiness::summary(row)));
        let no_session_note = self
            .no_session_note()
            .filter(|_| readiness.is_none() && !to_agent);
        let request_note = self.device_request_note(cx).filter(|_| !to_agent);
        // EXP-827: ICON-ONLY — the ONE round send of `composer::glass_composer`
        // (the steer composer's button, same ring, same 32px hit box). The
        // label the web `submitLabel` mirrors is the TOOLTIP now: the composer
        // card is the page's only control, so a word beside the arrow only
        // repeated what the chips above it already say.
        let label = chat_launch::submit_label(&self.subject_kind());
        // EXP-1271: the PENDING face — inert from the first moment, the
        // spinner in the icon slot only after `feedback::PENDING_DELAY`.
        let submit = crate::composer::composer_submit(
            "chat-send",
            registry::UI_SUBMIT,
            blocker.is_some() || self.start_pending.active(),
            cx,
        )
        .tooltip(SharedString::from(label))
        .loading(self.sending || self.start_pending.shown())
        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.send(window, cx)));
        // EXP-1019: the subject CHIPS moved out of the card into the
        // headline (NEW-2: a prompt has no typed inputs). NEW-2: the `/`
        // menu leads the card, above the field.
        let leading: Option<AnyElement> = self.render_slash(window, cx);
        let strip = (!self.images.is_empty()).then(|| {
            self.images
                .render_strip("chat-pending-remove", self.sending, Self::remove_image, cx)
        });
        // N-6: in the DIALOG the "+" menu's MCP servers picker opens INLINE
        // in the card, above the tools (the host rule: a dialog's pickers
        // open in its flow and the window follows its content, EXP-1155),
        // instead of a popover the dialog would clip.
        let strip = match (strip, self.inline_picker(cx)) {
            (strip, None) => strip,
            (strip, Some(picker)) => Some(
                v_flex().w_full().min_w_0().gap_1().children(strip).child(picker).into_any_element(),
            ),
        };
        let mut composer = crate::composer::GlassComposer::new(
            div()
                .w_full()
                .min_w_0()
                // The captures run before the field's own handlers: with the
                // `/` menu open the keys drive it, never the send.
                .capture_action(cx.listener(Self::on_slash_up))
                .capture_action(cx.listener(Self::on_slash_down))
                .capture_action(cx.listener(Self::on_slash_escape))
                .capture_action(cx.listener(Self::on_slash_enter))
                .capture_action(cx.listener(Self::on_slash_tab))
                .child(self.mention.clone())
                .into_any_element(),
        )
        .strip(strip)
        // EXP-1249: the ONE tool — the "+" menu (the `ui-add` plus ×4).
        .tool(self.plus_tool(window, cx))
        .submit(submit);
        if let Some(leading) = leading {
            composer = composer.leading(leading);
        }
        let headline = self.render_headline(cx);
        // EXP-1271: while a start is on its way the options line is the
        // STARTING row: the mark, "Starting on <device>", the elapsed time.
        // NEW-3: an agent subject HIDES the options line (Device · Account ·
        // Model · Plan · Repository): the DM runs on the agent's own device
        // and login.
        // S-3: NO options line under the card (Device, Account, Model and
        // Repository are "+" rows now): only the STARTING row while a start
        // is out.
        let options = match &self.starting {
            Some(starting) => Some(div()
                .w_full()
                .min_w_0()
                .px_1()
                .child(crate::feedback::starting_row(
                    crate::feedback::TEST_ID_STARTING,
                    &starting.device,
                    starting.since,
                    crate::surface::page_ground(),
                    cx,
                ))
                .into_any_element()),
            None => None,
        };
        let muted = cx.theme().muted_foreground;
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
            .filter(|_| !self.sending)
            .filter(|reason| chat_launch::note_for_blocker(Some(reason.as_ref())).is_some());
        let blocker_note = blocker_note.filter(|reason| Some(reason) != readiness_summary.as_ref());
        if let Some((row, target)) = readiness {
            notes = notes.child(Self::render_readiness_row(&row, target, cx));
        }
        if let Some(reason) = blocker_note {
            // EXP-862: the blocker is a sentence and nothing else — the
            // "Sign in to <agent>" pill it used to carry is gone ×4; a login
            // is offered ONCE, on the account chip that owns it (Devices).
            notes = if reason.as_ref() == chat_launch::ACTIONS_LOADING {
                notes.child(crate::feedback::inline_status(crate::feedback::WORD_LOADING, true, cx))
            } else {
                notes.child(div().w_full().min_w_0().text_color(muted).child(reason))
            };
        }
        if let Some(note) = no_session_note {
            notes = notes.child(div().text_color(muted).child(note));
        }
        v_flex()
            .w_full()
            .min_w_0()
            .gap_2()
            // EXP-1019: the headline is the launcher's MAIN element — the
            // verb plus the subject's chips, with the card (and its muted
            // placeholder) reading as the secondary one under it.
            .children(headline)
            // S-3: the agent row stands directly above the card (the page
            // only: a play button's dialog always starts a session).
            .children((!self.in_dialog()).then(|| self.render_agent_row(cx)))
            .child(
                crate::composer::glass_composer(composer)
                    .capture_action(cx.listener(Self::on_paste)),
            )
            .children(options)
            .child(notes)
            .into_any_element()
    }

    /// HOME (`Screen::Home`, FEED-100): the DAWN over the launcher, nothing
    /// under its options line but Getting started while the team has no
    /// board, centred in the page's one scroll. The composer IS the page, so
    /// it sits in the middle of it (web `justify-center`).
    fn render_page(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        let launcher = self.render_launcher(window, cx);
        // EXP-1270: a team with no board yet gets the Getting started block
        // under the composer (its first step creates the board).
        let getting_started = self
            .team_id
            .clone()
            .filter(|team_id| {
                Store::global(cx)
                    .collections()
                    .boards_in_team(team_id, cx)
                    .is_empty()
            })
            .and_then(|team_id| crate::getting_started::inline_list(&team_id, cx));
        // The stack must NOT shrink: inside the scroll column a
        // flex-shrinkable child gets squeezed to the viewport and its
        // trailing rows (options, the blocker note) clipped.
        let column = v_flex()
            .id(home_greeting::TEST_ID_HOME)
            .w_full()
            .max_w(px(PROMPT_MAX_W))
            .min_w_0()
            .flex_shrink_0()
            .child(launcher)
            .children(getting_started.map(|block| {
                div()
                    .w_full()
                    .pt(px(home_greeting::COMPOSER_TO_GETTING_STARTED_PX))
                    .child(block)
            }));
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
                    .min_h_full()
                    .justify_center()
                    // EXP-1314: the dawn stands ON the card (its fade ends
                    // at the card's top edge), so no gap between the two.
                    .child(v_flex().w_full().items_center().child(dawn(cx)).child(column)),
            ))
            .into_any_element()
    }

    /// EXP-1037 — the composer in its DIALOG window: the launcher alone, in
    /// the window's own scroll. No page chrome — a dialog always has a
    /// subject, and the one thing left to decide is whether to send it.
    fn render_dialog(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        let body = self.render_launcher(window, cx);
        self.fit_dialog(window, cx);
        // EXP-1155: both boxes report at prepaint; a change repaints once so
        // the fit above reads this frame's heights (steady state: no notify).
        let view_id = cx.entity_id();
        let (content_slot, body_slot) = match &self.dialog_fit {
            Some(fit) => (fit.content_h.clone(), fit.body_h.clone()),
            None => Default::default(),
        };
        v_flex()
            .size_full()
            .min_h_0()
            .track_focus(&self.focus_handle)
            .child(
                crate::scroll_pane::v_scroll_pane(
                    "chat-dialog-scroll",
                    &self.page_scroll,
                    v_flex()
                        .w_full()
                        .min_w_0()
                        .flex_shrink_0()
                        .on_prepaint(move |bounds, _, cx| {
                            if content_slot.get() != bounds.size.height {
                                content_slot.set(bounds.size.height);
                                cx.notify(view_id);
                            }
                        })
                        .child(body),
                )
                .on_prepaint(move |bounds, _, cx| {
                    if body_slot.get() != bounds.size.height {
                        body_slot.set(bounds.size.height);
                        cx.notify(view_id);
                    }
                }),
            )
            .into_any_element()
    }

    /// EXP-1155: size the dialog WINDOW to the launcher — its measured
    /// content plus the chrome around the body (window viewport minus the
    /// body box), capped at [`DialogFit::max_height`] plus that chrome.
    /// Refits only when the content height moved, so a manual drag holds;
    /// never touches a maximized or fullscreen window. The resize is deferred
    /// and top-anchored, so the dialog grows downward.
    fn fit_dialog(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(fit) = self.dialog_fit.as_mut() else {
            return;
        };
        let content = fit.content_h.get();
        let body = fit.body_h.get();
        if content <= px(0.) || body <= px(0.) || fit.fitted == Some(content) {
            return;
        }
        if window.is_maximized() || window.is_fullscreen() {
            return;
        }
        fit.fitted = Some(content);
        let current = window.viewport_size();
        let chrome = (current.height - body).max(px(0.));
        let target = content.min(fit.max_height) + chrome;
        if (target - current.height).abs() <= px(1.) {
            return;
        }
        crate::native_dialog::resize_dialog_keeping_top(
            window,
            cx,
            gpui::size(current.width, target),
        );
    }

}

/// FEED-100 — the DAWN: the faint brand mark's TOP HALF above the card, the
/// page only (never the dialog), with no hitbox so it never takes a click.
/// The half FADES out toward the card (solid for its top third, transparent
/// at the clip's bottom edge): gpui paints no alpha mask, so the clip is
/// [`PAGE_MARK_FADE_STEPS`] horizontal slices, each the icon offset up by
/// its row and inked a step fainter (web's `mask-image: linear-gradient(
/// black 33%, transparent)`).
pub(crate) fn dawn(cx: &App) -> AnyElement {
    let slice_h = PAGE_MARK_SIZE / 2. / PAGE_MARK_FADE_STEPS as f32;
    let foreground = cx.theme().foreground;
    v_flex()
        .id(home_greeting::TEST_ID_MARK)
        .w(px(PAGE_MARK_SIZE))
        .max_w_full()
        .flex_shrink_0()
        .overflow_hidden()
        .children((0..PAGE_MARK_FADE_STEPS).map(|row| {
            let top = (row as f32 + 0.5) / PAGE_MARK_FADE_STEPS as f32;
            let fade = ((1. - top) / (1. - 0.33)).clamp(0., 1.);
            div()
                .w_full()
                .h(px(slice_h))
                .flex_shrink_0()
                .overflow_hidden()
                .flex()
                .justify_center()
                .items_start()
                .child(
                    Icon::from(crate::icons::ExpIcon::Logo)
                        .size(px(PAGE_MARK_SIZE))
                        .flex_shrink_0()
                        .mt(px(-(row as f32) * slice_h))
                        .text_color(foreground.opacity(PAGE_MARK_ALPHA * fade)),
                )
        }))
        .into_any_element()
}

/// EXP-1037 — the composer that is currently open in a dialog window. WEAK:
/// the dialog window owns the view, and a closed dialog must not keep it
/// alive; a stale handle simply stops answering.
struct OpenComposerDialog(gpui::WeakEntity<ChatScreenView>);

impl gpui::Global for OpenComposerDialog {}

/// EXP-1249 — one row of the "+" menu, in `fixtures/composer-menu.json`
/// `rows` order ([`composer_menu_rows`] decides which render). S-3: the
/// runner submenus (Device, Account, Model, Repository) are rows now; the
/// `message-agent` row is gone (the agent row above the card picks).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComposerMenuRow {
    AddFile,
    ImplementIssue,
    Separator,
    Device,
    Account,
    Model,
    Repository,
    Effort,
    Subagents,
    Ultracode,
    McpServers,
    ComputerUse,
}

impl ComposerMenuRow {
    /// The row's CONCEPT icon (the fixture's `icon`); a separator has none.
    pub(crate) fn icon(self) -> Option<crate::icons::ExpIcon> {
        Some(match self {
            Self::AddFile => registry::UI_ATTACH,
            Self::ImplementIssue => registry::EDITOR_ISSUE_REF,
            Self::Separator => return None,
            Self::Device => registry::NAV_DEVICES,
            Self::Account => registry::SETTINGS_ACCOUNT,
            Self::Model => registry::CODING_ASSISTANT,
            Self::Repository => registry::SETTINGS_REPOSITORIES,
            Self::Effort => registry::UI_ESTIMATE,
            Self::Subagents => registry::CODING_SUBAGENT,
            Self::Ultracode => registry::ACTION_DEFAULT,
            Self::McpServers => registry::UI_MCP,
            Self::ComputerUse => registry::NAV_COMPUTER,
        })
    }

    pub(crate) fn menu_icon(self) -> Icon {
        Icon::new(self.icon().unwrap_or(registry::UI_ADD))
    }

    /// The fixture's `label` (Effort: the claude word; codex reads
    /// `codexLabel`, [`coding::CodingAgent::effort_label`]).
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::AddFile => MENU_ADD_FILE,
            Self::ImplementIssue => MENU_IMPLEMENT_ISSUE,
            Self::Separator => "",
            Self::Device => MENU_DEVICE,
            Self::Account => MENU_ACCOUNT,
            Self::Model => MENU_MODEL,
            Self::Repository => MENU_REPOSITORY,
            Self::Effort => "Effort",
            Self::Subagents => MENU_SUBAGENTS,
            Self::Ultracode => MENU_ULTRACODE,
            Self::McpServers => MENU_MCP_SERVERS,
            Self::ComputerUse => MENU_COMPUTER_USE,
        }
    }

    /// The fixture's `surfaces`.
    fn surfaces(self) -> &'static [ComposerSurface] {
        use ComposerSurface::*;
        match self {
            Self::AddFile | Self::ImplementIssue | Self::Separator | Self::Device | Self::Account | Self::Model => {
                &[Launch, Session, Agent]
            }
            _ => &[Launch],
        }
    }

    /// The fixture's `when` (all must hold).
    fn when(self, c: &ComposerMenuConditions) -> bool {
        match self {
            Self::AddFile | Self::ImplementIssue | Self::Separator => true,
            Self::Device => c.device,
            Self::Account => c.account,
            Self::Model => c.model,
            Self::Repository => c.temporary && c.repositories,
            Self::Effort => c.temporary,
            Self::Subagents => c.temporary && c.subagent_model,
            Self::Ultracode => c.temporary && c.ultracode,
            Self::McpServers => c.temporary && c.mcp,
            Self::ComputerUse => c.temporary && c.computer_use,
        }
    }
}

/// S-3: which composer the "+" sits on (a row's `surfaces`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComposerSurface {
    Launch,
    Session,
    Agent,
}

/// The fixture's `conditions`: which `when` rows hold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ComposerMenuConditions {
    /// Launch: the agent row picks Temporary session.
    pub(crate) temporary: bool,
    pub(crate) device: bool,
    pub(crate) account: bool,
    pub(crate) model: bool,
    /// The team has more than one repository.
    pub(crate) repositories: bool,
    pub(crate) subagent_model: bool,
    pub(crate) ultracode: bool,
    pub(crate) mcp: bool,
    pub(crate) computer_use: bool,
}

impl ComposerMenuConditions {
    /// The LAUNCH composer's conditions off the cluster's menu state, the
    /// agent row's pick and the team's repositories.
    pub(crate) fn launch(options: &launch_options::ComposerMenu, temporary: bool, repositories: usize) -> Self {
        Self {
            temporary,
            device: temporary,
            account: temporary,
            model: temporary,
            repositories: repositories > 1,
            // NEW-39: like web's composer-plus-menu (EXP-1293), `sessions.send`
            // carries no subagent model, so the Subagents row hides.
            subagent_model: false,
            ultracode: options.ultracode.is_some(),
            mcp: !options.mcp_servers.is_empty(),
            computer_use: options.computer_use.is_some(),
        }
    }
}

/// EXP-1249 — the "+" menu's rows (fixtures/composer-menu.json, ×4) on
/// `surface`: a row only where it exists and while its `when` holds, then
/// the separators TIDIED (never leading, trailing or doubled). Pure.
pub(crate) fn composer_menu_rows(surface: ComposerSurface, conditions: ComposerMenuConditions) -> Vec<ComposerMenuRow> {
    use ComposerMenuRow::*;
    const ALL: [ComposerMenuRow; 15] = [
        AddFile,
        ImplementIssue,
        Separator,
        Device,
        Account,
        Model,
        Repository,
        Separator,
        Effort,
        Subagents,
        Ultracode,
        Separator,
        McpServers,
        ComputerUse,
        Separator,
    ];
    let mut rows: Vec<ComposerMenuRow> = Vec::new();
    for row in ALL {
        if !row.surfaces().contains(&surface) || !row.when(&conditions) {
            continue;
        }
        if row == Separator && rows.last().is_none_or(|last| *last == Separator) {
            continue;
        }
        rows.push(row);
    }
    if rows.last() == Some(&Separator) {
        rows.pop();
    }
    rows
}

/// S-3: the launch composer's runner submenus' state (Device, Account,
/// Model, Repository), read off the host view in ONE pass for the menu.
#[derive(Clone, Default)]
pub(crate) struct RunnerMenu {
    /// `(device_id, label)` candidates; the picked id.
    pub(crate) devices: Vec<(String, String)>,
    pub(crate) device_picked: Option<String>,
    pub(crate) accounts: Vec<coding::AccountOption>,
    pub(crate) account_picked: String,
    pub(crate) models: &'static [(&'static str, &'static str)],
    pub(crate) model_picked: String,
    pub(crate) repos: Vec<ActionRepoRow>,
    pub(crate) repo_picked: Option<String>,
}

impl RunnerMenu {
    fn device_value(&self) -> Option<String> {
        let picked = self.device_picked.as_deref()?;
        self.devices.iter().find(|(id, _)| id == picked).map(|(_, label)| label.clone())
    }

    fn account_value(&self) -> Option<String> {
        self.accounts
            .iter()
            .find(|option| option.account_option_key() == self.account_picked)
            .or_else(|| self.accounts.first())
            .map(|option| option.email.clone())
    }

    fn model_value(&self) -> Option<String> {
        self.models
            .iter()
            .find(|(_, value)| *value == self.model_picked)
            .map(|(label, _)| (*label).to_string())
    }

    fn repo_value(&self) -> Option<String> {
        self.repo_picked
            .as_deref()
            .and_then(|id| self.repos.iter().find(|repo| repo.id == id))
            .or_else(|| self.repos.first())
            .map(|repo| repo.full_name.clone())
    }
}

/// A runner submenu's title: the row's word, then ` · value` when one is
/// picked (the fixture: value = the picked label).
pub(crate) fn titled_row(row: ComposerMenuRow, value: Option<String>) -> String {
    match value {
        Some(value) if !value.is_empty() => format!("{} · {value}", row.label()),
        _ => row.label().to_string(),
    }
}

/// EXP-1249 — the LAUNCH composer's "+" menu (web `launch-composer.tsx`'s
/// menu, the styleguide `menu` entry's composer specimen), row by row off
/// [`composer_menu_rows`] on the launch surface. `options` is `None` until
/// the launch cluster exists (then only the first group shows). S-3:
/// `temporary` = the agent row picks Temporary session (an agent picked
/// hides the runner and work rows); `runner` feeds the Device / Account /
/// Model / Repository submenus (THE pickers' rows, the pick checked).
#[allow(clippy::too_many_arguments)]
pub(crate) fn plus_menu(
    mut menu: gpui_component::menu::PopupMenu,
    view: &gpui::WeakEntity<ChatScreenView>,
    options: Option<&launch_options::ComposerMenu>,
    runner: &RunnerMenu,
    temporary: bool,
    window: &mut Window,
    cx: &mut gpui::Context<gpui_component::menu::PopupMenu>,
) -> gpui_component::menu::PopupMenu {
    use crate::controls::{
        menu_row, menu_row_value, pointer_check_item, pointer_menu_item, pointer_toggle_item,
    };
    let open = |picker: ComposerPicker| {
        let view = view.clone();
        move |_: &mut Window, cx: &mut App| {
            if let Some(view) = view.upgrade() {
                view.update(cx, |this, cx| this.open_composer_picker(picker, cx));
            }
        }
    };
    let root = cx.entity().downgrade();
    menu = search_list::search_submenu_parent(menu.min_w(px(260.)));
    let rows = match options {
        Some(options) => composer_menu_rows(
            ComposerSurface::Launch,
            ComposerMenuConditions::launch(options, temporary, runner.repos.len()),
        ),
        None => vec![ComposerMenuRow::AddFile, ComposerMenuRow::ImplementIssue],
    };
    for row in rows {
        menu = match (row, options) {
            (ComposerMenuRow::Separator, _) => menu.separator(),
            // SLOP-28 (EXP-1259 D5): THE search list as a submenu — hover
            // or Right opens it, Left/Escape hand the keys back.
            (ComposerMenuRow::ImplementIssue, _) => {
                let view = view.clone();
                let root = root.clone();
                menu.submenu_with_icon(
                    Some(row.menu_icon()),
                    MENU_IMPLEMENT_ISSUE,
                    window,
                    cx,
                    move |sub, window, cx| {
                        search_list::menu_search_list_submenu(
                            "chat-implement-issue",
                            implement_issue_source(&view),
                            sub,
                            root.clone(),
                            window,
                            cx,
                        )
                    },
                )
            }
            (ComposerMenuRow::AddFile, _) => menu.item({
                let view = view.clone();
                pointer_menu_item(None, MENU_ADD_FILE, |_| None, move |window, cx| {
                    if let Some(view) = view.upgrade() {
                        view.update(cx, |_, cx| {
                            composer_images::pick_attachment_files(window, cx, |this, read, window, cx| {
                                this.stage_images(read, window, cx)
                            });
                        });
                    }
                })
                .icon(row.menu_icon())
            }),
            // S-3: THE device picker's rows as a submenu (the options
            // line's Device pin moved here).
            (ComposerMenuRow::Device, _) => {
                let devices = runner.devices.clone();
                let picked = runner.device_picked.clone();
                let view = view.clone();
                menu.submenu_with_icon(
                    Some(row.menu_icon()),
                    titled_row(row, runner.device_value()),
                    window,
                    cx,
                    move |mut sub, _, _| {
                        for (id, label) in &devices {
                            let view = view.clone();
                            let id = id.clone();
                            let checked = picked.as_deref() == Some(id.as_str());
                            sub = sub.item(pointer_check_item(label.clone(), checked, move |window, cx| {
                                if let Some(view) = view.upgrade() {
                                    view.update(cx, |this, cx| {
                                        this.set_device(id.clone(), window, cx);
                                        cx.notify();
                                    });
                                }
                            }));
                        }
                        sub
                    },
                )
            }
            // S-3: THE account picker's rows (every login the target machine
            // reports, across agents: picking a login picks its agent).
            (ComposerMenuRow::Account, _) => {
                if runner.accounts.is_empty() {
                    continue;
                }
                let accounts = runner.accounts.clone();
                let picked = runner.account_picked.clone();
                let view = view.clone();
                menu.submenu_with_icon(
                    Some(row.menu_icon()),
                    titled_row(row, runner.account_value()),
                    window,
                    cx,
                    move |mut sub, _, _| {
                        for option in &accounts {
                            let view = view.clone();
                            let checked = option.account_option_key() == picked;
                            let agent = option.agent;
                            let label = SharedString::from(option.email.clone());
                            let option = option.clone();
                            sub = sub.item(crate::controls::pointer_check_item_with_icon(
                                move || crate::coding_selects::agent_mark(agent),
                                label,
                                checked,
                                move |window, cx| {
                                    if let Some(view) = view.upgrade() {
                                        let option = option.clone();
                                        view.update(cx, |this, cx| {
                                            ChatScreenView::launch_access(this).set_account(&option, window, cx);
                                            cx.notify();
                                        });
                                    }
                                },
                            ));
                        }
                        sub
                    },
                )
            }
            // S-3: THE model list of the picked agent.
            (ComposerMenuRow::Model, _) => {
                let models = runner.models;
                let picked = runner.model_picked.clone();
                let view = view.clone();
                menu.submenu_with_icon(
                    Some(row.menu_icon()),
                    titled_row(row, runner.model_value()),
                    window,
                    cx,
                    move |mut sub, _, _| {
                        for (label, value) in models {
                            let view = view.clone();
                            let value = (*value).to_string();
                            sub = sub.item(pointer_check_item(*label, picked == value, move |window, cx| {
                                if let Some(view) = view.upgrade() {
                                    view.update(cx, |this, cx| {
                                        ChatScreenView::launch_access(this).pick_model(&value, window, cx);
                                        cx.notify();
                                    });
                                }
                            }));
                        }
                        sub
                    },
                )
            }
            // S-3: THE repository rows (only while the team has > 1).
            (ComposerMenuRow::Repository, _) => {
                let repos = runner.repos.clone();
                let picked = runner.repo_picked.clone().or_else(|| repos.first().map(|repo| repo.id.clone()));
                let view = view.clone();
                menu.submenu_with_icon(
                    Some(row.menu_icon()),
                    titled_row(row, runner.repo_value()),
                    window,
                    cx,
                    move |mut sub, _, _| {
                        for repo in &repos {
                            let view = view.clone();
                            let checked = picked.as_deref() == Some(repo.id.as_str());
                            let repo = repo.clone();
                            sub = sub.item(pointer_check_item(repo.full_name.clone(), checked, move |_, cx| {
                                let repo = repo.clone();
                                if let Some(view) = view.upgrade() {
                                    view.update(cx, |this, cx| {
                                        this.chat_repo = Some(repo);
                                        cx.notify();
                                    });
                                }
                            }));
                        }
                        sub
                    },
                )
            }
            (_, None) => menu,
            (ComposerMenuRow::Effort, Some(options)) => {
                let effort_title = format!("{} · {}", options.effort_label, options.effort_value());
                if options.effort_locked {
                    // Ultracode IS the effort level: the row says so and opens nothing.
                    let label = SharedString::from(effort_title);
                    menu.item(
                        PopupMenuItem::element(move |_, cx| menu_row(None, label.clone(), None, true, cx))
                            .disabled(true)
                            .icon(row.menu_icon()),
                    )
                } else {
                    let choices = options.effort_choices;
                    let picked = options.effort_picked.clone();
                    let view = view.clone();
                    menu.submenu_with_icon(
                        Some(row.menu_icon()),
                        effort_title,
                        window,
                        cx,
                        move |mut sub, _, _| {
                            for (label, value) in choices {
                                let view = view.clone();
                                let value = (*value).to_string();
                                sub = sub.item(pointer_check_item(*label, picked == value, move |window, cx| {
                                    if let Some(view) = view.upgrade() {
                                        view.update(cx, |this, cx| {
                                            ChatScreenView::launch_access(this).pick_effort(&value, window, cx);
                                            cx.notify();
                                        });
                                    }
                                }));
                            }
                            sub
                        },
                    )
                }
            }
            (ComposerMenuRow::Subagents, Some(options)) => {
                let (Some(picked), Some(value)) = (options.subagent_picked.clone(), options.subagent_value()) else {
                    continue;
                };
                let view = view.clone();
                menu.submenu_with_icon(
                    Some(row.menu_icon()),
                    format!("{MENU_SUBAGENTS} · {value}"),
                    window,
                    cx,
                    move |mut sub, _, _| {
                        // THE model list, explicit: no "Default" row.
                        for (label, choice) in crate::coding_selects::MODEL_CHOICES.iter() {
                            let view = view.clone();
                            let choice = (*choice).to_string();
                            sub = sub.item(pointer_check_item(*label, picked == choice, move |window, cx| {
                                if let Some(view) = view.upgrade() {
                                    view.update(cx, |this, cx| {
                                        ChatScreenView::launch_access(this)
                                            .pick_subagent_model(&choice, window, cx);
                                        cx.notify();
                                    });
                                }
                            }));
                        }
                        sub
                    },
                )
            }
            (ComposerMenuRow::Ultracode, Some(options)) => {
                let Some(on) = options.ultracode else {
                    continue;
                };
                let view = view.clone();
                menu.item(
                    pointer_toggle_item("chat-menu-ultracode", None, MENU_ULTRACODE, on, move |on, _, cx| {
                        if let Some(view) = view.upgrade() {
                            view.update(cx, |this, cx| {
                                ChatScreenView::launch_access(this).ultracode = on;
                                cx.notify();
                            });
                        }
                    })
                    .icon(row.menu_icon()),
                )
            }
            (ComposerMenuRow::McpServers, Some(options)) => {
                // The fixture: the row shows the number picked, no value at zero.
                let value = options.mcp_value().map(SharedString::from);
                menu.item(
                    pointer_menu_item(
                        None,
                        MENU_MCP_SERVERS,
                        move |cx| Some(menu_row_value(value.clone(), true, cx)),
                        open(ComposerPicker::McpServers),
                    )
                    .icon(row.menu_icon()),
                )
            }
            (ComposerMenuRow::ComputerUse, Some(options)) => {
                let Some(on) = options.computer_use else {
                    continue;
                };
                let view = view.clone();
                menu.item(
                    pointer_toggle_item(
                        "chat-menu-computer-use",
                        None,
                        MENU_COMPUTER_USE,
                        on,
                        move |on, _, cx| {
                            if let Some(view) = view.upgrade() {
                                view.update(cx, |this, cx| {
                                    ChatScreenView::launch_access(this).set_computer_use(on);
                                    cx.notify();
                                });
                            }
                        },
                    )
                    .icon(row.menu_icon()),
                )
            }
        };
    }
    menu
}

/// SLOP-28 — "+" → Implement issue ›: the issue rows (status glyph, mono
/// identifier, title), the picks pinned first with their check, the repo
/// probe's note on the rest; a pick toggles and keeps the menu open, the
/// footer's Implement button closes it (the picks already are the subject).
fn implement_issue_source(view: &gpui::WeakEntity<ChatScreenView>) -> search_list::MenuListSource {
    let rows_view = view.clone();
    let pick_view = view.clone();
    let empty_view = view.clone();
    let footer_view = view.clone();
    search_list::MenuListSource {
        placeholder: search_list::ISSUE_SEARCH_PLACEHOLDER,
        rows: Rc::new(move |query: &str, _window: &mut Window, cx: &mut App| {
            let Some(view) = rows_view.upgrade() else {
                return Vec::new();
            };
            let this = view.read(cx);
            let (rows, checked, notes) = this.implement_pool(cx);
            let (order, _, _) = this.issue_pick_memo.borrow_mut().get(&rows, &checked, query);
            let at_cap = checked.len() >= issue_picker::MAX_ISSUES_PER_RUN;
            order
                .into_iter()
                .filter_map(|ix| rows.get(ix))
                .map(|row| {
                    let picked = checked.contains(&row.issue_id);
                    let trailing = if picked {
                        Some(search_list::row_check(cx))
                    } else {
                        notes
                            .get(&row.issue_id)
                            .cloned()
                            .or_else(|| row.state_hint.map(SharedString::from))
                            .map(|note| {
                                div()
                                    .flex_shrink_0()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(note)
                                    .into_any_element()
                            })
                    };
                    search_list::MenuListRow {
                        id: row.issue_id.clone(),
                        body: search_list::issue_row_body(
                            Some(
                                crate::icons::resolved_status_icon(&row.resolved, cx)
                                    .xsmall()
                                    .into_any_element(),
                            ),
                            row.identifier.clone(),
                            row.title.clone(),
                            trailing,
                            cx,
                        ),
                        disabled: at_cap && !picked,
                    }
                })
                .collect()
        }),
        pick: Rc::new(move |issue_id: &str, window: &mut Window, cx: &mut App| {
            if let Some(view) = pick_view.upgrade() {
                view.update(cx, |this, cx| {
                    let on = !this.checked_issue_ids().contains(issue_id);
                    this.toggle_issue(issue_id.to_string(), on, window, cx);
                });
            }
            false
        }),
        empty: Rc::new(move |_query: &str, cx: &App| {
            let empty_pool = empty_view
                .upgrade()
                .is_none_or(|view| view.read(cx).implement_pool(cx).0.is_empty());
            if empty_pool {
                "No open issues in this team.".into()
            } else {
                "No matches. Only open issues are shown.".into()
            }
        }),
        // Open issues are synced rows: never a first load to wait out.
        loading: Rc::new(|_: &App| false),
        footer: Some(Rc::new(
            move |query: &str, close: search_list::MenuClose, _window: &mut Window, cx: &mut App| {
                let view = footer_view.upgrade()?;
                let (hidden, picked) = {
                    let this = view.read(cx);
                    let (rows, checked, _) = this.implement_pool(cx);
                    let (_, hidden, _) = this.issue_pick_memo.borrow_mut().get(&rows, &checked, query);
                    (hidden, checked.len())
                };
                let note = (hidden > 0)
                    .then(|| issue_picker::list_note(format!("+{hidden} more. Refine your search."), cx));
                // The fixture's `implementButton`: once anything is picked,
                // the footer submit closes the menu.
                let submit = (picked > 0).then(|| {
                    let view = view.downgrade();
                    h_flex().w_full().p_1().child(
                        Button::new(IMPLEMENT_SUBMIT_ID)
                            .primary()
                            .small()
                            .w_full()
                            .cursor_pointer()
                            .label(implement_button_label(picked))
                            .on_click(move |_, window, cx| {
                                close(window, cx);
                                if let Some(view) = view.upgrade() {
                                    view.update(cx, |this, cx| this.refocus_input(window, cx));
                                }
                            }),
                    )
                });
                (note.is_some() || submit.is_some()).then(|| {
                    v_flex()
                        .w_full()
                        .children(note)
                        .children(submit)
                        .into_any_element()
                })
            },
        )),
    }
}

/// Register the composer a freshly opened dialog window hosts (called by
/// [`crate::composer_dialog::open`]) so the rail's pinned action row keeps
/// lighting up while its run is composed.
pub(crate) fn register_open_dialog(view: &Entity<ChatScreenView>, cx: &mut App) {
    cx.set_global(OpenComposerDialog(view.downgrade()));
    // Escape and the titlebar close drop the window, and the view with it:
    // the global must not keep naming a composer that is gone.
    let view_id = view.entity_id();
    cx.observe_release(view, move |_, cx| clear_open_dialog(view_id, cx))
        .detach();
}

/// Forget the open dialog once `view_id`'s composer is done (its run
/// started, or its window closed). A global naming ANOTHER, newer dialog
/// is left alone.
fn clear_open_dialog(view_id: gpui::EntityId, cx: &mut App) {
    let names_this = cx
        .try_global::<OpenComposerDialog>()
        .is_some_and(|open| open.0.entity_id() == view_id);
    if names_this {
        cx.remove_global::<OpenComposerDialog>();
    }
}

/// EXP-1271 — a start on its way (the composer's STARTING row): the device
/// it went to, since when, and which start it is.
struct Starting {
    device: String,
    since: std::time::Instant,
    generation: u64,
}

/// Review #3 — a start whose deadline passed before its send answered: its
/// generation, the draft as it was then (a late answer lands only on an
/// unchanged draft) and the timeout toast a late answer takes back.
struct LateStart {
    generation: u64,
    draft: String,
    toast: Option<crate::toast::ShownToast>,
}

impl LateStart {
    fn dismiss_toast(&self, cx: &mut App) {
        if let Some(toast) = &self.toast {
            toast.dismiss(cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-1249 / S-3 — `fixtures/composer-menu.json` is the "+" menu's ×4
    /// spec: the words and concept icons row by row, `surfaces` and `when`,
    /// the Effort row's codex label, and every `cases` entry (with its
    /// `surface`) replayed through [`composer_menu_rows`].
    #[test]
    fn the_plus_menu_replays_the_composer_menu_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/composer-menu.json"
        ))
        .unwrap();
        assert_eq!(fixture["plusLabel"], PLUS_TOOLTIP);
        use ComposerMenuRow as R;
        let row_of = |id: &str| match id {
            "add-file" => R::AddFile,
            "implement-issue" => R::ImplementIssue,
            "device" => R::Device,
            "account" => R::Account,
            "model" => R::Model,
            "repository" => R::Repository,
            "effort" => R::Effort,
            "subagents" => R::Subagents,
            "ultracode" => R::Ultracode,
            "mcp-servers" => R::McpServers,
            "computer-use" => R::ComputerUse,
            "-" => R::Separator,
            other => panic!("unknown composer menu row {other}"),
        };
        let surface_of = |name: &str| match name {
            "launch" => ComposerSurface::Launch,
            "session" => ComposerSurface::Session,
            "agent" => ComposerSurface::Agent,
            other => panic!("unknown surface {other}"),
        };
        let mut every = Vec::new();
        for row in fixture["rows"].as_array().unwrap() {
            if row["kind"] == "separator" {
                every.push(R::Separator);
                continue;
            }
            let id = row["id"].as_str().unwrap();
            let menu_row = row_of(id);
            assert_eq!(row["label"], menu_row.label(), "{id} label");
            use gpui_component::IconNamed as _;
            assert_eq!(
                menu_row.icon().map(|icon| icon.path()),
                crate::icons::registry::concept_by_name(row["icon"].as_str().unwrap())
                    .map(|icon| icon.path()),
                "{id} icon"
            );
            if let Some(codex) = row["codexLabel"].as_str() {
                assert_eq!(coding::CodingAgent::Codex.effort_label(), codex);
                assert_eq!(coding::CodingAgent::Claude.effort_label(), menu_row.label());
            }
            let surfaces: Vec<ComposerSurface> = row["surfaces"]
                .as_array()
                .unwrap()
                .iter()
                .map(|surface| surface_of(surface.as_str().unwrap()))
                .collect();
            assert_eq!(menu_row.surfaces(), surfaces.as_slice(), "{id} surfaces");
            every.push(menu_row);
        }
        let all = ComposerMenuConditions {
            temporary: true,
            device: true,
            account: true,
            model: true,
            repositories: true,
            subagent_model: true,
            ultracode: true,
            mcp: true,
            computer_use: true,
        };
        assert_eq!(composer_menu_rows(ComposerSurface::Launch, all), every, "rows order");
        for case in fixture["cases"].as_array().unwrap() {
            let when = &case["conditions"];
            let flag = |key: &str| when[key].as_bool().unwrap();
            let conditions = ComposerMenuConditions {
                temporary: flag("temporary"),
                device: flag("device"),
                account: flag("account"),
                model: flag("model"),
                repositories: flag("repositories"),
                subagent_model: flag("subagentModel"),
                ultracode: flag("ultracode"),
                mcp: flag("mcp"),
                computer_use: flag("computerUse"),
            };
            let expected: Vec<ComposerMenuRow> = case["expected"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| row_of(id.as_str().unwrap()))
                .collect();
            let surface = surface_of(case["surface"].as_str().unwrap());
            assert_eq!(composer_menu_rows(surface, conditions), expected, "{}", case["name"]);
            if surface != ComposerSurface::Launch {
                continue;
            }
            // The same launch case off a REAL launch-menu state.
            let menu = launch_options::ComposerMenu {
                effort_label: "Effort",
                effort_choices: &[("High", "high")],
                effort_picked: "high".into(),
                effort_locked: false,
                subagent_picked: conditions.subagent_model.then(String::new),
                ultracode: conditions.ultracode.then_some(false),
                mcp_servers: if conditions.mcp {
                    vec![launch_options::McpServerOption {
                        id: "m-1".into(),
                        name: "Linear".into(),
                        blocked: None,
                        enabled_by_default: true,
                        url: None,
                        command: None,
                    }]
                } else {
                    Vec::new()
                },
                mcp_selected: Vec::new(),
                computer_use: conditions.computer_use.then_some(false),
            };
            let repos = if conditions.repositories { 2 } else { 1 };
            // NEW-39: off real state the Subagents row always hides (web's
            // `subagentModel: false`); the fixture's own rows replay above.
            let expected: Vec<ComposerMenuRow> =
                expected.into_iter().filter(|row| *row != R::Subagents).collect();
            assert_eq!(
                composer_menu_rows(
                    ComposerSurface::Launch,
                    ComposerMenuConditions::launch(&menu, conditions.temporary, repos)
                ),
                expected,
                "{} (from state)",
                case["name"]
            );
        }
        // The issue picker's footer submit (`implementButton`, its test id
        // and every `implementLabels` case).
        assert_eq!(fixture["testIds"]["implementSubmit"], IMPLEMENT_SUBMIT_ID);
        assert_eq!(fixture["implementButton"]["one"], implement_button_label(1));
        assert_eq!(
            fixture["implementButton"]["many"]
                .as_str()
                .unwrap()
                .replace("{n}", "7"),
            implement_button_label(7)
        );
        for case in fixture["implementLabels"].as_array().unwrap() {
            let count = case["count"].as_u64().unwrap() as usize;
            assert_eq!(case["expected"], implement_button_label(count));
        }
    }

    /// EXP-993: the Repository pin seeds itself with the FIRST connected
    /// repo, whatever the count. A repo-less seed used to be the default
    /// with two or more, which quietly sent the common case to a scratch dir
    /// and made the agent ask; there is no repo-less ENTRY any more, so the
    /// seed has to be a real repo. Only a team with none stays repo-less.
    #[test]
    fn the_repository_pin_seeds_the_first_repo() {
        let row = |id: &str, full_name: &str| ActionRepoRow {
            id: id.to_string(),
            full_name: full_name.to_string(),
        };
        let only = action_run::preselect_repo(&[row("repo-1", "niach/exponential")]);
        assert_eq!(only.map(|repo| repo.id), Some("repo-1".to_string()));
        assert!(action_run::preselect_repo(&[]).is_none());
        let several = action_run::preselect_repo(&[
            row("repo-1", "niach/exponential"),
            row("repo-2", "niach/other"),
        ]);
        assert_eq!(several.map(|repo| repo.id), Some("repo-1".to_string()));
    }

    /// EXP-825: the field's hint follows the subject like the web
    /// `composerPlaceholder`: no subject asks the home's question, any
    /// picked subject takes extra instructions (NEW-2: prompts carry no
    /// composer hint).
    #[test]
    fn composer_placeholder_follows_the_subject() {
        // EXP-1253: no subject = the home's question.
        assert_eq!(CHAT_PLACEHOLDER, "What do you want to build?");
        assert_eq!(SUBJECT_PLACEHOLDER, "Additional instructions (optional)…");
        assert_eq!(placeholder_for_subject(&Subject::None), CHAT_PLACEHOLDER);
        let prompt = Subject::Action(ActionSubject { action_id: "p-1".to_string() });
        assert_eq!(placeholder_for_subject(&prompt), SUBJECT_PLACEHOLDER);
        let issues = Subject::Issues(IssueSubject {
            rows: Rc::default(),
            checked: HashSet::new(),
            repos: HashMap::new(),
        });
        assert_eq!(placeholder_for_subject(&issues), SUBJECT_PLACEHOLDER);
        // S-3: the agent row's pick never moves the subject, so an agent
        // picked keeps the home question (iOS / web parity).
    }
}
