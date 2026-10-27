//! EXP-1248/EXP-1294 — `session-row`: THE session card in every state it
//! has, the REAL [`crate::work_card::session_card`] over
//! `domain::session_tree::session_card`, fed `fixtures/session-card.json`'s
//! own cases (the first case per state): Working with its spinner, Waiting,
//! Queued (no elapsed), Needs you, Usage limit, Ended, Background task
//! (EXP-1374, spinning), Interrupted (EXP-1374). A card is the work card's header row in the glass
//! frame plus a chevron; no device, account or branch.

use gpui::{px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::v_flex;

use domain::rows::SessionRow;
use domain::session_tree::{session_card, SessionCard, SessionState};

use super::fixture_rows;

pub(crate) const ID: &str = "session-row";
pub(crate) const OWNER: &str = "EXP-1248";

/// The fixture's cards, one per state in the fixture's order.
pub(crate) fn cards() -> Vec<SessionCard> {
    let fixture: serde_json::Value =
        serde_json::from_str(fixture_rows::SESSION_CARD_JSON).expect("session-card.json parses");
    let mut seen: Vec<SessionState> = Vec::new();
    let mut cards = Vec::new();
    for case in fixture["cases"].as_array().into_iter().flatten() {
        let input = &case["input"];
        let session: SessionRow = fixture_rows::row(&input["session"]);
        let title = input["title"].as_str().unwrap_or_default();
        let card = session_card(&session, title, fixture_rows::now_ms(&input["now"]));
        if !seen.contains(&card.state) {
            seen.push(card.state);
            cards.push(card);
        }
    }
    cards
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let mut column = v_flex().w(px(520.)).gap_2();
    for card in cards() {
        column = column.child(crate::work_card::session_card(&card, |_, _| {}, cx));
    }
    column
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One card per state, every state the card has.
    #[test]
    fn the_demo_draws_every_state_once() {
        let states: Vec<SessionState> = cards().iter().map(|card| card.state).collect();
        assert_eq!(
            states,
            vec![
                SessionState::Working,
                SessionState::Waiting,
                SessionState::Queued,
                SessionState::NeedsYou,
                SessionState::Blocked,
                SessionState::Ended,
                SessionState::Background,
                SessionState::Interrupted,
            ]
        );
    }
}
