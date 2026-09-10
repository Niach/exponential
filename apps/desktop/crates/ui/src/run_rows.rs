//! EXP-746 — the ONE agent-run row.
//!
//! Three lists render a `coding_sessions` row as a card: the Automations
//! tab's "Recent automated runs" (EXP-637/686), and the Devices screen's
//! Running and Past sections. They were about to become three hand-drawn
//! rows, so the row generalized instead: [`render_run_row`] takes a
//! [`RunRowSpec`] and every list is a caller.
//!
//! EXP-773 flattened the shape: a lead glyph, the run's name, a muted caption
//! and nothing else. A row is a plain LINK — clicking it opens the fullscreen
//! session view, which is where the transcript, the run's summary and its
//! Resume button live now. What the lists vary is the lead (the automation
//! glyph, the run's agent mark, a live status dot), the caption (a relative
//! time, a device byline) and whether the row offers a ⋯ menu that ends the
//! run.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, App, ClickEvent, Hsla, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{menu::DropdownMenu as _, ActiveTheme as _, Icon, Sizable as _};

use crate::coding_selects::agent_icon;
use crate::icons::registry;

/// A row callback (open / toggle / resume / kill) — boxed so the spec stays
/// one type across callers that close over entities of different views.
pub(crate) type RunRowAction = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// What leads the row.
pub(crate) enum RunRowLead {
    /// The Automations list: every row IS an automated run, so the glyph
    /// says which list you are in, not which agent ran.
    Automation,
    /// The run's agent CLI (Past rows — the agent is the thing you scan for).
    /// `None` for a run with no agent on the row: an EXP-746 external agent
    /// (the server's vocabulary is closed, so it records none) or a row from
    /// before the column existed. It leads with the neutral session glyph
    /// rather than picking a brand at random.
    Agent(Option<coding::CodingAgent>),
    /// A live status dot in the display's tone (Running rows).
    Live(Hsla),
}

/// The ⋯ menu's single destructive item — "end this run".
pub(crate) struct RunRowKill {
    /// "Stop session" (a run this process hosts) / "Kill session" (a run on
    /// another machine): the two ends read differently, so the caller names
    /// the item.
    pub(crate) label: SharedString,
    pub(crate) on_kill: RunRowAction,
}

pub(crate) struct RunRowSpec {
    /// Element-id namespace for the row's stateful children. The Devices
    /// screen renders two run lists in one scroll, and ids collide within a
    /// parent — each list names its own.
    pub(crate) id_prefix: &'static str,
    pub(crate) index: usize,
    pub(crate) lead: RunRowLead,
    /// The issue identifier, rendered muted ahead of the title. `None` for a
    /// run with no issue (an action run, a batch, an automation).
    pub(crate) identifier: Option<SharedString>,
    pub(crate) title: SharedString,
    pub(crate) caption: Option<SharedString>,
    pub(crate) on_open: Option<RunRowAction>,
    pub(crate) kill: Option<RunRowKill>,
}

/// One run card (EXP-637/686, generalized by EXP-746, flattened by EXP-773).
///
/// Every row is a LINK: `on_open` makes the whole card open the run's
/// fullscreen session view, where the transcript, the summary and Resume live.
/// The chevron that used to unfold a summary inside the list is gone — a run
/// was described in two places, and the list is not the better one. Same rule
/// in every runs list on every client.
pub(crate) fn render_run_row(spec: RunRowSpec, cx: &App) -> gpui::AnyElement {
    let RunRowSpec {
        id_prefix,
        index,
        lead,
        identifier,
        title,
        caption,
        on_open,
        kill,
    } = spec;
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let header = div()
        .flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_2()
        .child(match lead {
            RunRowLead::Automation => Icon::from(registry::ACTION_AUTOMATION)
                .xsmall()
                .text_color(muted)
                .into_any_element(),
            RunRowLead::Agent(agent) => Icon::from(match agent {
                Some(agent) => agent_icon(agent),
                None => registry::SESSION_SHELL,
            })
            .xsmall()
            .text_color(muted)
            .into_any_element(),
            // The strip's live dot (`ChipLead::Dot`), at the glyph's altitude.
            RunRowLead::Live(tone) => div()
                .flex_shrink_0()
                .size_1p5()
                .rounded_full()
                .bg(tone)
                .into_any_element(),
        })
        .children(identifier.map(|identifier| {
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
        )
        .children(caption.map(|caption| {
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(muted)
                .child(caption)
        }))
        .when_some(kill, |this, kill| {
            let on_kill = std::rc::Rc::new(kill.on_kill);
            let label = kill.label.clone();
            let menu = crate::controls::glass_icon_button(
                (SharedString::from(format!("{id_prefix}-menu")), index),
                Icon::from(registry::UI_MORE),
                cx,
            )
            .dropdown_menu(move |menu, _window, cx| {
                let on_kill = on_kill.clone();
                menu.item(
                    crate::controls::danger_menu_item(
                        label.clone(),
                        Icon::from(registry::CODING_STOP),
                        cx,
                    )
                    .on_click(move |event, window, cx| {
                        cx.stop_propagation();
                        on_kill(event, window, cx);
                    }),
                )
            });
            // The card under the menu may be clickable: opening the menu must
            // not also open the session. The wrapper's bubble-phase handler
            // runs after the popover's own, so the menu still opens.
            this.child(
                div()
                    .id((SharedString::from(format!("{id_prefix}-menu-stop")), index))
                    .flex_shrink_0()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(menu),
            )
        });
    crate::surface::flat_row()
        .id((SharedString::from(format!("{id_prefix}-card")), index))
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .gap_2()
        .px_3()
        .py_2p5()
        .when_some(on_open, |this, on_open| {
            this.cursor_pointer()
                .on_click(move |event, window, cx| on_open(event, window, cx))
        })
        .child(header)
        .into_any_element()
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

/// The title + caption + chevron gate an AUTOMATION run row carries — the
/// EXP-686 vocabulary, moved here with the row it feeds so the Automations
/// tab and this module can never disagree about it.
pub(crate) struct AutomationRowParts {
    /// The action's name SNAPSHOT (it survives the action's deletion; only a
    /// pre-EXP-253 row could lack it).
    pub(crate) title: SharedString,
    /// The ONLY status word left is "Running" (EXP-686) — an ended row is
    /// just a name and a time.
    pub(crate) caption: SharedString,
}

pub(crate) fn automation_row_parts(
    session: &domain::rows::CodingSession,
    now_epoch: i64,
) -> AutomationRowParts {
    let when = run_started_at(session)
        .map(|at| crate::comments::relative_time(at, now_epoch))
        .unwrap_or_default();
    AutomationRowParts {
        title: SharedString::from(
            session
                .action_name
                .clone()
                .unwrap_or_else(|| "Action".to_string()),
        ),
        caption: SharedString::from(if run_has_ended(session) {
            when
        } else {
            format!("Running · {when}")
        }),
    }
}

/// EXP-746 — the ×4 byline under a PAST run: which machine ran it, which
/// agent, who ended it and when. Parts the row cannot prove are dropped
/// rather than guessed (a missing device label, an agent-less external run,
/// an `ended_by` this build predates), and the separator is the same middle
/// dot every client uses.
///
/// Byte-identical on web, iOS and Android — change it in four places or not
/// at all.
pub(crate) fn past_run_byline(
    session: &domain::rows::CodingSession,
    device_label: Option<&str>,
    now_epoch: i64,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(label) = device_label.map(str::trim).filter(|label| !label.is_empty()) {
        parts.push(label.to_string());
    }
    if let Some(agent) = session
        .agent
        .as_deref()
        .map(str::trim)
        .filter(|agent| !agent.is_empty())
    {
        parts.push(crate::launch_options::agent_label(agent));
    }
    if let Some(who) = ended_by_words(session.ended_by.as_deref()) {
        parts.push(format!("ended by {who}"));
    }
    if let Some(at) = past_run_ended_at(session) {
        let when = crate::comments::relative_time(at, now_epoch);
        if !when.is_empty() {
            parts.push(when);
        }
    }
    parts.join(" · ")
}

/// How the byline names each `ended_by` path (contract
/// `codingSession.endedBy`). An unknown value says nothing at all — a newer
/// server's vocabulary must not print a raw enum id at people.
fn ended_by_words(ended_by: Option<&str>) -> Option<&'static str> {
    match ended_by? {
        domain::contract::CODING_SESSION_ENDED_BY_AGENT => Some("agent"),
        domain::contract::CODING_SESSION_ENDED_BY_USER => Some("you"),
        domain::contract::CODING_SESSION_ENDED_BY_CLIENT => Some("the app"),
        domain::contract::CODING_SESSION_ENDED_BY_MERGE => Some("a merge"),
        domain::contract::CODING_SESSION_ENDED_BY_SYSTEM => Some("the system"),
        _ => None,
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

    fn session(id: &str) -> domain::rows::CodingSession {
        serde_json::from_value(serde_json::json!({ "id": id })).expect("row")
    }

    /// EXP-746 moved the Automations row here; its rendered shape must not
    /// have moved with it. The row is built from these two values, so locking
    /// them locks the row: the action-name snapshot (with the pre-EXP-253
    /// fallback) and the EXP-686 caption vocabulary ("Running · …" while live,
    /// a bare relative time once ended). EXP-773 retired the chevron — a row
    /// opens the session view instead of unfolding a summary in place.
    #[test]
    fn an_automation_row_renders_the_same_shape_it_used_to() {
        let now = 1_700_000_000;
        let started = chrono::DateTime::from_timestamp(now - 7_200, 0)
            .expect("timestamp")
            .to_rfc3339();

        let mut live = session("s-live");
        live.action_name = Some("Release train".to_string());
        live.started_at = Some(started.clone());
        live.status = Some(domain::contract::CODING_SESSION_STATUS_RUNNING.to_string());
        let parts = automation_row_parts(&live, now);
        assert_eq!(parts.title, SharedString::from("Release train"));
        assert_eq!(parts.caption, SharedString::from("Running · 2 hours ago"));

        let mut ended = live.clone();
        ended.id = "s-ended".to_string();
        ended.status = Some(domain::contract::CODING_SESSION_STATUS_ENDED.to_string());
        let parts = automation_row_parts(&ended, now);
        assert_eq!(parts.caption, SharedString::from("2 hours ago"));

        // A row from before the action-name snapshot still names itself.
        let mut nameless = ended.clone();
        nameless.action_name = None;
        assert_eq!(
            automation_row_parts(&nameless, now).title,
            SharedString::from("Action")
        );

        // The automation lead carries no kill affordance and no identifier —
        // the shape the Automations tab always had.
        let spec = RunRowSpec {
            id_prefix: "run",
            index: 0,
            lead: RunRowLead::Automation,
            identifier: None,
            title: parts.title.clone(),
            caption: Some(parts.caption.clone()),
            on_open: None,
            kill: None,
        };
        assert!(matches!(spec.lead, RunRowLead::Automation));
        assert!(spec.kill.is_none());
        assert!(spec.identifier.is_none());
    }

    /// EXP-746 — the ×4 past byline. Locked because web, iOS and Android
    /// render the same sentence from the same row fields: machine, agent, who
    /// ended the run, how long ago.
    #[test]
    fn the_past_byline_names_device_agent_and_who_ended_it() {
        let now = 1_700_000_000;
        let ended_at = chrono::DateTime::from_timestamp(now - 300, 0)
            .expect("timestamp")
            .to_rfc3339();
        let mut run = session("s-1");
        run.agent = Some("codex".to_string());
        run.ended_by = Some(domain::contract::CODING_SESSION_ENDED_BY_USER.to_string());
        run.ended_at = Some(ended_at.clone());
        assert_eq!(
            past_run_byline(&run, Some("Studio"), now),
            "Studio · Codex · ended by you · 5 minutes ago"
        );

        // Each `ended_by` path has its own words.
        for (value, words) in [
            (domain::contract::CODING_SESSION_ENDED_BY_AGENT, "agent"),
            (domain::contract::CODING_SESSION_ENDED_BY_CLIENT, "the app"),
            (domain::contract::CODING_SESSION_ENDED_BY_MERGE, "a merge"),
            (domain::contract::CODING_SESSION_ENDED_BY_SYSTEM, "the system"),
        ] {
            run.ended_by = Some(value.to_string());
            assert!(
                past_run_byline(&run, Some("Studio"), now).contains(&format!("ended by {words}")),
                "{value} must read as '{words}'"
            );
        }

        // Nothing is invented: an unknown end path, a missing device label
        // and a missing agent each drop their part instead of guessing.
        let mut sparse = session("s-2");
        sparse.ended_by = Some("teleport".to_string());
        sparse.ended_at = Some(ended_at.clone());
        assert_eq!(past_run_byline(&sparse, None, now), "5 minutes ago");

        // A row the server swept without an `ended_at` still dates itself.
        let mut swept = session("s-3");
        swept.updated_at = Some(ended_at);
        assert_eq!(past_run_byline(&swept, Some("Server"), now), "Server · 5 minutes ago");
    }
}
