//! EXP-746 — the agent-run rows, EXP-874 — ONE layout per kind.
//!
//! Every runs list (the Agent page's Running/Recent bands, an issue's Runs
//! band, the Sessions list nav, an action page's Runs)
//! draws a `coding_sessions` row through one of two renderers:
//!
//! - [`render_running_run_row`]: run mark · identifier · title, the agent
//!   caption, a toned status line and the usage-wall badge. The whole row
//!   opens the run (EXP-893: no trailing Merge / open-the-subject buttons);
//!   "Stop session" is on the row's right-click menu, never a button.
//! - [`render_past_run_row`]: the dimmed run mark, identifier · title over
//!   the byline, a trailing chevron. The whole row opens the session.
//!
//! EXP-1208: both LEAD with the shared run mark
//! ([`crate::coding_selects::run_lead`], never a dot) at the row's base inset
//! `12 + 14·depth`, and a parent's fold chevron FOLLOWS it — so a parent's
//! mark lines up exactly with a standalone row's and a child's connector
//! elbow ends at its mark (×4). The data half ([`running_run_facts`],
//! [`past_run_facts`]) is shared too, so the lists cannot disagree about what
//! a run says.

use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, App, ClickEvent, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{menu::ContextMenuExt as _, ActiveTheme as _, Icon, Sizable as _};

use crate::icons::registry;
use crate::queries::{self, CodingSessionDisplay};

/// A row callback (open / toggle / kill) — boxed so the spec stays one type
/// across callers that close over entities of different views.
pub(crate) type RunRowAction = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// The context menu's single destructive item — "end this run".
pub(crate) struct RunRowKill {
    /// "Stop session" (EXP-849: one verb wherever the run is hosted).
    pub(crate) label: SharedString,
    pub(crate) on_kill: RunRowAction,
}

/// EXP-827: the fold chevron a row with NESTED sub-sessions carries.
pub(crate) struct RunRowFold {
    pub(crate) collapsed: bool,
    pub(crate) on_toggle: RunRowAction,
}

// ---------------------------------------------------------------------------
// Facts
// ---------------------------------------------------------------------------

/// The status line's colour role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusTone {
    Muted,
    Amber,
    Green,
    Blue,
}

impl StatusTone {
    pub(crate) fn color(self, muted: Hsla) -> Hsla {
        match self {
            StatusTone::Muted => muted,
            StatusTone::Amber => theme::tokens::YELLOW.to_hsla(),
            StatusTone::Green => theme::tokens::GREEN.to_hsla(),
            StatusTone::Blue => theme::tokens::BLUE.to_hsla(),
        }
    }
}

/// Everything a running row draws, derived off the synced rows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RunningRunFacts {
    pub(crate) session_id: String,
    pub(crate) identifier: Option<SharedString>,
    pub(crate) title: SharedString,
    pub(crate) agent_caption: Option<SharedString>,
    pub(crate) status: SharedString,
    pub(crate) status_tone: StatusTone,
    /// The state's dot tone — the entity preview card's dot; the list rows
    /// wear [`Self::mark`] instead (EXP-1208).
    pub(crate) dot: Hsla,
    /// EXP-1208: the run mark's state ([`running_row_mark`]) — `None` = the
    /// bare mark (a paused run).
    pub(crate) mark: Option<CodingSessionDisplay>,
    /// EXP-1175: the display state the status line was read from (the Run
    /// face's status row keys its caption on it).
    pub(crate) display: CodingSessionDisplay,
    pub(crate) blocked: Option<SharedString>,
    /// The machine's name (the kill confirm names it).
    pub(crate) device_label: Option<String>,
    pub(crate) paused: bool,
    /// EXP-848/EXP-1184: the agent is mid-turn RIGHT NOW on a live row
    /// (`queries::session_row_is_working`, never `running` alone).
    pub(crate) working: bool,
    /// The run's agent (`None` = an id this build does not know) — whose
    /// mark to draw.
    pub(crate) agent: Option<coding::CodingAgent>,
}

/// Everything a past row draws.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PastRunFacts {
    pub(crate) session_id: String,
    pub(crate) identifier: Option<SharedString>,
    pub(crate) title: SharedString,
    pub(crate) byline: SharedString,
    /// The run's agent — whose (dimmed) mark leads the row.
    pub(crate) agent: Option<coding::CodingAgent>,
}

/// EXP-1208 — a live row's run-mark state, ×4 (web `runningRowMarkState`): the
/// display state, except a paused run (offline host) wears the bare mark
/// (`None`) and only a WORKING row animates (EXP-848: the turn flag, never
/// `running` alone). Pure.
pub(crate) fn running_row_mark(
    display: CodingSessionDisplay,
    paused: bool,
    working: bool,
) -> Option<CodingSessionDisplay> {
    if paused {
        return None;
    }
    match display {
        CodingSessionDisplay::Working if !working => None,
        display => Some(display),
    }
}

/// The run's agent off its synced id: an absent id is claude
/// (`codingSessions.start`'s default), an id this build does not know `None`.
pub(crate) fn run_agent(session: &domain::rows::CodingSession) -> Option<coding::CodingAgent> {
    match session.agent.as_deref() {
        None => Some(coding::CodingAgent::default()),
        Some(id) => coding::CodingAgent::parse(id),
    }
}

/// A LIVE run's row facts. `local_caption` and `local_busy` are the engine's
/// own caption and turn signal for a run this process hosts
/// (`session_agent_caption` / `session_agent_busy` precedence).
pub(crate) fn running_run_facts(
    session: &domain::rows::CodingSession,
    local_caption: Option<String>,
    local_busy: Option<bool>,
    now_epoch: i64,
    cx: &App,
) -> RunningRunFacts {
    let muted = cx.theme().muted_foreground;
    let store = sync::Store::try_global(cx);
    let collections = store.map(|store| store.collections().clone());
    let issue = collections.as_ref().and_then(|collections| {
        session
            .issue_id
            .as_deref()
            .and_then(|id| collections.issues.read(cx).get(id).cloned())
    });
    let presentation = match collections.as_ref() {
        Some(collections) => queries::session_device_presentation(
            session,
            collections.devices.read(cx).iter(),
            now_epoch * 1_000,
        ),
        None => queries::SessionDevicePresentation {
            label: session.device_label.clone(),
            offline: false,
        },
    };
    // EXP-734: an issue-less run (action/chat) carries its own PR state.
    let pr_state = issue
        .as_ref()
        .and_then(|issue| issue.pr_state.as_deref())
        .or(session.pr_state.as_deref());
    let busy = queries::session_agent_busy(
        session,
        local_busy.or_else(|| queries::local_agent_busy(&session.id, cx)),
        now_epoch,
    );
    let display = queries::coding_session_display(session, busy, pr_state);
    let paused = queries::session_is_paused(display, session.status.as_deref(), &presentation);
    let started = run_started_at(session)
        .map(|at| crate::comments::relative_time(at, now_epoch))
        .unwrap_or_default();
    let (status, status_tone) =
        running_status_line(display, paused, presentation.label.as_deref(), &started);
    let blocked = crate::usage_bar::parse_blocked(session.blocked.as_ref());
    // EXP-876: a batch row names itself after the issues it covers.
    let batch_issues = batch_run_issues(session, cx);
    // EXP-848/EXP-1184: the turn flag, the ONE input the working mark keys
    // on — never on a paused row.
    let working = !paused && queries::session_row_is_working(session, display);
    RunningRunFacts {
        session_id: session.id.clone(),
        identifier: run_identifier(session, issue.as_ref(), &batch_issues),
        title: run_title(session, issue.as_ref(), &batch_issues),
        agent_caption: queries::session_agent_caption(session, local_caption, now_epoch)
            .map(SharedString::from),
        status: SharedString::from(status),
        status_tone,
        // EXP-862: the ONE dot mapping, shared with the rail and the header.
        dot: queries::session_dot_tone(
            queries::SessionDotFacts::from_display(display, false, paused),
            muted,
        ),
        // EXP-1208: the list rows' mark.
        mark: running_row_mark(display, paused, working),
        display,
        blocked: crate::usage_bar::blocked_badge_label(blocked.as_ref(), now_epoch)
            .map(SharedString::from),
        device_label: presentation.label,
        paused,
        working,
        agent: run_agent(session),
    }
}

/// A FINISHED run's row facts.
pub(crate) fn past_run_facts(
    session: &domain::rows::CodingSession,
    now_epoch: i64,
    cx: &App,
) -> PastRunFacts {
    let collections = sync::Store::try_global(cx).map(|store| store.collections().clone());
    let issue = collections.as_ref().and_then(|collections| {
        session
            .issue_id
            .as_deref()
            .and_then(|id| collections.issues.read(cx).get(id).cloned())
    });
    let device_label = match collections.as_ref() {
        Some(collections) => queries::session_device_presentation(
            session,
            collections.devices.read(cx).iter(),
            now_epoch * 1_000,
        )
        .label,
        None => session.device_label.clone(),
    };
    let batch_issues = batch_run_issues(session, cx);
    PastRunFacts {
        session_id: session.id.clone(),
        identifier: run_identifier(session, issue.as_ref(), &batch_issues),
        title: run_title(session, issue.as_ref(), &batch_issues),
        byline: SharedString::from(past_run_byline(session, device_label.as_deref(), now_epoch)),
        agent: run_agent(session),
    }
}

/// EXP-874 — a running row's status line and its tone. Parts the row cannot
/// prove are dropped (no device, no start stamp). Pure.
pub(crate) fn running_status_line(
    display: CodingSessionDisplay,
    paused: bool,
    device: Option<&str>,
    started_relative: &str,
) -> (String, StatusTone) {
    let device = device.map(str::trim).filter(|device| !device.is_empty());
    let join = |head: &str| match device {
        Some(device) => format!("{head} · {device}"),
        None => head.to_string(),
    };
    if paused {
        // EXP-550: an offline host means the run is parked, not dead.
        return (join("Paused"), StatusTone::Muted);
    }
    match display {
        CodingSessionDisplay::NeedsInput => (join("Needs input"), StatusTone::Amber),
        CodingSessionDisplay::Review => (join("Ready for review"), StatusTone::Green),
        CodingSessionDisplay::Done => (join("Done"), StatusTone::Blue),
        CodingSessionDisplay::Working => {
            let started = (!started_relative.is_empty()).then(|| format!("started {started_relative}"));
            let line = [device.map(str::to_string), started]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
            (line, StatusTone::Muted)
        }
    }
}

/// EXP-1175 — the Show work preference's default: the Run face opens as the
/// thread (fixture `run-row.json` `showWorkDefault`, ×4).
pub(crate) const SHOW_WORK_DEFAULT: bool = false;

/// EXP-1175 — the status row's trailing button: what pressing it DOES next.
pub(crate) fn show_work_label(show_work: bool) -> &'static str {
    if show_work {
        "Hide work"
    } else {
        "Show work"
    }
}

/// EXP-1175 — the Run face status row's state: the display state plus the two
/// the row alone tells apart, a paused (offline host) run and an ended one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunRowState {
    Working,
    NeedsInput,
    Paused,
    Review,
    Done,
    Ended,
}

/// EXP-1175 — the status row's state: the VIEWER's live signals folded over
/// the synced display state, so the row never contradicts the Run tab's
/// mark. Paused first, then ended, then needs input (a pending card the
/// viewer sees, or the synced flag), then working (the viewer's working
/// predicate, or the synced state), else review/done. Fixture `run-row.json`
/// `states` (×4).
pub(crate) fn run_row_state(
    paused: bool,
    ended: bool,
    awaiting_input: bool,
    working: bool,
    display: CodingSessionDisplay,
) -> RunRowState {
    if paused {
        return RunRowState::Paused;
    }
    if ended {
        return RunRowState::Ended;
    }
    if awaiting_input || display == CodingSessionDisplay::NeedsInput {
        return RunRowState::NeedsInput;
    }
    if working || display == CodingSessionDisplay::Working {
        return RunRowState::Working;
    }
    match display {
        CodingSessionDisplay::Review => RunRowState::Review,
        _ => RunRowState::Done,
    }
}

/// EXP-1175 — a timestamp column as epoch ms; unparsable = `None`.
pub(crate) fn stamp_ms(value: Option<&str>) -> Option<i64> {
    value
        .and_then(crate::inbox::parse_timestamp)
        .map(|parsed| parsed.timestamp_millis())
}

/// EXP-1175 — the status row's first line and tone: `Building on <device> ·
/// <elapsed>` while it works (now − start), `Ended on <device> · <elapsed>`
/// once over (end − start), else the session row's words; a missing stamp
/// drops the elapsed part. Fixture `run-row.json` `captions` (×4).
pub(crate) fn run_row_caption(
    state: RunRowState,
    device: &str,
    started_ms: Option<i64>,
    ended_ms: Option<i64>,
    now_ms: i64,
) -> (String, StatusTone) {
    match state {
        RunRowState::Paused => (format!("Paused · {device}"), StatusTone::Muted),
        RunRowState::NeedsInput => (format!("Needs input · {device}"), StatusTone::Amber),
        RunRowState::Review => (format!("Ready for review · {device}"), StatusTone::Green),
        RunRowState::Done => (format!("Done · {device}"), StatusTone::Blue),
        RunRowState::Working | RunRowState::Ended => {
            let (verb, end) = match state {
                RunRowState::Working => ("Building on", Some(now_ms)),
                _ => ("Ended on", ended_ms),
            };
            let elapsed = match (started_ms, end) {
                (Some(start), Some(end)) => {
                    format!(" · {}", crate::session_rows::format_duration(end - start))
                }
                _ => String::new(),
            };
            (format!("{verb} {device}{elapsed}"), StatusTone::Muted)
        }
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

pub(crate) struct RunningRunSpec {
    /// Element-id namespace: two run lists can share one parent.
    pub(crate) id_prefix: &'static str,
    pub(crate) index: usize,
    /// EXP-827/EXP-965: where the row sits in the tree — the 14px-per-level
    /// indent AND the connector it draws ([`domain::tree_guides`]).
    pub(crate) guides: domain::tree_guides::Guides,
    pub(crate) fold: Option<RunRowFold>,
    pub(crate) facts: RunningRunFacts,
    pub(crate) on_open: RunRowAction,
    /// `Some` = the row's right-click menu offers "Stop session".
    pub(crate) kill: Option<RunRowKill>,
}

pub(crate) struct PastRunSpec {
    pub(crate) id_prefix: &'static str,
    pub(crate) index: usize,
    pub(crate) guides: domain::tree_guides::Guides,
    pub(crate) fold: Option<RunRowFold>,
    pub(crate) facts: PastRunFacts,
    pub(crate) on_open: RunRowAction,
}

/// SLOP-2: a run's TITLE in its OWN action's Runs list (web `actionRunTitle`,
/// ×4): every row there ran the same action, so the row says what started it
/// instead of repeating the action's name.
pub(crate) fn action_run_title(started_reason: Option<&str>) -> &'static str {
    match started_reason {
        Some("schedule") => "Scheduled run",
        Some("event") => "Event run",
        // Another run started it (`agent`, or a reason added later).
        Some(reason) if !reason.is_empty() => "Agent run",
        _ => "Manual run",
    }
}

/// EXP-965: the session lists' base left padding — the connector's gutters
/// are measured off it.
const ROW_PAD: f32 = 12.;

/// EXP-965: the vertical space between two of these rows — ZERO. Every list
/// that draws them stacks them flush under a section band and reads as a
/// table (EXP-818's `flat_row` rule: the Recent panel, an action's Runs,
/// the list nav), so there is no gap for the connector to bridge. A list
/// that ever spaces them has to move this number with its own `gap_*`, or
/// the connector goes back to dashes.
const ROW_GAP: f32 = 0.;

/// EXP-1208: the run mark's square (= one indent level, so its centre IS the
/// gutter centre the child's connector hangs off) and its badge, ×4.
const RUN_MARK_PX: f32 = 14.;
const RUN_BADGE_PX: f32 = 6.;

/// The flat list row both kinds sit in: `list_hover` under the pointer,
/// `list_active` while its session is on screen (EXP-811/862). EXP-965: a
/// NESTED row paints its tree connector in the gutter its indent reserves.
fn row_shell(
    id_prefix: &'static str,
    index: usize,
    guides: &domain::tree_guides::Guides,
    active: bool,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let theme = cx.theme();
    let row_hover = theme.list_hover;
    let row_active = theme.list_active;
    crate::surface::flat_row()
        .id((SharedString::from(format!("{id_prefix}-card")), index))
        .flex()
        .w_full()
        .min_w_0()
        .relative()
        .items_center()
        .gap_2()
        .px_3()
        .py_2p5()
        .pl(gpui::px(ROW_PAD + crate::tree_guides::LEVEL_PITCH * guides.depth() as f32))
        .when(active, |this| this.bg(row_active))
        .cursor_pointer()
        .hover(move |style| style.bg(if active { row_active } else { row_hover }))
        .children(crate::tree_guides::guide_layer(guides, ROW_PAD, ROW_GAP))
}

/// The fold chevron's accessible labels, byte-identical ×4.
pub(crate) const EXPAND_CHILD_RUNS: &str = "Expand child runs";
pub(crate) const COLLAPSE_CHILD_RUNS: &str = "Collapse child runs";

fn fold_chevron(id_prefix: &'static str, index: usize, fold: RunRowFold, muted: Hsla) -> impl IntoElement {
    let RunRowFold { collapsed, on_toggle } = fold;
    div()
        .id((SharedString::from(format!("{id_prefix}-fold")), index))
        .flex_shrink_0()
        // EXP-1208: 14 wide ×4, AFTER the run mark.
        .w(gpui::px(RUN_MARK_PX))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        // EXP-897: the fold's accessible label, byte-identical ×4.
        .tooltip(move |window, cx| {
            gpui_component::tooltip::Tooltip::new(if collapsed {
                EXPAND_CHILD_RUNS
            } else {
                COLLAPSE_CHILD_RUNS
            })
                .build(window, cx)
        })
        .child(
            Icon::from(if collapsed {
                registry::UI_CHEVRON_RIGHT
            } else {
                registry::UI_CHEVRON_DOWN
            })
            .xsmall()
            .text_color(muted),
        )
        .on_click(move |event, window, cx| {
            // The row itself opens the run — folding must not.
            cx.stop_propagation();
            on_toggle(event, window, cx);
        })
}

/// EXP-1175 — the status row's lead: a live run's mark state (`None` = the
/// bare mark) or the ended run's dimmed mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunStatusMark {
    Live(Option<CodingSessionDisplay>),
    Ended,
}

/// EXP-1175 — what the Run face's status row draws.
pub(crate) struct RunStatusRowSpec {
    pub(crate) id: SharedString,
    pub(crate) agent: Option<coding::CodingAgent>,
    pub(crate) mark: RunStatusMark,
    pub(crate) caption: SharedString,
    pub(crate) tone: StatusTone,
    /// The newest tool call's line ([`steer::feed::last_tool_line`]); live
    /// runs only.
    pub(crate) tool_line: Option<SharedString>,
    pub(crate) show_work: bool,
    pub(crate) on_toggle: Option<RunRowAction>,
}

/// EXP-1175 — the Run face's ONE status row, over the thread or the
/// transcript: the run mark, the caption in its tone, the tool line muted
/// under it, the Show work / Hide work text button trailing (web
/// `RunStatusRow`).
pub(crate) fn run_status_row(spec: RunStatusRowSpec, cx: &App) -> gpui::AnyElement {
    use crate::controls::WebText as _;
    let muted = cx.theme().muted_foreground;
    let lead = match spec.mark {
        RunStatusMark::Live(state) => {
            crate::coding_selects::run_lead(spec.agent, RUN_MARK_PX, RUN_BADGE_PX, state)
        }
        RunStatusMark::Ended => crate::coding_selects::ended_run_lead(spec.agent, RUN_MARK_PX),
    };
    let body = gpui_component::v_flex()
        .flex_1()
        .min_w_0()
        .child(
            div()
                .w_full()
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(spec.tone.color(muted))
                .child(spec.caption),
        )
        .children(spec.tool_line.map(|line| {
            div()
                .w_full()
                .min_w_0()
                .truncate()
                .text_2xs()
                .text_color(muted.opacity(0.7))
                .child(line)
        }));
    let mut toggle = crate::controls::text_button(
        SharedString::from(format!("{}-toggle", spec.id)),
        show_work_label(spec.show_work),
        crate::controls::TextButtonVariant::Text,
        cx,
    )
    .flex_shrink_0();
    if let Some(on_toggle) = spec.on_toggle {
        toggle = toggle.on_click(move |event, window, cx| on_toggle(event, window, cx));
    }
    div()
        .id(spec.id)
        .flex()
        .flex_row()
        .w_full()
        .min_w_0()
        .items_center()
        .gap(gpui::px(10.))
        .py(gpui::px(6.))
        .child(lead)
        .child(body)
        .child(toggle)
        .into_any_element()
}

/// One LIVE run row (EXP-874). The whole row opens the run.
pub(crate) fn render_running_run_row(
    spec: RunningRunSpec,
    active: bool,
    cx: &App,
) -> gpui::AnyElement {
    let RunningRunSpec {
        id_prefix,
        index,
        guides,
        fold,
        facts,
        on_open,
        kill,
    } = spec;
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let foreground = theme.foreground;

    let line1 = div()
        .flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_2()
        .children(facts.identifier.clone().map(|identifier| {
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(muted)
                .child(identifier)
        }))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .truncate()
                .text_color(foreground)
                .child(facts.title.clone()),
        );
    let small_line = |text: SharedString, color: Hsla| {
        div()
            .w_full()
            .min_w_0()
            .truncate()
            .text_xs()
            .text_color(color)
            .child(text)
    };
    let body = gpui_component::v_flex()
        .flex_1()
        .min_w_0()
        .gap_0p5()
        .child(line1)
        .children(facts.agent_caption.clone().map(|caption| small_line(caption, muted)))
        .when(!facts.status.is_empty(), |this| {
            this.child(small_line(facts.status.clone(), facts.status_tone.color(muted)))
        })
        .children(
            facts
                .blocked
                .clone()
                .map(|label| small_line(label, theme::tokens::YELLOW.to_hsla())),
        );

    // EXP-1208: [run mark][fold, parents only][body] — the mark at the base
    // inset on every row, the chevron after it.
    let row = row_shell(id_prefix, index, &guides, active, cx)
        .on_click(move |event, window, cx| on_open(event, window, cx))
        .child(crate::coding_selects::run_lead(
            facts.agent,
            RUN_MARK_PX,
            RUN_BADGE_PX,
            facts.mark,
        ))
        .children(fold.map(|fold| fold_chevron(id_prefix, index, fold, muted)))
        .child(body);
    match kill {
        Some(RunRowKill { label, on_kill }) => {
            let on_kill = Rc::new(on_kill);
            row.context_menu(move |menu, _window, cx| {
                let on_kill = on_kill.clone();
                menu.item(
                    crate::controls::danger_menu_item(
                        label.clone(),
                        Icon::from(registry::CODING_STOP),
                        cx,
                    )
                    .on_click(move |event, window, cx| on_kill(event, window, cx)),
                )
            })
            .into_any_element()
        }
        None => row.into_any_element(),
    }
}

/// One FINISHED run row (EXP-874): a plain link to the session.
pub(crate) fn render_past_run_row(spec: PastRunSpec, active: bool, cx: &App) -> gpui::AnyElement {
    let PastRunSpec {
        id_prefix,
        index,
        guides,
        fold,
        facts,
        on_open,
    } = spec;
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let title = facts.title;
    let byline = facts.byline;
    let line1 = div()
        .flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_2()
        .children(facts.identifier.map(|identifier| {
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(muted)
                .child(identifier)
        }))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_sm()
                .truncate()
                .text_color(theme.foreground)
                .child(title),
        );
    let body = gpui_component::v_flex()
        .flex_1()
        .min_w_0()
        .gap_0p5()
        .child(line1)
        .when(!byline.is_empty(), |this| {
            this.child(
                div()
                    .w_full()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(muted)
                    .child(byline),
            )
        });
    row_shell(id_prefix, index, &guides, active, cx)
        .on_click(move |event, window, cx| on_open(event, window, cx))
        // EXP-1208: the ENDED run mark (dimmed, no badge) leads, the fold
        // follows it.
        .child(crate::coding_selects::ended_run_lead(facts.agent, RUN_MARK_PX))
        .children(fold.map(|fold| fold_chevron(id_prefix, index, fold, muted)))
        .child(body)
        .child(
            div()
                .flex_shrink_0()
                .child(Icon::from(registry::UI_CHEVRON_RIGHT).xsmall().text_color(muted)),
        )
        .into_any_element()
}

/// A row of a mixed list (an action's Runs): live runs
/// draw as running rows, ended ones as past rows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RunListFacts {
    Running(RunningRunFacts),
    Past(PastRunFacts),
}

impl RunListFacts {
    /// The row under another title (SLOP-2: an action's Runs names what
    /// started the run — [`action_run_title`]).
    pub(crate) fn with_title(mut self, title: &'static str) -> Self {
        match &mut self {
            RunListFacts::Running(facts) => facts.title = title.into(),
            RunListFacts::Past(facts) => facts.title = title.into(),
        }
        self
    }

    pub(crate) fn derive(session: &domain::rows::CodingSession, now_epoch: i64, cx: &App) -> Self {
        if run_has_ended(session) {
            RunListFacts::Past(past_run_facts(session, now_epoch, cx))
        } else {
            RunListFacts::Running(running_run_facts(session, None, None, now_epoch, cx))
        }
    }

    pub(crate) fn session_id(&self) -> &str {
        match self {
            RunListFacts::Running(facts) => &facts.session_id,
            RunListFacts::Past(facts) => &facts.session_id,
        }
    }
}

/// A row of a mixed list (never killable). EXP-897: it carries the tree's
/// place and fold like every other session row — an action's Runs nests its
/// child runs too.
pub(crate) fn render_run_list_row(
    id_prefix: &'static str,
    index: usize,
    guides: domain::tree_guides::Guides,
    fold: Option<RunRowFold>,
    facts: RunListFacts,
    active: bool,
    on_open: RunRowAction,
    cx: &App,
) -> gpui::AnyElement {
    match facts {
        RunListFacts::Running(facts) => render_running_run_row(
            RunningRunSpec {
                id_prefix,
                index,
                guides,
                fold,
                facts,
                on_open,
                kill: None,
            },
            active,
            cx,
        ),
        RunListFacts::Past(facts) => render_past_run_row(
            PastRunSpec {
                id_prefix,
                index,
                guides,
                fold,
                facts,
                on_open,
            },
            active,
            cx,
        ),
    }
}

// ---------------------------------------------------------------------------
// Shared run vocabulary
// ---------------------------------------------------------------------------

/// EXP-888: the staleness sweep's end — `ended_by = stale`. The server flips a
/// silent row to `ended` only when its host device advertises the `stale-end`
/// cap; that device IGNORES the flip (`sync::session_row_fires_kill`) and its
/// next heartbeat (up to 30 min later) revives the row to `running`. So a
/// sweep end says "silent for the staleness window", NEVER "this run is over".
/// ×4 (web `runIsStaleEnd`, iOS `PastRuns.isStaleEnd`, Android `runIsStaleEnd`).
pub(crate) fn run_is_stale_end(session: &domain::rows::CodingSession) -> bool {
    session.status.as_deref() == Some(domain::contract::CODING_SESSION_STATUS_ENDED)
        && session.ended_by.as_deref() == Some(domain::contract::CODING_SESSION_ENDED_BY_STALE)
}

/// THE predicate: whether the run is OVER — the terminal `ended` status, minus
/// [`run_is_stale_end`]. Every Running-vs-Past, Stop-vs-Resume and
/// composer-enabled decision goes through this and never through the raw
/// status, or a swept-but-alive run lists as past, greys its composer out and
/// offers a Resume that would put a SECOND agent on the same worktree.
/// Byte-identical ×4 (web `runHasEnded`, iOS `PastRuns.hasEnded`, Android
/// `runHasEnded`).
pub(crate) fn run_has_ended(session: &domain::rows::CodingSession) -> bool {
    session.status.as_deref() == Some(domain::contract::CODING_SESSION_STATUS_ENDED)
        && !run_is_stale_end(session)
}

/// When a run started: its `started_at`, else the row's creation stamp.
pub(crate) fn run_started_at(session: &domain::rows::CodingSession) -> Option<&str> {
    session
        .started_at
        .as_deref()
        .or(session.created_at.as_deref())
}

/// The row's subject line: the issue title, a sync placeholder while that
/// issue row is missing, the action-name snapshot (a chat run's reads its
/// `agent_title`, else "Chat", EXP-615/EXP-908), else the batch's own name (EXP-876: its first covered issue's
/// title, else "Batch run"). Byte-identical ×4: web `pastRunTitle`, iOS
/// `PastRuns.title`, Android `pastRunTitle`.
pub(crate) fn run_title(
    session: &domain::rows::CodingSession,
    issue: Option<&domain::rows::Issue>,
    batch_issues: &[domain::rows::Issue],
) -> SharedString {
    if let Some(issue) = issue {
        let title = issue.title.trim();
        return SharedString::from(if title.is_empty() {
            "Untitled issue".to_string()
        } else {
            title.to_string()
        });
    }
    if session.issue_id.is_some() {
        return SharedString::from("Issue syncing…");
    }
    // EXP-908: a chat run reads the agent's auto-named title, else "Chat".
    match domain::batch_run::action_run_subject(session) {
        Some(subject) => SharedString::from(subject),
        None => SharedString::from(domain::batch_run::batch_run_name(session, batch_issues).subject),
    }
}

/// EXP-876 — the row's mono lead-in: the issue's identifier, a batch's
/// `EXP-874 +2`, else none. The twin of [`run_title`], ×4 lockstep (web
/// `pastRunIdentifier`).
pub(crate) fn run_identifier(
    session: &domain::rows::CodingSession,
    issue: Option<&domain::rows::Issue>,
    batch_issues: &[domain::rows::Issue],
) -> Option<SharedString> {
    if let Some(issue) = issue {
        return Some(SharedString::from(issue.identifier.clone()));
    }
    if session.issue_id.is_some() || session.action_name.is_some() {
        return None;
    }
    domain::batch_run::batch_run_name(session, batch_issues)
        .identifier
        .map(SharedString::from)
}

/// EXP-876 — the issues a BATCH row names itself after, resolved against the
/// synced set. Cloned out of the store so the name can be built after the
/// collection borrow ends; empty (and free) for every other subject.
pub(crate) fn batch_run_issues(
    session: &domain::rows::CodingSession,
    cx: &App,
) -> Vec<domain::rows::Issue> {
    if !domain::batch_run::is_batch_run(session) {
        return Vec::new();
    }
    let Some(store) = sync::Store::try_global(cx) else {
        return Vec::new();
    };
    let collections = store.collections().clone();
    let issues = collections.issues.read(cx);
    domain::batch_run::batch_run_issues(session, issues.iter())
        .into_iter()
        .cloned()
        .collect()
}

/// EXP-746 — the ×4 byline under a PAST run: which machine ran it and when
/// it ended. Parts the row cannot prove are dropped (EXP-833: no agent, no
/// "ended by"). Byte-identical on web, iOS and Android.
pub(crate) fn past_run_byline(
    session: &domain::rows::CodingSession,
    device_label: Option<&str>,
    now_epoch: i64,
) -> String {
    let when = past_run_ended_at(session)
        .map(|at| crate::comments::relative_time(at, now_epoch))
        .unwrap_or_default();
    byline_parts(device_label, when)
}

/// `<device> · <when>`, either part dropped when it has nothing to say.
fn byline_parts(device_label: Option<&str>, when: String) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(label) = device_label.map(str::trim).filter(|label| !label.is_empty()) {
        parts.push(label.to_string());
    }
    if !when.is_empty() {
        parts.push(when);
    }
    parts.join(" · ")
}

/// EXP-886 — the word a live run wears in the run switcher's time slot, in
/// place of the ended relative time. Byte-identical ×4 (web `LIVE_RUN_LABEL`).
pub(crate) const LIVE_RUN_LABEL: &str = "Live";

/// EXP-886 — the switcher's `<when>` for a run: [`LIVE_RUN_LABEL`] for a
/// live-status run, else when it ended (empty without an honest stamp). The
/// switcher's trigger names the run on show by this. ×4 (web `issueRunWhen`).
pub(crate) fn issue_run_when(session: &domain::rows::CodingSession, now_epoch: i64) -> String {
    if queries::is_live_run_status(session) {
        return LIVE_RUN_LABEL.to_string();
    }
    past_run_ended_at(session)
        .map(|at| crate::comments::relative_time(at, now_epoch))
        .unwrap_or_default()
}

/// EXP-886 — one run switcher entry: the Recent byline with
/// [`issue_run_when`] in the time slot. ×4 (web `issueRunEntryLabel`).
pub(crate) fn issue_run_label(
    session: &domain::rows::CodingSession,
    device_label: Option<&str>,
    now_epoch: i64,
) -> String {
    byline_parts(device_label, issue_run_when(session, now_epoch))
}

/// EXP-950: the Run item's menu rows for `issue_id` — my runs of the issue
/// ([`crate::queries::issue_runs`]: live first, then newest end first), each
/// labelled [`issue_run_label`]. Empty when signed out or before the store
/// exists.
pub(crate) fn issue_run_entries(issue_id: &str, cx: &App) -> Vec<crate::work_header::RunEntry> {
    let Some(me) = crate::queries::active_account(cx).map(|account| account.user_id) else {
        return Vec::new();
    };
    let Some(store) = sync::Store::try_global(cx) else {
        return Vec::new();
    };
    let now = chrono::Utc::now().timestamp();
    let sessions = store.collections().coding_sessions.read(cx);
    let devices = store.collections().devices.read(cx);
    crate::queries::issue_runs(sessions.iter(), &me, issue_id)
        .into_iter()
        .map(|session| run_entry(session, devices.iter(), now))
        .collect()
}

/// EXP-974: the Run item's menu rows for an ISSUE-LESS run (chat / action /
/// batch) — `session_id`'s resume chain ([`crate::queries::run_chain`]: the
/// same run under every row that continued it), NEWEST FIRST like an issue's
/// runs, each labelled [`issue_run_label`]. A run nothing resumed and that
/// resumed nothing is a one-row menu, which the toggle shows no caret for.
/// Empty when signed out or before the store exists.
pub(crate) fn chain_run_entries(session_id: &str, cx: &App) -> Vec<crate::work_header::RunEntry> {
    let Some(me) = crate::queries::active_account(cx).map(|account| account.user_id) else {
        return Vec::new();
    };
    let Some(store) = sync::Store::try_global(cx) else {
        return Vec::new();
    };
    let now = chrono::Utc::now().timestamp();
    let sessions = store.collections().coding_sessions.read(cx);
    let devices = store.collections().devices.read(cx);
    let mut chain = crate::queries::run_chain(
        sessions
            .iter()
            .filter(|session| session.user_id.as_deref() == Some(me.as_str())),
        session_id,
    );
    chain.reverse();
    chain
        .into_iter()
        .map(|session| run_entry(session, devices.iter(), now))
        .collect()
}

/// One [`crate::work_header::RunEntry`] for a run: the machine's current
/// label ([`crate::queries::session_device_presentation`]) and
/// [`issue_run_label`], live-status runs marked.
fn run_entry<'a>(
    session: &domain::rows::CodingSession,
    devices: impl Iterator<Item = &'a domain::rows::DeviceRow>,
    now: i64,
) -> crate::work_header::RunEntry {
    let label = crate::queries::session_device_presentation(session, devices, now * 1_000).label;
    crate::work_header::RunEntry {
        id: session.id.clone(),
        label: issue_run_label(session, label.as_deref(), now),
        live: crate::queries::is_live_run_status(session),
    }
}

/// When a past run finished: its `ended_at`, else the last `updated_at` (a
/// row the server swept never got an `ended_at`) — the same key
/// [`crate::queries::own_ended_runs`] orders by.
pub(crate) fn past_run_ended_at(session: &domain::rows::CodingSession) -> Option<&str> {
    session
        .ended_at
        .as_deref()
        .or(session.updated_at.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-1208: a live row's run mark, ×4 (web `runningRowMarkState`).
    #[test]
    fn a_running_row_mark_follows_the_display_state() {
        use CodingSessionDisplay::*;
        // A paused run (offline host) is the bare mark, whatever its state.
        assert_eq!(running_row_mark(Review, true, false), None);
        // Only a WORKING row animates.
        assert_eq!(running_row_mark(Working, false, true), Some(Working));
        assert_eq!(running_row_mark(Working, false, false), None);
        for state in [NeedsInput, Review, Done] {
            assert_eq!(running_row_mark(state, false, false), Some(state));
        }
    }

    /// EXP-1208: the mark is one indent level square, so its centre is the
    /// gutter centre a child's connector elbow hangs off and the stub ends
    /// at the child's mark.
    #[test]
    fn the_run_mark_fills_one_indent_level() {
        assert_eq!(RUN_MARK_PX, crate::tree_guides::LEVEL_PITCH);
    }

    /// SLOP-2: the four cases, byte-identical ×4 (web `actionRunTitle`).
    #[test]
    fn action_run_title_names_what_started_the_run() {
        assert_eq!(action_run_title(Some("schedule")), "Scheduled run");
        assert_eq!(action_run_title(Some("event")), "Event run");
        for reason in ["agent", "something-new"] {
            assert_eq!(action_run_title(Some(reason)), "Agent run", "{reason}");
        }
        assert_eq!(action_run_title(None), "Manual run");
        assert_eq!(action_run_title(Some("")), "Manual run");
    }

    /// EXP-1175 — fixture `run-row.json` ×4: the captions and the labels.
    #[test]
    fn run_row_caption_matches_the_shared_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/run-row.json"
        ))
        .unwrap();
        assert_eq!(show_work_label(false), fixture["showWorkLabel"].as_str().unwrap());
        assert_eq!(show_work_label(true), fixture["hideWorkLabel"].as_str().unwrap());
        assert_eq!(SHOW_WORK_DEFAULT, fixture["showWorkDefault"].as_bool().unwrap());
        let cases = fixture["captions"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let state = match case["state"].as_str().unwrap() {
                "working" => RunRowState::Working,
                "needs_input" => RunRowState::NeedsInput,
                "paused" => RunRowState::Paused,
                "review" => RunRowState::Review,
                "done" => RunRowState::Done,
                "ended" => RunRowState::Ended,
                other => panic!("unknown state {other}"),
            };
            let (text, tone) = run_row_caption(
                state,
                case["device"].as_str().unwrap(),
                stamp_ms(case["startedAt"].as_str()),
                stamp_ms(case["endedAt"].as_str()),
                stamp_ms(case["now"].as_str()).unwrap(),
            );
            let tone = match tone {
                StatusTone::Muted => "muted",
                StatusTone::Amber => "amber",
                StatusTone::Green => "emerald",
                StatusTone::Blue => "sky",
            };
            assert_eq!(text, case["expected"]["text"].as_str().unwrap(), "{name}");
            assert_eq!(tone, case["expected"]["tone"].as_str().unwrap(), "{name}");
        }
    }

    /// EXP-1175 — fixture `run-row.json` `states` ×4.
    #[test]
    fn run_row_state_matches_the_shared_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/run-row.json"
        ))
        .unwrap();
        let cases = fixture["states"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let flag = |key: &str| case[key].as_bool().unwrap();
            let display = match case["display"].as_str().unwrap() {
                "working" => CodingSessionDisplay::Working,
                "needs_input" => CodingSessionDisplay::NeedsInput,
                "review" => CodingSessionDisplay::Review,
                "done" => CodingSessionDisplay::Done,
                other => panic!("unknown display {other}"),
            };
            let state = run_row_state(
                flag("paused"),
                flag("ended"),
                flag("awaitingInput"),
                flag("working"),
                display,
            );
            let actual = match state {
                RunRowState::Working => "working",
                RunRowState::NeedsInput => "needs_input",
                RunRowState::Paused => "paused",
                RunRowState::Review => "review",
                RunRowState::Done => "done",
                RunRowState::Ended => "ended",
            };
            assert_eq!(actual, case["expected"].as_str().unwrap(), "{name}");
        }
    }

    fn session(id: &str) -> domain::rows::CodingSession {
        serde_json::from_value(serde_json::json!({ "id": id })).expect("row")
    }

    /// EXP-888 — the ONE live/ended predicate ×4 (web `runHasEnded`, iOS
    /// `PastRuns.hasEnded`, Android `runHasEnded`): a sweep end is not an end.
    #[test]
    fn a_sweep_end_is_not_an_end() {
        let row = |status: &str, ended_by: Option<&str>| -> domain::rows::CodingSession {
            let mut run = session("s-1");
            run.status = Some(status.to_string());
            run.ended_by = ended_by.map(str::to_string);
            run
        };
        assert!(!run_has_ended(&row("running", None)));
        assert!(!run_has_ended(&row("in_review", None)));
        assert!(run_has_ended(&row("ended", Some("agent"))));
        assert!(run_has_ended(&row("ended", Some("user"))));
        assert!(run_has_ended(&row("ended", Some("merge"))));
        // The staleness sweep's end: the host ignores the flip and heartbeats
        // the row back to `running`, so the run is LIVE, not past.
        assert!(!run_has_ended(&row(
            "ended",
            Some(domain::contract::CODING_SESSION_ENDED_BY_STALE)
        )));
        // A legacy row that stamped no reason still reads as ended.
        assert!(run_has_ended(&row("ended", None)));

        assert!(run_is_stale_end(&row("ended", Some("stale"))));
        // `stale` only ever rides an `ended` row; on a live one it means nothing.
        assert!(!run_is_stale_end(&row("running", Some("stale"))));

        // The live-badge/ordering twins say the same about a swept row.
        let now = 1_800_000_000;
        let mut swept = row("ended", Some("stale"));
        swept.updated_at = Some(
            chrono::DateTime::from_timestamp(now, 0)
                .expect("timestamp")
                .to_rfc3339(),
        );
        assert!(crate::queries::is_live_run_status(&swept));
        assert!(crate::queries::coding_session_is_live(&swept, now));
        assert!(!crate::queries::coding_session_is_live(
            &row("ended", Some("agent")),
            now
        ));
    }

    /// EXP-886: a switcher entry says `Live` for a live-status run and the
    /// ended time otherwise, either part dropped when the row cannot prove it
    /// — byte-identical with the web `issueRunEntryLabel`.
    #[test]
    fn a_run_entry_says_live_for_a_live_run_and_the_ended_time_otherwise() {
        let now = 1_800_000_000;
        let live: domain::rows::CodingSession = serde_json::from_value(serde_json::json!({
            "id": "live", "status": "running"
        }))
        .expect("row");
        assert_eq!(issue_run_when(&live, now), "Live");
        assert_eq!(issue_run_label(&live, Some("macbook"), now), "macbook · Live");
        // The device label may be missing; the word alone then.
        assert_eq!(issue_run_label(&live, Some("  "), now), "Live");
        let review: domain::rows::CodingSession = serde_json::from_value(serde_json::json!({
            "id": "review", "status": "in_review"
        }))
        .expect("row");
        assert_eq!(issue_run_when(&review, now), "Live");
        // An ended run with no honest stamp keeps only its machine.
        let ended: domain::rows::CodingSession = serde_json::from_value(serde_json::json!({
            "id": "ended", "status": "ended"
        }))
        .expect("row");
        assert_eq!(issue_run_when(&ended, now), "");
        assert_eq!(issue_run_label(&ended, Some("macbook"), now), "macbook");
        assert_eq!(past_run_byline(&ended, Some("macbook"), now), "macbook");
        assert_eq!(LIVE_RUN_LABEL, "Live");
    }

    /// EXP-874: the status line names the state first and the machine after
    /// it — except a plain running run, which says where and since when.
    #[test]
    fn the_running_status_line_names_state_device_and_tone() {
        use CodingSessionDisplay as D;
        let line = |display, paused, device, rel| running_status_line(display, paused, device, rel);
        assert_eq!(
            line(D::Working, false, Some("Studio"), "2 minutes ago"),
            ("Studio · started 2 minutes ago".to_string(), StatusTone::Muted)
        );
        // EXP-550: the offline host wins over the display word.
        assert_eq!(
            line(D::NeedsInput, true, Some("Studio"), "2 minutes ago"),
            ("Paused · Studio".to_string(), StatusTone::Muted)
        );
        assert_eq!(
            line(D::NeedsInput, false, Some("Studio"), ""),
            ("Needs input · Studio".to_string(), StatusTone::Amber)
        );
        assert_eq!(
            line(D::Review, false, Some("Studio"), ""),
            ("Ready for review · Studio".to_string(), StatusTone::Green)
        );
        assert_eq!(
            line(D::Done, false, Some("Studio"), ""),
            ("Done · Studio".to_string(), StatusTone::Blue)
        );
        // Nothing is invented: no device, no time.
        assert_eq!(
            line(D::Review, false, Some("  "), ""),
            ("Ready for review".to_string(), StatusTone::Green)
        );
        assert_eq!(
            line(D::Working, false, None, "5 minutes ago"),
            ("started 5 minutes ago".to_string(), StatusTone::Muted)
        );
        assert_eq!(line(D::Working, false, None, ""), (String::new(), StatusTone::Muted));
    }

    /// EXP-746 — the ×4 past byline: machine and how long ago. EXP-833: the
    /// agent and who ended the run are NOT part of it.
    #[test]
    fn the_past_byline_names_the_device_and_when_it_ended() {
        let now = 1_700_000_000;
        let ended_at = chrono::DateTime::from_timestamp(now - 300, 0)
            .expect("timestamp")
            .to_rfc3339();
        let mut run = session("s-1");
        run.agent = Some("codex".to_string());
        run.ended_by = Some(domain::contract::CODING_SESSION_ENDED_BY_MERGE.to_string());
        run.ended_at = Some(ended_at.clone());
        assert_eq!(
            past_run_byline(&run, Some("Studio"), now),
            "Studio · 5 minutes ago"
        );

        let mut sparse = session("s-2");
        sparse.ended_at = Some(ended_at.clone());
        assert_eq!(past_run_byline(&sparse, None, now), "5 minutes ago");

        // A row the server swept without an `ended_at` still dates itself.
        let mut swept = session("s-3");
        swept.updated_at = Some(ended_at);
        assert_eq!(past_run_byline(&swept, Some("Server"), now), "Server · 5 minutes ago");
    }

    /// The subject line falls back: the issue's title, a sync placeholder
    /// while the issue is missing, the action's name, else the batch.
    /// Byte-identical ×4 so every list names the same run the same way.
    #[test]
    fn a_row_titles_itself_from_whatever_it_has() {
        let row = |value: serde_json::Value| -> domain::rows::CodingSession {
            serde_json::from_value(value).expect("row")
        };
        let issue = |title: &str| -> domain::rows::Issue {
            serde_json::from_value(serde_json::json!({
                "id": "i-1",
                "board_id": "b-1",
                "identifier": "EXP-1",
                "number": 1,
                "title": title,
                "status": "in_progress",
                "priority": "none",
            }))
            .expect("issue")
        };
        let scoped = row(serde_json::json!({ "id": "s-0", "issue_id": "i-1" }));
        assert_eq!(
            run_title(&scoped, Some(&issue("Fix the sync loop")), &[]),
            SharedString::from("Fix the sync loop")
        );
        assert_eq!(
            run_title(&scoped, Some(&issue("  ")), &[]),
            SharedString::from("Untitled issue")
        );
        let batch = row(serde_json::json!({ "id": "s-1" }));
        assert_eq!(run_title(&batch, None, &[]), SharedString::from("Batch run"));
        let action = row(serde_json::json!({ "id": "s-2", "action_name": "Release train" }));
        assert_eq!(
            run_title(&action, None, &[]),
            SharedString::from("Release train")
        );
        // A chat run carries "Chat" as its action snapshot (EXP-615).
        let chat = row(serde_json::json!({
            "id": "s-4", "action_name": "Chat", "branch": "exp/chat-1a2b3c4d"
        }));
        assert_eq!(run_title(&chat, None, &[]), SharedString::from("Chat"));
        // EXP-908: once the agent names the chat, the subject is that title.
        let named = row(serde_json::json!({
            "id": "s-5", "action_name": "Chat", "agent_title": " Tab shell polish "
        }));
        assert_eq!(run_title(&named, None, &[]), SharedString::from("Tab shell polish"));
        let syncing = row(serde_json::json!({ "id": "s-3", "issue_id": "i-1" }));
        assert_eq!(
            run_title(&syncing, None, &[]),
            SharedString::from("Issue syncing…")
        );
    }

    /// EXP-876 — a batch fills the SAME two slots as an issue run, so two of
    /// them in one list are told apart. The rule itself is
    /// `domain::batch_run`; this is the row's wiring, ×4 with web
    /// `pastRunIdentifier`.
    #[test]
    fn a_batch_row_names_itself_after_its_issues() {
        let row = |value: serde_json::Value| -> domain::rows::CodingSession {
            serde_json::from_value(value).expect("row")
        };
        let covered = |id: &str, identifier: &str, title: &str| -> domain::rows::Issue {
            serde_json::from_value(serde_json::json!({
                "id": id,
                "board_id": "b-1",
                "identifier": identifier,
                "number": 1,
                "title": title,
                "status": "in_progress",
                "priority": "none",
                "branch": "exp/batch-1a2b3c4d",
                "created_at": "2026-09-01T10:00:00Z",
            }))
            .expect("issue")
        };
        let issues = vec![
            covered("i-1", "EXP-874", "Session list fixes"),
            covered("i-2", "EXP-876", "Batch run names"),
        ];
        let batch = row(serde_json::json!({
            "id": "s-1", "batch_issue_ids": ["i-1", "i-2"]
        }));
        assert_eq!(
            run_identifier(&batch, None, &issues),
            Some(SharedString::from("EXP-874 +1"))
        );
        assert_eq!(
            run_title(&batch, None, &issues),
            SharedString::from("Session list fixes")
        );
        // Nothing to name it by: the generic label, and no lead-in.
        let unknown = row(serde_json::json!({ "id": "s-2" }));
        assert_eq!(run_identifier(&unknown, None, &issues), None);
        assert_eq!(
            run_title(&unknown, None, &issues),
            SharedString::from("Batch run")
        );
        // An action run never grows one.
        let action = row(serde_json::json!({ "id": "s-3", "action_name": "Chat" }));
        assert_eq!(run_identifier(&action, None, &issues), None);
    }
}
