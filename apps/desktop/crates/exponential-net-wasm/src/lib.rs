//! The wasm-bindgen facade over `exponential-net` (eNet) for the
//! Exponential web app. Browsers cannot open UDP sockets, so a browser node
//! is RELAY-ONLY: it reaches peers through the relay over a WebSocket, and
//! the traffic stays end-to-end encrypted (QUIC/TLS between the endpoints;
//! the relay forwards ciphertext). Blobs are kept in memory.
//!
//! ```js
//! import init, { WasmNode, generateIdentity } from "exponential-net-wasm";
//! await init();
//! const node = await WasmNode.start("https://relay.exponential.at", generateIdentity());
//! node.addTicket(desktopTicket);            // "enet1…"
//! node.allowPeer(desktopId);
//! node.onEvent((e) => console.log(e.kind, e));
//! await node.sendChat(desktopId, new TextEncoder().encode("hi"));
//! ```
//!
//! Events are plain objects with a `kind`: `chat {from, id, timestampMs,
//! body}`, `record {record}`, `callIncoming {call, from}`, `callAccepted
//! {call}`, `callFrame {call, seq, data}`, `callEnded {call, reason}`,
//! `peerRefused {peer, alpn}`. Byte fields are `Uint8Array`, call ids
//! numbers. Use `onEvent` OR `nextEvent`, not both.

use std::{rc::Rc, str::FromStr};

use enet_core as net;
use js_sys::{Array, Object, Reflect, Uint8Array};
use net::{EndpointId, Node};
use tokio::sync::mpsc::UnboundedReceiver;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;

type Events = Rc<tokio::sync::Mutex<UnboundedReceiver<net::Event>>>;

fn err(e: impl std::fmt::Display) -> JsValue {
    JsError::new(&e.to_string()).into()
}

fn peer(s: &str) -> Result<EndpointId, JsValue> {
    EndpointId::from_str(s.trim()).map_err(|e| err(format!("endpoint id {s}: {e}")))
}

fn bytes32(b: &[u8], what: &str) -> Result<[u8; 32], JsValue> {
    b.try_into().map_err(|_| err(format!("{what} must be 32 bytes, got {}", b.len())))
}

fn obj(fields: &[(&str, JsValue)]) -> JsValue {
    let o = Object::new();
    for (k, v) in fields {
        let _ = Reflect::set(&o, &JsValue::from_str(k), v);
    }
    o.into()
}

fn bytes(b: &[u8]) -> JsValue {
    Uint8Array::from(b).into()
}

fn record_js(r: &net::Record) -> JsValue {
    obj(&[
        ("space", bytes(&r.space)),
        ("key", JsValue::from_str(&r.key)),
        ("value", bytes(&r.value)),
        ("author", JsValue::from_str(&r.author.to_string())),
        ("clockMillis", JsValue::from_f64(r.clock.millis as f64)),
        ("clockCounter", JsValue::from_f64(r.clock.counter as f64)),
    ])
}

fn event_js(e: net::Event) -> JsValue {
    let kind = |k: &str| ("kind", JsValue::from_str(k));
    let call = |c: u64| ("call", JsValue::from_f64(c as f64));
    match e {
        net::Event::Chat { from, message } => obj(&[
            kind("chat"),
            ("from", JsValue::from_str(&from.to_string())),
            ("id", bytes(&message.id)),
            ("timestampMs", JsValue::from_f64(message.timestamp_ms as f64)),
            ("body", bytes(&message.body)),
        ]),
        net::Event::Record { record } => obj(&[kind("record"), ("record", record_js(&record))]),
        net::Event::CallIncoming { call: c, from } => {
            obj(&[kind("callIncoming"), call(c), ("from", JsValue::from_str(&from.to_string()))])
        }
        net::Event::CallAccepted { call: c } => obj(&[kind("callAccepted"), call(c)]),
        net::Event::CallFrame { call: c, seq, data } => {
            obj(&[kind("callFrame"), call(c), ("seq", JsValue::from_f64(seq as f64)), ("data", bytes(&data))])
        }
        net::Event::CallEnded { call: c, reason } => {
            let reason = match reason {
                net::CallEndReason::Local => "local".to_string(),
                net::CallEndReason::Remote => "remote".to_string(),
                net::CallEndReason::Failed(r) => format!("failed: {r}"),
            };
            obj(&[kind("callEnded"), call(c), ("reason", JsValue::from_str(&reason))])
        }
        net::Event::PeerRefused { peer, alpn } => obj(&[
            kind("peerRefused"),
            ("peer", JsValue::from_str(&peer.to_string())),
            ("alpn", JsValue::from_str(&String::from_utf8_lossy(&alpn))),
        ]),
    }
}

/// A browser eNet node (relay-only).
#[wasm_bindgen]
pub struct WasmNode {
    node: Rc<Node>,
    events: Events,
}

#[wasm_bindgen]
impl WasmNode {
    /// Starts a node homed on `relayUrl` with a 32-byte secret key.
    pub async fn start(relay_url: String, secret_key: Vec<u8>) -> Result<WasmNode, JsValue> {
        let identity = net::Identity::from_slice(&secret_key).map_err(err)?;
        let config = net::NetConfig { relay: net::RelayChoice::Custom(vec![relay_url]), ..Default::default() };
        let node = Node::start(config, identity).await.map_err(err)?;
        let events = node.events().expect("fresh node has its events");
        Ok(WasmNode { node: Rc::new(node), events: Rc::new(tokio::sync::Mutex::new(events)) })
    }

    #[wasm_bindgen(getter, js_name = endpointId)]
    pub fn endpoint_id(&self) -> String {
        self.node.endpoint_id().to_string()
    }

    /// `enet1…` (home relay; a browser has no direct addresses).
    pub fn ticket(&self) -> String {
        self.node.ticket().to_string()
    }

    /// Resolves once the relay connection is up (rejects after `timeoutMs`).
    pub fn online(&self, timeout_ms: u32) -> js_sys::Promise {
        let node = self.node.clone();
        future_to_promise(async move {
            node.online(std::time::Duration::from_millis(timeout_ms as u64)).await.map_err(err)?;
            Ok(JsValue::UNDEFINED)
        })
    }

    /// Learns a peer from its ticket; returns its endpoint id.
    #[wasm_bindgen(js_name = addTicket)]
    pub fn add_ticket(&self, ticket: String) -> Result<String, JsValue> {
        let t = net::Ticket::from_str(&ticket).map_err(err)?;
        self.node.add_ticket(&t);
        Ok(t.endpoint_id.to_string())
    }

    #[wasm_bindgen(js_name = allowPeer)]
    pub fn allow_peer(&self, id: String) -> Result<(), JsValue> {
        self.node.access().allow(peer(&id)?);
        Ok(())
    }

    #[wasm_bindgen(js_name = revokePeer)]
    pub fn revoke_peer(&self, id: String) -> Result<(), JsValue> {
        self.node.access().revoke(&peer(&id)?);
        Ok(())
    }

    #[wasm_bindgen(js_name = setOpen)]
    pub fn set_open(&self, open: bool) {
        self.node.access().set_open(open)
    }

    /// Calls `callback(event)` for every event, until shutdown.
    #[wasm_bindgen(js_name = onEvent)]
    pub fn on_event(&self, callback: js_sys::Function) {
        let events = self.events.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let mut rx = events.lock().await;
            while let Some(e) = rx.recv().await {
                let _ = callback.call1(&JsValue::NULL, &event_js(e));
            }
        });
    }

    /// The next event, or `undefined` after shutdown.
    #[wasm_bindgen(js_name = nextEvent)]
    pub fn next_event(&self) -> js_sys::Promise {
        let events = self.events.clone();
        future_to_promise(async move { Ok(events.lock().await.recv().await.map(event_js).unwrap_or(JsValue::UNDEFINED)) })
    }

    /// Sends a chat message; resolves `{id, timestampMs}` once acknowledged.
    #[wasm_bindgen(js_name = sendChat)]
    pub fn send_chat(&self, to: String, body: Vec<u8>) -> js_sys::Promise {
        let node = self.node.clone();
        future_to_promise(async move {
            let m = node.send_chat(peer(&to)?, body).await.map_err(err)?;
            Ok(obj(&[("id", bytes(&m.id)), ("timestampMs", JsValue::from_f64(m.timestamp_ms as f64))]))
        })
    }

    /// Subscribes to a 32-byte space; its records arrive as `record` events.
    #[wasm_bindgen(js_name = joinSpace)]
    pub fn join_space(&self, space: Vec<u8>, bootstrap: Vec<String>) -> js_sys::Promise {
        let node = self.node.clone();
        future_to_promise(async move {
            let space = bytes32(&space, "space")?;
            let bootstrap = bootstrap.iter().map(|p| peer(p)).collect::<Result<Vec<_>, _>>()?;
            node.join_space(space, bootstrap).await.map_err(err)?;
            Ok(JsValue::UNDEFINED)
        })
    }

    #[wasm_bindgen(js_name = putRecord)]
    pub fn put_record(&self, space: Vec<u8>, key: String, value: Vec<u8>) -> js_sys::Promise {
        let node = self.node.clone();
        future_to_promise(async move {
            let r = node.put_record(bytes32(&space, "space")?, key, value).await.map_err(err)?;
            Ok(record_js(&r))
        })
    }

    #[wasm_bindgen(js_name = getRecord)]
    pub fn get_record(&self, space: Vec<u8>, key: String) -> Result<JsValue, JsValue> {
        Ok(self.node.get_record(&bytes32(&space, "space")?, &key).map(|r| record_js(&r)).unwrap_or(JsValue::UNDEFINED))
    }

    pub fn records(&self, space: Vec<u8>) -> Result<Array, JsValue> {
        Ok(self.node.records(&bytes32(&space, "space")?).iter().map(record_js).collect())
    }

    /// Adds bytes to the in-memory blob store: `{hash, size, ticket}`.
    #[wasm_bindgen(js_name = blobAdd)]
    pub fn blob_add(&self, data: Vec<u8>) -> js_sys::Promise {
        let node = self.node.clone();
        future_to_promise(async move {
            let b = node.blob_add(data).await.map_err(err)?;
            Ok(obj(&[
                ("hash", bytes(&b.hash)),
                ("size", JsValue::from_f64(b.size as f64)),
                ("ticket", JsValue::from_str(&b.ticket)),
            ]))
        })
    }

    /// Fetches a blob (by iroh `blob…` ticket) into memory.
    #[wasm_bindgen(js_name = blobFetchTicket)]
    pub fn blob_fetch_ticket(&self, ticket: String) -> js_sys::Promise {
        let node = self.node.clone();
        future_to_promise(async move { Ok(bytes(&node.blob_fetch_ticket(&ticket).await.map_err(err)?)) })
    }

    pub fn shutdown(&self) -> js_sys::Promise {
        let node = self.node.clone();
        future_to_promise(async move {
            node.shutdown().await.map_err(err)?;
            Ok(JsValue::UNDEFINED)
        })
    }
}

/// 32 random identity bytes (keep them in IndexedDB, never on the server).
#[wasm_bindgen(js_name = generateIdentity)]
pub fn generate_identity() -> Vec<u8> {
    net::Identity::generate().to_bytes().to_vec()
}

/// The identity's ed25519 signature (64 bytes) over `message`: what the web
/// app's `installs.register` carries (EXP-1457).
#[wasm_bindgen(js_name = identitySign)]
pub fn identity_sign(identity: Vec<u8>, message: Vec<u8>) -> Result<Vec<u8>, JsValue> {
    Ok(net::Identity::from_slice(&identity).map_err(err)?.sign(&message).to_vec())
}

/// The X25519 push public key derived from an identity (register it with the push relay).
#[wasm_bindgen(js_name = pushPublicKey)]
pub fn push_public_key(identity: Vec<u8>) -> Result<Vec<u8>, JsValue> {
    let id = net::Identity::from_slice(&identity).map_err(err)?;
    Ok(net::PushKeypair::derive(&id).public().to_vec())
}

/// Seals a push payload to a recipient's X25519 public key.
#[wasm_bindgen(js_name = pushSeal)]
pub fn push_seal(recipient_public: Vec<u8>, plaintext: Vec<u8>) -> Result<Vec<u8>, JsValue> {
    net::push_seal(&bytes32(&recipient_public, "recipient public key")?, &plaintext).map_err(err)
}

/// Opens a sealed push payload with the recipient's X25519 secret.
#[wasm_bindgen(js_name = pushOpen)]
pub fn push_open(recipient_secret: Vec<u8>, sealed: Vec<u8>) -> Result<Vec<u8>, JsValue> {
    net::push_open(&bytes32(&recipient_secret, "recipient secret key")?, &sealed).map_err(err)
}

// --- sealed session content (EXP-1458) ----------------------------------
// fixtures/sealed-content.json is the spec (the web app's own reference is
// @exp/domain-contract/sealed-content; these are the same functions over
// enet-core). Cards cross as JSON strings, public keys and envelopes as
// base64url strings, secrets as Uint8Array; `keys` = [{epoch, secret}].

fn epoch_keys(keys: Array) -> Result<impl Fn(u32) -> Option<[u8; 32]>, JsValue> {
    let mut out = Vec::new();
    for key in keys.iter() {
        let epoch = Reflect::get(&key, &JsValue::from_str("epoch"))?
            .as_f64()
            .filter(|e| e.fract() == 0.0 && *e >= 1.0 && *e <= u32::MAX as f64)
            .ok_or_else(|| err("keys[].epoch must be a positive integer"))? as u32;
        let secret = Uint8Array::new(&Reflect::get(&key, &JsValue::from_str("secret"))?).to_vec();
        out.push((epoch, bytes32(&secret, "tree secret")?));
    }
    Ok(move |epoch| out.iter().find(|(e, _)| *e == epoch).map(|(_, s)| *s))
}

fn epoch_value(epoch: Option<u32>) -> JsValue {
    epoch.map(|e| JsValue::from_f64(e as f64)).unwrap_or(JsValue::NULL)
}

/// `{secret: Uint8Array, publicKey: string}` (publicKey = `sessions.seal_public_key`).
#[wasm_bindgen(js_name = generateTreeKey)]
pub fn generate_tree_key() -> JsValue {
    let key = net::sealed::generate_tree_key();
    obj(&[
        ("secret", Uint8Array::from(&key.secret[..]).into()),
        ("publicKey", JsValue::from_str(&key.public_key())),
    ])
}

/// `session_keys.wrapped` for a reader install (its base64url x25519 public).
#[wasm_bindgen(js_name = wrapTreeKey)]
pub fn wrap_tree_key(install_public: String, tree_secret: Vec<u8>) -> Result<String, JsValue> {
    let public = net::sealed::public_key_from_b64(&install_public).map_err(err)?;
    net::sealed::wrap_tree_key(&public, &bytes32(&tree_secret, "tree secret")?).map_err(err)
}

#[wasm_bindgen(js_name = unwrapTreeKey)]
pub fn unwrap_tree_key(install_secret: Vec<u8>, wrapped: String) -> Result<Vec<u8>, JsValue> {
    Ok(net::sealed::unwrap_tree_key(&bytes32(&install_secret, "install secret")?, &wrapped).map_err(err)?.to_vec())
}

/// `exp-sealed:v1:<epoch>:<b64url>` (a message body or a sealed title).
#[wasm_bindgen(js_name = sealBody)]
pub fn seal_body(tree_public_key: String, epoch: u32, text: String) -> Result<String, JsValue> {
    let public = net::sealed::public_key_from_b64(&tree_public_key).map_err(err)?;
    net::sealed::seal_body(&public, epoch, &text).map_err(err)
}

/// `{status, epoch, text}` (status: plain, opened, noKey, unsupported, malformed, refused).
#[wasm_bindgen(js_name = openBody)]
pub fn open_body(body: String, keys: Array) -> Result<JsValue, JsValue> {
    let opened = net::sealed::open_body(&body, &epoch_keys(keys)?);
    Ok(obj(&[
        ("status", JsValue::from_str(opened.status.as_str())),
        ("epoch", epoch_value(opened.epoch)),
        ("text", opened.text.map(|t| JsValue::from_str(&t)).unwrap_or(JsValue::NULL)),
    ]))
}

/// The card envelope as JSON: `{...clear, "sealed": "v1:…"}`.
#[wasm_bindgen(js_name = sealCard)]
pub fn seal_card(tree_public_key: String, epoch: u32, card_json: String) -> Result<String, JsValue> {
    let public = net::sealed::public_key_from_b64(&tree_public_key).map_err(err)?;
    let card: serde_json::Value = serde_json::from_str(&card_json).map_err(err)?;
    Ok(net::sealed::seal_card(&public, epoch, &card).map_err(err)?.to_string())
}

/// `{status, epoch, cardJson}`: the full card (plain / opened) or the clear part.
#[wasm_bindgen(js_name = openCard)]
pub fn open_card(card_json: String, keys: Array) -> Result<JsValue, JsValue> {
    let card: serde_json::Value = serde_json::from_str(&card_json).map_err(err)?;
    let opened = net::sealed::open_card(&card, &epoch_keys(keys)?);
    Ok(obj(&[
        ("status", JsValue::from_str(opened.status.as_str())),
        ("epoch", epoch_value(opened.epoch)),
        ("cardJson", JsValue::from_str(&opened.card.to_string())),
    ]))
}
