//! EXP-1175 → EXP-1294: `run-status-row` (Special components): the session
//! view's status row, the IDE half of the styleguide entry the web page
//! draws — the REAL row ([`crate::steer_viewer::session_status_row`]): the
//! agent's mark for the state beside the session card's state word in its
//! tone (`session-card.json`), in its everyday states. No tool line and no
//! Show work toggle (the session view always shows the work).

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};

use domain::session_tree::SessionState;

pub(crate) const ID: &str = "run-status-row";
pub(crate) const OWNER: &str = "EXP-1175";

/// The demo rows' states (S-3: the four display states), in the web
/// entry's order.
const STATES: [SessionState; 4] = [SessionState::Working, SessionState::NeedsYou, SessionState::Idle, SessionState::Done];

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let rows = STATES.iter().enumerate().map(|(index, state)| {
        crate::steer_viewer::session_status_row(
            ("styleguide-run-status-row", index),
            Some(coding::CodingAgent::default()),
            *state,
            state.word(),
            cx,
        )
    });
    div().flex().flex_col().gap_3().pt_2().w(px(560.)).children(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every state reads a distinct word off the shared session-card copy.
    #[test]
    fn every_state_has_its_own_word() {
        let words: std::collections::BTreeSet<&str> = STATES.iter().map(|state| state.word()).collect();
        assert_eq!(words.len(), STATES.len());
    }
}
