//! EXP-1248 — `session-row`: THE run row in BOTH sizes, the REAL
//! [`run_rows::run_row`] (web `@exp/ui` SessionRow, fixture `list-item.json`)
//! over the product's own pipeline: synced-shape `CodingSession`s through
//! `domain::session_tree` + `visible_session_tree_rows`, the connector through
//! `domain::tree_guides::guides_for`, the facts through
//! `RunListFacts::derive` (no store installed: every read is `try_global`).
//!
//! The SAME runs twice: SMALL on the sidebar's ground (the rail's Running) and
//! BIG on the page's (Agent › Recent, an action's Runs). A standalone live
//! root, a parent with two children (one waiting on you), an ended chat root.
//! Every mark sits at `12 + 14·depth`, no fold chevron, the device glyph
//! trailing in both sizes.

use std::collections::HashSet;

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};

use domain::rows::CodingSession;
use domain::session_tree::{coding_session_facts, session_tree, visible_session_tree_rows};

use crate::run_rows::{self, RunListFacts, RunRowSize};

pub(crate) const ID: &str = "session-row";
pub(crate) const OWNER: &str = "EXP-1248";

/// One synced-shape row, `minutes_ago` old.
#[allow(clippy::too_many_arguments)]
fn run(
    id: &str,
    action_name: &str,
    parent: Option<&str>,
    minutes_ago: i64,
    ended: bool,
    needs_input: bool,
    device: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> CodingSession {
    let started = (now - chrono::Duration::minutes(minutes_ago)).to_rfc3339();
    let ended_at = ended.then(|| (now - chrono::Duration::minutes(minutes_ago / 2)).to_rfc3339());
    serde_json::from_value(serde_json::json!({
        "id": id,
        "action_name": action_name,
        "parent_session_id": parent,
        "status": if ended { "ended" } else { "running" },
        "ended_by": ended.then_some("agent"),
        "needs_input": needs_input,
        "started_at": started,
        "ended_at": ended_at,
        "created_at": started,
        "updated_at": ended_at.clone().unwrap_or_else(|| started.clone()),
        "device_label": device,
        "agent": "claude",
    }))
    .expect("a styleguide run row deserializes")
}

/// The static runs: a live root, a parent over two children (one waiting on
/// you), an ended chat root.
pub(crate) fn rows(now: chrono::DateTime<chrono::Utc>) -> Vec<CodingSession> {
    vec![
        run("sg-live", "ios double button", None, 21 * 60, false, false, "macbook", now),
        run("sg-parent", "Exponential UI completeness pass", None, 120, false, false, "mint", now),
        run("sg-waiting", "Exponential UI round 2", Some("sg-parent"), 5, false, true, "mint", now),
        run("sg-done", "SwiftUI parity", Some("sg-parent"), 60, true, false, "mint", now),
        run("sg-chat", "Chat", None, 20 * 60, true, false, "macbook", now),
    ]
}

/// The runs as `size` rows, on the list's own pipeline.
fn column(size: RunRowSize, prefix: &'static str, cx: &mut App) -> Div {
    let now = chrono::Utc::now();
    let sessions = rows(now);
    let tree = session_tree(
        sessions.iter().collect::<Vec<_>>(),
        |session: &&CodingSession| coding_session_facts(session),
    );
    let flat = visible_session_tree_rows(&tree, &HashSet::new());
    let guides =
        domain::tree_guides::guides_for(&flat.iter().map(|row| row.depth).collect::<Vec<_>>());
    let mut column = div().flex().flex_col().min_w_0();
    for (index, row) in flat.iter().enumerate() {
        let session: &CodingSession = row.node.session();
        let spec = RunListFacts::derive(session, now.timestamp(), cx).row_spec(
            prefix,
            index,
            size,
            guides.get(index).cloned().unwrap_or_default(),
            false,
            Box::new(|_, _, _| {}),
        );
        column = column.child(run_rows::run_row(spec, cx));
    }
    column
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let sidebar = div()
        .w(px(300.))
        .p_2()
        .rounded(px(theme::tokens::radius::LG))
        .bg(theme::tokens::glass::FILL_SECTION.to_hsla())
        .child(column(RunRowSize::Small, "styleguide-session-row-small", cx));
    let page = div()
        .w(px(520.))
        .child(column(RunRowSize::Big, "styleguide-session-row-big", cx));
    div().flex().flex_row().items_start().gap_6().child(sidebar).child(page)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The demo really nests: two roots plus the parent over its two
    /// children — the same data for both sizes.
    #[test]
    fn the_demo_nests_a_parent_over_two_children() {
        let sessions = rows(chrono::Utc::now());
        let tree = session_tree(
            sessions.iter().collect::<Vec<_>>(),
            |session: &&CodingSession| coding_session_facts(session),
        );
        let flat = visible_session_tree_rows(&tree, &HashSet::new());
        let depths: Vec<usize> = flat.iter().map(|row| row.depth).collect();
        assert_eq!(flat.len(), 5, "{depths:?}");
        assert_eq!(depths.iter().filter(|depth| **depth == 1).count(), 2);
    }
}
