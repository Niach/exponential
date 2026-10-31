//! EXP-1459: the desktop's sealed session content (design #EXP-1457,
//! `packages/domain-contract/fixtures/sealed-content.json`).
//!
//! * READ: `sync::sealed` opens every row where it enters memory; this
//!   module hands it the device's seal service and re-opens the closed rows
//!   whenever the service's change counter moves (a wrap arrived).
//! * WRITE: person messages and card answers are sealed before
//!   `sessions.send` / `sessions.answer` ([`send_blocking`],
//!   [`answer_blocking`]); mentions ride clear as `mentionUserIds`. Without
//!   a service (or a key) the message goes as plaintext, which stays legal.

use std::sync::Arc;
use std::time::Duration;

use api::sessions::{AnswerInput, SealWrap, SendInput, SendOutput, SendSeal};
use coding::sealing::{self, NewRootSeal};
use domain::plan_card::{CardAnswerInput, CardAnswers};
use domain::rows::{MessageRow, SessionRow};
use domain::sealed::SealState;
use gpui::App;
use serde_json::Value;
use steer::enet::InstallNode;
use sync::sealed::{OpenedBody, OpenedCard, Opener, Opens};
use sync::Store;

/// `sync::sealed::Opens` over the installed keyring (`coding::sealing`).
/// Cached secrets only: the ingest path never blocks (the key holder pulls
/// `sessionKeys.mine` on start and every minute; a new wrap moves
/// [`sealing::changed`] and the locked rows re-open).
struct KeyringOpens;

fn cached(root_id: &str) -> impl Fn(u32) -> Option<[u8; 32]> + '_ {
    move |epoch| sealing::service().and_then(|service| service.keyring().secret(root_id, epoch))
}

impl Opens for KeyringOpens {
    fn open_body(&self, root_id: &str, body: &str) -> OpenedBody {
        enet_core::sealed::open_body(body, &cached(root_id))
    }
    fn open_card(&self, root_id: &str, card: &Value) -> OpenedCard {
        enet_core::sealed::open_card(card, &cached(root_id))
    }
    fn open_title(&self, row: &SessionRow) -> Option<String> {
        // The keyring learns session → root and each root's current key
        // (what `ensure_key` writes with) from the synced rows.
        sealing::note_sessions(std::slice::from_ref(row));
        let sealed = row.sealed_title.as_deref().filter(|title| !title.is_empty())?;
        let opened = enet_core::sealed::open_body(sealed, &cached(row.root_id_or_self()));
        opened.text.filter(|_| matches!(opened.status, sync::sealed::OpenStatus::Opened | sync::sealed::OpenStatus::Plain))
    }
}

/// How often the keyring's change counter is read (an atomic).
const KEYS_POLL: Duration = Duration::from_secs(2);

/// Bootstrap (before the first account connects): the opener reads the
/// keyring, and the closed rows re-open whenever new keys land.
pub fn init(cx: &mut App) {
    Opener::global().set_keys(Arc::new(KeyringOpens));
    cx.spawn(async move |cx| {
        let mut seen = sealing::changed();
        loop {
            cx.background_executor().timer(KEYS_POLL).await;
            let changed = sealing::changed();
            if changed != seen {
                seen = changed;
                cx.update(|cx| Store::global(cx).clone().reopen_sealed(cx));
            }
        }
    })
    .detach();
}

/// The writer's key operations (BLOCKING). [`Live`] = the installed seal
/// service + this account's eNet install; tests fake it.
trait Writer {
    /// `text` sealed for an existing tree (its key is created if missing).
    fn seal_body(&self, root_id: &str, text: &str) -> Result<String, String>;
    fn seal_card(&self, root_id: &str, card: &Value) -> Result<Value, String>;
    /// A fresh tree key for a root `sessions.send` creates, wrapped for
    /// `target_device`'s install too.
    fn new_root(&self, target_device: Option<&str>) -> Result<NewRootSeal, String>;
    fn adopt(&self, root_id: &str, seal: &NewRootSeal);
}

struct Live(Option<InstallNode>);

impl Writer for Live {
    fn seal_body(&self, root_id: &str, text: &str) -> Result<String, String> {
        sealing::seal_body(root_id, text).map_err(|err| err.to_string())
    }
    fn seal_card(&self, root_id: &str, card: &Value) -> Result<Value, String> {
        sealing::seal_card(root_id, card).map_err(|err| err.to_string())
    }
    fn new_root(&self, target_device: Option<&str>) -> Result<NewRootSeal, String> {
        self.0.as_ref().ok_or("eNet is not running")?.new_root_seal(target_device)
    }
    fn adopt(&self, root_id: &str, seal: &NewRootSeal) {
        match sealing::service() {
            Some(service) => service.adopt(root_id, seal),
            None => sealing::adopt_new_root(root_id, seal),
        }
    }
}

/// Where a send's text goes, decided on the foreground ([`plan_send`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub enum SealTarget {
    /// An existing tree: seal for its root (its key is created if missing).
    Root(String),
    /// `sessions.send` creates the root: its key rides inline (`seal`),
    /// wrapped for the executing device's install too (`device_id`).
    NewRoot { device_id: Option<String> },
    /// Nothing to seal (no text, no name).
    #[default]
    None,
}

/// A send's sealing facts, read on the foreground before the blocking call.
#[derive(Clone, Default)]
pub struct SendPlan {
    pub target: SealTarget,
    /// This account's eNet install (a new root's key comes from it).
    node: Option<InstallNode>,
    pub mention_user_ids: Vec<String>,
    /// `refIssueIds`: the `#IDENT` refs the text makes, resolved here.
    pub ref_issue_ids: Vec<String>,
}

/// The tree a send writes into and the users its text mentions.
pub fn plan_send(input: &SendInput, cx: &App) -> SendPlan {
    let collections = Store::global(cx).collections();
    let mention_user_ids = input
        .text
        .as_deref()
        .map(|text| {
            let users = collections.users.read(cx);
            let users: Vec<(&str, &str)> =
                users.iter().filter_map(|user| Some((user.id.as_str(), user.email.as_deref()?))).collect();
            domain::sealed::mention_user_ids(text, users.iter().copied())
        })
        .unwrap_or_default();
    let sessions = collections.sessions.read(cx);
    let team_id = input
        .team_id
        .clone()
        .or_else(|| input.session_id.as_ref().or(input.parent_id.as_ref()).and_then(|id| sessions.get(id)?.team_id.clone()));
    let ref_issue_ids = match (input.text.as_deref(), team_id.as_deref()) {
        (Some(text), Some(team_id)) => {
            let issues = collections.issue_refs_in_team(team_id, cx);
            domain::sealed::ref_issue_ids(text, issues.iter().map(|issue| (issue.id.as_str(), issue.identifier.as_str())))
        }
        _ => Vec::new(),
    };
    let root_of = |id: &str| {
        sessions
            .get(id)
            .map(|row| row.root_id_or_self().to_string())
            .or_else(|| Opener::global().root_of(id))
    };
    let has_content = input.text.is_some() || input.name.is_some();
    let target = if !has_content {
        SealTarget::None
    } else if let Some(session_id) = &input.session_id {
        root_of(session_id).map_or(SealTarget::None, SealTarget::Root)
    } else if let Some(parent_id) = &input.parent_id {
        root_of(parent_id).map_or(SealTarget::None, SealTarget::Root)
    } else if let Some(existing) = existing_conversation(input, &sessions.iter().collect::<Vec<_>>(), cx) {
        SealTarget::Root(existing)
    } else if input.team_id.is_some() {
        SealTarget::NewRoot { device_id: input.device_id.clone() }
    } else {
        SealTarget::None
    };
    let node = crate::enet_wiring::active_node(cx);
    SendPlan { target, node, mention_user_ids, ref_issue_ids }
}

/// An agent / app send without a session id lands in the member's existing
/// conversation root when there is one.
fn existing_conversation(input: &SendInput, sessions: &[&SessionRow], cx: &App) -> Option<String> {
    let me = crate::queries::active_account(cx)?.user_id;
    let mine = |row: &&&SessionRow| row.is_root() && row.user_id.as_deref() == Some(me.as_str());
    if let Some(agent_id) = &input.agent_id {
        return sessions.iter().filter(mine).find(|row| row.agent_id.as_ref() == Some(agent_id)).map(|row| row.id.clone());
    }
    if let Some(app_id) = &input.app_id {
        return sessions.iter().filter(mine).find(|row| row.app_id.as_ref() == Some(app_id)).map(|row| row.id.clone());
    }
    None
}

/// Seals `input` per `plan` (BLOCKING): the text and name become body
/// envelopes, a new root's key rides inline. Returns the new root's seal
/// to adopt once the server names the root. Any failure leaves the content
/// plaintext.
fn seal_send(writer: &dyn Writer, input: &mut SendInput, plan: &SendPlan) -> Option<NewRootSeal> {
    match &plan.target {
        SealTarget::None => None,
        SealTarget::Root(root_id) => {
            let sealed_text = match input.text.as_deref() {
                Some(text) => match writer.seal_body(root_id, text) {
                    Ok(sealed) => Some(sealed),
                    Err(err) => {
                        log::warn!("[sealing] sending plaintext, no key for {root_id}: {err}");
                        return None;
                    }
                },
                None => None,
            };
            let sealed_name = input.name.as_deref().and_then(|name| writer.seal_body(root_id, name).ok());
            if sealed_text.is_some() {
                input.text = sealed_text;
            }
            if sealed_name.is_some() {
                input.sealed_name = sealed_name;
                input.name = None;
            }
            None
        }
        SealTarget::NewRoot { device_id } => {
            // The executing device reads the tree from its first message
            // on; any gap is filled by a holder (`pending`).
            let seal = match writer.new_root(device_id.as_deref()) {
                Ok(seal) => seal,
                Err(err) => {
                    log::warn!("[sealing] sending plaintext, no new tree key: {err}");
                    return None;
                }
            };
            let public = enet_core::sealed::public_key_from_b64(&seal.public_key).ok()?;
            let body = |text: &str| enet_core::sealed::seal_body(&public, seal.epoch, text).ok();
            let sealed_text = match input.text.as_deref() {
                Some(text) => Some(body(text)?),
                None => None,
            };
            if let Some(name) = input.name.as_deref() {
                input.sealed_name = Some(body(name)?);
                input.name = None;
            }
            if sealed_text.is_some() {
                input.text = sealed_text;
            }
            input.seal = Some(SendSeal {
                install_id: seal.install_id.clone(),
                public_key: seal.public_key.clone(),
                epoch: seal.epoch,
                wraps: seal
                    .wraps
                    .iter()
                    .map(|wrap| SealWrap { install_id: wrap.install_id.clone(), wrapped: wrap.wrapped.clone() })
                    .collect(),
            });
            Some(seal)
        }
    }
}

/// THE sealed `sessions.send` (BLOCKING, off the UI thread).
pub fn send_blocking(trpc: &api::TrpcClient, input: &SendInput, plan: &SendPlan) -> Result<SendOutput, api::ApiError> {
    let mut input = input.clone();
    if !plan.mention_user_ids.is_empty() {
        input.mention_user_ids = Some(plan.mention_user_ids.clone());
    }
    if !plan.ref_issue_ids.is_empty() {
        input.ref_issue_ids = Some(plan.ref_issue_ids.clone());
    }
    let writer = Live(plan.node.clone());
    let seal = sealing::service().and_then(|_| seal_send(&writer, &mut input, plan));
    let out = api::sessions::send(trpc, &input)?;
    if let Some(seal) = seal {
        writer.adopt(&out.session_id, &seal);
    }
    Ok(out)
}

/// An answer's sealing facts: the card's tree and, for a SEALED card, the
/// card as opened (EXP-1486: the stored card alone picks the wire,
/// `sealed-content.json` `answerCases`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnswerPlan {
    pub root_id: Option<String>,
    /// The opened card of a sealed row; `None` = a plaintext card, answered
    /// in clear even inside a keyed tree.
    pub card: Option<Value>,
    /// A sealed card this install cannot open: refused locally.
    pub locked: bool,
}

/// What a sealed card this install cannot answer says (Android's
/// `LOCKED_ANSWER`).
pub const LOCKED_ANSWER: &str = "This card can't be answered on this device yet.";

pub fn plan_answer(message_id: &str, cx: &App) -> AnswerPlan {
    let messages = Store::global(cx).collections().messages.read(cx);
    messages.get(message_id).map(answer_plan_for).unwrap_or_default()
}

/// A synced row's [`AnswerPlan`]: `seal` is `None` for a plaintext row.
fn answer_plan_for(row: &MessageRow) -> AnswerPlan {
    let root_id = row.root_id.clone();
    match row.seal {
        None => AnswerPlan { root_id, ..AnswerPlan::default() },
        Some(SealState::Opened) => AnswerPlan { root_id, card: row.card.clone(), locked: false },
        Some(_) => AnswerPlan { root_id, card: None, locked: true },
    }
}

/// `input` as a sealed answer: the FULL card re-sealed with the answer, the
/// clear `decision` / `fresh` beside it, no plaintext answer fields.
fn seal_answer(writer: &dyn Writer, input: &AnswerInput, plan: &AnswerPlan) -> Option<AnswerInput> {
    let (root_id, card) = (plan.root_id.as_deref()?, plan.card.as_ref()?);
    let answers = input.answers.as_ref().and_then(|answers| match answers {
        Value::Array(labels) => Some(CardAnswers::Labels(labels.iter().filter_map(|l| l.as_str().map(str::to_owned)).collect())),
        Value::Object(_) => serde_json::from_value(answers.clone()).ok().map(CardAnswers::ByQuestion),
        _ => None,
    });
    let answered = domain::plan_card::answered_card(
        card,
        &CardAnswerInput {
            answer: input.answer.clone(),
            answers,
            decision: input.decision.clone(),
            fresh: input.fresh,
            feedback: input.feedback.clone(),
            surface_data: input.surface_data.clone(),
        },
    )?;
    let sealed = writer.seal_card(root_id, &answered).map_err(|err| log::warn!("[sealing] can't seal the answer: {err}")).ok()?;
    Some(AnswerInput {
        message_id: input.message_id.clone(),
        decision: input.decision.clone(),
        fresh: input.fresh,
        sealed_card: Some(sealed),
        ..AnswerInput::default()
    })
}

/// The answer that leaves: a plaintext card's as given, a sealed card's
/// re-sealed; a sealed card this install can't (re)seal is refused, since a
/// plaintext answer would leak it.
fn answer_wire(writer: Option<&dyn Writer>, input: &AnswerInput, plan: &AnswerPlan) -> Result<AnswerInput, String> {
    if plan.locked {
        return Err(LOCKED_ANSWER.into());
    }
    if plan.card.is_none() {
        return Ok(input.clone());
    }
    writer.and_then(|writer| seal_answer(writer, input, plan)).ok_or_else(|| LOCKED_ANSWER.into())
}

/// THE sealed `sessions.answer` (BLOCKING, off the UI thread).
pub fn answer_blocking(trpc: &api::TrpcClient, input: &AnswerInput, plan: &AnswerPlan) -> Result<api::sessions::TxOutput, api::ApiError> {
    let live = sealing::service().map(|_| Live(None));
    let wire = answer_wire(live.as_ref().map(|w| w as &dyn Writer), input, plan)
        .map_err(|message| api::ApiError::Http { status: 400, message })?;
    api::sessions::answer(trpc, &wire)
}

#[cfg(test)]
mod tests {
    use super::*;
    use enet_core::sealed as contract;
    use std::sync::Mutex;

    /// A one-tree writer on a fixed key.
    struct OneTree {
        key: contract::TreeKey,
        devices: Mutex<Vec<Option<String>>>,
    }

    impl OneTree {
        fn open_body(&self, body: &str) -> OpenedBody {
            contract::open_body(body, &|epoch| (epoch == 1).then_some(self.key.secret))
        }
        fn open_card(&self, card: &Value) -> OpenedCard {
            contract::open_card(card, &|epoch| (epoch == 1).then_some(self.key.secret))
        }
    }

    impl Writer for OneTree {
        fn seal_body(&self, _root_id: &str, text: &str) -> Result<String, String> {
            contract::seal_body(&self.key.public, 1, text).map_err(|e| e.to_string())
        }
        fn seal_card(&self, _root_id: &str, card: &Value) -> Result<Value, String> {
            contract::seal_card(&self.key.public, 1, card).map_err(|e| e.to_string())
        }
        fn new_root(&self, target_device: Option<&str>) -> Result<NewRootSeal, String> {
            self.devices.lock().unwrap().push(target_device.map(str::to_string));
            Ok(NewRootSeal {
                install_id: "me".into(),
                public_key: self.key.public_key(),
                epoch: 1,
                wraps: vec![api::session_keys::InstallWrap { install_id: "me".into(), wrapped: "w".into() }],
                secret: self.key.secret,
            })
        }
        fn adopt(&self, _root_id: &str, _seal: &NewRootSeal) {}
    }

    fn tree() -> OneTree {
        OneTree { key: contract::generate_tree_key(), devices: Mutex::new(Vec::new()) }
    }

    #[test]
    fn a_message_to_an_existing_tree_seals_text_and_name() {
        let writer = tree();
        let mut input = SendInput::message("s1", "hello @a@b.co", Vec::new());
        input.name = Some("Title".into());
        let plan = SendPlan { target: SealTarget::Root("r1".into()), ..SendPlan::default() };
        assert!(seal_send(&writer, &mut input, &plan).is_none());
        let text = input.text.as_deref().unwrap();
        assert!(text.starts_with("exp-sealed:v1:1:"));
        assert_eq!(writer.open_body(text).text.as_deref(), Some("hello @a@b.co"));
        assert_eq!(input.name, None);
        assert_eq!(writer.open_body(input.sealed_name.as_deref().unwrap()).text.as_deref(), Some("Title"));
        assert!(input.seal.is_none());
    }

    #[test]
    fn a_new_root_carries_its_key_inline() {
        let writer = tree();
        let mut input = SendInput { team_id: Some("t".into()), device_id: Some("d".into()), text: Some("go".into()), ..SendInput::default() };
        let plan = SendPlan { target: SealTarget::NewRoot { device_id: Some("d".into()) }, ..SendPlan::default() };
        let seal = seal_send(&writer, &mut input, &plan).unwrap();
        assert_eq!(seal.secret, writer.key.secret);
        assert_eq!(writer.devices.lock().unwrap().clone(), vec![Some("d".to_string())]);
        let wire = input.seal.clone().unwrap();
        assert_eq!((wire.install_id.as_str(), wire.epoch, wire.public_key), ("me", 1, writer.key.public_key()));
        assert_eq!(writer.open_body(input.text.as_deref().unwrap()).text.as_deref(), Some("go"));
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["seal"]["wraps"][0], serde_json::json!({"installId": "me", "wrapped": "w"}));
    }

    #[test]
    fn an_answer_reseals_the_full_card_and_drops_plaintext_fields() {
        let writer = tree();
        let card = serde_json::json!({"kind": "question", "question": "Which?", "options": ["A", "B"]});
        let input = AnswerInput {
            message_id: "m1".into(),
            answer: Some("A".into()),
            answers: Some(serde_json::json!(["A"])),
            ..AnswerInput::default()
        };
        let plan = AnswerPlan { root_id: Some("r1".into()), card: Some(card), locked: false };
        let sealed = seal_answer(&writer, &input, &plan).unwrap();
        assert_eq!((sealed.answer.clone(), sealed.answers.clone(), sealed.feedback.clone()), (None, None, None));
        let wire = serde_json::to_value(&sealed).unwrap();
        assert_eq!(wire["sealedCard"]["kind"], "question");
        assert_eq!(wire["sealedCard"]["answered"], true);
        let opened = writer.open_card(&wire["sealedCard"]).card;
        assert_eq!(opened["answers"], serde_json::json!(["A"]));
        assert_eq!(opened["question"], "Which?");

        let plan_card = AnswerPlan { root_id: Some("r1".into()), card: Some(serde_json::json!({"kind": "plan", "plan": "p"})), locked: false };
        let decide = AnswerInput { message_id: "m2".into(), decision: Some("rejected".into()), feedback: Some("no".into()), ..AnswerInput::default() };
        let sealed = seal_answer(&writer, &decide, &plan_card).unwrap();
        assert_eq!((sealed.decision.as_deref(), sealed.feedback.as_deref()), (Some("rejected"), None));
        let wire = serde_json::to_value(&sealed).unwrap();
        assert_eq!(wire["sealedCard"]["decision"], "rejected");
        assert_eq!(writer.open_card(&wire["sealedCard"]).card["feedback"], "no");
        // An unknown card (not synced, or closed here) never seals.
        assert!(seal_answer(&writer, &decide, &AnswerPlan::default()).is_none());
    }

    /// EXP-1486 (`sealed-content.json` `answerCases`): the STORED card picks
    /// the wire. A plaintext plan inside a keyed tree answers in clear (0.14.88
    /// sealed it and the server refused), a sealed one re-seals, a locked one
    /// never leaves.
    #[test]
    fn the_fixture_answer_cases_pick_the_wire_by_the_stored_card() {
        struct FixtureKeys(Option<[u8; 32]>);
        impl Opens for FixtureKeys {
            fn open_body(&self, _root_id: &str, body: &str) -> OpenedBody {
                contract::open_body(body, &|epoch| (epoch == 1).then_some(self.0).flatten())
            }
            fn open_card(&self, _root_id: &str, card: &Value) -> OpenedCard {
                contract::open_card(card, &|epoch| (epoch == 1).then_some(self.0).flatten())
            }
            fn open_title(&self, _row: &SessionRow) -> Option<String> {
                None
            }
        }
        let fixture: Value = serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/sealed-content.json")).unwrap();
        let hex = |text: &str| -> [u8; 32] {
            let bytes: Vec<u8> = (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect();
            bytes.try_into().unwrap()
        };
        let secret = hex(fixture["tree"]["secret"].as_str().unwrap());
        let public = contract::public_key_from_b64(fixture["tree"]["publicKey"].as_str().unwrap()).unwrap();
        let writer = OneTree { key: contract::TreeKey { secret, public }, devices: Mutex::new(Vec::new()) };
        let splits = fixture["card"]["splits"].as_array().unwrap();
        let cases = fixture["answerCases"].as_array().unwrap();
        assert!(cases.len() >= 5);
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let stored = match case["stored"]["split"].as_str() {
                Some(split) => splits.iter().find(|s| s["name"] == split).unwrap()["envelope"].clone(),
                None => case["stored"].clone(),
            };
            let with_key = case["keys"] == "tree";
            let mut row = MessageRow { id: "m1".into(), root_id: Some("r1".into()), body: Some(String::new()), card: Some(stored), ..MessageRow::default() };
            Opener::new(Arc::new(FixtureKeys(with_key.then_some(secret)))).open_message(&mut row);
            let plan = answer_plan_for(&row);
            let given = &case["input"];
            let text = |field: &str| given[field].as_str().map(str::to_string);
            let input = AnswerInput {
                message_id: "m1".into(),
                answer: text("answer"),
                decision: text("decision"),
                fresh: given["fresh"].as_bool(),
                feedback: text("feedback"),
                answers: given.get("answers").cloned(),
                ..AnswerInput::default()
            };
            let wire = answer_wire(Some(&writer), &input, &plan);
            match case["wire"].as_str().unwrap() {
                "refused" => assert_eq!(wire, Err(LOCKED_ANSWER.to_string()), "{name}"),
                "clear" => assert_eq!(wire.as_ref(), Ok(&input), "{name}"),
                "sealed" => {
                    let wire = serde_json::to_value(wire.unwrap()).unwrap();
                    let keys: Vec<&String> = wire.as_object().unwrap().keys().filter(|k| *k != "messageId").collect();
                    assert!(keys.iter().all(|k| ["sealedCard", "decision", "fresh"].contains(&k.as_str())), "{name}: {keys:?}");
                    for (field, value) in case["clear"].as_object().unwrap() {
                        assert_eq!(&wire["sealedCard"][field], value, "{name}: clear {field}");
                    }
                    let opened = writer.open_card(&wire["sealedCard"]).card;
                    for (field, value) in case["opened"].as_object().unwrap() {
                        assert_eq!(&opened[field], value, "{name}: opened {field}");
                    }
                }
                other => panic!("{name}: unknown wire {other}"),
            }
        }
    }
}
