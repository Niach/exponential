//! EXP-1029 contract — `session-tree` (3 Special components). EXP-996 fills
//! this entry's demo and never edits `entries/mod.rs` or the section index.
//!
//! EXP-1049 draws the DESKTOP half of the rule: every session list (the rail's
//! Running section, the Recent panel) flattens
//! [`domain::session_tree::session_tree`] and paints the EXP-965 connector over
//! the depths. The tree is parent nesting (a run started by another run sits
//! under it) and resume-collapse (a resume succession is ONE row).
//!
//! EXP-1092: the demo is the PRODUCT's pipeline end to end, over static rows
//! (never live ones): synced-shape `CodingSession`s go through
//! `domain::session_tree::session_tree` + `visible_session_tree_rows`, the
//! connector through `domain::tree_guides::guides_for`, and every row is drawn
//! by `run_rows::render_run_list_row` over `RunListFacts::derive` — the Recent
//! panel's and an action's Runs' exact calls. The runs are action runs so
//! their titles need no synced issue.
//!
//! EXP-1208: so the demo shows the ×4 row layout as the product draws it —
//! every row leads with the run mark (`coding_selects::run_lead`, the ended
//! rows' dimmed `ended_run_lead`) at the base inset, the parent's fold
//! chevron AFTER its mark, and the child's elbow ending at the child's mark.

use std::collections::HashSet;

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};

use domain::rows::CodingSession;
use domain::session_tree::{coding_session_facts, session_tree, visible_session_tree_rows};

use crate::run_rows::{self, RunListFacts, RunRowFold};

pub(crate) const ID: &str = "session-tree";
pub(crate) const OWNER: &str = "EXP-996";

const ID_PREFIX: &str = "styleguide-session-tree";

/// One synced-shape row, `minutes_ago` old; `ended` = a past run.
fn run(
    id: &str,
    action_name: &str,
    parent: Option<&str>,
    resumed_from: Option<&str>,
    minutes_ago: i64,
    ended: bool,
    now: chrono::DateTime<chrono::Utc>,
) -> CodingSession {
    let started = (now - chrono::Duration::minutes(minutes_ago)).to_rfc3339();
    let ended_at = ended.then(|| (now - chrono::Duration::minutes(minutes_ago / 2)).to_rfc3339());
    serde_json::from_value(serde_json::json!({
        "id": id,
        "action_name": action_name,
        "parent_session_id": parent,
        "resumed_from_id": resumed_from,
        "status": if ended { "ended" } else { "running" },
        "ended_by": ended.then_some("agent"),
        "started_at": started,
        "ended_at": ended_at,
        "created_at": started,
        "updated_at": ended_at.clone().unwrap_or_else(|| started.clone()),
        "device_label": "Studio Mac",
        "agent": "claude",
    }))
    .expect("a styleguide run row deserializes")
}

/// The static tree: a resumed run (two rows, ONE line), and a parent run with
/// the two runs it started (`sessions_start`) — one still running, one past.
fn rows(now: chrono::DateTime<chrono::Utc>) -> Vec<CodingSession> {
    vec![
        run("sg-first", "Checkout rework", None, None, 50, true, now),
        run("sg-resumed", "Checkout rework", None, Some("sg-first"), 20, false, now),
        run("sg-parent", "Release train", None, None, 40, false, now),
        run("sg-child-live", "Doc sweep", Some("sg-parent"), None, 12, false, now),
        run("sg-child-past", "Changelog entry", Some("sg-parent"), None, 30, true, now),
    ]
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let now = chrono::Utc::now();
    let now_epoch = now.timestamp();
    let sessions = rows(now);
    let tree = session_tree(
        sessions.iter().collect::<Vec<_>>(),
        |session: &&CodingSession| coding_session_facts(session),
    );
    let flat = visible_session_tree_rows(&tree, &HashSet::new());
    // EXP-965: the connector comes off the VISIBLE depth sequence, exactly as
    // the real lists derive it.
    let guides =
        domain::tree_guides::guides_for(&flat.iter().map(|row| row.depth).collect::<Vec<_>>());
    let mut column = div().flex().flex_col().w(px(360.)).min_w_0();
    for (index, row) in flat.iter().enumerate() {
        let guides = guides.get(index).cloned().unwrap_or_default();
        let fold = row.has_children.then(|| RunRowFold {
            collapsed: false,
            on_toggle: Box::new(|_, _, _| {}),
        });
        let session: &CodingSession = row.node.session();
        column = column.child(run_rows::render_run_list_row(
            ID_PREFIX,
            index,
            guides,
            fold,
            RunListFacts::derive(session, now_epoch, cx),
            false,
            Box::new(|_, _, _| {}),
            cx,
        ));
    }
    column
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The demo's tree really nests: the resume succession folds into ONE
    /// row, and the parent run sits over the two it started.
    #[test]
    fn the_demo_tree_collapses_a_resume_and_nests_a_parent_run() {
        let sessions = rows(chrono::Utc::now());
        let tree = session_tree(
            sessions.iter().collect::<Vec<_>>(),
            |session: &&CodingSession| coding_session_facts(session),
        );
        let flat = visible_session_tree_rows(&tree, &HashSet::new());
        let depths: Vec<usize> = flat.iter().map(|row| row.depth).collect();
        assert_eq!(flat.len(), 4, "{depths:?}");
        assert_eq!(depths.iter().filter(|depth| **depth == 1).count(), 2);
    }
}
