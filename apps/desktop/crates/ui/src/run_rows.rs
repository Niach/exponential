//! EXP-746 — the agent-run rows, EXP-874 — ONE layout per kind.
//!
//! Every runs list (the Agent page's Running/Recent bands, an issue's Runs
//! band, the Sessions list nav, the Automations page's "Recent automated
//! runs" and its list nav)
//! draws a `coding_sessions` row through one of two renderers:
//!
//! - [`render_running_run_row`]: dot · identifier · title, the agent caption,
//!   a toned status line and the usage-wall badge. The whole row opens the
//!   run (EXP-893: no trailing Merge / open-the-subject buttons); "Stop
//!   session" is on the row's right-click menu, never a button.
//! - [`render_past_run_row`]: identifier · title over the byline, a trailing
//!   chevron. The whole row opens the session.
//!
//! No agent brand mark appears in either (EXP-874). The data half
//! ([`running_run_facts`], [`past_run_facts`]) is shared too, so the lists
//! cannot disagree about what a run says.

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
    fn color(self, muted: Hsla) -> Hsla {
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
    pub(crate) dot: Hsla,
    pub(crate) blocked: Option<SharedString>,
    /// The machine's name (the kill confirm names it).
    pub(crate) device_label: Option<String>,
    pub(crate) paused: bool,
}

/// Everything a past row draws.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PastRunFacts {
    pub(crate) session_id: String,
    pub(crate) identifier: Option<SharedString>,
    pub(crate) title: SharedString,
    pub(crate) byline: SharedString,
}

/// A LIVE run's row facts. `local_caption` is the engine's own caption for a
/// run this process hosts (`session_agent_caption` precedence).
pub(crate) fn running_run_facts(
    session: &domain::rows::CodingSession,
    local_caption: Option<String>,
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
    let display = queries::coding_session_display(
        session,
        issue
            .as_ref()
            .and_then(|issue| issue.pr_state.as_deref())
            .or(session.pr_state.as_deref()),
    );
    let paused = queries::session_is_paused(display, &presentation);
    let started = run_started_at(session)
        .map(|at| crate::comments::relative_time(at, now_epoch))
        .unwrap_or_default();
    let (status, status_tone) =
        running_status_line(display, paused, presentation.label.as_deref(), &started);
    let blocked = crate::usage_bar::parse_blocked(session.blocked.as_ref());
    // EXP-876: a batch row names itself after the issues it covers.
    let batch_issues = batch_run_issues(session, cx);
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
        blocked: crate::usage_bar::blocked_badge_label(blocked.as_ref(), now_epoch)
            .map(SharedString::from),
        device_label: presentation.label,
        paused,
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
        CodingSessionDisplay::Running => {
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

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

pub(crate) struct RunningRunSpec {
    /// Element-id namespace: two run lists can share one parent.
    pub(crate) id_prefix: &'static str,
    pub(crate) index: usize,
    /// EXP-827: nesting depth (0 = a root run), 14px per level.
    pub(crate) depth: usize,
    pub(crate) fold: Option<RunRowFold>,
    pub(crate) facts: RunningRunFacts,
    pub(crate) on_open: RunRowAction,
    /// `Some` = the row's right-click menu offers "Stop session".
    pub(crate) kill: Option<RunRowKill>,
}

pub(crate) struct PastRunSpec {
    pub(crate) id_prefix: &'static str,
    pub(crate) index: usize,
    pub(crate) depth: usize,
    pub(crate) fold: Option<RunRowFold>,
    pub(crate) facts: PastRunFacts,
    pub(crate) on_open: RunRowAction,
}

/// The flat list row both kinds sit in: `list_hover` under the pointer,
/// `list_active` while its session is on screen (EXP-811/862).
fn row_shell(id_prefix: &'static str, index: usize, depth: usize, active: bool, cx: &App) -> gpui::Stateful<gpui::Div> {
    let theme = cx.theme();
    let row_hover = theme.list_hover;
    let row_active = theme.list_active;
    crate::surface::flat_row()
        .id((SharedString::from(format!("{id_prefix}-card")), index))
        .flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_2()
        .px_3()
        .py_2p5()
        .pl(gpui::px(12. + 14. * depth as f32))
        .cursor_pointer()
        .when(active, |this| this.bg(row_active))
        .hover(move |style| style.bg(if active { row_active } else { row_hover }))
}

fn fold_chevron(id_prefix: &'static str, index: usize, fold: RunRowFold, muted: Hsla) -> impl IntoElement {
    let RunRowFold { collapsed, on_toggle } = fold;
    div()
        .id((SharedString::from(format!("{id_prefix}-fold")), index))
        .flex_shrink_0()
        .cursor_pointer()
        // EXP-897: the fold's accessible label, byte-identical ×4.
        .tooltip(move |window, cx| {
            gpui_component::tooltip::Tooltip::new(if collapsed {
                domain::pr_stack::EXPAND_CHILD_RUNS
            } else {
                domain::pr_stack::COLLAPSE_CHILD_RUNS
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

/// One LIVE run row (EXP-874). The whole row opens the run.
pub(crate) fn render_running_run_row(
    spec: RunningRunSpec,
    active: bool,
    cx: &App,
) -> gpui::AnyElement {
    let RunningRunSpec {
        id_prefix,
        index,
        depth,
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
        .children(fold.map(|fold| fold_chevron(id_prefix, index, fold, muted)))
        .child(div().flex_shrink_0().size_1p5().rounded_full().bg(facts.dot))
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

    let row = row_shell(id_prefix, index, depth, active, cx)
        .on_click(move |event, window, cx| on_open(event, window, cx))
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
        depth,
        fold,
        facts,
        on_open,
    } = spec;
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let line1 = div()
        .flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_2()
        .children(fold.map(|fold| fold_chevron(id_prefix, index, fold, muted)))
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
                .child(facts.title),
        );
    let body = gpui_component::v_flex()
        .flex_1()
        .min_w_0()
        .gap_0p5()
        .child(line1)
        .when(!facts.byline.is_empty(), |this| {
            this.child(
                div()
                    .w_full()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(muted)
                    .child(facts.byline),
            )
        });
    row_shell(id_prefix, index, depth, active, cx)
        .on_click(move |event, window, cx| on_open(event, window, cx))
        .child(body)
        .child(
            div()
                .flex_shrink_0()
                .child(Icon::from(registry::UI_CHEVRON_RIGHT).xsmall().text_color(muted)),
        )
        .into_any_element()
}

/// A row of a mixed list (the Automations logs): live runs draw as running
/// rows, ended ones as past rows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RunListFacts {
    Running(RunningRunFacts),
    Past(PastRunFacts),
}

impl RunListFacts {
    pub(crate) fn derive(session: &domain::rows::CodingSession, now_epoch: i64, cx: &App) -> Self {
        if run_has_ended(session) {
            RunListFacts::Past(past_run_facts(session, now_epoch, cx))
        } else {
            RunListFacts::Running(running_run_facts(session, None, now_epoch, cx))
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
/// `depth` and fold like every other session row — the Automations log nests
/// its child runs too.
pub(crate) fn render_run_list_row(
    id_prefix: &'static str,
    index: usize,
    depth: usize,
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
                depth,
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
                depth,
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

/// Whether the row's synced status is the terminal `ended`.
pub(crate) fn run_has_ended(session: &domain::rows::CodingSession) -> bool {
    session.status.as_deref() == Some(domain::contract::CODING_SESSION_STATUS_ENDED)
}

/// When a run started: its `started_at`, else the row's creation stamp.
pub(crate) fn run_started_at(session: &domain::rows::CodingSession) -> Option<&str> {
    session
        .started_at
        .as_deref()
        .or(session.created_at.as_deref())
}

/// The row's subject line: the issue title, a sync placeholder while that
/// issue row is missing, the action-name snapshot (a chat run's reads "Chat",
/// EXP-615), else the batch's own name (EXP-876: its first covered issue's
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
    match session.action_name.as_deref() {
        Some(name) if !name.trim().is_empty() => SharedString::from(name.to_string()),
        _ => SharedString::from(domain::batch_run::batch_run_name(session, batch_issues).subject),
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

    fn session(id: &str) -> domain::rows::CodingSession {
        serde_json::from_value(serde_json::json!({ "id": id })).expect("row")
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
            line(D::Running, false, Some("Studio"), "2 minutes ago"),
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
            line(D::Running, false, None, "5 minutes ago"),
            ("started 5 minutes ago".to_string(), StatusTone::Muted)
        );
        assert_eq!(line(D::Running, false, None, ""), (String::new(), StatusTone::Muted));
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
