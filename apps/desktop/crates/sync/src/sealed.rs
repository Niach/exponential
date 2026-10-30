//! EXP-1459: the desktop opens sealed session content where rows enter
//! memory (`packages/domain-contract/fixtures/sealed-content.json`; design
//! #EXP-1457). The SQLite store keeps the envelopes as synced; every row the
//! collections hold is OPENED here, so no renderer ever sees an envelope:
//!
//! * `messages`: `body` (body envelope) + `card` (card envelope);
//! * `sessions`: `sealed_title` opens into `title`;
//! * `conversations`: the digest's kept messages, like `messages`.
//!
//! A row this install holds no key for keeps an empty body / the card's clear
//! part and carries [`SealState::Locked`] (the renderers draw the locked
//! row); when keys arrive ([`TreeKeys::on_change`]) the collections re-open
//! exactly those rows. gpui-free: the daemon links it too.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

use domain::rows::{ConversationRow, MessageRow, SealState, SessionRow};
use enet_core::sealed::{self as seal, OpenStatus, SEALED_BODY_PREFIX};
use serde_json::Value;

/// The tree keys this install holds (the cache beside `steer::enet`
/// implements it: unwrapped `sessionKeys.mine` rows, answers `pending`).
pub trait TreeKeys: Send + Sync + 'static {
    /// The cached secret for (root, epoch). Never blocks, no network: it
    /// runs on the ingest path.
    fn secret(&self, root_id: &str, epoch: u32) -> Option<[u8; 32]>;
    /// The writer path, BLOCKING (off the UI thread): the root's current
    /// (epoch, tree public); a root without a key gets one first
    /// (`sessionKeys.create` with wraps for every reader it resolves).
    fn writer_key(&self, root_id: &str) -> Result<(u32, [u8; 32]), String>;
    /// `f` runs whenever new secrets land (any thread).
    fn on_change(&self, f: Box<dyn Fn() + Send + Sync>);
}

/// No key cache (yet): everything sealed stays locked, writers send plaintext.
pub struct NoKeys;

impl TreeKeys for NoKeys {
    fn secret(&self, _root_id: &str, _epoch: u32) -> Option<[u8; 32]> {
        None
    }
    fn writer_key(&self, _root_id: &str) -> Result<(u32, [u8; 32]), String> {
        Err("no tree key cache".into())
    }
    fn on_change(&self, _f: Box<dyn Fn() + Send + Sync>) {}
}

/// The process's opener: the active account's [`TreeKeys`] plus the
/// session → root map the ingest learns from the `sessions` shape (a
/// conversation names its session, a key belongs to the root).
pub struct Opener {
    keys: RwLock<Arc<dyn TreeKeys>>,
    roots: RwLock<HashMap<String, String>>,
    /// A digest held sealed content of a session whose root is unknown.
    unresolved: AtomicBool,
}

impl Opener {
    pub fn new(keys: Arc<dyn TreeKeys>) -> Self {
        Self { keys: RwLock::new(keys), roots: RwLock::new(HashMap::new()), unresolved: AtomicBool::new(false) }
    }

    /// The process-wide opener ([`NoKeys`] until [`Opener::set_keys`]).
    pub fn global() -> &'static Opener {
        static OPENER: OnceLock<Opener> = OnceLock::new();
        OPENER.get_or_init(|| Opener::new(Arc::new(NoKeys)))
    }

    /// Swap the key cache (sign-in / account switch). The caller re-opens
    /// the collections afterwards.
    pub fn set_keys(&self, keys: Arc<dyn TreeKeys>) {
        *self.keys.write().unwrap_or_else(|p| p.into_inner()) = keys;
    }

    pub fn keys(&self) -> Arc<dyn TreeKeys> {
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

    fn lookup(&self, root_id: Option<&str>) -> impl Fn(u32) -> Option<[u8; 32]> + '_ {
        let keys = self.keys();
        let root = root_id.map(str::to_string);
        move |epoch| root.as_deref().and_then(|root| keys.secret(root, epoch))
    }

    /// Opens a `messages` row in place (`root_id` names its tree).
    pub fn open_message(&self, row: &mut MessageRow) {
        let keys = self.lookup(row.root_id.as_deref());
        row.seal = open_parts(&mut row.body, &mut row.card, &keys);
    }

    /// Opens a `sessions` row's sealed title into `title`.
    pub fn open_session(&self, row: &mut SessionRow) {
        let root = row.root_id_or_self().to_string();
        self.remember_root(&row.id, &root);
        row.title_seal = None;
        let Some(sealed) = row.sealed_title.as_deref().filter(|t| t.starts_with(SEALED_BODY_PREFIX)) else {
            return;
        };
        let opened = seal::open_body(sealed, &self.lookup(Some(&root)));
        row.title_seal = SealState::from_status(opened.status.as_str());
        if let Some(text) = opened.text.filter(|_| opened.status == OpenStatus::Opened) {
            row.title = Some(text);
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
        let keys = self.lookup(root.as_deref());
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
            let state = open_parts(&mut body, &mut card, &keys);
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

/// Opens one message's body + card in place; `None` = nothing was sealed.
/// Not opened = an empty body and the card's clear part.
fn open_parts(body: &mut Option<String>, card: &mut Option<Value>, keys: &dyn Fn(u32) -> Option<[u8; 32]>) -> Option<SealState> {
    let mut state = None;
    if let Some(text) = body.as_deref().filter(|b| b.starts_with(SEALED_BODY_PREFIX)) {
        let opened = seal::open_body(text, keys);
        let body_state = SealState::from_status(opened.status.as_str());
        *body = Some(opened.text.filter(|_| opened.status == OpenStatus::Opened).unwrap_or_default());
        state = worst(state, body_state);
    }
    if let Some(value) = card.as_ref().filter(|c| c.get("sealed").is_some()) {
        let opened = seal::open_card(value, keys);
        *card = Some(opened.card);
        state = worst(state, SealState::from_status(opened.status.as_str()));
    }
    state
}

/// The writer's half: `text` sealed for `root_id`'s tree, or `None` when
/// this install cannot seal it (no key cache, offline): the caller sends
/// plaintext, which stays legal. BLOCKING.
pub fn seal_text_for(root_id: &str, text: &str) -> Option<String> {
    let keys = Opener::global().keys();
    let (epoch, public) = match keys.writer_key(root_id) {
        Ok(key) => key,
        Err(err) => {
            log::debug!("[sealed] no writer key for {root_id}: {err}");
            return None;
        }
    };
    seal::seal_body(&public, epoch, text).map_err(|err| log::warn!("[sealed] seal failed: {err}")).ok()
}

/// The writer's half for a card (an answer re-seals the FULL card). BLOCKING.
pub fn seal_card_for(root_id: &str, card: &Value) -> Option<Value> {
    let keys = Opener::global().keys();
    let (epoch, public) = keys.writer_key(root_id).ok()?;
    seal::seal_card(&public, epoch, card).map_err(|err| log::warn!("[sealed] seal failed: {err}")).ok()
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
    struct FixtureKeys(Vec<(u32, [u8; 32])>, [u8; 32]);
    impl TreeKeys for FixtureKeys {
        fn secret(&self, root_id: &str, epoch: u32) -> Option<[u8; 32]> {
            (root_id == "root").then(|| self.0.iter().find(|(e, _)| *e == epoch).map(|(_, s)| *s)).flatten()
        }
        fn writer_key(&self, _root_id: &str) -> Result<(u32, [u8; 32]), String> {
            Ok((1, self.1))
        }
        fn on_change(&self, _f: Box<dyn Fn() + Send + Sync>) {}
    }

    fn opener(fx: &Value) -> Opener {
        let keys = fx["keys"].as_object().unwrap().iter().map(|(k, v)| (k.parse().unwrap(), hex32(v))).collect();
        let public = seal::public_key_from_b64(fx["tree"]["publicKey"].as_str().unwrap()).unwrap();
        Opener::new(Arc::new(FixtureKeys(keys, public)))
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
            sealed_title: Some(envelope.into()),
            ..SessionRow::default()
        };
        opener.open_session(&mut session);
        assert_eq!(session.title.as_deref(), fx["body"]["text"].as_str());
        assert_eq!(session.title_seal, Some(SealState::Opened));

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

    #[test]
    fn writers_seal_for_the_tree_and_readers_open_it() {
        let fx = fixture();
        let opener = opener(&fx);
        let keys = opener.keys();
        let (epoch, public) = keys.writer_key("root").unwrap();
        let body = seal::seal_body(&public, epoch, "hello @a@b.c").unwrap();
        let mut row = message(&body, None);
        opener.open_message(&mut row);
        assert_eq!((row.body.as_deref(), row.seal), (Some("hello @a@b.c"), Some(SealState::Opened)));
    }
}
