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

/// An `#IDENT` of the card's team as the issue store knows it (uppercased
/// identifier -> id).
pub(crate) type Idents = std::collections::HashMap<String, String>;

/// The item's issue ids `sessions.send` takes (`cardStamps` WIRE: uuids as
/// they are, an `#IDENT` the issue store resolves, unknown dropped).
fn issue_ids(item: &serde_json::Map<String, Value>, idents: &Idents) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in item.get("issueIds").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
        let id = if uuid::Uuid::parse_str(raw).is_ok() {
            Some(raw.to_string())
        } else {
            idents.get(&raw.trim_start_matches('#').to_uppercase()).cloned()
        };
        if let Some(id) = id.filter(|id| !out.contains(id)) {
            out.push(id);
        }
    }
    out
}

fn start_send(card_session: &str, message_id: &str, item: &serde_json::Map<String, Value>, picks: &Picks, idents: &Idents) -> SendInput {
    let item_id = str_of(item, "id");
    SendInput {
        device_id: picks.device_id.clone(),
        model: picks.model.clone(),
        ..SendInput::card_start(card_session, message_id, item_id, str_of(item, "title"), str_of(item, "text"), issue_ids(item, idents))
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
    idents: &Idents,
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
            .map(|item| start_send(card_session, message_id, item, picks, idents))
            .into_iter()
            .collect(),
        CardItemAction::StartAll => {
            items(card).filter(|item| suggestion_open(item)).map(|item| start_send(card_session, message_id, item, picks, idents)).collect()
        }
        CardItemAction::Dismiss | CardItemAction::DismissReplies | CardItemAction::File => Vec::new(),
    }
}

/// Every open reply of an opened replies card (`dismissReplies` `itemIds`).
pub(crate) fn open_reply_ids(card: &Value) -> Vec<String> {
    items(card).filter(|item| !flag(item, "sent")).map(|item| str_of(item, "id").to_string()).filter(|id| !id.is_empty()).collect()
}

/// The issues of the card session's CHAT TEAMS by identifier (Start's
/// `#IDENT` refs; web / Android read their issue stores the same way).
/// #EXP-2079 (agent-settings.json `assistant.chatTeams` (1)): the session's
/// own team while it is synced, else the turn's teams; an identifier found
/// in several is the first team's.
fn team_idents(session_id: &str, cx: &App) -> Idents {
    let Some(store) = sync::Store::try_global(cx) else { return Idents::new() };
    let collections = store.collections();
    let Some(session) = collections.sessions.read(cx).get(session_id).filter(|row| row.team_id.is_some()).cloned() else {
        return Idents::new();
    };
    let mut idents = Idents::new();
    for team_id in crate::chat_teams::session_team_ids(&session, cx) {
        for issue in collections.issue_refs_in_team(&team_id, cx) {
            idents.entry(issue.identifier.to_uppercase()).or_insert_with(|| issue.id.clone());
        }
    }
    idents
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
                    // An item the opened card no longer carries fails, never a stamp for nothing.
                    if item(&card, &item_id).is_none() {
                        return Tap::Refused;
                    }
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
            let idents = team_idents(&row.session_id, cx);
            let sends = card_sends(&card, &row.session_id, message_id, action, item_id.unwrap_or_default(), &picks, &idents);
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
        let sends = card_sends(&replies, "s-1", "m-1", CardItemAction::Reply, "r1", &Picks::default(), &Idents::new());
        assert_eq!(sends, vec![SendInput::card_message("s-1", "m-1", "r1", "Yes")]);
        assert!(card_sends(&replies, "s-1", "m-1", CardItemAction::Reply, "r2", &Picks::default(), &Idents::new()).is_empty());
        assert_eq!(open_reply_ids(&replies), vec!["r1".to_string()]);

        let issue = "00000000-0000-4000-8000-000000000012";
        let suggestions = json!({"kind": "suggestions", "items": [
            {"id": "s1", "title": "Add tests", "text": "Cover it", "issueIds": [issue, "EXP-9"]},
            {"id": "s2", "title": "Done", "text": "x", "here": true},
            {"id": "s3", "title": "Docs", "text": "Write them"},
        ]});
        let here = card_sends(&suggestions, "s-1", "m-1", CardItemAction::Here, "s1", &Picks::default(), &Idents::new());
        assert_eq!(here, vec![SendInput::card_message("s-1", "m-1", "s1", "Add tests\n\nCover it")]);
        let picks = Picks { device_id: Some("dev-1".into()), model: Some("opus".into()) };
        let start = card_sends(&suggestions, "s-1", "m-1", CardItemAction::Start, "s1", &picks, &Idents::new());
        assert_eq!(start.len(), 1);
        assert_eq!(start[0].parent_id.as_deref(), Some("s-1"));
        assert_eq!(start[0].text.as_deref(), Some("Add tests\n\nCover it"));
        assert_eq!(start[0].name.as_deref(), Some("Add tests"));
        assert_eq!(start[0].issue_ids, Some(vec![issue.to_string()]));
        assert_eq!((start[0].device_id.as_deref(), start[0].model.as_deref()), (Some("dev-1"), Some("opus")));
        // An `#IDENT` the issue store knows resolves (with or without `#`,
        // any case), once; an unknown one drops.
        let known = "00000000-0000-4000-8000-000000000034";
        let idents: Idents = [("EXP-9".to_string(), known.to_string())].into();
        let refs = json!({"kind": "suggestions", "items": [
            {"id": "s1", "title": "Add tests", "text": "Cover it", "issueIds": ["#exp-9", issue, "EXP-9", "#EXP-404", 7]},
        ]});
        let start = card_sends(&refs, "s-1", "m-1", CardItemAction::Start, "s1", &picks, &idents);
        assert_eq!(start[0].issue_ids, Some(vec![known.to_string(), issue.to_string()]));
        assert!(card_sends(&suggestions, "s-1", "m-1", CardItemAction::Start, "s2", &picks, &Idents::new()).is_empty());
        let all = card_sends(&suggestions, "s-1", "m-1", CardItemAction::StartAll, "", &Picks::default(), &Idents::new());
        let ids: Vec<_> = all.iter().map(|send| send.card_item.as_ref().unwrap().item_id.as_str()).collect();
        assert_eq!(ids, ["s1", "s3"]);
    }
}

#[cfg(test)]
pub(crate) fn team_idents_for_test(session_id: &str, cx: &App) -> Idents {
    team_idents(session_id, cx)
}
