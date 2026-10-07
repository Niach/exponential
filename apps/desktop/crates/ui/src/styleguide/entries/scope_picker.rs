//! FEED-76: `scope-picker` (2 General components): the team/board scope
//! picker the Create-API-key dialog shows (the MCP OAuth consent screen's
//! twin on the web) — the REAL `scope_picker::ScopePicker` over the web
//! specimen's demo tree with "Everything" already off, so the tree shows:
//! Acme ticked whole (its Web and Mobile read checked and disabled), Lab's
//! Research ticked on its own. Kept alive by `window.use_keyed_state`;
//! every tick is live.

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};

use crate::scope_picker::{ScopeBoard, ScopePicker, ScopeSelection, ScopeTeam};

pub(crate) const ID: &str = "scope-picker";
pub(crate) const OWNER: &str = "FEED-76";

/// The web entry's tree: two teams, three boards.
fn tree() -> Vec<ScopeTeam> {
    vec![
        ScopeTeam {
            id: "t-acme".into(),
            name: "Acme".into(),
            boards: vec![
                ScopeBoard { id: "b-web".into(), name: "Web".into(), prefix: "WEB".into() },
                ScopeBoard { id: "b-mob".into(), name: "Mobile".into(), prefix: "MOB".into() },
            ],
        },
        ScopeTeam {
            id: "t-lab".into(),
            name: "Lab".into(),
            boards: vec![ScopeBoard {
                id: "b-rnd".into(),
                name: "Research".into(),
                prefix: "RND".into(),
            }],
        },
    ]
}

/// The web entry's pick: Acme whole, Research alone.
fn value() -> ScopeSelection {
    ScopeSelection {
        all_teams: false,
        team_ids: vec!["t-acme".into()],
        board_ids: vec!["b-rnd".into()],
    }
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let demo = window.use_keyed_state("sg-scope-picker", cx, |_, _| {
        ScopePicker::new("sg-scope", tree(), value())
    });
    // The web island's `w-[24rem]`.
    div().w(px(384.)).child(demo)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scope_picker::effective_scope_selection;

    /// The demo's pick sends Acme whole and Research alone — nothing Acme
    /// already covers.
    #[test]
    fn the_demo_pick_is_acme_whole_plus_research() {
        let sent = effective_scope_selection(&tree(), &value());
        assert_eq!(sent, value());
        assert!(sent.has_selection());
    }
}
