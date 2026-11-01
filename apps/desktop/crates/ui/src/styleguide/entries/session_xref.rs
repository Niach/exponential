//! S-3: `session-xref` (2 General components): THE inter-session link
//! ([`crate::session_xref`]), case for case with the web entry: the four
//! kinds (`Message from` · `Report from` · `Started by` · `Reported to`),
//! two through an agent's avatar, two through the session glyph (#EXP-1553:
//! web's four compact rows, no captions). No `sync::Store` here, so the demo agents'
//! marks are drawn from their icon + colour directly.

use gpui::{px, AnyElement, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::v_flex;

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

fn line(key: &str, xref: &SessionXref, agent: Option<(&str, &str)>, cx: &App) -> AnyElement {
    let mark = match agent {
        Some((icon, color)) => agent_mark(Some(icon), Some(color), cx),
        None => xref_mark(None, cx),
    };
    session_xref_with(xref_id(&format!("sg-{key}")), xref, mark, |_, _| {}, cx)
}

/// #EXP-1553: web's `SessionXrefSpecimen` row for row (`w-[22rem]`,
/// `gap-3`), on the bare page: no captions, no card around it.
pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let agent = Some(("bot", "#8b5cf6"));
    v_flex()
        .w(px(352.))
        .items_start()
        .gap_3()
        .child(line("message", &xref("message", "Triage", Some("sg-agent")), agent, cx))
        .child(line("report", &xref("report", "Fix the sign-in redirect", None), None, cx))
        .child(line("started", &xref("started", "Triage", Some("sg-agent")), agent, cx))
        .child(line("reported", &xref("reported", "Ship the login flow", None), None, cx))
}
