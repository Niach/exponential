//! S-3: `session-xref` (2 General components): THE inter-session link
//! ([`crate::session_xref`]), case for case with the web entry: the four
//! kinds (`Message from` · `Report from` · `Started by` · `Reported to`),
//! two through an agent's avatar, two through the session glyph, then one
//! whose long name truncates. No `sync::Store` here, so the demo agents'
//! marks are drawn from their icon + colour directly.

use gpui::{div, px, AnyElement, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::{v_flex, ActiveTheme as _};

use domain::session_tree::SessionXref;

use crate::session_xref::{agent_mark, session_xref_with, xref_id, xref_mark};

pub(crate) const ID: &str = "session-xref";
pub(crate) const OWNER: &str = "S-3";

fn xref(kind: &str, name: &str, agent: Option<&str>) -> SessionXref {
    SessionXref {
        kind: kind.into(),
        session_id: format!("sg-xref-{kind}"),
        name: name.into(),
        agent_id: agent.map(str::to_string),
    }
}

/// One case: the line, its caption under it (the web `Captioned`).
fn captioned(caption: &'static str, line: AnyElement, cx: &App) -> Div {
    v_flex()
        .gap_1()
        .items_start()
        .child(line)
        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(caption))
}

fn line(key: &str, xref: &SessionXref, agent: Option<(&str, &str)>, cx: &App) -> AnyElement {
    let mark = match agent {
        Some((icon, color)) => agent_mark(Some(icon), Some(color), cx),
        None => xref_mark(None, cx),
    };
    session_xref_with(xref_id(&format!("sg-{key}")), xref, mark, |_, _| {}, cx)
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let big_brother = Some(("bot", "#8b5cf6"));
    v_flex()
        .gap_5()
        .max_w(px(420.))
        .child(captioned(
            "message: another session's line, above its bubble",
            line("message", &xref("message", "Big Brother", Some("a1")), big_brother, cx),
            cx,
        ))
        .child(captioned(
            "report: a child's end report, the whole row",
            line("report", &xref("report", "Batch 2", None), None, cx),
            cx,
        ))
        .child(captioned(
            "started: the agent conversation that started this session",
            line("started", &xref("started", "Big Brother", Some("a1")), big_brother, cx),
            cx,
        ))
        .child(captioned(
            "reported: an ended session's parent",
            line("reported", &xref("reported", "Fix the checkout", None), None, cx),
            cx,
        ))
        .child(captioned(
            "a long name truncates on one line",
            line(
                "long",
                &xref("message", "Migrate every board's shape proxy to the new columns allowlist", None),
                None,
                cx,
            ),
            cx,
        ))
}
