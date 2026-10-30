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
