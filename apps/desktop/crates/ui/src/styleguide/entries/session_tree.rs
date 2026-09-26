//! EXP-1029 contract — `session-tree` (3 Special components). EXP-996 fills
//! this entry's demo and never edits `entries/mod.rs` or the section index.
//!
//! EXP-1049 draws the DESKTOP half of the rule: every session list (the rail's
//! Running section, the Recent panel) flattens
//! [`domain::session_tree::session_tree`] and paints the EXP-965 connector over
//! the depths. A GROUP row — a workflow, or a PR stack — leads with its concept
//! icon and folds behind the same chevron a parent run wears; it is NOT a
//! session, so it carries no state dot, no device glyph and no Stop.
//!
//! EXP-1092: the demo is the PRODUCT's pipeline end to end, over static rows
//! (never live ones): synced-shape `CodingSession`s go through
//! `domain::session_tree::session_tree` + `visible_session_tree_rows`, the
//! connector through `domain::tree_guides::guides_for`, and every row is drawn
//! by `run_rows` itself (`render_group_row`, `render_run_list_row` over
//! `RunListFacts::derive`) — the Recent panel's and the Automations log's
//! exact calls. The runs are action runs so their titles need no synced issue.

use std::collections::HashSet;

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};

use domain::rows::CodingSession;
use domain::session_tree::{
    coding_session_facts, session_tree, visible_session_tree_rows, SessionTreeContext,
    SessionTreeNode, WorkflowFacts,
};

use crate::run_rows::{self, GroupRowSpec, RunListFacts, RunRowFold, SessionGroupFacts};

pub(crate) const ID: &str = "session-tree";
pub(crate) const OWNER: &str = "EXP-996";

const ID_PREFIX: &str = "styleguide-session-tree";

/// One synced-shape row, `minutes_ago` old; `ended` = a past run.
fn run(
    id: &str,
    action_name: &str,
    parent: Option<&str>,
    workflow: Option<&str>,
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
        "workflow_id": workflow,
        "workflow_role": workflow.map(|_| "author"),
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

/// The static tree: a workflow's two node runs, and a parent run with the
/// two runs it started (`sessions_start`) — one still running, one past.
fn rows(now: chrono::DateTime<chrono::Utc>) -> Vec<CodingSession> {
    vec![
        run("sg-wf-1", "Checkout: fixture table", None, Some("sg-wf"), 20, false, now),
        run("sg-wf-2", "Checkout: sessionTree ×4", None, Some("sg-wf"), 30, true, now),
        run("sg-parent", "Release train", None, None, 40, false, now),
        run("sg-child-live", "Doc sweep", Some("sg-parent"), None, 12, false, now),
        run("sg-child-past", "Changelog entry", Some("sg-parent"), None, 30, true, now),
    ]
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let now = chrono::Utc::now();
    let now_epoch = now.timestamp();
    let sessions = rows(now);
    let workflows = [WorkflowFacts {
        id: "sg-wf".to_string(),
        name: "Checkout rework".to_string(),
        status: "running".to_string(),
    }];
    let context = SessionTreeContext {
        workflows: &workflows,
        workflow_nodes: &[],
        issues: &[],
    };
    let tree = session_tree(
        sessions.iter().collect::<Vec<_>>(),
        |session: &&CodingSession| coding_session_facts(session),
        &context,
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
        column = column.child(match row.node {
            SessionTreeNode::Session(node) => {
                let session: &CodingSession = node.session();
                run_rows::render_run_list_row(
                    ID_PREFIX,
                    index,
                    guides,
                    fold,
                    RunListFacts::derive(session, now_epoch, cx),
                    false,
                    Box::new(|_, _, _| {}),
                    cx,
                )
            }
            group => run_rows::render_group_row(
                GroupRowSpec {
                    id_prefix: ID_PREFIX,
                    index,
                    guides,
                    fold,
                    facts: SessionGroupFacts::from_node(group)
                        .expect("a node that is not a session is a group"),
                    on_open: None,
                },
                cx,
            ),
        });
    }
    column
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The demo's tree really nests: a workflow group over its two runs, and
    /// the parent run over the two it started.
    #[test]
    fn the_demo_tree_nests_a_workflow_and_a_parent_run() {
        let sessions = rows(chrono::Utc::now());
        let workflows = [WorkflowFacts {
            id: "sg-wf".to_string(),
            name: "Checkout rework".to_string(),
            status: "running".to_string(),
        }];
        let context = SessionTreeContext {
            workflows: &workflows,
            workflow_nodes: &[],
            issues: &[],
        };
        let tree = session_tree(
            sessions.iter().collect::<Vec<_>>(),
            |session: &&CodingSession| coding_session_facts(session),
            &context,
        );
        let flat = visible_session_tree_rows(&tree, &HashSet::new());
        let depths: Vec<usize> = flat.iter().map(|row| row.depth).collect();
        assert_eq!(flat.len(), 6, "{depths:?}");
        assert_eq!(flat.iter().filter(|row| row.node.is_group()).count(), 1);
        assert_eq!(depths.iter().filter(|depth| **depth == 1).count(), 4);
    }
}
