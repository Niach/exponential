//! EXP-1248/EXP-1323 — `session-row`: THE session row, the REAL
//! [`crate::run_rows::run_row`] (web `@exp/ui` SessionRow, fixture
//! `list-item.json` small): [tree guides][the agent's mark at 12 + 14·depth]
//! [mono id · title], a bold title = unread. EXP-1654: the same board ×4:
//! a "Running" caption over a live tree (APP-5 needing you, its working child,
//! the idle grandchild), a working root and an ended one, the first row lit.
//! The session CARD is `chat`'s, never this view's.

use gpui::{div, px, App, Div, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::ActiveTheme as _;

use domain::session_tree::SessionState;

pub(crate) const ID: &str = "session-row";
pub(crate) const OWNER: &str = "EXP-1248";

/// One specimen row: depth, identifier, title, state, unread.
pub(crate) struct SpecimenRow {
    pub(crate) depth: usize,
    pub(crate) identifier: Option<&'static str>,
    pub(crate) title: &'static str,
    pub(crate) state: SessionState,
    pub(crate) unread: bool,
}

/// The web island's rows (`ROWS`, their `sessionCard` states).
pub(crate) const ROWS: [SpecimenRow; 5] = [
    SpecimenRow {
        depth: 0,
        identifier: Some("APP-5"),
        title: "Reduce cold start below 800 ms",
        state: SessionState::NeedsYou,
        unread: true,
    },
    SpecimenRow {
        depth: 1,
        identifier: None,
        title: "Profile the launch path",
        state: SessionState::Working,
        unread: false,
    },
    SpecimenRow {
        depth: 2,
        identifier: None,
        title: "Trace the splash screen",
        state: SessionState::Idle,
        unread: true,
    },
    SpecimenRow {
        depth: 0,
        identifier: Some("APP-14"),
        title: "Group board issues by status",
        state: SessionState::Working,
        unread: false,
    },
    SpecimenRow {
        depth: 0,
        identifier: None,
        title: "Pre-cleanup release",
        state: SessionState::Done,
        unread: false,
    },
];

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let theme = cx.theme();
    let (sidebar, muted) = (theme.sidebar, theme.muted_foreground);
    let depths: Vec<usize> = ROWS.iter().map(|row| row.depth).collect();
    let guides = domain::tree_guides::guides_for(&depths);
    let mut column = div()
        .flex()
        .flex_col()
        .w(px(300.))
        .gap_1()
        .p_2()
        .rounded_lg()
        .bg(sidebar)
        .child(div().px_2().py_1().text_xs().text_color(muted).child("Running"));
    for (index, (row, guides)) in ROWS.iter().zip(guides).enumerate() {
        column = column.child(crate::run_rows::run_row(
            crate::run_rows::RunRowSpec {
                id_prefix: "styleguide-session-row",
                index,
                agent: Some(coding::CodingAgent::Claude),
                state: row.state,
                identifier: row.identifier.map(SharedString::from),
                title: SharedString::from(row.title),
                active: index == 0,
                on_open: Box::new(|_, _, _| {}),
                trailing: None,
                guides,
                unread: row.unread,
            },
            cx,
        ));
    }
    column
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The web island's tree: a root, its child, its grandchild, two roots.
    #[test]
    fn the_specimen_is_the_web_islands_tree() {
        let depths: Vec<usize> = ROWS.iter().map(|row| row.depth).collect();
        assert_eq!(depths, vec![0, 1, 2, 0, 0]);
        let states: Vec<SessionState> = ROWS.iter().map(|row| row.state).collect();
        assert_eq!(
            states,
            vec![
                SessionState::NeedsYou,
                SessionState::Working,
                SessionState::Idle,
                SessionState::Working,
                SessionState::Done,
            ]
        );
    }
}
