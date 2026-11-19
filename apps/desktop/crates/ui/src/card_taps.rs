//! #EXP-1958 (`packages/domain-contract/fixtures/sealed-content.json`
//! `cardStamps`): a tap on a replies / suggestions card's item. The STORED
//! card picks the wire (`enet_core::sealed::card_item_wire`): an unsealed
//! card keeps the server actions (the caller's existing call); a SEALED card
//! this install opened sends the opened item's text through THE composer
//! sealing writer (`sealing::plan_send` + `send_blocking`, `cardItem`
//! beside it) or names the opened ids / text on the stamp actions; a sealed
//! card this install cannot open is refused locally (nothing leaves).
//!
//! The rows the store holds are already opened (`sync::sealed`), the clear
//! `stamps` laid over their items, so `row.card` is the card as read.

use api::sessions::{suggestion_request, SendInput};
use domain::rows::{MessageRow, SealState};
use enet_core::sealed::{card_item_wire, CardItemAction, CardItemWire};
use gpui::App;
use serde_json::Value;

/// One write off the UI thread (BLOCKING); the callers' toasts read its error.
pub(crate) type Call = Box<dyn FnOnce(&api::TrpcClient) -> Result<serde::de::IgnoredAny, api::ApiError> + Send + 'static>;

/// What the tap does.
pub(crate) enum Tap {
    /// An unsealed card (or a row the store lacks): the caller's server action.
    Server,
    /// A sealed card's write.
    Call(Call),
    /// A sealed card this install cannot open (or whose item is gone).
    Refused,
}

/// Start's picks (the Options menu's device / model).
#[derive(Clone, Debug, Default)]
pub(crate) struct Picks {
    pub device_id: Option<String>,
    pub model: Option<String>,
}

/// The store's row of `message_id` (sealed or not, opened or not).
fn wire_of(row: Option<&MessageRow>, action: CardItemAction) -> CardItemWire {
    let Some(row) = row else { return CardItemWire::Server };
    let sealed = row.seal.is_some() && row.card.is_some();
    card_item_wire(sealed, row.seal == Some(SealState::Opened), action).0
}

fn items(card: &Value) -> impl Iterator<Item = &serde_json::Map<String, Value>> {
    card.get("items").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_object)
}

fn str_of<'a>(item: &'a serde_json::Map<String, Value>, key: &str) -> &'a str {
    item.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn flag(item: &serde_json::Map<String, Value>, key: &str) -> bool {
    item.get(key) == Some(&Value::Bool(true))
}

fn item<'a>(card: &'a Value, item_id: &str) -> Option<&'a serde_json::Map<String, Value>> {
    items(card).find(|item| str_of(item, "id") == item_id)
}

/// A suggestion still open (web `suggestionState` = `open`).
fn suggestion_open(item: &serde_json::Map<String, Value>) -> bool {
    !item.contains_key("sessionId") && !flag(item, "here") && !item.contains_key("issueId") && !flag(item, "dismissed")
}

/// The item's issue ids `sessions.send` takes (uuids; anything else drops).
fn issue_ids(item: &serde_json::Map<String, Value>) -> Vec<String> {
    let ids = item.get("issueIds").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str);
    ids.filter(|id| uuid::Uuid::parse_str(id).is_ok()).map(str::to_string).collect()
}

fn start_send(card_session: &str, message_id: &str, item: &serde_json::Map<String, Value>, picks: &Picks) -> SendInput {
    let item_id = str_of(item, "id");
    SendInput {
        device_id: picks.device_id.clone(),
        model: picks.model.clone(),
        ..SendInput::card_start(card_session, message_id, item_id, str_of(item, "title"), str_of(item, "text"), issue_ids(item))
    }
}

/// The sealed sends of a tap on an OPENED card (`cardStamps` WIRE): reply,
/// Do it here, Start, Start all (one per open item, card order). Empty =
/// the item is gone / handled.
pub(crate) fn card_sends(
    card: &Value,
    card_session: &str,
    message_id: &str,
    action: CardItemAction,
    item_id: &str,
    picks: &Picks,
) -> Vec<SendInput> {
    match action {
        CardItemAction::Reply => item(card, item_id)
            .filter(|item| !flag(item, "sent"))
            .map(|item| SendInput::card_message(card_session, message_id, item_id, str_of(item, "text")))
            .into_iter()
            .collect(),
        CardItemAction::Here => item(card, item_id)
            .filter(|item| suggestion_open(item))
            .map(|item| {
                let text = suggestion_request(str_of(item, "title"), str_of(item, "text"));
                SendInput::card_message(card_session, message_id, item_id, &text)
            })
            .into_iter()
            .collect(),
        CardItemAction::Start => item(card, item_id)
            .filter(|item| suggestion_open(item))
            .map(|item| start_send(card_session, message_id, item, picks))
            .into_iter()
            .collect(),
        CardItemAction::StartAll => {
            items(card).filter(|item| suggestion_open(item)).map(|item| start_send(card_session, message_id, item, picks)).collect()
        }
        CardItemAction::Dismiss | CardItemAction::DismissReplies | CardItemAction::File => Vec::new(),
    }
}

/// Every open reply of an opened replies card (`dismissReplies` `itemIds`).
pub(crate) fn open_reply_ids(card: &Value) -> Vec<String> {
    items(card).filter(|item| !flag(item, "sent")).map(|item| str_of(item, "id").to_string()).filter(|id| !id.is_empty()).collect()
}

/// The tap on `message_id`'s item `item_id` (`None` for Start all / the
/// replies' ×), read from the store.
pub(crate) fn tap(message_id: &str, item_id: Option<&str>, action: CardItemAction, picks: Picks, cx: &App) -> Tap {
    let row = sync::Store::try_global(cx).and_then(|store| store.collections().messages.read(cx).get(message_id).cloned());
    match wire_of(row.as_ref(), action) {
        CardItemWire::Server => Tap::Server,
        CardItemWire::Refused => Tap::Refused,
        CardItemWire::Stamp => {
            let (Some(row), message_id) = (row, message_id.to_string()) else { return Tap::Refused };
            let card = row.card.unwrap_or_default();
            let item_id = item_id.unwrap_or_default().to_string();
            match action {
                CardItemAction::Dismiss => {
                    Tap::Call(Box::new(move |trpc| api::sessions::dismiss_suggestion(trpc, &message_id, &item_id)))
                }
                CardItemAction::DismissReplies => {
                    let ids = open_reply_ids(&card);
                    if ids.is_empty() {
                        return Tap::Refused;
                    }
                    Tap::Call(Box::new(move |trpc| api::sessions::dismiss_replies(trpc, &message_id, Some(&ids))))
                }
                _ => {
                    let Some(item) = item(&card, &item_id) else { return Tap::Refused };
                    let (title, text) = (str_of(item, "title").to_string(), str_of(item, "text").to_string());
                    Tap::Call(Box::new(move |trpc| api::sessions::file_suggestion(trpc, &message_id, &item_id, Some((&title, &text)))))
                }
            }
        }
        CardItemWire::Sealed => {
            let Some(row) = row else { return Tap::Refused };
            let card = row.card.unwrap_or_default();
            let sends = card_sends(&card, &row.session_id, message_id, action, item_id.unwrap_or_default(), &picks);
            if sends.is_empty() {
                return Tap::Refused;
            }
            // The plans read the store on the foreground; the sends run in order.
            let planned: Vec<_> = sends.into_iter().map(|input| (crate::sealing::plan_send(&input, cx), input)).collect();
            Tap::Call(Box::new(move |trpc| {
                for (plan, input) in &planned {
                    crate::sealing::send_blocking(trpc, input, plan)?;
                }
                Ok(serde::de::IgnoredAny)
            }))
        }
    }
}

/// What a refused tap's toast says.
pub(crate) const REFUSED: &str = crate::sealing::LOCKED_ANSWER;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/sealed-content.json")).unwrap()
    }

    fn row(seal: Option<SealState>) -> MessageRow {
        MessageRow { id: "m-1".into(), session_id: "s-1".into(), card: Some(json!({"kind": "replies"})), seal, ..MessageRow::default() }
    }

    /// `cardStamps.tapCases` through the store's rows: `stored` sealed =
    /// the row carries a seal state, `opened` = it is `Opened`.
    #[test]
    fn replays_card_stamp_taps() {
        let fx = fixture();
        for case in fx["cardStamps"]["tapCases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let seal = match (case["stored"] == "sealed", case["opened"] == true) {
                (false, _) => None,
                (true, true) => Some(SealState::Opened),
                (true, false) => Some(SealState::Locked),
            };
            let action = CardItemAction::parse(case["action"].as_str().unwrap()).unwrap();
            assert_eq!(wire_of(Some(&row(seal)), action).as_str(), case["wire"].as_str().unwrap(), "{name}");
        }
        assert_eq!(wire_of(None, CardItemAction::Reply), CardItemWire::Server);
    }

    #[test]
    fn sealed_sends_carry_the_opened_text_and_the_item() {
        let replies = json!({"kind": "replies", "items": [{"id": "r1", "text": "Yes"}, {"id": "r2", "text": "No", "sent": true}]});
        let sends = card_sends(&replies, "s-1", "m-1", CardItemAction::Reply, "r1", &Picks::default());
        assert_eq!(sends, vec![SendInput::card_message("s-1", "m-1", "r1", "Yes")]);
        assert!(card_sends(&replies, "s-1", "m-1", CardItemAction::Reply, "r2", &Picks::default()).is_empty());
        assert_eq!(open_reply_ids(&replies), vec!["r1".to_string()]);

        let issue = "00000000-0000-4000-8000-000000000012";
        let suggestions = json!({"kind": "suggestions", "items": [
            {"id": "s1", "title": "Add tests", "text": "Cover it", "issueIds": [issue, "EXP-9"]},
            {"id": "s2", "title": "Done", "text": "x", "here": true},
            {"id": "s3", "title": "Docs", "text": "Write them"},
        ]});
        let here = card_sends(&suggestions, "s-1", "m-1", CardItemAction::Here, "s1", &Picks::default());
        assert_eq!(here, vec![SendInput::card_message("s-1", "m-1", "s1", "Add tests\n\nCover it")]);
        let picks = Picks { device_id: Some("dev-1".into()), model: Some("opus".into()) };
        let start = card_sends(&suggestions, "s-1", "m-1", CardItemAction::Start, "s1", &picks);
        assert_eq!(start.len(), 1);
        assert_eq!(start[0].parent_id.as_deref(), Some("s-1"));
        assert_eq!(start[0].text.as_deref(), Some("Add tests\n\nCover it"));
        assert_eq!(start[0].name.as_deref(), Some("Add tests"));
        assert_eq!(start[0].issue_ids, Some(vec![issue.to_string()]));
        assert_eq!((start[0].device_id.as_deref(), start[0].model.as_deref()), (Some("dev-1"), Some("opus")));
        assert!(card_sends(&suggestions, "s-1", "m-1", CardItemAction::Start, "s2", &picks).is_empty());
        let all = card_sends(&suggestions, "s-1", "m-1", CardItemAction::StartAll, "", &Picks::default());
        let ids: Vec<_> = all.iter().map(|send| send.card_item.as_ref().unwrap().item_id.as_str()).collect();
        assert_eq!(ids, ["s1", "s3"]);
    }
}
