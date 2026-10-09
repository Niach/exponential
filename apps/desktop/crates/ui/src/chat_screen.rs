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
//! │ +                              ( Start batch · 2 ) │
//! └───────────────────────────────────────────────┘
//!  Device ▾ · Account ▾ · Model ▾ · Plan ○ · Resume ○ · Repository ▾
//!  ⚡ suggestion   ⚡ suggestion   …   (quiet rows, empty chat only)
//! ```
//!
//! EXP-1249: the "+" is the composer's ONE tool — a menu (Implement issue ›,
//! Run action ›, Add file or image | Effort ›, Subagents ›, Ultracode |
//! MCP servers ›, Computer use). "Implement issue", "Run action" and "MCP
//! servers" open their searchable pickers anchored to the +.
//!
//! **Subject by swap** (decision 2026-09-10): issue chips OR one action chip.
//! Picking an issue while an action is picked replaces it, and the other way
//! round; the chips make it visible, no control is ever disabled. **Free
//! text** is the chat prompt with no subject, the Create-action builtin's
//! request, and optional additional instructions beside a subject
//! ([`chat_launch::text_required`]). **Images** ride the text as steer
//! embeds, uploaded to the team route before the session exists.
//!
//! **Options row B** under the card: Device, Account, Model, Plan on one
//! muted line (+ Resume while a single issue has a resumable worktree, +
//! Repository while no subject is picked); the rest of the run's options
//! live in the "+" menu. The options are the ONE launch model every desktop
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
//! EXP-790/EXP-820: suggestions for the EMPTY, subject-less field — four
//! drawn once per page from a pool byte-identical to the web's
//! `CHAT_SUGGESTIONS` (locked below). EXP-1249: quiet text rows UNDER the
//! options line, not pills over the card; a faint brand mark sits behind
//! the page's headline.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui::{
    div, px, AnyElement, App, AppContext as _, ClickEvent, Entity,
    FocusHandle, Focusable, InteractiveElement as _, IntoElement, ParentElement, Render,
    prelude::FluentBuilder as _, SharedString, StatefulInteractiveElement as _, Styled, Subscription,
    Window,
};
use gpui_component::input::{InputEvent, TextareaState};
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{
    h_flex, v_flex, ActiveTheme as _, Disableable as _, ElementExt as _, Icon, Sizable as _,
};
use sync::Store;

use coding::{
    run_registry::RunRecord, LaunchOptions, LaunchOrigin, Prepared, PrepareRequest,
    ResumeRunRequest,
};

use crate::action_inputs::ActionInputPicks;
use crate::action_run::{self, ActionRepo, ActionRepoRow, StartActionArgs};
use crate::chat_launch::{self, RemoteSubject, RepoState, SubjectKind};
use domain::blocked_start;
use crate::coding_flow::{self, CodingHub, SessionSubject};
use crate::composer_images::{self, PendingImages};
use crate::device_readiness;
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

/// EXP-1155: the parameter bands' caps — about three rows of issue chips and
/// four typed action inputs; past them each band scrolls on its own.
const CHIPS_MAX_H: f32 = 88.;
const FIELDS_MAX_H: f32 = 280.;

/// EXP-923: the history button's tooltip — the only word on the Agent page's
/// own chrome, and the panel's own heading is the ×4 "Recent".
const RECENT_RUNS_LABEL: &str = "Recent runs";

/// EXP-1249: the composer "+" menu's words (web `launch-composer.tsx`'s
/// menu, the styleguide `menu` entry's composer specimen).
pub(crate) const PLUS_TOOLTIP: &str = "Add";
pub(crate) const MENU_IMPLEMENT_ISSUE: &str = "Implement issue";
pub(crate) const MENU_RUN_ACTION: &str = "Run action";
pub(crate) const MENU_ADD_FILE: &str = "Add file or image";
pub(crate) const MENU_SUBAGENTS: &str = "Subagents";
pub(crate) const MENU_ULTRACODE: &str = "Ultracode";
pub(crate) const MENU_MCP_SERVERS: &str = "MCP servers";
pub(crate) const MENU_COMPUTER_USE: &str = "Computer use";

/// EXP-1249: how tall the "+" menu may grow, for the side-with-room fit.
const PLUS_MENU_WANTED_HEIGHT: f32 = 360.;

/// EXP-1249: the faint brand mark behind the Agent page's headline: its
/// edge and its ink (the foreground at ~3.5%).
const PAGE_MARK_SIZE: f32 = 520.;
const PAGE_MARK_ALPHA: f32 = 0.035;

/// EXP-1249: the searchable picker a "+" menu row opens, anchored to the +.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComposerPicker {
    Issues,
    Actions,
    McpServers,
}

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
    "Create an action that labels new issues",
    "Set up a weekly standup digest action",
    "Draft release notes from the issues completed this month",
    "Summarize what changed across the boards this week",
    "Start a run for # on my other machine",
    "Move stale in-progress issues back to the backlog",
    "Comment a plan on #",
    "Which issues are blocked, and by what?",
];

/// How many of the pool a page shows.
pub(crate) const CHAT_SUGGESTION_COUNT: usize = 3;

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

/// EXP-1249: whether the synced row advertises `computer-use-run` — the
/// machine reads a start's per-run `computerUse`, so the "+" menu may offer
/// the toggle for it. An unsynced row cannot say so: no toggle.
fn device_reads_computer_use(row_id: &str, cx: &App) -> bool {
    if row_id.is_empty() {
        return false;
    }
    let collections = Store::global(cx).collections();
    let devices = collections.devices.read(cx);
    devices
        .iter()
        .find(|row| row.id == row_id)
        .is_some_and(|row| {
            row.cap_ids()
                .iter()
                .any(|cap| cap == coding::doctor::COMPUTER_USE_RUN_CAP)
        })
}

/// EXP-862 — a machine's glyph for the device picker: EXP-924 made it the
/// owner's PICK when the row carries one, and otherwise the kind default the
/// Devices list and Getting started wear (the headless CLI daemon is a SERVER,
/// everything else a desktop). An unsynced row reads as a plain desktop, which
/// is what this IDE is.
fn device_kind_icon(device_id: &str, cx: &App) -> crate::icons::ExpIcon {
    // EXP-1030: through the ONE resolver the device picker's rows go through,
    // so the pin's glyph and its menu's glyphs cannot come apart.
    crate::icons::device_icon(
        Some(crate::launch_options::device_glyph_name(device_id, cx)),
        false,
    )
}

/// The checked issues and everything their launch needs.
struct IssueSubject {
    /// The picker's pool (open team issues + the seeded ones).
    rows: Rc<Vec<IssueRow>>,
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
    /// EXP-1233: a merge refused by a REAL conflict opened the composer on
    /// the Fix merge conflicts builtin (the seed's `conflict`), so its card
    /// adds the refusal line. Lives on the subject, so any other pick clears it.
    conflict_refused: bool,
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
/// EXP-1019: both hints are the shared contract's, never local literals.
const CHAT_PLACEHOLDER: &str = domain::contract::COMPOSER_UI_CHAT_PLACEHOLDER;
const SUBJECT_PLACEHOLDER: &str = domain::contract::COMPOSER_UI_INSTRUCTIONS_PLACEHOLDER;

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

/// EXP-1233 — one of the picked pull request's issues as a headline chip:
/// THE issue chip the issues subject draws (status glyph · mono identifier ·
/// title), id `chat-chip-issue-<IDENT>`, its ✕ titled "Clear the pull
/// request" (the caller wires what it does). The styleguide draws it too.
pub(crate) fn fix_conflicts_issue_chip(
    issue: &domain::rows::Issue,
    cx: &App,
) -> crate::issue_chip::IssueChip {
    crate::issue_chip::issue_chip(
        SharedString::from(format!("chat-chip-issue-{}", issue.identifier)),
        issue.identifier.clone(),
        issue.title.clone(),
    )
    .status(queries::resolve_issue_status(cx, issue))
    .max_title_width(px(220.))
    .remove_tooltip("Clear the pull request")
}

/// EXP-1233 — the fix-conflicts card's PR ROW as a ghost [`Button`], so the
/// composer can hang the `pr` dropdown on it. Its content is THE PR row
/// ([`crate::pr_rows::pr_row`]: the open-PR node, `#n` as the mono
/// identifier, `branch → base` as the title) with the dropdown chevron
/// trailing; nothing picked = the contract's muted placeholder alone.
pub(crate) fn fix_conflicts_pr_row(
    id: impl Into<gpui::ElementId>,
    pr: Option<&domain::fix_conflicts::FixConflictsPr>,
    cx: &App,
) -> Button {
    let theme = cx.theme();
    let chevron = Icon::new(registry::UI_CHEVRON_DOWN)
        .size_3p5()
        .text_color(theme.foreground.opacity(0.5));
    let content = match pr {
        Some(pr) => {
            let mut spec = crate::pr_rows::PrRowSpec::open(
                "chat-fix-conflicts-pr-row",
                pr.branch_line(),
            );
            spec.identifier = pr.pr_number.map(|number| SharedString::from(format!("#{number}")));
            spec.trailing = Some(div().flex_shrink_0().child(chevron).into_any_element());
            crate::pr_rows::pr_row(spec, cx)
        }
        None => h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .text_sm()
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .text_color(theme.muted_foreground)
                    .child(domain::contract::COMPOSER_UI_PR_PLACEHOLDER),
            )
            .child(div().ml_auto().flex_shrink_0().child(chevron))
            .into_any_element(),
    };
    Button::new(id)
        .ghost()
        .cursor_pointer()
        .w_full()
        .h_auto()
        .px_1()
        .py_1()
        .rounded_none()
        .child(content)
}

/// EXP-1233 — the fix-conflicts card's SURFACE: the form ladder's glass
/// group holding the PR `row` and, when `refused`, the refusal line under a
/// hairline (the warning glyph + the contract's note, destructive tone).
pub(crate) fn fix_conflicts_card(
    id: impl Into<gpui::ElementId>,
    row: AnyElement,
    refused: bool,
    cx: &App,
) -> AnyElement {
    let danger = cx.theme().danger;
    crate::surface::glass_group()
        .id(id)
        .child(row)
        .when(refused, |card| {
            card.child(crate::surface::glass_row_divider(
                h_flex()
                    .id("chat-fix-conflicts-note")
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_2()
                    .px_4()
                    .py(px(10.))
                    .text_sm()
                    .text_color(danger)
                    .child(Icon::new(registry::UI_WARNING).size_4().flex_shrink_0())
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(domain::contract::COMPOSER_UI_CONFLICT_NOTE),
                    ),
            ))
        })
        .into_any_element()
}

/// EXP-1037 — WHERE this composer is drawn. One view, one set of state, one
/// `send`/`start`; only the chrome around the launcher differs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Presentation {
    /// The Agent screen (`Screen::Chat`): the centred column, the suggestion
    /// band over an empty subject-less draft, and the "Recent runs" button.
    Page,
    /// A dialog window opened by a play button ([`crate::composer_dialog`]):
    /// the launcher alone — headline, card, options, notes — and nothing of
    /// the page's chrome. It always has a subject, so it never shows
    /// suggestions.
    Dialog,
}

pub(crate) struct ChatScreenView {
    nav: Entity<Navigation>,
    presentation: Presentation,
    /// EXP-1037: the dialog's seed, applied on the first render that has a
    /// team AND synced shapes (the page takes the same seed off the nav).
    dialog_seed: Option<ChatSeed>,
    /// EXP-1037/EXP-897: the blocked-start question while the composer is in
    /// a DIALOG — asked inside the dialog instead of as a nested alert (see
    /// [`Self::prompt_blocked_start`]).
    blocked: Option<PendingBlocked>,
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
    /// FEED-50: a Tidy up seed's board, filling the builtin's `board` input.
    pending_board: Option<String>,
    /// EXP-1233: the seed's `conflict` flag, riding the pending action to
    /// [`Self::select_action`] (fix-conflicts builtin only).
    pending_conflict: bool,
    /// Release review R5: the `#` picker's ranked list, memoised so a busy
    /// run's 60 fps repaint never re-ranks the pool. EXP-1030: the picker's
    /// QUERY and its keyboard cursor are the primitive's now — this memo is
    /// all the host still holds.
    issue_pick_memo: RefCell<issue_picker::VisibleRowsMemo>,
    /// EXP-868: the issue picker's pool while nothing is picked, keyed by the
    /// team and the revisions of every collection it reads. The composer
    /// renders on EVERY window redraw (a caret blink, a keystroke anywhere),
    /// and rebuilding the pool each time made typing crawl.
    team_pool: RefCell<Option<(TeamPoolKey, Rc<Vec<IssueRow>>)>>,
    /// EXP-946/EXP-1249: where the "+" tool actually painted, captured at
    /// prepaint (the `Popup` recipe). Its menu and the pickers it opens pick
    /// the side with room and cap themselves to it — gpui-component's
    /// `Popover` clamps but never flips, so a picker taller than the room
    /// above ran off the window.
    plus_tool_bounds: Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    /// EXP-1249: the picker a "+" menu row opened (anchored to the +), until
    /// it closes itself.
    open_picker: Option<ComposerPicker>,
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
    /// EXP-897: the blocked-issue dialog was answered "Start anyway" for the
    /// ONE start it was opened for, so `start` runs straight through. Cleared
    /// by [`Self::after_started`] and by every subject change.
    blocked_confirmed: bool,
    /// SLOP-3: a "Stacked PR" answer's issue to start (`run[0]` of the
    /// plan, not necessarily the picked one). Taken by the ONE
    /// [`Self::start`] pass it was given for.
    stacked_issue: Option<String>,
    /// EXP-792: the team's MCP servers with the person's own connection to
    /// each, one fetch per team; `None` while the fetch is out.
    mcp: Option<Vec<api::mcp_servers::McpServerListEntry>>,
    mcp_team: Option<String>,
    images: PendingImages,
    /// The image strip's notice (too many / too big).
    notice: Option<SharedString>,
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
    /// EXP-923: the page's one scroll. It holds the composer and NOTHING
    /// else — the live runs moved to the rail and the finished ones behind
    /// the history button's panel, so the Agent page is a composer again.
    page_scroll: gpui::ScrollHandle,
    /// EXP-1155: the two PARAMETER bands that scroll on their own once they
    /// pass their cap (the subject chips, the action's typed inputs), so the
    /// rest of the launcher never has to.
    chips_scroll: gpui::ScrollHandle,
    fields_scroll: gpui::ScrollHandle,
    /// EXP-1155: the dialog window follows its content (`None` on the page).
    dialog_fit: Option<DialogFit>,
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
                    // The suggestion chips and the blocker are functions of
                    // the draft.
                    InputEvent::Change => cx.notify(),
                    _ => {}
                },
            ),
            cx.observe(&nav, |_, _, cx| cx.notify()),
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
            presentation: Presentation::Page,
            dialog_seed: None,
            blocked: None,
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
            pending_board: None,
            pending_conflict: false,
            issue_pick_memo: RefCell::new(issue_picker::VisibleRowsMemo::default()),
            team_pool: RefCell::new(None),
            plus_tool_bounds: Rc::new(std::cell::Cell::new(gpui::Bounds::default())),
            open_picker: None,
            team_repos: Vec::new(),
            repos_team: None,
            chat_repo: None,
            launch: None,
            device: DevicePick::default(),
            blocked_confirmed: false,
            stacked_issue: None,
            mcp: None,
            mcp_team: None,
            images: PendingImages::default(),
            notice: None,
            sending: false,
            launching: false,
            error: None,
            suggestions: pick_chat_suggestions(suggestion_seed()),
            spare_picks: ActionInputPicks::default(),
            focus_handle: cx.focus_handle(),
            page_scroll: gpui::ScrollHandle::new(),
            chips_scroll: gpui::ScrollHandle::new(),
            fields_scroll: gpui::ScrollHandle::new(),
            dialog_fit: None,
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

    /// Whether a start is in flight — the dialog's close gate (the view owns
    /// the launch the prepare reports back to).
    pub(crate) fn starting(&self) -> bool {
        self.launching || self.sending
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
        self.probe_generation += 1;
        self.pending_action = None;
        self.pending_pr = None;
        self.pending_icon = None;
        self.pending_board = None;
        self.pending_conflict = false;
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
            self.pending_conflict =
                seed.conflict && action_id == api::actions::BUILTIN_FIX_CONFLICTS_ID;
            self.pending_action = Some(action_id);
            self.pending_pr = seed.pr_issue_id;
            self.pending_icon = seed.icon;
            self.pending_board = seed.board_id;
            self.refresh_actions(window, cx);
        } else if !seed.issue_ids.is_empty() {
            self.pending_conflict = false;
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
            rows: Rc::new(rows),
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
    /// the last uncheck (the label then reads "Start chat" again).
    fn toggle_issue(&mut self, issue_id: String, on: bool, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        // EXP-897: a different set of issues is a different question.
        self.blocked_confirmed = false;
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
        // EXP-897: the blocked-issue answer belongs to the subject it was
        // given for — a new subject asks again.
        self.blocked_confirmed = false;
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
        let conflict_refused = std::mem::take(&mut self.pending_conflict)
            && action_id == api::actions::BUILTIN_FIX_CONFLICTS_ID;
        self.subject = Subject::Action(ActionSubject {
            action_id: action_id.clone(),
            picks: ActionInputPicks::default(),
            conflict_refused,
        });
        if !had_subject {
            if let Some(launch) = self.launch.as_mut() {
                launch.reseed_plan_for_subject(true, cx);
            }
        }
        self.seed_action_repo_inputs();
        let pending_pr = self.pending_pr.take();
        let pending_icon = self.pending_icon.take();
        let pending_board = self.pending_board.take();
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
        // FEED-50: the board list's Tidy up button names its board; a board
        // that no longer resolves in the team leaves the pick empty.
        if let Some(board_id) = pending_board {
            let name = Store::global(cx)
                .collections()
                .boards_in_team(&action.team_id, cx)
                .into_iter()
                .find(|board| board.id == board_id)
                .map(|board| board.name);
            if let Some(name) = name {
                subject.picks.preselect_board(&action, &board_id, &name);
            }
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

    /// EXP-792: one `mcpServers.list` per team (it carries the person's own
    /// connection to each server).
    fn ensure_mcp_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        if self.team_id == self.mcp_team {
            return;
        }
        self.mcp_team = self.team_id.clone();
        self.mcp = None;
        let (Some(team), Some(trpc)) = (self.team_id.clone(), queries::trpc_client(cx)) else {
            return;
        };
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
                    computer_use_run: device_reads_computer_use(&device.row_id, cx),
                }
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
                let agent = match self.resume_active(cx) {
                    true => self
                        .resume_candidate()
                        .map(|(_, record)| record.agent)
                        .unwrap_or(launch.agent),
                    false => launch.agent,
                };
                let hub = CodingHub::global_ref(cx)?;
                let hub = hub.read(cx);
                let doctor = hub.doctor.device.as_ref()?;
                Some((device_readiness::blocking_row(doctor, agent, true)?, None))
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
                device_readiness::remote_props("chat-readiness", device_id, label)
            }
        };
        device_readiness::render_single(row, props, cx)
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
            // EXP-1233: the Fix merge conflicts builtin with a PR picked
            // wears its own verb and send.
            Subject::Action(_) if self.fix_conflicts_pr_id().is_some() => SubjectKind::FixConflicts,
            Subject::Action(action) => SubjectKind::Action {
                id: action.action_id.clone(),
            },
        }
    }

    /// EXP-1233: the representative issue id picked into the Fix merge
    /// conflicts builtin's `pr` input; `None` for any other subject or while
    /// nothing is picked.
    fn fix_conflicts_pr_id(&self) -> Option<&str> {
        let Subject::Action(subject) = &self.subject else {
            return None;
        };
        if subject.action_id != api::actions::BUILTIN_FIX_CONFLICTS_ID {
            return None;
        }
        subject.picks.pr_value(self.selected_action()?)
    }

    /// EXP-1233: the picked pull request off the synced issue rows (every
    /// open issue it links — a batch PR — its number and branches).
    fn fix_conflicts_pr(&self, cx: &App) -> Option<domain::fix_conflicts::FixConflictsPr> {
        let pr_issue_id = self.fix_conflicts_pr_id()?;
        let store = Store::try_global(cx)?;
        let issues = store.collections().issues.read(cx);
        domain::fix_conflicts::resolve_fix_conflicts_pr(Some(pr_issue_id), issues.iter())
    }

    /// EXP-1233: the headline chip's ✕ — clear the PR pick, keep the action.
    fn clear_fix_conflicts_pr(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(action) = self.selected_action().cloned() else {
            return;
        };
        if let Subject::Action(subject) = &mut self.subject {
            subject.picks.clear_pr(&action);
        }
        cx.notify();
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
            return Some("Sign in and wait for sync before starting a run.".into());
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
                for row in issues.rows.iter() {
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
                                "{} already has a live run on {device} (one run per issue).",
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
    /// FEED-73: a real action's run takes the action's own MCP list
    /// (server-side), never the composer's pick.
    fn options(&self, cx: &App) -> LaunchOptions {
        let mut options = self.launch_ref().options(self.resume_active(cx), cx);
        if chat_launch::subject_owns_mcp_servers(&self.subject_kind()) {
            options.mcp_server_ids.clear();
        }
        options
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
        // SLOP-3: a Stacked PR answer starts THIS issue (run[0]) in place of
        // the pick, on the same launcher, device and settings.
        let stacked_issue = self.stacked_issue.take();
        let Some(team_id) = self.team_id.clone() else {
            return;
        };
        // EXP-897/EXP-980/SLOP-3: a pick that something OPEN blocks asks
        // first: Cancel, Start anyway or Stacked PR. Asked once per subject;
        // the answer rides `blocked_confirmed` into the second pass (a
        // stacked answer only rewrites the message). A BATCH asks too, about
        // the blockers outside it.
        if !self.blocked_confirmed {
            if let Some(blocked) = self.open_blockers(cx) {
                self.prompt_blocked_start(message, blocked, window, cx);
                return;
            }
        }
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
                    let mut checked: Vec<String> = match &stacked_issue {
                        Some(issue_id) => vec![issue_id.clone()],
                        None => issues
                            .rows
                            .iter()
                            .filter(|row| issues.checked.contains(&row.issue_id))
                            .map(|row| row.issue_id.clone())
                            .collect(),
                    };
                    let subject = match checked.len() {
                        0 => return,
                        1 => RemoteSubject::Issue {
                            issue_id: &checked.pop().expect("one checked"),
                            resume: stacked_issue.is_none() && self.resume_active(cx),
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
                // EXP-1037: a chat run's rails hang off the WINDOW — the
                // session-bar host that owns the launch and the tab the run
                // lands in. A dialog composer has neither, so the launch
                // runs in the window that opened it (for the page, that IS
                // this window, one tick later).
                let target = navigation::deferred_open_window(window, cx);
                if crate::session_bar::host_for_window_id(target.window_id(), cx).is_none() {
                    self.error = Some("Open a team window to start a chat.".into());
                    cx.notify();
                    return;
                }
                let repo = self
                    .chat_repo
                    .as_ref()
                    .map(|repo| (repo.id.clone(), repo.full_name.clone()));
                navigation::defer_in_result_window(window, cx, move |window, cx| {
                    let Some(host) = crate::session_bar::host_for_window(window, cx) else {
                        return;
                    };
                    host.update(cx, |host, cx| {
                        host.launch_chat_run(options, repo, prompt, window, cx);
                    });
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
                        // EXP-1037: the run's tab lands in the window the ▶
                        // was pressed in — a dialog composer closes itself.
                        target: Some(navigation::deferred_open_window(window, cx)),
                        activate_app: false,
                        reservation: None,
                        // A person pressed Run — never an automation firing.
                        trigger: None,
                        automation_id: None,
                        prompt,
                        on_settled: None,
                        report: crate::steer_wiring::StartReport::none(),
                    },
                    cx,
                );
                self.after_started(window, cx);
            }
            Subject::Issues(issues) => {
                if stacked_issue.is_some() || issues.checked.len() == 1 {
                    let issue_id = match &stacked_issue {
                        Some(issue_id) => issue_id.clone(),
                        None => issues.checked.iter().next().cloned().expect("one checked"),
                    };
                    // EXP-662: an active resume relaunches the RECORDED run
                    // exactly; only model/effort may be nudged, and only
                    // while the pick sits on that same agent (D2). A stacked
                    // start never resumes (the blocked question skips a
                    // resume, and its issue may not be the pick).
                    let record = (stacked_issue.is_none() && self.resume_active(cx))
                        .then(|| self.resume_candidate().map(|(_, record)| record.clone()))
                        .flatten();
                    if let Some(record) = record {
                        let same_agent = options.agent == record.agent;
                        let Some(deps) = coding_flow::build_resume_deps(&record, cx) else {
                            self.error = Some("Sign in and wait for sync before starting a run.".into());
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
                        // EXP-1158: an account picked for the resume is a
                        // person's switch — it moves the last used login (a
                        // LOCAL resume is also a rotation hop's shape, so the
                        // launcher leaves this one to its caller).
                        if let Some(account) = &request.account {
                            coding::record_last_used(
                                &coding_flow::coding_data_dir(cx),
                                request.record.agent,
                                Some(account),
                            );
                        }
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
                        self.error = Some("Sign in and wait for sync before starting a run.".into());
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
                    self.error = Some("Sign in and wait for sync before starting a run.".into());
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

    /// EXP-897/EXP-980 — the OPEN issues blocking the pick, or `None` when
    /// the blocked-start question does not apply at all: no issue subject, a
    /// resume (it re-enters an existing worktree, base included), or nothing
    /// unfinished in the way.
    ///
    /// A BATCH asks about the blockers OUTSIDE the picked set only — a
    /// blocker picked into the same batch is not in its way. "Unfinished" is
    /// the ANCHOR status: `done`, `cancelled` and `duplicate` blockers are
    /// history, everything else still has work to land.
    fn open_blockers(&self, cx: &App) -> Option<BlockedStart> {
        let Subject::Issues(issues) = &self.subject else {
            return None;
        };
        if issues.checked.is_empty() || self.resume_active(cx) {
            return None;
        }
        // The picker's order, so the dialog reads like the composer.
        let picked: Vec<String> = issues
            .rows
            .iter()
            .filter(|row| issues.checked.contains(&row.issue_id))
            .map(|row| row.issue_id.clone())
            .collect();
        if picked.is_empty() {
            return None;
        }
        let refs: Vec<&str> = picked.iter().map(String::as_str).collect();
        let blockers = crate::issue_graph::open_blockers_outside(&refs, cx);
        if blockers.is_empty() {
            return None;
        }
        let (stack, stack_note) = stack_choice(&picked, cx);
        Some(BlockedStart {
            picked,
            blockers,
            stack,
            stack_note,
        })
    }

    /// EXP-897/EXP-980/SLOP-3: the blocked-start dialog: the sentence, the
    /// transitive blocks GRAPH underneath it, and Cancel / Start anyway /
    /// Stacked PR (primary). Both answers record the confirmation and
    /// re-enter [`Self::start`]; Stacked PR only swaps the composed message
    /// for [`BlockedStart::stacked_message`], so the two paths stay one code
    /// path. Stacked PR is never hidden: without a stack base it is DISABLED
    /// and its reason is captioned under the graph.
    fn prompt_blocked_start(
        &mut self,
        message: String,
        blocked: BlockedStart,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // EXP-1037: in a dialog window there is no alert to open —
        // `open_dialog_window` never nests (`dialog_open_here`), and closing
        // this window to ask in the opener would drop the view holding the
        // draft. The question takes over the dialog's body instead
        // ([`Self::render_blocked_panel`]), answered by the same
        // `blocked_confirmed` + `start` pair.
        if self.in_dialog() {
            self.blocked = Some(PendingBlocked { message, blocked });
            cx.notify();
            return;
        }
        let refs: Vec<&str> = blocked.picked.iter().map(String::as_str).collect();
        let graph = crate::issue_graph::graph_for(&refs, cx);
        let stacked = blocked.stacked_start(&message);

        let entity = cx.entity().downgrade();
        let opener = window.window_handle();
        let resume = move |message: String, stacked_issue: Option<String>, cx: &mut App| {
            let entity = entity.clone();
            let _ = opener.update(cx, move |_, window, cx| {
                let _ = entity.update(cx, |this, cx| {
                    this.blocked_confirmed = true;
                    this.stacked_issue = stacked_issue;
                    this.start(message, window, cx);
                });
            });
        };
        let anyway = resume.clone();
        let spec = blocked_start_alert(
            &blocked,
            graph,
            move |_, cx| {
                anyway(message.clone(), None, cx);
                true
            },
            move |_, cx| {
                if let Some((issue_id, message)) = stacked.clone() {
                    resume(message, Some(issue_id), cx);
                }
                true
            },
        );
        crate::native_dialog::open_alert(window, cx, spec);
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
            self.error = Some("Sign in and wait for sync before starting a run.".into());
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
                        // EXP-1037: a dialog composer is closing itself, so
                        // the toast belongs to the window the ▶ was pressed
                        // in (on the page that IS this window).
                        let notice = SharedString::from(format!(
                            "Start sent to {device_label}."
                        ));
                        navigation::defer_in_result_window(window, cx, move |window, cx| {
                            crate::toast::success(notice, window, cx);
                        });
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
        self.blocked_confirmed = false;
        self.blocked = None;
        self.clear_subject(cx);
        // EXP-1037: a dialog composer has done its one job — the run opens in
        // the window the play button was pressed in (the navigation hands
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
        Some(
            h_flex()
                .w_full()
                .min_w_0()
                .flex_wrap()
                .items_center()
                .gap_2()
                .px_1()
                .child(
                    div()
                        .flex_shrink_0()
                        .text_lg()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(cx.theme().foreground)
                        .child(chat_launch::headline(&kind)),
                )
                .child(chips)
                .into_any_element(),
        )
    }

    /// The subject chips: one per checked issue, or the action's one. They
    /// sit in the HEADLINE (EXP-1019), not in the card's leading slot.
    /// EXP-1233: the Fix merge conflicts builtin with a PR picked shows the
    /// PR's ISSUE chips instead (the ones "Implement" draws); their ✕ clears
    /// the pick, never the action.
    fn render_chips(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let muted = cx.theme().muted_foreground;
        let mut chips: Vec<AnyElement> = Vec::new();
        let fix_pr = self.fix_conflicts_pr(cx);
        match &self.subject {
            Subject::None => return None,
            Subject::Action(_) if fix_pr.is_some() => {
                for issue in fix_pr.iter().flat_map(|pr| pr.issues.iter()) {
                    chips.push(
                        fix_conflicts_issue_chip(issue, cx)
                            .on_remove(
                                SharedString::from(format!(
                                    "chat-chip-issue-{}-remove",
                                    issue.identifier
                                )),
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.clear_fix_conflicts_pr(cx);
                                }),
                            )
                            .into_any_element(),
                    );
                }
            }
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
        // EXP-1155: a 30-issue batch wraps into many chip rows; past the cap
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

    /// The picked action's remaining typed inputs (repo/board/pr/icon).
    /// EXP-1233: the Fix merge conflicts builtin's `pr` input is its CARD
    /// ([`Self::render_fix_conflicts_card`]), not the generic dropdown.
    fn render_action_fields(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let Subject::Action(subject) = &self.subject else {
            return None;
        };
        let action = self.selected_action()?.clone();
        if action.inputs.is_empty() {
            return None;
        }
        let team_id = action.team_id.clone();
        let fix_conflicts = action.id == api::actions::BUILTIN_FIX_CONFLICTS_ID;
        let mut fields = v_flex().w_full().gap_2().px_1().py_1();
        for (ix, input) in action.inputs.iter().enumerate() {
            if fix_conflicts && input.input_type == "pr" {
                fields = fields.child(self.render_fix_conflicts_card(input, &team_id, cx));
                continue;
            }
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
        // EXP-1155: the inputs grow the dialog up to a cap, then scroll in
        // their own band so the text field and the send stay in view.
        Some(
            crate::scroll_pane::capped_v_scroll(
                "chat-fields-scroll",
                &self.fields_scroll,
                px(FIELDS_MAX_H),
                fields,
            )
            .w_full()
            .into_any_element(),
        )
    }

    /// EXP-1233 — the Fix merge conflicts CARD: a glass group whose first
    /// row IS the `pr` picker (the open-PR glyph, `#n`, `branch → base`, the
    /// chevron; the placeholder alone while nothing is picked) opening the
    /// generic field's own dropdown ([`crate::action_inputs::pr_menu`]), and,
    /// only when a refused merge opened the composer, the refusal line under
    /// a hairline. Mirrors web `FixConflictsCard`.
    fn render_fix_conflicts_card(
        &self,
        input: &api::actions::ActionInput,
        team_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let pr = self.fix_conflicts_pr(cx);
        let refused = matches!(&self.subject, Subject::Action(subject) if subject.conflict_refused);
        let pulls = crate::action_inputs::pr_pick_options(cx, team_id);
        let row = fix_conflicts_pr_row("chat-fix-conflicts-pr", pr.as_ref(), cx)
            .dropdown_menu(crate::action_inputs::pr_menu(
                cx.entity().downgrade(),
                input.key.clone(),
                !input.required,
                pulls,
                Self::picks_access,
            ))
            .into_any_element();
        fix_conflicts_card("chat-fix-conflicts-card", row, refused && pr.is_some(), cx)
    }

    /// EXP-868: the open team pool behind the issue picker, rebuilt only when
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

    /// "+" → Implement issue ›: the issue picker, mounted on THE picker
    /// primitive (EXP-1030) and OPENED by the menu row (EXP-1249), anchored
    /// to the + through `trigger`. Everything the primitive cannot know
    /// rides in as hooks — the ranking (`domain::issue_search` through
    /// [`issue_picker::visible_rows`], memoised here), the row anatomy and
    /// the overflow notes — and everything else (the surface, the filter
    /// field, ↑/↓/Enter, the multi-select highlight) is its own.
    fn issue_picker_surface(
        &self,
        trigger: AnyElement,
        fit: (gpui::Anchor, gpui::Pixels),
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let (rows, checked, notes): (Rc<Vec<IssueRow>>, HashSet<String>, Vec<(String, SharedString)>) =
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
                        .map(|team| self.team_pool(team, cx))
                        .unwrap_or_default(),
                    HashSet::new(),
                    Vec::new(),
                ),
            };
        let items = issue_picker::picker_items(&rows);
        let picked: Vec<String> = rows
            .iter()
            .filter(|row| checked.contains(&row.issue_id))
            .map(|row| row.issue_id.clone())
            .collect();
        let view = cx.entity().downgrade();
        // Where a row's body reads its data from: the pool by id, and the
        // transient probe note ("no repository linked") beside it.
        let bodies: Rc<HashMap<String, usize>> = Rc::new(
            rows.iter()
                .enumerate()
                .map(|(ix, row)| (row.issue_id.clone(), ix))
                .collect(),
        );
        let notes: Rc<HashMap<String, SharedString>> = Rc::new(notes.into_iter().collect());
        // ONE ranking per frame, memoised on this view: `rank` and `footer`
        // both read it, and the composer repaints at 60 fps while a run is
        // busy (release review R5).
        let ranked = {
            let rows = rows.clone();
            let checked = checked.clone();
            let view = view.clone();
            move |query: &str, cx: &mut App| -> (Vec<usize>, usize, bool) {
                match view.upgrade() {
                    Some(host) => host
                        .read(cx)
                        .issue_pick_memo
                        .borrow_mut()
                        .get(&rows, &checked, query),
                    None => issue_picker::visible_rows(&rows, &checked, query),
                }
            }
        };
        let rank_of = ranked.clone();
        let footer_of = ranked;
        let empty_pool = rows.is_empty();
        let body_rows = rows.clone();
        let on_close = self.picker_on_close(cx);
        crate::picker::deferred(move |window, cx| {
            crate::picker::Picker::multi(
                items,
                picked,
                trigger,
                Rc::new(move |values: Vec<String>, window: &mut Window, cx: &mut App| {
                    // The primitive reports the WHOLE new set; the composer
                    // stores one toggle at a time, so the one row that moved
                    // is the difference.
                    let Some(view) = view.upgrade() else {
                        return;
                    };
                    let before = view.read(cx).checked_issue_ids();
                    let after: HashSet<String> = values.into_iter().collect();
                    let added = after.difference(&before).next().cloned();
                    let removed = before.difference(&after).next().cloned();
                    let Some((issue_id, on)) = added
                        .map(|id| (id, true))
                        .or_else(|| removed.map(|id| (id, false)))
                    else {
                        return;
                    };
                    view.update(cx, |this, cx| this.toggle_issue(issue_id, on, window, cx));
                }),
            )
            .search(true)
            .id("chat-issue-picker")
            .open(on_close)
            // The run cap (contract ×4): at it the unpicked rows go
            // disabled, a checked one still toggles off.
            .max(issue_picker::MAX_ISSUES_PER_RUN)
            .width(px(480.))
            .fit(fit)
            .empty_text("No open issues in this team.")
            // EXP-892: the ONE engine ranks, checked rows pinned first.
            .rank(move |_items, query, cx| rank_of(query, cx).0)
            .render_item(move |item, cx| {
                let Some(row) = bodies
                    .get(&item.value)
                    .and_then(|ix| body_rows.get(*ix))
                else {
                    return gpui::Empty.into_any_element();
                };
                issue_picker::issue_row_body(row, notes.get(&row.issue_id).cloned(), cx)
            })
            .footer(move |query, cx| {
                let (_, hidden, no_matches) = footer_of(query, cx);
                let mut notes: Vec<SharedString> = Vec::new();
                if empty_pool {
                    notes.push("No open issues in this team.".into());
                }
                if no_matches {
                    notes.push("No matches. Only open issues are shown.".into());
                }
                if hidden > 0 {
                    notes.push(format!("+{hidden} more. Refine your search.").into());
                }
                if notes.is_empty() {
                    return None;
                }
                Some(
                    v_flex()
                        .w_full()
                        .children(
                            notes
                                .into_iter()
                                .map(|note| issue_picker::list_note(note, cx)),
                        )
                        .into_any_element(),
                )
            })
            .render(window, cx)
        })
        .into_any_element()
    }

    /// "+" → Run action ›: the actions picker (builtins pinned first, Create
    /// action included; Chat is never listed) — THE action picker
    /// (EXP-1030), whose rows are the curated icon, the name and the muted
    /// description — opened by the menu row, anchored to the + (EXP-1249).
    fn action_picker_surface(
        &self,
        trigger: AnyElement,
        fit: (gpui::Anchor, gpui::Pixels),
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let actions: Vec<crate::picker::action_picker::ActionPickerAction> = self
            .actions
            .iter()
            .filter(|action| action.id != api::actions::BUILTIN_CHAT_ID)
            .map(|action| crate::picker::action_picker::ActionPickerAction {
                id: action.id.clone(),
                name: action.name.clone(),
                icon: action.icon.clone(),
                description: action
                    .description
                    .as_deref()
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .map(str::to_string),
            })
            .collect();
        let ready = self.actions_ready;
        let picked = match &self.subject {
            Subject::Action(subject) => Some(subject.action_id.clone()),
            _ => None,
        };
        let view = cx.entity().downgrade();
        let on_close = self.picker_on_close(cx);
        crate::picker::deferred(move |window, cx| {
            crate::picker::action_picker::action_picker(
                &actions,
                picked,
                trigger,
                Rc::new(move |values: Vec<String>, _window: &mut Window, cx: &mut App| {
                    let Some(id) = values.into_iter().next() else {
                        return;
                    };
                    if let Some(view) = view.upgrade() {
                        view.update(cx, |this, cx| this.select_action(id, cx));
                    }
                }),
            )
            .id("chat-action-picker")
            .open(on_close)
            .width(px(360.))
            .fit(fit)
            // A list that has not arrived says so; an arrived empty one says
            // there is nothing to run.
            .empty_text(if ready {
                "No actions yet."
            } else {
                "Loading actions…"
            })
            .render(window, cx)
        })
        .into_any_element()
    }

    /// "+" → MCP servers ›: the team's servers behind the shared multi
    /// picker (EXP-792), opened by the menu row and anchored to the +
    /// (EXP-1249). A blocked server reads its reason as the second line and
    /// DIMS, but still toggles (the launch starts without it — a warning,
    /// never a blocker).
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
        let view = cx.entity().downgrade();
        let trigger = crate::composer::composer_tool("chat-tool-plus", registry::UI_ADD, cx)
            .tooltip(PLUS_TOOLTIP)
            .disabled(self.sending);
        let menu = crate::controls::PointerMenu::new("chat-tool-plus-menu", trigger, {
            move |menu, window, cx| plus_menu(menu, &view, options.as_ref(), window, cx)
        })
        .anchor(menu_fit.0);

        // The picker's anchor: as tall as the + (the fit measures the room
        // above from it), no width, nothing to hit.
        let anchor = || div().w(px(0.)).h(bounds.size.height.max(px(24.))).into_any_element();
        let picker = match self.open_picker {
            Some(ComposerPicker::Issues) => Some(self.issue_picker_surface(anchor(), picker_fit, cx)),
            Some(ComposerPicker::Actions) => Some(self.action_picker_surface(anchor(), picker_fit, cx)),
            Some(ComposerPicker::McpServers) => self.mcp_picker_surface(anchor(), picker_fit, cx),
            None => None,
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
        // EXP-1030: the rows and their glyphs are THE device picker's now
        // ([`crate::picker::device_picker`]); only the inline pin trigger is
        // the composer's own.
        let selected_kind = self
            .device
            .device_id
            .as_deref()
            .map(|id| device_kind_icon(id, cx))
            .unwrap_or(registry::UI_DEVICE);
        let rows = crate::launch_options::launch_device_rows(&candidates, cx);
        let view = cx.entity().downgrade();
        let trigger = crate::launch_options::inline_pin_trigger_with(
            "chat-pin-device".into(),
            Some(selected_kind),
            label,
            cx,
        )
        .into_any_element();
        crate::picker::deferred(move |window, cx| {
            crate::picker::device_picker::device_picker(
                &rows,
                bound.clone(),
                trigger,
                Rc::new(move |values: Vec<String>, window: &mut Window, cx: &mut App| {
                    let Some(device_id) = values.into_iter().next() else {
                        return;
                    };
                    if let Some(view) = view.upgrade() {
                        view.update(cx, |this, cx| {
                            this.set_device(device_id, window, cx);
                            cx.notify();
                        });
                    }
                }),
            )
            .id("chat-pin-device-picker")
            .render(window, cx)
        })
        .into_any_element()
    }

    /// EXP-993: the Repository pin (no-subject chats only), offered ONLY
    /// while the team has more than one repo. One repo is not a choice (it is
    /// already pre-picked), and there is no "No repository" entry any more: a
    /// chat runs somewhere the person can name, never in an unexplained
    /// scratch dir picked by default.
    fn repo_pin(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        if self.team_repos.len() < 2 {
            return None;
        }
        let repos = self.team_repos.clone();
        let label = match &self.chat_repo {
            Some(repo) => repo.full_name.clone(),
            None => repos[0].full_name.clone(),
        };
        let picked = self.chat_repo.as_ref().map(|repo| repo.id.clone());
        let view = cx.entity().downgrade();
        Some(
            inline_pin_trigger("chat-pin-repo".into(), label, cx)
                .dropdown_menu(move |mut menu, _window, _cx| {
                    for repo in &repos {
                        let view = view.clone();
                        let repo = repo.clone();
                        let checked = picked.as_deref() == Some(repo.id.as_str());
                        menu = menu.item(
                            crate::controls::pointer_label_item(repo.full_name.clone(), false)
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
                .into_any_element(),
        )
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
                "Resumes the {} run exactly (its own transcript); a resume keeps the run's own agent.",
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

    /// EXP-991 — options row B: Device · Account · Model · Plan (· Resume ·
    /// Repository).
    ///
    /// EXP-872 dropped the Agent pin: the ACCOUNT pin carries its agent, so
    /// the row names the login a run spends rather than the brand twice.
    /// EXP-1249 dropped the `⋯`: Effort, Subagents, Ultracode, MCP servers
    /// and Computer use live in the "+" menu ([`plus_menu`]).
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
                // EXP-872: WHICH LOGIN a run spends is the first decision on
                // the line, and it is also the agent pick.
                .children(self.launch_ref().account_pin("chat", Self::launch_access, cx))
                .child(self.launch_ref().model_pin("chat", Self::launch_access, cx))
                .children(self.launch_ref().plan_toggle("chat", Self::launch_access, cx));
        }
        row = row.children(self.resume_switch(cx));
        if matches!(self.subject, Subject::None) {
            row = row.children(self.repo_pin(cx));
        }
        v_flex().w_full().min_w_0().gap_1().child(row).into_any_element()
    }

    /// EXP-790/EXP-1249: the suggestions, shown for the EMPTY, subject-less
    /// field only — QUIET rows under the options line (muted 13px text
    /// behind a faint `action-default` glyph; no pill, no border). A click
    /// inserts the text through the mention widget and parks the caret after
    /// the suggestion's first `#` ([`MentionInput::insert_suggestion`]), so
    /// the issue picker opens exactly as typing the token would — wherever
    /// in the sentence it sits.
    fn render_suggestions(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        if !matches!(self.subject, Subject::None) || !self.input.read(cx).value().trim().is_empty() {
            return None;
        }
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;
        let rows = self.suggestions.iter().enumerate().map(|(index, &pick)| {
            let text: &'static str = CHAT_SUGGESTIONS[pick];
            h_flex()
                .id(("chat-suggestion", index))
                .w_full()
                .min_w_0()
                .gap_2()
                .items_center()
                .px_1()
                .py_1()
                .rounded(px(theme::tokens::radius::SM))
                .cursor_pointer()
                .text_size(px(13.))
                .text_color(muted)
                .hover(move |style| style.text_color(foreground))
                .child(
                    Icon::new(registry::ACTION_DEFAULT)
                        .size(px(14.))
                        .flex_shrink_0()
                        .text_color(muted.opacity(0.6)),
                )
                .child(div().min_w_0().truncate().child(text))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.mention
                        .update(cx, |mention, cx| mention.insert_suggestion(text, window, cx));
                    cx.notify();
                }))
        });
        Some(
            v_flex()
                .w_full()
                .min_w_0()
                .pt_2()
                .children(rows)
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
        let mcp = self.mcp_options();
        let mcp_owned = chat_launch::subject_owns_mcp_servers(&self.subject_kind());
        if let Some(launch) = self.launch.as_mut() {
            launch.set_mcp_servers(mcp);
            launch.set_mcp_owned_by_subject(mcp_owned);
        }

        let blocker = self.launch_blocker(cx);
        // EXP-1196: a device/agent failure renders as THE failing readiness
        // row (with its action) instead of a sentence.
        let readiness = self.readiness_blocker(cx);
        let readiness_summary = readiness
            .as_ref()
            .map(|(row, _)| SharedString::from(device_readiness::summary(row)));
        let no_session_note = self.no_session_note().filter(|_| readiness.is_none());
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
        // EXP-1019: the subject CHIPS moved out of the card into the
        // headline; the picked action's typed inputs stay in it (they are
        // fields of the form, not the subject).
        let leading = self.render_action_fields(cx);
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
        // EXP-1249: the ONE tool — the "+" menu (the `ui-add` plus ×4).
        .tool(self.plus_tool(window, cx))
        .submit(submit);
        if let Some(leading) = leading {
            composer = composer.leading(leading);
        }
        // EXP-1037: the suggestions are a subject-less-CHAT affordance — the
        // dialog always has a subject, so it never shows them.
        let suggestions = (!self.in_dialog())
            .then(|| self.render_suggestions(cx))
            .flatten();
        let headline = self.render_headline(cx);
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
        let blocker_note = blocker_note.filter(|reason| Some(reason) != readiness_summary.as_ref());
        if let Some((row, target)) = readiness {
            notes = notes.child(Self::render_readiness_row(&row, target, cx));
        }
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
        v_flex()
            .w_full()
            .min_w_0()
            .gap_2()
            // EXP-1019: the headline is the launcher's MAIN element — the
            // verb plus the subject's chips, with the card (and its muted
            // placeholder) reading as the secondary one under it.
            .children(headline)
            .child(
                crate::composer::glass_composer(composer)
                    .capture_action(cx.listener(Self::on_paste)),
            )
            .child(options)
            .child(notes)
            // EXP-1249: the suggestions sit UNDER the line, quiet.
            .children(suggestions)
            .into_any_element()
    }

    /// EXP-923 — the Agent SCREEN: the launcher centred in the page's one
    /// scroll, with the "Recent runs" ghost button in the corner.
    ///
    /// The composer IS the page, so it sits in the middle of it (web
    /// `justify-center`) rather than pinned under the top edge — the two
    /// session bands that used to follow it are the rail's Running section
    /// and the history button's panel now.
    ///
    /// That button is the page's one piece of chrome: a ghost glyph in the
    /// page's top-left corner, over the composer column rather than in it
    /// (the column is centred; the button is not). EXP-1192: it ALWAYS
    /// renders — the history glyph opens Recent runs as a second sidebar in
    /// the card, and while that is up the same corner is Back, which puts it
    /// away (the panel has no back row of its own).
    fn render_page(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        let launcher = self.render_launcher(window, cx);
        let history_open = crate::navigation::recent_runs_open(window, cx);
        let (icon, tooltip) = if history_open {
            (registry::UI_BACK, "Back")
        } else {
            (registry::SETTINGS_SESSIONS, RECENT_RUNS_LABEL)
        };
        let history = Button::new("chat-recent-runs")
            .ghost()
            .cursor_pointer()
            .small()
            .icon(Icon::from(icon))
            .tooltip(tooltip)
            .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                crate::navigation::toggle_recent_runs(window, cx);
            }));
        // EXP-1249: the faint brand mark behind the headline and the card —
        // the page only (never the dialog), painted first so it sits under
        // everything, and with no hitbox so it never takes a click.
        let mark = div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .child(
                Icon::from(crate::icons::ExpIcon::Logo)
                    .size(px(PAGE_MARK_SIZE))
                    .flex_shrink_0()
                    .text_color(cx.theme().foreground.opacity(PAGE_MARK_ALPHA)),
            );
        v_flex()
            .size_full()
            .min_h_0()
            .relative()
            .track_focus(&self.focus_handle)
            .child(mark)
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
                    // The stack must NOT shrink: inside the scroll column a
                    // flex-shrinkable child gets squeezed to the viewport and
                    // its trailing rows (options, the blocker note) clipped.
                    .child(
                        div()
                            .w_full()
                            .max_w(px(PROMPT_MAX_W))
                            .min_w_0()
                            .flex_shrink_0()
                            .child(launcher),
                    ),
            ))
            .child(div().absolute().top_2().left_2().child(history))
            .into_any_element()
    }

    /// EXP-1037 — the composer in its DIALOG window: the launcher alone, in
    /// the window's own scroll. No page scroll chrome, no "Recent runs"
    /// button, no suggestion band — a dialog always has a subject, and the
    /// one thing left to decide is whether to send it.
    ///
    /// EXP-897: the blocked-start question is asked HERE rather than as a
    /// nested alert. `native_dialog::open_dialog_window` refuses to open from
    /// a dialog window at all (`dialog_open_here`), and closing this window
    /// first would drop the view that owns the draft, the picks and the
    /// in-flight start — so the same title, body, graph and three answers
    /// take over the dialog's body instead.
    fn render_dialog(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        let body = if let Some(blocked) = self.blocked.as_ref() {
            self.render_blocked_panel(blocked, cx)
        } else {
            self.render_launcher(window, cx)
        };
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

    /// EXP-1037/EXP-897/SLOP-3: the blocked-start question drawn INSIDE the
    /// composer dialog: the alert's own title, body, transitive blocks graph
    /// and disabled-reason caption, with Cancel · Start anyway · Stacked PR.
    /// Both answers run [`Self::answer_blocked`], the ONE code path the
    /// alert's answers take on the page.
    fn render_blocked_panel(
        &self,
        pending: &PendingBlocked,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let refs: Vec<&str> = pending.blocked.picked.iter().map(String::as_str).collect();
        let graph = crate::issue_graph::graph_for(&refs, cx);
        let title = pending.blocked.title();
        let description = pending.blocked.description();
        let note = pending.blocked.stack_note.clone();
        let stackable = pending.blocked.stack.is_some();
        let muted = cx.theme().muted_foreground;
        v_flex()
            .w_full()
            .min_w_0()
            .gap_3()
            .child(
                div()
                    .text_lg()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(cx.theme().foreground)
                    .child(SharedString::from(title)),
            )
            .child(div().text_sm().text_color(muted).child(SharedString::from(description)))
            .child(crate::issue_graph::graph_in_dialog(
                &graph,
                BLOCKED_PANEL_GRAPH_W,
                cx,
            ))
            .children(note.map(|note| {
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(note))
            }))
            .child(
                h_flex()
                    .w_full()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("chat-blocked-cancel")
                            .outline()
                            .cursor_pointer()
                            .small()
                            .label("Cancel")
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.blocked = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("chat-blocked-anyway")
                            .outline()
                            .cursor_pointer()
                            .small()
                            .label(blocked_start::START_ANYWAY)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.answer_blocked(false, window, cx);
                            })),
                    )
                    .child(
                        Button::new("chat-blocked-stacked")
                            .primary()
                            .cursor_pointer()
                            .small()
                            .label(blocked_start::STACKED_PR)
                            .disabled(!stackable)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.answer_blocked(true, window, cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    /// Record the blocked-start answer and re-enter [`Self::start`]: Start
    /// anyway with the SAME composed message, Stacked PR with
    /// [`BlockedStart::stacked_message`] (the alert's closures, in-window).
    fn answer_blocked(&mut self, stacked: bool, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(pending) = self.blocked.take() else {
            return;
        };
        let (message, stacked_issue) = if stacked {
            match pending.blocked.stacked_start(&pending.message) {
                Some((issue_id, message)) => (message, Some(issue_id)),
                // A disabled Stacked PR never answers.
                None => {
                    self.blocked = Some(pending);
                    return;
                }
            }
        } else {
            (pending.message, None)
        };
        self.blocked_confirmed = true;
        self.stacked_issue = stacked_issue;
        self.start(message, window, cx);
    }
}

/// EXP-1037 — the composer that is currently open in a dialog window. WEAK:
/// the dialog window owns the view, and a closed dialog must not keep it
/// alive; a stale handle simply stops answering.
struct OpenComposerDialog(gpui::WeakEntity<ChatScreenView>);

impl gpui::Global for OpenComposerDialog {}

/// EXP-1249 — one row of the "+" menu, in `fixtures/composer-menu.json`
/// `rows` order ([`composer_menu_rows`] decides which render).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComposerMenuRow {
    ImplementIssue,
    RunAction,
    AddFile,
    Separator,
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
            Self::ImplementIssue => registry::EDITOR_ISSUE_REF,
            Self::RunAction => registry::ACTION_RUN,
            Self::AddFile => registry::UI_ATTACH,
            Self::Separator => return None,
            Self::Effort => registry::UI_ESTIMATE,
            Self::Subagents => registry::CODING_SUBAGENT,
            Self::Ultracode => registry::ACTION_DEFAULT,
            Self::McpServers => registry::UI_MCP,
            Self::ComputerUse => registry::NAV_COMPUTER,
        })
    }

    fn menu_icon(self) -> Icon {
        Icon::new(self.icon().unwrap_or(registry::UI_ADD))
    }
}

/// The fixture's `conditions`: which `when` rows hold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ComposerMenuConditions {
    pub(crate) subagent_model: bool,
    pub(crate) ultracode: bool,
    pub(crate) mcp: bool,
    pub(crate) computer_use: bool,
}

impl ComposerMenuConditions {
    fn of(options: &launch_options::ComposerMenu) -> Self {
        Self {
            subagent_model: options.subagent_value().is_some(),
            ultracode: options.ultracode.is_some(),
            mcp: !options.mcp_servers.is_empty(),
            computer_use: options.computer_use.is_some(),
        }
    }
}

/// EXP-1249 — the "+" menu's rows (fixtures/composer-menu.json, ×4): the
/// subject pickers and the attach, then the run's options, then its tools;
/// a `when` row only while its condition holds, then the separators TIDIED
/// (never leading, trailing or doubled). `None` = the launch cluster does
/// not exist yet: the first group only. Pure.
pub(crate) fn composer_menu_rows(conditions: Option<ComposerMenuConditions>) -> Vec<ComposerMenuRow> {
    use ComposerMenuRow::*;
    let Some(when) = conditions else {
        return vec![ImplementIssue, RunAction, AddFile];
    };
    let all = [
        (ImplementIssue, true),
        (RunAction, true),
        (AddFile, true),
        (Separator, true),
        (Effort, true),
        (Subagents, when.subagent_model),
        (Ultracode, when.ultracode),
        (Separator, true),
        (McpServers, when.mcp),
        (ComputerUse, when.computer_use),
    ];
    let mut rows: Vec<ComposerMenuRow> = Vec::new();
    for (row, shown) in all {
        if !shown || (row == Separator && rows.last().is_none_or(|last| *last == Separator)) {
            continue;
        }
        rows.push(row);
    }
    if rows.last() == Some(&Separator) {
        rows.pop();
    }
    rows
}

/// EXP-1249 — the "+" menu (web `launch-composer.tsx`'s menu, the
/// styleguide `menu` entry's composer specimen), row by row off
/// [`composer_menu_rows`]. `options` is `None` until the launch cluster
/// exists (then only the first group shows).
pub(crate) fn plus_menu(
    mut menu: gpui_component::menu::PopupMenu,
    view: &gpui::WeakEntity<ChatScreenView>,
    options: Option<&launch_options::ComposerMenu>,
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
    let chevron = |cx: &App| Some(menu_row_value(None, true, cx));
    menu = menu.min_w(px(260.));
    for row in composer_menu_rows(options.map(ComposerMenuConditions::of)) {
        menu = match (row, options) {
            (ComposerMenuRow::Separator, _) => menu.separator(),
            (ComposerMenuRow::ImplementIssue, _) => menu.item(
                pointer_menu_item(None, MENU_IMPLEMENT_ISSUE, chevron, open(ComposerPicker::Issues))
                    .icon(row.menu_icon()),
            ),
            (ComposerMenuRow::RunAction, _) => menu.item(
                pointer_menu_item(None, MENU_RUN_ACTION, chevron, open(ComposerPicker::Actions))
                    .icon(row.menu_icon()),
            ),
            (ComposerMenuRow::AddFile, _) => menu.item({
                let view = view.clone();
                pointer_menu_item(None, MENU_ADD_FILE, |_| None, move |window, cx| {
                    if let Some(view) = view.upgrade() {
                        view.update(cx, |_, cx| {
                            composer_images::pick_image_files(window, cx, |this, read, window, cx| {
                                this.stage_images(read, window, cx)
                            });
                        });
                    }
                })
                .icon(row.menu_icon())
            }),
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
                        for (label, choice) in crate::coding_selects::SUBAGENT_MODEL_CHOICES.iter() {
                            let view = view.clone();
                            let choice = (*choice).to_string();
                            let label = if choice.is_empty() { launch_options::CLI_DEFAULT_LABEL } else { *label };
                            sub = sub.item(pointer_check_item(label, picked == choice, move |window, cx| {
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

/// EXP-862/EXP-1037 — the action the OPEN composer dialog is seeded with.
/// `None` when no dialog is up. [`crate::screens::chat_action_id`] consults
/// this before the window's own Agent screen: since EXP-1037 a pinned
/// action's ▶ opens the dialog instead of navigating, and the row must still
/// read as active while it is up.
pub(crate) fn dialog_action_id(cx: &App) -> Option<String> {
    let view = cx.try_global::<OpenComposerDialog>()?.0.upgrade()?;
    let id = view.read(cx).active_action_id()?.to_string();
    Some(id)
}

/// EXP-980 — what the blocked-start dialog is about: everything that was
/// picked (the graph's subjects, and what "Start anyway" starts) plus the
/// identifiers of the open issues blocking it from outside.
pub(crate) struct BlockedStart {
    pub(crate) picked: Vec<String>,
    pub(crate) blockers: Vec<String>,
    /// SLOP-3: what "Stacked PR" starts; `None` = the button is disabled,
    /// captioned by `stack_note`.
    pub(crate) stack: Option<StackedStart>,
    /// The caption under the graph: the disabled reason, or (enabled, 2+
    /// issues to run) the plan note.
    pub(crate) stack_note: Option<String>,
}

/// SLOP-3: the stacked start's plan and the issue it starts: `run[0]`, the
/// bottom of the line to build (NOT necessarily the picked issue).
#[derive(Clone)]
pub(crate) struct StackedStart {
    pub(crate) issue_id: String,
    pub(crate) plan: blocked_start::StackPlan,
}

/// The blocked-start alert: the title and the sentence, the blocks graph
/// with the caption under it, and Cancel · Start anyway · Stacked PR (the
/// primary, disabled without a stack). [`ChatScreenView::prompt_blocked_start`]
/// opens it; the styleguide specimen draws the same spec.
pub(crate) fn blocked_start_alert(
    blocked: &BlockedStart,
    graph: domain::issue_graph::IssueGraph,
    on_anyway: impl Fn(&mut Window, &mut App) -> bool + 'static,
    on_stacked: impl Fn(&mut Window, &mut App) -> bool + 'static,
) -> crate::native_dialog::AlertSpec {
    let note: Option<SharedString> = blocked.stack_note.clone().map(SharedString::from);
    crate::native_dialog::AlertSpec::new(
        blocked.title(),
        blocked.description(),
        blocked_start::STACKED_PR,
    )
    .height(gpui::px(BLOCKED_DIALOG_HEIGHT))
    .secondary(blocked_start::START_ANYWAY, on_anyway)
    .ok_disabled(blocked.stack.is_none())
    .on_ok(on_stacked)
    .content(move |_, cx| {
        gpui_component::v_flex()
            .min_w_0()
            .gap_2()
            .child(crate::issue_graph::graph_in_dialog(
                &graph,
                BLOCKED_DIALOG_GRAPH_W,
                cx,
            ))
            .children(note.clone().map(|note| {
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(note)
            }))
            .into_any_element()
    })
}

impl BlockedStart {
    fn batch(&self) -> bool {
        self.picked.len() > 1
    }

    fn title(&self) -> &'static str {
        if self.batch() {
            blocked_start::BATCH_TITLE
        } else {
            blocked_start::TITLE
        }
    }

    /// The batch body, or the one-issue sentence whose suffix offers the
    /// stacked start only while it is enabled.
    pub(crate) fn description(&self) -> String {
        if self.batch() {
            blocked_start::BATCH_BODY.to_string()
        } else {
            blocked_start::body(&self.blockers, self.stack.is_some())
        }
    }

    /// What a Stacked PR answer starts: `run[0]`'s issue id and its message,
    /// [`blocked_start::stacked_start_prompt`] over the TYPED text, with the
    /// composed image embeds kept attached. The launcher and its settings
    /// are exactly Start anyway's.
    fn stacked_start(&self, message: &str) -> Option<(String, String)> {
        let stack = self.stack.as_ref()?;
        let parsed = domain::image_message::parse_steer_message(message);
        let prompt = blocked_start::stacked_start_prompt(&stack.plan, &parsed.text);
        let message = domain::image_message::build_steer_image_message(
            &prompt,
            &parsed.attachment_ids,
        );
        Some((stack.issue_id.clone(), message))
    }
}

/// SLOP-3: the stacked start for `picked`, or why there is none: the line
/// below the subject ([`blocked_start::stack_line`]) read off the synced
/// relations, then [`blocked_start::stack_plan`]. A repository is the
/// issue's BOARD's; a line member is `running` while it has a live run (the
/// one-session-per-issue rule's own test) and no open pull request.
fn stack_choice(picked: &[String], cx: &App) -> (Option<StackedStart>, Option<String>) {
    use domain::issue_graph::{GraphIssue, GraphRelation};
    let Some(store) = Store::try_global(cx) else {
        return (None, None);
    };
    let collections = store.collections();
    let issues = collections.issues.read(cx);
    let boards = collections.boards.read(cx);
    let relations = collections.issue_relations.read(cx);
    let repository_of = |board_id: &str| {
        boards
            .get(board_id)
            .and_then(|board| board.repository_id.clone())
    };
    let Some(subject_row) = picked.first().and_then(|id| issues.get(id)) else {
        return (None, None);
    };
    let subject = blocked_start::StackSubject {
        identifier: subject_row.identifier.clone(),
        repository_id: repository_of(&subject_row.board_id),
    };

    let (line_ids, fork, cycle) = if picked.len() > 1 {
        // A batch never stacks: the plan says so before it reads a line.
        (Vec::new(), None, false)
    } else {
        let graph_issues: Vec<GraphIssue<'_>> = issues
            .iter()
            .map(|issue| GraphIssue {
                id: &issue.id,
                identifier: &issue.identifier,
                status: issue.status.as_wire().unwrap_or_default(),
            })
            .collect();
        let graph_relations: Vec<GraphRelation<'_>> = relations
            .iter()
            .map(|row| GraphRelation {
                kind: row.kind.as_deref().unwrap_or_default(),
                issue_id: &row.issue_id,
                related_issue_id: &row.related_issue_id,
            })
            .collect();
        let found = blocked_start::stack_line(&subject_row.id, &graph_relations, &graph_issues);
        (
            found
                .line
                .iter()
                .map(|issue| issue.id.to_string())
                .collect::<Vec<_>>(),
            found.fork.map(str::to_string),
            found.cycle,
        )
    };

    let now = chrono::Utc::now().timestamp();
    let local = coding_flow::LocalSessions::global_ref(cx);
    let sessions = collections.coding_sessions.read(cx);
    let has_live_run = |issue_id: &str| -> bool {
        local
            .as_ref()
            .is_some_and(|local| local.read(cx).get(issue_id).is_some())
            || sessions.iter().any(|session| {
                let covers = session.issue_id.as_deref() == Some(issue_id)
                    || domain::batch_run::parse_batch_issue_ids(session.batch_issue_ids.as_ref())
                        .iter()
                        .any(|id| id == issue_id);
                covers && queries::coding_session_is_live(session, now)
            })
    };
    let line: Vec<blocked_start::StackMember> = line_ids
        .iter()
        .filter_map(|id| issues.get(id))
        .map(|issue| {
            let pr_open = issue.pr_state.as_deref() == Some("open");
            blocked_start::StackMember {
                identifier: issue.identifier.clone(),
                pr_state: issue.pr_state.clone(),
                branch: issue.branch.clone(),
                repository_id: repository_of(&issue.board_id),
                running: !pr_open && has_live_run(&issue.id),
            }
        })
        .collect();

    let planned = blocked_start::stack_plan(picked.len(), &subject, &line, fork.as_deref(), cycle);
    stack_outcome(planned, |first| {
        if first == subject.identifier {
            Some(subject_row.id.clone())
        } else {
            line_ids
                .iter()
                .find(|id| issues.get(id).is_some_and(|issue| issue.identifier == first))
                .cloned()
        }
    })
}

/// What a plan means to the dialog: the stacked start (`run[0]`'s issue,
/// resolved by `issue_id_of`) with its plan note, or no stack and the
/// refusal's note. An unresolvable `run[0]` is no stack and no note.
pub(crate) fn stack_outcome(
    planned: Result<blocked_start::StackPlan, blocked_start::StackRefusal>,
    issue_id_of: impl Fn(&str) -> Option<String>,
) -> (Option<StackedStart>, Option<String>) {
    match planned {
        Ok(plan) => {
            let first = plan.run.first().cloned().unwrap_or_default();
            let Some(issue_id) = issue_id_of(&first) else {
                return (None, None);
            };
            let note = blocked_start::stack_plan_note(&plan.run);
            (Some(StackedStart { issue_id, plan }), note)
        }
        Err(refusal) => (None, Some(refusal.note())),
    }
}

/// EXP-1037 — the blocked-start question while the composer lives in a
/// dialog window: the composed message it was asked for and what it is
/// about.
struct PendingBlocked {
    message: String,
    blocked: BlockedStart,
}

/// The blocked-start dialog is taller than a plain alert — it hosts the
/// graph.
const BLOCKED_DIALOG_HEIGHT: f32 = 460.;
/// The graph's viewport inside it (the 416px alert minus its padding).
const BLOCKED_DIALOG_GRAPH_W: f32 = 380.;
/// EXP-1037: the same graph inside the composer DIALOG, which is the wider
/// launcher window (640px content) rather than a 416px alert.
const BLOCKED_PANEL_GRAPH_W: f32 = 560.;

/// [`RemoteSubject`] borrows the ids it names; the issue arms need an owned
/// carrier so the checked list can be built inside `start` and borrowed
/// afterwards.
enum OwnedRemoteSubject {
    Issue {
        issue_id: String,
        resume: bool,
    },
    Batch {
        issue_ids: Vec<String>,
    },
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

    /// EXP-1249 — `fixtures/composer-menu.json` is the "+" menu's ×4 spec:
    /// the words and concept icons row by row, the Effort row's codex label,
    /// and every `cases` entry replayed through [`composer_menu_rows`].
    #[test]
    fn the_plus_menu_replays_the_composer_menu_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/composer-menu.json"
        ))
        .unwrap();
        assert_eq!(fixture["plusLabel"], PLUS_TOOLTIP);
        let row_of = |id: &str| match id {
            "implement-issue" => (ComposerMenuRow::ImplementIssue, MENU_IMPLEMENT_ISSUE),
            "run-action" => (ComposerMenuRow::RunAction, MENU_RUN_ACTION),
            "add-file" => (ComposerMenuRow::AddFile, MENU_ADD_FILE),
            "effort" => (ComposerMenuRow::Effort, coding::CodingAgent::Claude.effort_label()),
            "subagents" => (ComposerMenuRow::Subagents, MENU_SUBAGENTS),
            "ultracode" => (ComposerMenuRow::Ultracode, MENU_ULTRACODE),
            "mcp-servers" => (ComposerMenuRow::McpServers, MENU_MCP_SERVERS),
            "computer-use" => (ComposerMenuRow::ComputerUse, MENU_COMPUTER_USE),
            "-" => (ComposerMenuRow::Separator, ""),
            other => panic!("unknown composer menu row {other}"),
        };
        let mut every = Vec::new();
        for row in fixture["rows"].as_array().unwrap() {
            if row["kind"] == "separator" {
                every.push(ComposerMenuRow::Separator);
                continue;
            }
            let id = row["id"].as_str().unwrap();
            let (menu_row, label) = row_of(id);
            assert_eq!(row["label"], label, "{id} label");
            use gpui_component::IconNamed as _;
            assert_eq!(
                menu_row.icon().map(|icon| icon.path()),
                crate::icons::registry::concept_by_name(row["icon"].as_str().unwrap())
                    .map(|icon| icon.path()),
                "{id} icon"
            );
            if let Some(codex) = row["codexLabel"].as_str() {
                assert_eq!(coding::CodingAgent::Codex.effort_label(), codex);
            }
            every.push(menu_row);
        }
        let all = ComposerMenuConditions {
            subagent_model: true,
            ultracode: true,
            mcp: true,
            computer_use: true,
        };
        assert_eq!(composer_menu_rows(Some(all)), every, "rows order");
        for case in fixture["cases"].as_array().unwrap() {
            let when = &case["conditions"];
            let conditions = ComposerMenuConditions {
                subagent_model: when["subagentModel"].as_bool().unwrap(),
                ultracode: when["ultracode"].as_bool().unwrap(),
                mcp: when["mcp"].as_bool().unwrap(),
                computer_use: when["computerUse"].as_bool().unwrap(),
            };
            let expected: Vec<ComposerMenuRow> = case["expected"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| row_of(id.as_str().unwrap()).0)
                .collect();
            assert_eq!(composer_menu_rows(Some(conditions)), expected, "{}", case["name"]);
        }
        // Before the launch cluster exists: the first group, no separator.
        assert_eq!(
            composer_menu_rows(None),
            [ComposerMenuRow::ImplementIssue, ComposerMenuRow::RunAction, ComposerMenuRow::AddFile]
        );
    }

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
                "Create an action that labels new issues",
                "Set up a weekly standup digest action",
                "Draft release notes from the issues completed this month",
                "Summarize what changed across the boards this week",
                "Start a run for # on my other machine",
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
            default_branch: None,
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
                conflict_refused: false,
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
            rows: Rc::default(),
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
                resume: true,
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
