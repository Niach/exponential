//! Sealed session content (EXP-1458; the spec is
//! `packages/domain-contract/fixtures/sealed-content.json`, its TS reference
//! `packages/domain-contract/src/sealed-content.ts`).
//!
//! Each session tree has one X25519 keypair per epoch. Content is
//! [`push_seal`](crate::push_seal)`(tree_public, plaintext)`; the tree secret
//! reaches every reader install wrapped with the same primitive.
//!
//! ```text
//! body  = "exp-sealed:" sealed-value            (messages.body, sessions.sealed_title)
//! card  = { ...clear, "sealed": sealed-value }  (messages.card)
//! sealed-value = "v1:" epoch ":" base64url(push_seal(tree_public, utf8(plaintext)))
//! ```
//!
//! A body without the prefix is plaintext. Base64url is unpadded and
//! canonical. Available without the `node` feature (like [`crate::push`]).

use data_encoding::BASE64URL_NOPAD;
use serde_json::{Map, Value};

use crate::error::{NetError, Result};
use crate::push::{push_open, push_seal_with, PushKeypair, PUSH_OVERHEAD};

/// The body envelope's prefix.
pub const SEALED_BODY_PREFIX: &str = "exp-sealed:";
/// The only sealed-value version this build reads and writes.
pub const SEALED_VERSION: &str = "v1";
/// Epochs are 1-based and fit Postgres `int`.
pub const SEAL_EPOCH_MAX: u32 = i32::MAX as u32;
/// A wrapped tree secret: `push_seal(install_public, secret[32])`.
pub const WRAPPED_TREE_KEY_BYTES: usize = PUSH_OVERHEAD + 32;
/// The `messages.card` fields that stay in clear; everything else is sealed.
/// `pictures` stays in clear PROJECTED (each entry's `label` and
/// `attachmentId` only) and is never laid over the opened card. `story`
/// (addendum 5) stays in clear only when a boolean (PR stories honour it).
pub const CARD_CLEAR_FIELDS: [&str; 13] = [
    "kind",
    "messageId",
    "sessionId",
    "issueId",
    "pullRequestId",
    "event",
    "status",
    "final",
    "decision",
    "fresh",
    "answered",
    "pictures",
    "story",
];

/// The clear projection of a card's `pictures`: `{label, attachmentId}` per
/// entry that names an attachment, `{label}` per entry still to upload (the
/// server mints its upload and files it by label); `None` when none names
/// either.
pub fn clear_pictures(pictures: Option<&Value>) -> Option<Value> {
    let out: Vec<Value> = pictures?
        .as_array()?
        .iter()
        .filter_map(|p| {
            let p = p.as_object()?;
            let mut m = Map::new();
            for key in ["label", "attachmentId"] {
                if let Some(value) = p.get(key).and_then(Value::as_str) {
                    m.insert(key.into(), Value::String(value.into()));
                }
            }
            (!m.is_empty()).then_some(Value::Object(m))
        })
        .collect();
    (!out.is_empty()).then_some(Value::Array(out))
}

/// A sealed card's pictures uploaded AFTER it was sealed (the server filed
/// them on the clear `pictures` entry of their label): each sealed picture
/// without an `attachmentId` takes its label's clear id and, on a plan,
/// every `](upload:<label>)` embed points at the attachment. A sealed id is
/// never replaced.
pub fn resolve_clear_pictures(card: &mut Map<String, Value>, clear: Option<&Value>) {
    let mut uploaded: Vec<(&str, &str)> = Vec::new();
    for entry in clear.and_then(Value::as_array).into_iter().flatten() {
        if let (Some(label), Some(id)) =
            (entry.get("label").and_then(Value::as_str), entry.get("attachmentId").and_then(Value::as_str))
        {
            if !uploaded.iter().any(|(known, _)| *known == label) {
                uploaded.push((label, id));
            }
        }
    }
    if uploaded.is_empty() {
        return;
    }
    let mut resolved: Vec<(String, String)> = Vec::new();
    if let Some(pictures) = card.get_mut("pictures").and_then(Value::as_array_mut) {
        for picture in pictures.iter_mut().filter_map(Value::as_object_mut) {
            if picture.get("attachmentId").is_some_and(Value::is_string) {
                continue;
            }
            let Some(label) = picture.get("label").and_then(Value::as_str).map(str::to_string) else { continue };
            if let Some((_, id)) = uploaded.iter().find(|(known, _)| *known == label) {
                picture.insert("attachmentId".into(), Value::String((*id).into()));
                resolved.push((label, (*id).to_string()));
            }
        }
    }
    if card.get("kind").and_then(Value::as_str) != Some("plan") {
        return;
    }
    if let Some(Value::String(plan)) = card.get_mut("plan") {
        for (label, id) in resolved {
            *plan = plan.replace(&format!("](upload:{label})"), &format!("](/api/attachments/{id})"));
        }
    }
}

/// A tree keypair (`sessions.seal_public_key` = base64url of `public`).
#[derive(Clone)]
pub struct TreeKey {
    pub secret: [u8; 32],
    pub public: [u8; 32],
}

impl std::fmt::Debug for TreeKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TreeKey(public={})", self.public_key())
    }
}

impl TreeKey {
    pub fn from_secret(secret: [u8; 32]) -> Self {
        Self { secret, public: PushKeypair::from_secret(secret).public() }
    }
    /// base64url of the public key (43 chars).
    pub fn public_key(&self) -> String {
        BASE64URL_NOPAD.encode(&self.public)
    }
}

pub fn generate_tree_key() -> TreeKey {
    TreeKey::from_secret(PushKeypair::generate().secret())
}

fn random_eph() -> [u8; 32] {
    PushKeypair::generate().secret()
}

/// Strict base64url (no padding, canonical trailing bits).
pub fn b64url_decode(text: &str) -> Option<Vec<u8>> {
    BASE64URL_NOPAD.decode(text.as_bytes()).ok()
}

pub fn b64url_encode(bytes: &[u8]) -> String {
    BASE64URL_NOPAD.encode(bytes)
}

/// A 32-byte public key given as base64url.
pub fn public_key_from_b64(text: &str) -> Result<[u8; 32]> {
    b64url_decode(text)
        .and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok())
        .ok_or_else(|| NetError::Invalid("public key must be base64url of 32 bytes".into()))
}

// --- tree key wraps ------------------------------------------------------

pub(crate) fn wrap_tree_key_with(eph: [u8; 32], install_public: &[u8; 32], tree_secret: &[u8; 32]) -> Result<String> {
    Ok(b64url_encode(&push_seal_with(eph, install_public, tree_secret)?))
}

/// `session_keys.wrapped` for one reader install (its `installs.x25519_public`).
pub fn wrap_tree_key(install_public: &[u8; 32], tree_secret: &[u8; 32]) -> Result<String> {
    wrap_tree_key_with(random_eph(), install_public, tree_secret)
}

/// The tree secret from a `session_keys.wrapped` value.
pub fn unwrap_tree_key(install_secret: &[u8; 32], wrapped: &str) -> Result<[u8; 32]> {
    let bytes = b64url_decode(wrapped)
        .filter(|b| b.len() == WRAPPED_TREE_KEY_BYTES)
        .ok_or_else(|| NetError::Invalid("malformed wrapped tree key".into()))?;
    let secret = push_open(install_secret, &bytes)?;
    secret.as_slice().try_into().map_err(|_| NetError::Crypto("wrapped tree key is not 32 bytes".into()))
}

// --- sealed values -------------------------------------------------------

/// A parsed `v1:<epoch>:<payload>` (or what it is instead).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SealedValue {
    Sealed { epoch: u32, sealed: Vec<u8> },
    Unsupported { version: String },
    Malformed,
}

pub fn parse_sealed_value(value: &str) -> SealedValue {
    let (version, rest) = match value.split_once(':') {
        Some((v, rest)) => (v, Some(rest)),
        None => (value, None),
    };
    if version != SEALED_VERSION {
        return SealedValue::Unsupported { version: version.to_string() };
    }
    let Some((epoch, payload)) = rest.and_then(|r| r.split_once(':')) else {
        return SealedValue::Malformed;
    };
    let canonical = !epoch.is_empty()
        && epoch.len() <= 10
        && epoch.bytes().all(|b| b.is_ascii_digit())
        && !epoch.starts_with('0');
    let Some(epoch) = epoch.parse::<u64>().ok().filter(|e| canonical && *e >= 1 && *e <= SEAL_EPOCH_MAX as u64) else {
        return SealedValue::Malformed;
    };
    match b64url_decode(payload) {
        Some(sealed) if sealed.len() >= PUSH_OVERHEAD => SealedValue::Sealed { epoch: epoch as u32, sealed },
        _ => SealedValue::Malformed,
    }
}

fn sealed_value(epoch: u32, sealed: &[u8]) -> Result<String> {
    if epoch == 0 || epoch > SEAL_EPOCH_MAX {
        return Err(NetError::Invalid(format!("epoch must be 1..={SEAL_EPOCH_MAX}")));
    }
    Ok(format!("{SEALED_VERSION}:{epoch}:{}", b64url_encode(sealed)))
}

/// What opening sealed content gave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenStatus {
    /// Not sealed (no prefix / no `sealed` field).
    Plain,
    Opened,
    /// No secret for its epoch (yet).
    NoKey,
    /// A newer envelope version.
    Unsupported,
    /// Not an envelope (bad epoch, base64, length).
    Malformed,
    /// The secret did not authenticate it (or the content is invalid).
    Refused,
}

impl OpenStatus {
    /// The fixture / TS spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            OpenStatus::Plain => "plain",
            OpenStatus::Opened => "opened",
            OpenStatus::NoKey => "noKey",
            OpenStatus::Unsupported => "unsupported",
            OpenStatus::Malformed => "malformed",
            OpenStatus::Refused => "refused",
        }
    }
}

fn open_value(value: SealedValue, keys: &dyn Fn(u32) -> Option<[u8; 32]>) -> (OpenStatus, Option<u32>, Option<Vec<u8>>) {
    match value {
        SealedValue::Unsupported { .. } => (OpenStatus::Unsupported, None, None),
        SealedValue::Malformed => (OpenStatus::Malformed, None, None),
        SealedValue::Sealed { epoch, sealed } => match keys(epoch) {
            None => (OpenStatus::NoKey, Some(epoch), None),
            Some(secret) => match push_open(&secret, &sealed) {
                Ok(bytes) => (OpenStatus::Opened, Some(epoch), Some(bytes)),
                Err(_) => (OpenStatus::Refused, Some(epoch), None),
            },
        },
    }
}

// --- bodies --------------------------------------------------------------

/// A parsed body: plaintext or a sealed value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyEnvelope {
    Plain(String),
    Value(SealedValue),
}

pub fn parse_body_envelope(body: &str) -> BodyEnvelope {
    match body.strip_prefix(SEALED_BODY_PREFIX) {
        None => BodyEnvelope::Plain(body.to_string()),
        Some(rest) => BodyEnvelope::Value(parse_sealed_value(rest)),
    }
}

pub(crate) fn seal_body_with(eph: [u8; 32], tree_public: &[u8; 32], epoch: u32, text: &str) -> Result<String> {
    let sealed = push_seal_with(eph, tree_public, text.as_bytes())?;
    Ok(format!("{SEALED_BODY_PREFIX}{}", sealed_value(epoch, &sealed)?))
}

/// `messages.body` / `sessions.sealed_title` sealed to the tree.
pub fn seal_body(tree_public: &[u8; 32], epoch: u32, text: &str) -> Result<String> {
    seal_body_with(random_eph(), tree_public, epoch, text)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenedBody {
    pub status: OpenStatus,
    pub epoch: Option<u32>,
    /// Set for `Plain` and `Opened` only.
    pub text: Option<String>,
}

/// Opens a body with the tree secrets this install holds (`keys(epoch)`).
pub fn open_body(body: &str, keys: &dyn Fn(u32) -> Option<[u8; 32]>) -> OpenedBody {
    let value = match parse_body_envelope(body) {
        BodyEnvelope::Plain(text) => return OpenedBody { status: OpenStatus::Plain, epoch: None, text: Some(text) },
        BodyEnvelope::Value(value) => value,
    };
    match open_value(value, keys) {
        (OpenStatus::Opened, epoch, Some(bytes)) => match String::from_utf8(bytes) {
            Ok(text) => OpenedBody { status: OpenStatus::Opened, epoch, text: Some(text) },
            Err(_) => OpenedBody { status: OpenStatus::Refused, epoch, text: None },
        },
        (status, epoch, _) => OpenedBody { status, epoch, text: None },
    }
}

// --- cards ---------------------------------------------------------------

fn answered(card: &Map<String, Value>) -> bool {
    card.get("answered") == Some(&Value::Bool(true))
        || card.get("answer").is_some_and(|v| !v.is_null())
        || card.get("answers").is_some_and(|v| !v.is_null())
}

/// The clear part (allowlisted fields that are set; `answered` only when
/// true) and the sealed plaintext: the JSON of the FULL card (minus a stale
/// `sealed`).
pub fn split_card(card: &Value) -> Result<(Map<String, Value>, String)> {
    let Some(obj) = card.as_object() else {
        return Err(NetError::Invalid("a card is a JSON object".into()));
    };
    if !obj.get("kind").is_some_and(Value::is_string) {
        return Err(NetError::Invalid("a card needs a kind".into()));
    }
    let mut full = obj.clone();
    full.remove("sealed");
    let mut clear = Map::new();
    for field in CARD_CLEAR_FIELDS {
        if field == "answered" {
            if answered(&full) {
                clear.insert(field.into(), Value::Bool(true));
            }
        } else if field == "pictures" {
            if let Some(pictures) = clear_pictures(full.get("pictures")) {
                clear.insert(field.into(), pictures);
            }
        } else if field == "story" {
            if let Some(v) = full.get(field).filter(|v| v.is_boolean()) {
                clear.insert(field.into(), v.clone());
            }
        } else if let Some(v) = full.get(field).filter(|v| !v.is_null()) {
            clear.insert(field.into(), v.clone());
        }
    }
    let plaintext = serde_json::to_string(&Value::Object(full)).map_err(|e| NetError::Invalid(e.to_string()))?;
    Ok((clear, plaintext))
}

pub(crate) fn seal_card_with(eph: [u8; 32], tree_public: &[u8; 32], epoch: u32, card: &Value) -> Result<Value> {
    let (mut clear, plaintext) = split_card(card)?;
    let sealed = push_seal_with(eph, tree_public, plaintext.as_bytes())?;
    clear.insert("sealed".into(), Value::String(sealed_value(epoch, &sealed)?));
    Ok(Value::Object(clear))
}

/// `messages.card` sealed to the tree: `{...clear, "sealed": "v1:<epoch>:<b64url>"}`.
pub fn seal_card(tree_public: &[u8; 32], epoch: u32, card: &Value) -> Result<Value> {
    seal_card_with(random_eph(), tree_public, epoch, card)
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenedCard {
    pub status: OpenStatus,
    pub epoch: Option<u32>,
    /// Plain / opened: the full card (the sealed card with the clear fields
    /// laid over it). Otherwise the clear part alone.
    pub card: Value,
}

pub fn open_card(card: &Value, keys: &dyn Fn(u32) -> Option<[u8; 32]>) -> OpenedCard {
    let Some(obj) = card.as_object() else {
        return OpenedCard { status: OpenStatus::Malformed, epoch: None, card: Value::Object(Map::new()) };
    };
    let mut clear = obj.clone();
    let sealed = match clear.remove("sealed") {
        None => return OpenedCard { status: OpenStatus::Plain, epoch: None, card: card.clone() },
        Some(Value::String(s)) => s,
        Some(_) => return OpenedCard { status: OpenStatus::Malformed, epoch: None, card: Value::Object(clear) },
    };
    let refused = |epoch, clear: Map<String, Value>| OpenedCard { status: OpenStatus::Refused, epoch, card: Value::Object(clear) };
    match open_value(parse_sealed_value(&sealed), keys) {
        (OpenStatus::Opened, epoch, Some(bytes)) => {
            let Ok(Value::Object(mut inner)) = serde_json::from_slice::<Value>(&bytes) else {
                return refused(epoch, clear);
            };
            if inner.get("kind") != clear.get("kind") {
                return refused(epoch, clear);
            }
            // The clear `pictures` is a projection: the sealed ones stay
            // whole, and only take the ids of pictures uploaded after sealing.
            let projected = clear.remove("pictures");
            for (k, v) in clear {
                inner.insert(k, v);
            }
            resolve_clear_pictures(&mut inner, projected.as_ref());
            OpenedCard { status: OpenStatus::Opened, epoch, card: Value::Object(inner) }
        }
        (status, epoch, _) => OpenedCard { status, epoch, card: Value::Object(clear) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/sealed-content.json")).unwrap()
    }
    fn hex32(v: &Value) -> [u8; 32] {
        data_encoding::HEXLOWER.decode(v.as_str().unwrap().as_bytes()).unwrap().try_into().unwrap()
    }
    fn keys(fx: &Value) -> impl Fn(u32) -> Option<[u8; 32]> {
        let map: Vec<(u32, [u8; 32])> =
            fx["keys"].as_object().unwrap().iter().map(|(k, v)| (k.parse().unwrap(), hex32(v))).collect();
        move |epoch| map.iter().find(|(e, _)| *e == epoch).map(|(_, s)| *s)
    }
    fn opt_u32(v: &Value) -> Option<u32> {
        v.as_u64().map(|n| n as u32)
    }

    #[test]
    fn replays_tree_keys_and_the_wrap() {
        let fx = fixture();
        assert_eq!(TreeKey::from_secret(hex32(&fx["tree"]["secret"])).public_key(), fx["tree"]["publicKey"]);
        assert_eq!(TreeKey::from_secret(hex32(&fx["rotatedTree"]["secret"])).public_key(), fx["rotatedTree"]["publicKey"]);
        let install = PushKeypair::from_secret(hex32(&fx["install"]["secret"]));
        assert_eq!(b64url_encode(&install.public()), fx["install"]["x25519Public"]);

        let w = &fx["wrap"];
        let install_public = public_key_from_b64(w["installPublic"].as_str().unwrap()).unwrap();
        let wrapped = wrap_tree_key_with(hex32(&w["ephemeralSecret"]), &install_public, &hex32(&w["treeSecret"])).unwrap();
        assert_eq!(wrapped, w["wrapped"].as_str().unwrap());
        assert_eq!(w["bytes"].as_u64().unwrap() as usize, WRAPPED_TREE_KEY_BYTES);
        assert_eq!(unwrap_tree_key(&install.secret(), &wrapped).unwrap(), hex32(&w["treeSecret"]));
        assert!(unwrap_tree_key(&hex32(&fx["tree"]["secret"]), &wrapped).is_err());
        assert!(unwrap_tree_key(&install.secret(), &wrapped[..wrapped.len() - 4]).is_err());

        let fresh = generate_tree_key();
        assert_eq!(fresh.public_key().len(), 43);
        let again = wrap_tree_key(&install.public(), &fresh.secret).unwrap();
        assert_eq!(unwrap_tree_key(&install.secret(), &again).unwrap(), fresh.secret);
    }

    #[test]
    fn replays_the_body_vector_and_cases() {
        let fx = fixture();
        let b = &fx["body"];
        let tree_public = public_key_from_b64(b["treePublicKey"].as_str().unwrap()).unwrap();
        let env = seal_body_with(hex32(&b["ephemeralSecret"]), &tree_public, opt_u32(&b["epoch"]).unwrap(), b["text"].as_str().unwrap()).unwrap();
        assert_eq!(env, b["envelope"].as_str().unwrap());
        let keys = keys(&fx);
        for case in fx["bodyCases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let got = open_body(case["body"].as_str().unwrap(), &keys);
            assert_eq!(got.status.as_str(), case["status"].as_str().unwrap(), "{name}");
            assert_eq!(got.epoch, opt_u32(&case["epoch"]), "{name}");
            assert_eq!(got.text.as_deref(), case["text"].as_str(), "{name}");
            let parsed = parse_body_envelope(case["body"].as_str().unwrap());
            match case["status"].as_str().unwrap() {
                "plain" => assert!(matches!(parsed, BodyEnvelope::Plain(_)), "{name}"),
                "unsupported" => assert_eq!(
                    parsed,
                    BodyEnvelope::Value(SealedValue::Unsupported { version: case["version"].as_str().unwrap().into() }),
                    "{name}"
                ),
                "malformed" => assert_eq!(parsed, BodyEnvelope::Value(SealedValue::Malformed), "{name}"),
                _ => assert!(matches!(parsed, BodyEnvelope::Value(SealedValue::Sealed { .. })), "{name}"),
            }
        }
        // A fresh seal opens with the tree secret.
        let tree = generate_tree_key();
        let own = |e: u32| (e == 3).then_some(tree.secret);
        let got = open_body(&seal_body(&tree.public, 3, "hi").unwrap(), &own);
        assert_eq!((got.status, got.epoch, got.text.as_deref()), (OpenStatus::Opened, Some(3), Some("hi")));
        assert!(seal_body(&tree.public, 0, "x").is_err());
    }

    #[test]
    fn replays_the_card_splits_and_cases() {
        let fx = fixture();
        let c = &fx["card"];
        let clear_fields: Vec<&str> = fx["cardClearFields"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(clear_fields, CARD_CLEAR_FIELDS);
        let tree_public = public_key_from_b64(c["treePublicKey"].as_str().unwrap()).unwrap();
        let keys = keys(&fx);
        for split in c["splits"].as_array().unwrap() {
            let name = split["name"].as_str().unwrap();
            let (clear, plaintext) = split_card(&split["card"]).unwrap();
            assert_eq!(Value::Object(clear.clone()), split["clear"], "{name}");
            // Sorted-key compact JSON: serde_json reproduces it byte-exact.
            assert_eq!(plaintext, split["plaintext"].as_str().unwrap(), "{name}");
            let env = seal_card_with(hex32(&c["ephemeralSecret"]), &tree_public, opt_u32(&c["epoch"]).unwrap(), &split["card"]).unwrap();
            assert_eq!(env, split["envelope"], "{name}");
            let opened = open_card(&split["envelope"], &keys);
            assert_eq!(opened.status, OpenStatus::Opened, "{name}");
            let mut want = split["card"].as_object().unwrap().clone();
            for (k, v) in clear.into_iter().filter(|(k, _)| k != "pictures") {
                want.insert(k, v);
            }
            assert_eq!(opened.card, Value::Object(want), "{name}");
        }
        for case in fx["cardCases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let got = open_card(&case["card"], &keys);
            assert_eq!(got.status.as_str(), case["status"].as_str().unwrap(), "{name}");
            assert_eq!(got.epoch, opt_u32(&case["epoch"]), "{name}");
            assert_eq!(got.card, case["opened"], "{name}");
        }
        let tree = generate_tree_key();
        let card = serde_json::json!({"kind": "report", "sessionId": "s", "summary": "done"});
        let sealed = seal_card(&tree.public, 4, &card).unwrap();
        assert!(sealed["sealed"].as_str().unwrap().starts_with("v1:4:"));
        assert!(sealed.get("summary").is_none());
        let opened = open_card(&sealed, &|e| (e == 4).then_some(tree.secret));
        assert_eq!(opened.card, card);
        assert!(split_card(&serde_json::json!({"summary": "no kind"})).is_err());
    }
}
