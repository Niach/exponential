//! EXP-1248/EXP-1294 — `session-row`: THE session card in every state it
//! has, the REAL [`crate::work_card::session_card`] over
//! `domain::session_tree::session_card`, fed `fixtures/session-card.json`'s
//! own cases (the first case per state): Working with its spinner, Waiting,
//! Queued (no elapsed), Needs you, Usage limit, Ended; EXP-1323: then the
//! fixture's captioned cards (a live caption, an ended aside's answer) with
//! the muted second line. A card is the work card's header row in the glass
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

/// EXP-1323: the fixture's cards that carry a caption line.
pub(crate) fn captioned_cards() -> Vec<SessionCard> {
    let fixture: serde_json::Value =
        serde_json::from_str(fixture_rows::SESSION_CARD_JSON).expect("session-card.json parses");
    fixture["cases"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|case| {
            let input = &case["input"];
            let session: SessionRow = fixture_rows::row(&input["session"]);
            let title = input["title"].as_str().unwrap_or_default();
            session_card(&session, title, fixture_rows::now_ms(&input["now"]))
        })
        .filter(|card| card.caption.is_some())
        .collect()
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let mut column = v_flex().w(px(520.)).gap_2();
    for card in cards().into_iter().chain(captioned_cards()) {
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
            ]
        );
    }

    /// EXP-1323: a live caption and an ended aside's answer both draw.
    #[test]
    fn the_demo_draws_the_captioned_cards() {
        let states: Vec<SessionState> = captioned_cards().iter().map(|card| card.state).collect();
        assert_eq!(states, vec![SessionState::Working, SessionState::Ended]);
    }
}
