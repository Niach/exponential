//! EXP-1459: the desktop opens sealed session content where rows enter
//! memory (`packages/domain-contract/fixtures/sealed-content.json`; design
//! #EXP-1457). The SQLite store keeps the envelopes as synced; every row the
//! collections hold is OPENED here, so no renderer ever sees an envelope:
//!
//! * `messages`: `body` (body envelope) + `card` (card envelope);
//! * `sessions`: the sealed title opens into `title`;
//! * `conversations`: the digest's kept messages, like `messages`.
//!
//! A row this install holds no key for keeps an empty body / the card's clear
//! part and carries [`SealState::Locked`] (the renderers draw the locked
//! row); when keys arrive the app calls `Store::reopen_sealed`, which
//! re-opens exactly those rows. gpui-free: the daemon links it too.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

use domain::rows::{ConversationRow, MessageRow, SealState, SessionRow};
use enet_core::sealed::{self as seal, SEALED_BODY_PREFIX};
pub use enet_core::sealed::{OpenStatus, OpenedBody, OpenedCard};
use serde_json::Value;

/// What opens a tree's sealed values on this install. The app hands in an
/// adapter over the device's key cache (`coding::sealing`); sync never
/// links it. Every call runs on the ingest path: cached keys only, no
/// network.
pub trait Opens: Send + Sync + 'static {
    fn open_body(&self, root_id: &str, body: &str) -> OpenedBody;
    fn open_card(&self, root_id: &str, card: &Value) -> OpenedCard;
    /// The session's sealed title opened, `None` = keep `title`.
    fn open_title(&self, row: &SessionRow) -> Option<String>;
}

/// No key cache (yet): every sealed value stays closed (`noKey` /
/// `unsupported` / `malformed` exactly as the contract parses it).
pub struct NoKeys;

impl Opens for NoKeys {
    fn open_body(&self, _root_id: &str, body: &str) -> OpenedBody {
        seal::open_body(body, &|_| None)
    }
    fn open_card(&self, _root_id: &str, card: &Value) -> OpenedCard {
        seal::open_card(card, &|_| None)
    }
    fn open_title(&self, _row: &SessionRow) -> Option<String> {
        None
    }
}

/// The process's opener: the active account's [`Opens`] plus the
/// session → root map the ingest learns from the `sessions` shape (a
/// conversation names its session, a key belongs to the root).
pub struct Opener {
    keys: RwLock<Arc<dyn Opens>>,
    roots: RwLock<HashMap<String, String>>,
    /// A digest held sealed content of a session whose root is unknown.
    unresolved: AtomicBool,
}

impl Opener {
    pub fn new(keys: Arc<dyn Opens>) -> Self {
        Self { keys: RwLock::new(keys), roots: RwLock::new(HashMap::new()), unresolved: AtomicBool::new(false) }
    }

    /// The process-wide opener ([`NoKeys`] until [`Opener::set_keys`]).
    pub fn global() -> &'static Opener {
        static OPENER: OnceLock<Opener> = OnceLock::new();
        OPENER.get_or_init(|| Opener::new(Arc::new(NoKeys)))
    }

    /// Swap the key cache (sign-in / account switch). The caller re-opens
    /// the collections afterwards.
    pub fn set_keys(&self, keys: Arc<dyn Opens>) {
        *self.keys.write().unwrap_or_else(|p| p.into_inner()) = keys;
    }

    pub fn keys(&self) -> Arc<dyn Opens> {
        Arc::clone(&self.keys.read().unwrap_or_else(|p| p.into_inner()))
    }

    /// Clears and returns the "a digest waits for its root" flag.
    pub fn take_unresolved(&self) -> bool {
        self.unresolved.swap(false, Ordering::SeqCst)
    }

    /// The root of `session_id`, as far as the synced sessions say.
    pub fn root_of(&self, session_id: &str) -> Option<String> {
        self.roots.read().unwrap_or_else(|p| p.into_inner()).get(session_id).cloned()
    }

    fn remember_root(&self, session_id: &str, root_id: &str) {
        let known = self.roots.read().unwrap_or_else(|p| p.into_inner()).get(session_id).map(String::as_str) == Some(root_id);
        if !known {
            self.roots.write().unwrap_or_else(|p| p.into_inner()).insert(session_id.to_string(), root_id.to_string());
        }
    }

    /// The opener for `root_id`'s tree (an unknown root opens nothing).
    fn tree(&self, root_id: Option<&str>) -> Tree {
        let keys: Arc<dyn Opens> = if root_id.is_some() { self.keys() } else { Arc::new(NoKeys) };
        Tree { keys, root: root_id.unwrap_or_default().to_string() }
    }

    /// Opens a `messages` row in place (`root_id` names its tree).
    pub fn open_message(&self, row: &mut MessageRow) {
        let tree = self.tree(row.root_id.as_deref());
        row.seal = open_parts(&mut row.body, &mut row.card, &tree);
    }

    /// Opens a `sessions` row's sealed title into `title` (a title this
    /// install cannot open keeps the neutral plaintext fallback).
    pub fn open_session(&self, row: &mut SessionRow) {
        let root = row.root_id_or_self().to_string();
        self.remember_root(&row.id, &root);
        if let Some(title) = self.keys().open_title(row) {
            row.title = Some(title);
        }
    }

    /// Opens a `conversations` row's digest messages in place. Returns
    /// `false` when it holds sealed content of a session whose root is not
    /// known yet (the sessions shape re-opens it once it is).
    pub fn open_conversation(&self, row: &mut ConversationRow) -> bool {
        let resolved = self.open_digest(row);
        if !resolved {
            self.unresolved.store(true, Ordering::SeqCst);
        }
        resolved
    }

    fn open_digest(&self, row: &mut ConversationRow) -> bool {
        let root = row.session_id.as_deref().and_then(|session| self.root_of(session));
        let tree = self.tree(root.as_deref());
        let Some(Value::Object(digest)) = row.digest.as_mut() else {
            return true;
        };
        let mut sealed = false;
        for slot in ["first", "preview", "waiting"] {
            let Some(Value::Object(message)) = digest.get_mut(slot) else {
                continue;
            };
            let mut body = message.get("body").and_then(Value::as_str).map(str::to_string);
            let mut card = message.get("card").cloned().filter(|card| !card.is_null());
            let state = open_parts(&mut body, &mut card, &tree);
            if let Some(state) = state {
                sealed = true;
                message.insert("body".into(), Value::String(body.unwrap_or_default()));
                message.insert("card".into(), card.unwrap_or(Value::Null));
                message.insert("_seal".into(), serde_json::to_value(state_name(state)).unwrap_or(Value::Null));
            }
        }
        root.is_some() || !sealed
    }
}

fn state_name(state: SealState) -> &'static str {
    match state {
        SealState::Opened => "opened",
        SealState::Locked => "locked",
        SealState::Unreadable => "unreadable",
    }
}

fn worst(a: Option<SealState>, b: Option<SealState>) -> Option<SealState> {
    let rank = |s: Option<SealState>| match s {
        None => 0,
        Some(SealState::Opened) => 1,
        Some(SealState::Locked) => 2,
        Some(SealState::Unreadable) => 3,
    };
    if rank(b) > rank(a) {
        b
    } else {
        a
    }
}

struct Tree {
    keys: Arc<dyn Opens>,
    root: String,
}

/// Opens one message's body + card in place; `None` = nothing was sealed.
/// Not opened = an empty body and the card's clear part.
fn open_parts(body: &mut Option<String>, card: &mut Option<Value>, tree: &Tree) -> Option<SealState> {
    let mut state = None;
    if let Some(text) = body.as_deref().filter(|b| b.starts_with(SEALED_BODY_PREFIX)) {
        let opened = tree.keys.open_body(&tree.root, text);
        let body_state = SealState::from_status(opened.status.as_str());
        *body = Some(opened.text.filter(|_| opened.status == OpenStatus::Opened).unwrap_or_default());
        state = worst(state, body_state);
    }
    if let Some(value) = card.as_ref().filter(|c| c.get("sealed").is_some()) {
        let opened = tree.keys.open_card(&tree.root, value);
        *card = Some(opened.card);
        state = worst(state, SealState::from_status(opened.status.as_str()));
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/sealed-content.json")).unwrap()
    }

    fn hex32(v: &Value) -> [u8; 32] {
        let s = v.as_str().unwrap();
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap();
        }
        out
    }

    /// The fixture's `keys` under one root.
    struct FixtureKeys(Vec<(u32, [u8; 32])>);
    impl FixtureKeys {
        fn lookup<'a>(&'a self, root_id: &'a str) -> impl Fn(u32) -> Option<[u8; 32]> + 'a {
            move |epoch| (root_id == "root").then(|| self.0.iter().find(|(e, _)| *e == epoch).map(|(_, s)| *s)).flatten()
        }
    }
    impl Opens for FixtureKeys {
        fn open_body(&self, root_id: &str, body: &str) -> OpenedBody {
            seal::open_body(body, &self.lookup(root_id))
        }
        fn open_card(&self, root_id: &str, card: &Value) -> OpenedCard {
            seal::open_card(card, &self.lookup(root_id))
        }
        fn open_title(&self, row: &SessionRow) -> Option<String> {
            (row.root_id_or_self() == "root").then(|| "Opened title".to_string())
        }
    }

    fn opener(fx: &Value) -> Opener {
        let keys = fx["keys"].as_object().unwrap().iter().map(|(k, v)| (k.parse().unwrap(), hex32(v))).collect();
        Opener::new(Arc::new(FixtureKeys(keys)))
    }

    fn message(body: &str, card: Option<Value>) -> MessageRow {
        MessageRow { id: "m".into(), root_id: Some("root".into()), body: Some(body.into()), card, ..MessageRow::default() }
    }

    /// `bodyCases`: every status the contract names maps onto the row.
    #[test]
    fn replays_the_body_cases_onto_message_rows() {
        let fx = fixture();
        let opener = opener(&fx);
        for case in fx["bodyCases"].as_array().unwrap() {
            let mut row = message(case["body"].as_str().unwrap(), None);
            opener.open_message(&mut row);
            let status = case["status"].as_str().unwrap();
            assert_eq!(row.seal, SealState::from_status(status), "{case}");
            match status {
                "plain" | "opened" => assert_eq!(row.body.as_deref(), case["text"].as_str(), "{case}"),
                _ => assert_eq!(row.body.as_deref(), Some(""), "a closed body never shows as text: {case}"),
            }
        }
    }

    /// `cardCases`: opened = the full card, closed = the clear part alone.
    #[test]
    fn replays_the_card_cases_onto_message_rows() {
        let fx = fixture();
        let opener = opener(&fx);
        for case in fx["cardCases"].as_array().unwrap() {
            let mut row = message("", Some(case["card"].clone()));
            opener.open_message(&mut row);
            let status = case["status"].as_str().unwrap();
            assert_eq!(row.seal, SealState::from_status(status), "{case}");
            if let Some(expected) = case.get("opened").or_else(|| case.get("result")) {
                assert_eq!(row.card.as_ref(), Some(expected), "{case}");
            }
            assert!(row.card.as_ref().is_none_or(|card| card.get("sealed").is_none()), "{case}");
        }
    }

    /// EXP-1459: `placeholder.messageCases`: {locked, body, card} per case.
    #[test]
    fn replays_the_placeholder_message_cases() {
        let fx = fixture();
        for case in fx["placeholder"]["messageCases"].as_array().unwrap() {
            let keys = case["keys"].as_object().unwrap().iter().map(|(k, v)| (k.parse().unwrap(), hex32(v))).collect();
            let opener = Opener::new(Arc::new(FixtureKeys(keys)));
            let m = &case["message"];
            let mut row = message(m["body"].as_str().unwrap(), m.get("card").filter(|c| !c.is_null()).cloned());
            opener.open_message(&mut row);
            let actual = serde_json::json!({
                "locked": row.locked(),
                "body": row.body.clone().unwrap_or_default(),
                "card": row.card.clone(),
            });
            assert_eq!(actual, case["expected"], "{}", case["name"]);
        }
    }

    /// A row of another tree never opens with this tree's keys.
    #[test]
    fn keys_belong_to_their_root() {
        let fx = fixture();
        let opener = opener(&fx);
        let mut row = message(fx["body"]["envelope"].as_str().unwrap(), None);
        row.root_id = Some("other".into());
        opener.open_message(&mut row);
        assert_eq!(row.seal, Some(SealState::Locked));
        assert_eq!(row.body.as_deref(), Some(""));
    }

    #[test]
    fn opens_sealed_titles_and_digests() {
        let fx = fixture();
        let opener = opener(&fx);
        let envelope = fx["body"]["envelope"].as_str().unwrap();
        let mut session = SessionRow {
            id: "child".into(),
            root_id: Some("root".into()),
            title: Some("Session".into()),
            ..SessionRow::default()
        };
        opener.open_session(&mut session);
        assert_eq!(session.title.as_deref(), Some("Opened title"));
        assert_eq!(opener.root_of("child").as_deref(), Some("root"));

        let split = &fx["card"]["splits"].as_array().unwrap()[0];
        let mut conversation = ConversationRow {
            id: "c".into(),
            session_id: Some("child".into()),
            digest: Some(serde_json::json!({
                "preview": {"id": "m1", "authorKind": "user", "body": envelope, "createdAt": "2026-10-30T00:00:00Z"},
                "waiting": {"id": "m2", "authorKind": "agent", "body": "", "card": split["envelope"], "createdAt": "2026-10-30T00:00:00Z"},
            })),
            ..ConversationRow::default()
        };
        assert!(opener.open_conversation(&mut conversation));
        let digest = domain::chats::ConversationDigest::decode(conversation.digest.as_ref()).unwrap();
        let preview = digest.preview.unwrap();
        assert_eq!(preview.body, fx["body"]["text"].as_str().unwrap());
        assert_eq!(preview.seal, Some(SealState::Opened));
        let waiting = digest.waiting.unwrap();
        assert_eq!(waiting.card.as_ref(), Some(&split["card"]));

        // A session the sessions shape has not delivered yet: locked, retried.
        let mut unknown = ConversationRow {
            id: "c2".into(),
            session_id: Some("unknown".into()),
            digest: Some(serde_json::json!({"preview": {"id": "m3", "body": envelope}})),
            ..ConversationRow::default()
        };
        assert!(!opener.open_conversation(&mut unknown));
        let preview = domain::chats::ConversationDigest::decode(unknown.digest.as_ref()).unwrap().preview.unwrap();
        assert_eq!((preview.body.as_str(), preview.seal), ("", Some(SealState::Locked)));
    }
}
