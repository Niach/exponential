//! #EXP-1561 / #EXP-1562: WAITING question and plan cards ×4
//! (`packages/domain-contract/src/waiting-cards.ts`'s twin, iOS ExpCore
//! `WaitingCards.swift`, Android `domain/WaitingCards.kt`): a card's state
//! (waiting / answered / dismissed / superseded) and the chat header's
//! waiting chips. Byte-locked by `fixtures/waiting-cards.json`; pure.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ask_dock::{ask_dock_label, ask_dock_title, AskDockKind};

const WAITING_CARDS_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/waiting-cards.json");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitingCardsCopy {
    pub dismiss: String,
    pub dismissed: String,
    pub waiting: String,
    pub waiting_count: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WaitingCardsIcons {
    pub plan: String,
    pub question: String,
    pub dismiss: String,
    pub waiting: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitingCardsConstants {
    pub label_max: usize,
    pub ellipsis: String,
    pub highlight_ms: u64,
    pub deep_link_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WaitingCardsTestIds {
    pub dismiss: String,
    pub dismissed: String,
    pub chip: String,
    pub button: String,
    pub menu: String,
}

/// `fixtures/waiting-cards.json` minus its cases.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitingCardsSpec {
    pub copy: WaitingCardsCopy,
    pub icons: WaitingCardsIcons,
    pub constants: WaitingCardsConstants,
    pub test_ids: WaitingCardsTestIds,
}

pub static WAITING_CARDS: LazyLock<WaitingCardsSpec> =
    LazyLock::new(|| serde_json::from_str(WAITING_CARDS_JSON).expect("waiting-cards.json parses"));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardState {
    Waiting,
    Answered,
    Dismissed,
    Superseded,
}

/// A question/plan card's state; `None` = not a question or plan.
pub fn card_state(card: &Value, answered_at: Option<&str>) -> Option<CardState> {
    let kind = card.get("kind").and_then(Value::as_str)?;
    if kind != "plan" && kind != "question" {
        return None;
    }
    let decision = card.get("decision").and_then(Value::as_str);
    if card.get("dismissed").and_then(Value::as_bool) == Some(true) || decision == Some("dismissed") {
        return Some(CardState::Dismissed);
    }
    if kind == "plan" {
        return Some(match decision {
            Some("superseded") => CardState::Superseded,
            Some(_) => CardState::Answered,
            None => CardState::Waiting,
        });
    }
    let answered = answered_at.is_some() || card.get("answer").is_some_and(Value::is_string);
    Some(if answered { CardState::Answered } else { CardState::Waiting })
}

/// One waiting card (a chat's `chat_pending_asks`, the session view's
/// waiting `session_cards`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitingCardInput {
    pub id: String,
    pub kind: AskDockKind,
    /// The plan, or the question's text (markdown).
    pub text: String,
    /// The asking session (#EXP-1785: `waiting_target`'s input).
    #[serde(default)]
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitingChip {
    pub id: String,
    pub kind: AskDockKind,
    /// The kind's concept icon.
    pub icon: String,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitingChipsLayout {
    Hidden,
    Chips,
    Button,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitingChips {
    pub layout: WaitingChipsLayout,
    pub count: usize,
    pub chips: Vec<WaitingChip>,
}

/// The kind's concept icon (`icons[kind]`).
pub fn waiting_icon(kind: AskDockKind) -> &'static str {
    match kind {
        AskDockKind::Plan => &WAITING_CARDS.icons.plan,
        AskDockKind::Question => &WAITING_CARDS.icons.question,
    }
}

/// A title clipped to `labelMax` characters (code points).
pub fn clip_waiting_label(title: &str) -> String {
    let max = WAITING_CARDS.constants.label_max;
    if title.chars().count() <= max {
        return title.to_string();
    }
    let head: String = title.chars().take(max.saturating_sub(1)).collect();
    format!("{}{}", head.trim_end(), WAITING_CARDS.constants.ellipsis)
}

/// The header's waiting chips (Conversation order kept): hidden when none
/// waits, ONE button on phones, else one chip per card.
pub fn waiting_chips(cards: &[WaitingCardInput], phone: bool) -> WaitingChips {
    let chips: Vec<WaitingChip> = cards
        .iter()
        .map(|card| {
            let label = clip_waiting_label(&ask_dock_title(&card.text));
            WaitingChip {
                id: card.id.clone(),
                kind: card.kind,
                icon: waiting_icon(card.kind).to_string(),
                label: if label.is_empty() { ask_dock_label(card.kind).to_string() } else { label },
            }
        })
        .collect();
    let layout = match (chips.is_empty(), phone) {
        (true, _) => WaitingChipsLayout::Hidden,
        (false, true) => WaitingChipsLayout::Button,
        (false, false) => WaitingChipsLayout::Chips,
    };
    WaitingChips { layout, count: chips.len(), chips }
}

/// What a waiting chip's pick opens (#EXP-1785 `waitingTarget`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitingOpen {
    /// THE plan page as the screen's opened item (any session's plan).
    Plan,
    /// The root's question: the Chat tab scrolled to its card + the ring.
    Card,
    /// A question under the root: that session opened in place, at its card.
    Session,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitingTarget {
    pub open: WaitingOpen,
    pub session_id: String,
}

/// A waiting chip's pick (`waitingTarget(card, rootId)`): a plan opens the
/// plan page; the root's question scrolls the Chat tab to its card; a
/// question under the root opens that session at its card.
pub fn waiting_target(kind: AskDockKind, session_id: &str, root_id: &str) -> WaitingTarget {
    let open = match kind {
        AskDockKind::Plan => WaitingOpen::Plan,
        AskDockKind::Question if session_id == root_id => WaitingOpen::Card,
        AskDockKind::Question => WaitingOpen::Session,
    };
    WaitingTarget { open, session_id: session_id.to_string() }
}

/// waiting-cards.json DEEP LINKS: a pending jump into a message's next step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeepLinkStep {
    /// Found, laid out and centred: ring it, the link settles.
    Ring,
    /// Found and laid out: scroll it to the centre.
    Scroll,
    /// A layout or a page in flight: run again.
    Wait,
    /// Not loaded, older rows exist: page ONE older page in.
    Page,
    /// Not loaded and nothing older (or past `deepLinkMs`): settle.
    Missing,
}

/// What a pending deep link knows this frame (waiting-cards.json DEEP LINKS).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeepLinkInput {
    pub found: bool,
    pub laid_out: bool,
    pub centred: bool,
    pub paging: bool,
    pub more: bool,
    pub elapsed_ms: u64,
}

/// The deep link's next step (`deepLinkStep`).
pub fn deep_link_step(input: DeepLinkInput) -> DeepLinkStep {
    if input.elapsed_ms >= WAITING_CARDS.constants.deep_link_ms {
        return if input.found { DeepLinkStep::Ring } else { DeepLinkStep::Missing };
    }
    if input.found {
        return if !input.laid_out {
            DeepLinkStep::Wait
        } else if input.centred {
            DeepLinkStep::Ring
        } else {
            DeepLinkStep::Scroll
        };
    }
    if input.paging {
        DeepLinkStep::Wait
    } else if input.more {
        DeepLinkStep::Page
    } else {
        DeepLinkStep::Missing
    }
}

/// The phone button's accessible name.
pub fn waiting_count_label(count: usize) -> String {
    WAITING_CARDS.copy.waiting_count.replace("{count}", &count.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cases(key: &str) -> Vec<Value> {
        let fixture: Value = serde_json::from_str(WAITING_CARDS_JSON).unwrap();
        fixture[key].as_array().unwrap().clone()
    }

    #[test]
    fn spec_parses() {
        assert_eq!(WAITING_CARDS.copy.dismiss, "Dismiss");
        assert_eq!(WAITING_CARDS.icons.dismiss, "ui-close");
        assert_eq!(WAITING_CARDS.constants.highlight_ms, 1600);
    }

    #[test]
    fn state_cases() {
        for case in cases("stateCases") {
            let got = card_state(&case["card"], case["answeredAt"].as_str());
            assert_eq!(serde_json::to_value(got).unwrap(), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn chip_cases() {
        for case in cases("chipCases") {
            let cards: Vec<WaitingCardInput> = serde_json::from_value(case["cards"].clone()).unwrap();
            let got = waiting_chips(&cards, case["phone"].as_bool().unwrap());
            assert_eq!(serde_json::to_value(got).unwrap(), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn target_cases() {
        for case in cases("targetCases") {
            let card = &case["card"];
            let kind: AskDockKind = serde_json::from_value(card["kind"].clone()).unwrap();
            let got = waiting_target(kind, card["sessionId"].as_str().unwrap(), case["rootId"].as_str().unwrap());
            assert_eq!(serde_json::to_value(got).unwrap(), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn deep_link_cases() {
        for case in cases("deepLinkCases") {
            let input: DeepLinkInput = serde_json::from_value(case["input"].clone()).unwrap();
            assert_eq!(serde_json::to_value(deep_link_step(input)).unwrap(), case["expected"], "{}", case["name"]);
        }
    }
}
