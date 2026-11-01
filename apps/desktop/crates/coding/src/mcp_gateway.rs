//! EXP-1458: the device MCP gateway's TRANSFORM (the loopback route itself
//! is `codemode::gateway`). A launched session's agent reaches
//! Exponential's MCP through it, so content leaves the device sealed and
//! comes back opened (`fixtures/sealed-content.json`):
//!
//! SEAL (the agent's arguments, before the upstream sees them):
//! - `exponential_sessions_send {text, name}`: `text` = a body envelope for
//!   the TARGET tree (the caller's own for a child start or an own-tree
//!   session, another tree's current key for a session elsewhere, a fresh
//!   key riding as `seal` for a new thread), `name` → `sealedName`.
//!   `{answer}` re-seals the answered card as `sealedCard`.
//! - `exponential_show`: the card is BUILT here (the port of
//!   `lib/sessions/plan.ts` buildPlanCard / buildQuestionCard /
//!   buildSurfaceCard and the show tool's checks) and sent as
//!   `{sealedCard, kind, replace?, story?}`.
//! - `exponential_sessions {op: "end", summary}` → `{sealedReport, copies}`.
//! - `exponential_sessions {op: "escalate"}` → `+ sealedCard` for the target.
//! - `exponential_session_self {op: "suggest"}` → `{sealed}`.
//!
//! - `exponential_sessions_send {agentId|appId}`: sealed to the caller's
//!   conversation with it (`sessions.sendTarget`: its root's key, else a
//!   fresh key as `seal`; an app's brief composed in).
//!
//! EXP-1472: content that cannot be sealed is NEVER sent in clear while
//! this process has a key holder: the call fails with the reason (another
//! tree without a key yet is first waited for, `sealing::SEND_KEY_WAIT`).
//! Only a process without one (eNet off, signed out) sends as given, and
//! the server refuses that for a keyed tree.
//!
//! OPEN (every result): body envelopes, card envelopes and sealed titles
//! anywhere in the result's JSON are opened; what this install cannot open
//! reads [`sealing::LOCKED_TEXT`].

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use api::trpc::TrpcClient;
use codemode::{GatewayTransform, GatewayUpstream};
use domain::rows::MessageRow;
use serde_json::{json, Map, Value};

use crate::sealing::{self, DeliverTo, NewRootSeal, OpenStatus, SealError, SEALED_BODY_PREFIX};

pub const SEND_TOOL: &str = "exponential_sessions_send";
pub const SHOW_TOOL: &str = "exponential_show";
pub const SESSIONS_TOOL: &str = "exponential_sessions";
pub const SELF_TOOL: &str = "exponential_session_self";

/// The transform of ONE launched session (the caller of every call).
pub struct SealingTransform {
    session_id: String,
    root_id: String,
    trpc: Option<Arc<TrpcClient>>,
    /// New-root seals sent and not yet adopted, by public key.
    new_roots: Mutex<HashMap<String, NewRootSeal>>,
}

impl SealingTransform {
    /// `root_id` = the session's root (itself for a root).
    pub fn new(session_id: &str, root_id: &str, trpc: Option<Arc<TrpcClient>>) -> Self {
        let root_id = if root_id.is_empty() { session_id } else { root_id };
        if let Some(service) = sealing::service() {
            service.keyring().note_root(session_id, root_id);
            service.keyring().note_root(root_id, root_id);
        }
        Self { session_id: session_id.to_string(), root_id: root_id.to_string(), trpc, new_roots: Mutex::default() }
    }

    fn root_for(&self, session_id: &str) -> Option<String> {
        if session_id == self.session_id || session_id == self.root_id {
            return Some(self.root_id.clone());
        }
        sealing::root_of(session_id)
    }

    /// A body for `root`: the own tree may create its key, another tree is
    /// sealed with its current key only.
    fn seal_text(&self, root: &str, text: &str) -> Result<String, SealError> {
        if root == self.root_id {
            sealing::seal_body(root, text)
        } else {
            sealing::seal_body_for_tree(root, text)
        }
    }

    /// A card for `root` (`{...clear, sealed}`); its clear `pictures` is
    /// the contract's PROJECTION: `{label, attachmentId}` per uploaded
    /// picture (the server claims it), `{label}` per picture still to
    /// upload (the server mints its upload and files it there).
    fn seal_card(&self, root: &str, card: &Value) -> Result<Value, SealError> {
        if root == self.root_id {
            sealing::seal_card(root, card)
        } else {
            sealing::seal_card_for_tree(root, card)
        }
    }

    /// A body envelope sealed with each listener tree's current key
    /// (`sessions.listeners`; keyless trees are skipped).
    fn listener_copies(&self, session_id: &str, own_root: &str, text: &str) -> Vec<(String, String)> {
        self.listeners(session_id)
            .into_iter()
            .filter(|listener| listener.root_id != own_root)
            .filter_map(|listener| {
                let public = sealing::public_key_from_b64(listener.public_key.as_deref()?).ok()?;
                let body = enet_core::sealed::seal_body(&public, listener.epoch?, text).ok()?;
                Some((listener.root_id, body))
            })
            .collect()
    }

    // --- seal ------------------------------------------------------------

    fn seal_send(&self, mut args: Value, upstream: Option<&dyn GatewayUpstream>) -> Result<Value, String> {
        // The issues a sealed text names (`#IDENT`), resolved before sealing
        // (the server reads only the clear ids).
        let refs = match (upstream, args.get("text").and_then(Value::as_str)) {
            (Some(upstream), Some(text)) if args.get("refIssueIds").is_none() && !text.starts_with(SEALED_BODY_PREFIX) => {
                resolve_issue_refs(upstream, text)
            }
            _ => Vec::new(),
        };
        let Some(obj) = args.as_object_mut() else { return Ok(args) };
        if obj.get("stop") == Some(&Value::Bool(true)) {
            return Ok(args);
        }
        let text = obj.get("text").and_then(Value::as_str).filter(|t| !t.is_empty()).map(str::to_string);
        let name = obj.get("name").and_then(Value::as_str).filter(|t| !t.trim().is_empty()).map(str::to_string);
        let text = text.filter(|t| !t.starts_with(SEALED_BODY_PREFIX));
        if text.is_none() && name.is_none() {
            return Ok(args);
        }
        let target = obj.get("sessionId").or_else(|| obj.get("parentId")).and_then(Value::as_str).map(str::to_string);
        let other = obj.get("agentId").or_else(|| obj.get("appId")).is_some();
        let sealed = match (target, other) {
            (Some(target), _) => {
                let Some(root) = self.root_for(&target) else {
                    unsealed("sessions_send", &no_root())?;
                    return Ok(args);
                };
                let seal = |value: &Option<String>| value.as_deref().map(|v| self.seal_text(&root, v)).transpose();
                match (seal(&text), seal(&name)) {
                    (Ok(text), Ok(name)) => (text, name, None),
                    (Err(err), _) | (_, Err(err)) => {
                        unsealed("sessions_send", &err)?;
                        return Ok(args);
                    }
                }
            }
            (None, true) => {
                // The caller's conversation with an agent / editor session
                // with an app (`sessions.sendTarget`): sealed to its key, or
                // with a fresh key riding as `seal` (none yet / keyless).
                let target = match self.send_target(obj) {
                    Ok(target) => target,
                    Err(err) => {
                        unsealed("sessions_send", &err)?;
                        return Ok(args);
                    }
                };
                // A sealed text is the writer's whole message: an app's
                // brief is composed in here (`briefedText`).
                let text = match (target.brief.as_deref().map(str::trim).filter(|b| !b.is_empty()), text) {
                    (Some(brief), Some(text)) => Some([brief, text.trim()].into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join("\n\n")),
                    (_, text) => text,
                };
                let Some(text) = text else { return Ok(args) };
                let keyed = target.root_id.as_deref().zip(target.epoch).zip(target.public_key.as_deref());
                match keyed {
                    Some(((root, epoch), key)) => {
                        let public = sealing::public_key_from_b64(key).map_err(|e| refused("sessions_send", &e))?;
                        if let Some(service) = sealing::service() {
                            service.keyring().set_current(root, epoch, public);
                        }
                        let body = enet_core::sealed::seal_body(&public, epoch, &text).map_err(|e| refused("sessions_send", &e))?;
                        (Some(body), None, None)
                    }
                    None => {
                        let seal = match self.new_root_seal() {
                            Ok(seal) => seal,
                            Err(err) => {
                                unsealed("sessions_send", &err)?;
                                return Ok(args);
                            }
                        };
                        let public = sealing::public_key_from_b64(&seal.public_key).map_err(|e| refused("sessions_send", &e))?;
                        let body = enet_core::sealed::seal_body(&public, seal.epoch, &text).map_err(|e| refused("sessions_send", &e))?;
                        (Some(body), None, Some(seal))
                    }
                }
            }
            (None, false) => match self.new_root_seal() {
                Err(err) => {
                    unsealed("a new thread", &err)?;
                    return Ok(args);
                }
                Ok(seal) => {
                    let public = sealing::public_key_from_b64(&seal.public_key).map_err(|e| refused("a new thread", &e))?;
                    let seal_one = |value: &Option<String>| {
                        value.as_deref().map(|v| enet_core::sealed::seal_body(&public, seal.epoch, v)).transpose()
                    };
                    match (seal_one(&text), seal_one(&name)) {
                        (Ok(text), Ok(name)) => (text, name, Some(seal)),
                        (Err(err), _) | (_, Err(err)) => return Err(refused("a new thread", &err)),
                    }
                }
            },
        };
        let (text, name, seal) = sealed;
        if let Some(text) = text {
            obj.insert("text".into(), Value::String(text));
            if !refs.is_empty() {
                obj.insert("refIssueIds".into(), json!(refs));
            }
        }
        if let Some(name) = name {
            obj.remove("name");
            obj.insert("sealedName".into(), Value::String(name));
        }
        if let Some(seal) = seal {
            obj.insert("seal".into(), serde_json::to_value(&seal).unwrap_or(Value::Null));
            self.new_roots.lock().unwrap_or_else(|p| p.into_inner()).insert(seal.public_key.clone(), seal);
        }
        Ok(args)
    }

    /// tRPC `sessions.sendTarget {agentId|appId}`: the conversation root a
    /// send to an agent / app lands in, its key and an app's brief.
    fn send_target(&self, args: &Map<String, Value>) -> Result<SendTarget, SealError> {
        sealing::service().ok_or(SealError::NoService)?;
        let Some(trpc) = &self.trpc else {
            return Err(SealError::NoKey("no tRPC client to look its chat up".into()));
        };
        let mut query = Map::new();
        for key in ["agentId", "appId"] {
            if let Some(id) = args.get(key).filter(|v| v.is_string()) {
                query.insert(key.into(), id.clone());
            }
        }
        trpc.query_with_input::<_, SendTarget>("sessions.sendTarget", &Value::Object(query))
            .map_err(|err| SealError::NoKey(format!("sessions.sendTarget failed: {err}")))
    }

    /// A fresh tree key for a new thread, wrapped for this install (its
    /// other readers get theirs through `pending`).
    fn new_root_seal(&self) -> Result<NewRootSeal, SealError> {
        let service = sealing::service().ok_or(SealError::NoService)?;
        let (Some(install_id), Some(public)) = (service.install_id(), service.install_public()) else {
            return Err(SealError::NoKey("this install cannot wrap a tree key yet".into()));
        };
        sealing::seal_new_root(&install_id, &public, &[])
    }

    /// `sessions_send {sessionId, answer}`: the target's open card read
    /// upstream, answered here and re-sealed whole; the plaintext answer
    /// never leaves (the clear `answer` keeps only messageId / decision /
    /// fresh).
    fn seal_answer(&self, args: Value, upstream: &dyn GatewayUpstream) -> Result<Value, String> {
        let (Some(target), Some(answer)) =
            (args.get("sessionId").and_then(Value::as_str), args.get("answer").and_then(Value::as_object))
        else {
            return Ok(args);
        };
        let wanted = answer.get("messageId").and_then(Value::as_str);
        let Some((message, tree_root)) = self.find_card(upstream, Some(target), |m| match wanted {
            Some(id) => m.get("id").and_then(Value::as_str) == Some(id),
            None => {
                m.get("sessionId").and_then(Value::as_str) == Some(target)
                    && m.get("answeredAt").is_none_or(Value::is_null)
                    && matches!(card_kind(m), Some("question" | "plan"))
            }
        }) else {
            log::info!("gateway: the card {target}'s answer names was not readable; the answer goes as given");
            return Ok(args);
        };
        let Some(card) = message.get("card").filter(|c| c.get("sealed").is_some()) else {
            // A plaintext card: the server answers it as before.
            return Ok(args);
        };
        let root = self.root_for(target).unwrap_or(tree_root);
        let opened = open_card_value(&root, card);
        if opened.0 != OpenStatus::Opened {
            log::warn!("gateway: the card to answer could not be opened ({}); the answer goes as given", opened.0.as_str());
            return Ok(args);
        }
        let answered = answered_card(&opened.1, answer)?;
        let sealed = match self.seal_card(&root, &answered) {
            Ok(sealed) => sealed,
            Err(err) => {
                unsealed("an answer", &err)?;
                return Ok(args);
            }
        };
        // The server takes a sealed card's answer as `sealedCard` only: no
        // answer/answers/feedback/surfaceData beside it.
        let mut clear = Map::new();
        if let Some(id) = message.get("id").cloned() {
            clear.insert("messageId".into(), id);
        }
        for key in ["decision", "fresh"] {
            if let Some(value) = answer.get(key) {
                clear.insert(key.into(), value.clone());
            }
        }
        clear.insert("sealedCard".into(), sealed);
        let mut args = args;
        args["answer"] = Value::Object(clear);
        Ok(args)
    }

    /// The newest message of a session tree matching `pick` (the tree of
    /// `session`, the caller's own when `None`), read with `sessions get`,
    /// and the tree's root.
    fn find_card(
        &self,
        upstream: &dyn GatewayUpstream,
        session: Option<&str>,
        pick: impl Fn(&Value) -> bool,
    ) -> Option<(Value, String)> {
        let mut query = json!({ "op": "get", "messages": 100 });
        if let Some(session) = session {
            query["id"] = json!(session);
        }
        let result = upstream.call(SESSIONS_TOOL, query).ok()?;
        let tree = result_json(&result)?;
        let root = tree.pointer("/root/id").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| self.root_id.clone());
        self.learn(&tree);
        let messages = tree.get("messages")?.as_array()?;
        messages.iter().rev().find(|m| pick(m)).map(|m| (m.clone(), root))
    }

    fn seal_show(&self, args: Value) -> Result<Value, String> {
        let input = ShowInput::parse(&args)?;
        let card = build_show_card(&input)?;
        // Pictures still to upload ride the clear part as `{label}`: the
        // result's upload curls file each on that clear entry (addendum 4).
        let sealed = match self.seal_card(&self.root_id, &card) {
            Ok(sealed) => sealed,
            Err(err) => {
                unsealed("exponential_show", &err)?;
                return Ok(strip_picture_ids(args));
            }
        };
        let mut out = json!({ "sealedCard": sealed, "kind": card["kind"] });
        if matches!(card["kind"].as_str(), Some("question" | "plan")) {
            // The ask mirrors' text: sealed for the own tree, one copy per
            // listening tree.
            let text = ask_text(&self.session_id, &card);
            if let Ok(body) = sealing::seal_body(&self.root_id, &text) {
                out["askText"] = json!(body);
            }
            let copies: Vec<Value> = self
                .listener_copies(&self.session_id, &self.root_id, &text)
                .into_iter()
                .map(|(root, body)| json!({ "rootId": root, "body": body }))
                .collect();
            if !copies.is_empty() {
                out["listenerCopies"] = json!(copies);
            }
        }
        if let Some(replace) = args.get("replace") {
            out["replace"] = replace.clone();
        }
        if let Some(story) = args.get("story") {
            out["story"] = story.clone();
        }
        Ok(out)
    }

    fn seal_end(&self, args: Value) -> Result<Value, String> {
        let Some(summary) = args.get("summary").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) else {
            return Ok(args);
        };
        let target = args.get("id").and_then(Value::as_str).unwrap_or(&self.session_id).to_string();
        let Some(root) = self.root_for(&target) else {
            unsealed("an end report", &no_root())?;
            return Ok(args);
        };
        let report = json!({ "kind": "report", "sessionId": target, "status": "ended", "final": true, "summary": summary });
        let sealed = match self.seal_card(&root, &report) {
            Ok(sealed) => sealed,
            Err(err) => {
                unsealed("an end report", &err)?;
                return Ok(args);
            }
        };
        let copies: Vec<Value> = self
            .listeners(&target)
            .into_iter()
            .filter(|listener| listener.root_id != root)
            .filter_map(|listener| {
                let public = sealing::public_key_from_b64(listener.public_key.as_deref()?).ok()?;
                let copy = enet_core::sealed::seal_card(&public, listener.epoch?, &report).ok()?;
                Some(json!({ "rootId": listener.root_id, "sealedReport": copy }))
            })
            .collect();
        let mut out = json!({ "op": "end", "sealedReport": sealed, "copies": copies });
        if let Some(id) = args.get("id") {
            out["id"] = id.clone();
        }
        Ok(out)
    }

    /// tRPC `sessions.listeners {sessionId}`: the wake_on listener trees.
    fn listeners(&self, session_id: &str) -> Vec<Listener> {
        let Some(trpc) = &self.trpc else { return Vec::new() };
        match trpc.query_with_input::<_, Vec<Listener>>("sessions.listeners", &json!({ "sessionId": session_id })) {
            Ok(listeners) => listeners,
            Err(err) => {
                log::warn!("gateway: sessions.listeners {session_id} failed, no listener copies: {err}");
                Vec::new()
            }
        }
    }

    fn seal_escalate(&self, args: Value, upstream: &dyn GatewayUpstream) -> Result<Value, String> {
        let Some(message_id) = args.get("messageId").and_then(Value::as_str).map(str::to_string) else { return Ok(args) };
        let Some(trpc) = &self.trpc else { return Ok(args) };
        let target = match trpc.query_with_input::<_, EscalateTarget>("sessions.escalateTarget", &json!({ "messageId": message_id })) {
            Ok(target) => target.root_id,
            Err(err) => {
                log::warn!("gateway: sessions.escalateTarget failed, the escalation goes as given: {err}");
                return Ok(args);
            }
        };
        let by_id = |id: String| move |m: &Value| m.get("id").and_then(Value::as_str) == Some(id.as_str());
        let Some((mut message, mut root)) = self.find_card(upstream, None, by_id(message_id.clone())) else {
            log::info!("gateway: the card {message_id} to escalate was not readable; it goes as given");
            return Ok(args);
        };
        // An ask mirror points at the card itself (possibly in a child's
        // session of another read).
        if card_kind(&message) == Some("ask") {
            let card = message.get("card").cloned().unwrap_or_default();
            let opened = open_card_value(&root, &card).1;
            let (Some(inner), session) = (
                opened.get("messageId").and_then(Value::as_str).map(str::to_string),
                opened.get("sessionId").and_then(Value::as_str).map(str::to_string),
            ) else {
                return Ok(args);
            };
            match self.find_card(upstream, session.as_deref(), by_id(inner)) {
                Some(found) => (message, root) = found,
                None => return Ok(args),
            }
        }
        let Some(card) = message.get("card").filter(|c| c.get("sealed").is_some()) else { return Ok(args) };
        if let Some(session) = message.get("sessionId").and_then(Value::as_str) {
            root = self.root_for(session).unwrap_or(root);
        }
        let opened = open_card_value(&root, card);
        if opened.0 != OpenStatus::Opened {
            log::warn!("gateway: the card to escalate could not be opened ({})", opened.0.as_str());
            return Ok(args);
        }
        match self.seal_card(&target, &opened.1) {
            Ok(sealed) => {
                let mut args = args;
                args["sealedCard"] = sealed;
                Ok(args)
            }
            Err(err) => {
                unsealed("an escalated card", &err)?;
                Ok(args)
            }
        }
    }

    /// `{op: "suggest", sealed}`: one card envelope per card (a
    /// `suggestions` card of the tasks, a `replies` card of the replies),
    /// built here like `postSuggestions` / `postReplies` build theirs.
    fn seal_suggest(&self, args: Value) -> Result<Value, String> {
        let cards = suggestion_cards(&args)?;
        if cards.is_empty() {
            return Ok(args);
        }
        let mut sealed = Vec::new();
        for card in &cards {
            match self.seal_card(&self.root_id, card) {
                Ok(envelope) => sealed.push(envelope),
                Err(err) => {
                    unsealed("suggestions", &err)?;
                    return Ok(args);
                }
            }
        }
        let sealed = if sealed.len() == 1 { sealed.remove(0) } else { Value::Array(sealed) };
        Ok(json!({ "op": "suggest", "sealed": sealed }))
    }

    // --- open ------------------------------------------------------------

    /// Learns session → root pairs from a result (`id` + `rootId`, a tree's
    /// root and children).
    fn learn(&self, value: &Value) {
        let Some(service) = sealing::service() else { return };
        let keyring = service.keyring();
        fn walk(value: &Value, keyring: &sealing::Keyring, depth: usize) {
            if depth > 32 {
                return;
            }
            match value {
                Value::Object(obj) => {
                    if let (Some(id), Some(root)) = (obj.get("id").and_then(Value::as_str), obj.get("rootId").and_then(Value::as_str)) {
                        keyring.note_root(id, root);
                    }
                    if let (Some(root), Some(children)) =
                        (obj.get("root").and_then(|r| r.get("id")).and_then(Value::as_str), obj.get("children").and_then(Value::as_array))
                    {
                        keyring.note_root(root, root);
                        for child in children.iter().filter_map(|c| c.get("id").and_then(Value::as_str)) {
                            keyring.note_root(child, root);
                        }
                    }
                    obj.values().for_each(|v| walk(v, keyring, depth + 1));
                }
                Value::Array(items) => items.iter().for_each(|v| walk(v, keyring, depth + 1)),
                _ => {}
            }
        }
        walk(value, keyring, 0);
    }

    /// Every sealed value in `value` opened; `inherited` = the root of the
    /// enclosing object.
    fn open_tree(&self, value: Value, inherited: &str, depth: usize) -> Value {
        if depth > 64 {
            return value;
        }
        match value {
            Value::String(text) if text.starts_with(SEALED_BODY_PREFIX) => {
                Value::String(sealing::open_body_text(inherited, &text).unwrap_or_else(|| sealing::LOCKED_TEXT.to_string()))
            }
            Value::Array(items) => Value::Array(items.into_iter().map(|v| self.open_tree(v, inherited, depth + 1)).collect()),
            Value::Object(obj) => {
                // A card: sealed for the tree of the row that holds it.
                if obj.contains_key("sealed") && obj.get("kind").is_some_and(Value::is_string) {
                    return open_card_for_agent(inherited, &Value::Object(obj));
                }
                let root = obj
                    .get("rootId")
                    .and_then(Value::as_str)
                    .filter(|r| !r.is_empty())
                    .map(str::to_string)
                    .or_else(|| {
                        let tree = obj.get("root").filter(|_| obj.contains_key("children"))?;
                        tree.get("id").and_then(Value::as_str).map(str::to_string)
                    })
                    .or_else(|| obj.get("sessionId").and_then(Value::as_str).and_then(|s| self.root_for(s)))
                    .unwrap_or_else(|| inherited.to_string());
                let mut out = Map::new();
                let mut title = None;
                for (key, value) in obj {
                    if key == "sealedTitle" || key == "sealed_title" {
                        if let Some(text) = value.as_str().filter(|t| !t.is_empty()) {
                            title = Some(sealing::open_body_text(&root, text));
                        }
                        continue;
                    }
                    out.insert(key, self.open_tree(value, &root, depth + 1));
                }
                // An unopenable title keeps the neutral plaintext fallback.
                if let Some(Some(text)) = title {
                    out.insert("title".into(), Value::String(text));
                }
                Value::Object(out)
            }
            other => other,
        }
    }

    /// The agent-to-agent hook and the new-root / child bookkeeping after a
    /// `sessions_send` the server accepted.
    fn after_send(&self, sent: &Value, out: &Value) {
        let Some(service) = sealing::service() else { return };
        let Some(target) = out.get("sessionId").and_then(Value::as_str) else { return };
        if let Some(public) = sent.pointer("/seal/publicKey").and_then(Value::as_str) {
            let seal = self.new_roots.lock().unwrap_or_else(|p| p.into_inner()).remove(public);
            if let Some(seal) = seal {
                service.adopt(target, &seal);
            }
        } else if let Some(parent) = sent.get("parentId").and_then(Value::as_str) {
            if let Some(root) = self.root_for(parent) {
                service.keyring().note_root(target, &root);
            }
        }
        if let Some(root) = out.get("rootId").and_then(Value::as_str) {
            service.keyring().note_root(target, root);
        }
        let deliver_to = out.get("deliverTo").filter(|v| !v.is_null()).and_then(|v| serde_json::from_value::<DeliverTo>(v.clone()).ok());
        let message_id = out.get("messageId").and_then(Value::as_str);
        let (Some(deliver_to), Some(message_id)) = (deliver_to, message_id) else { return };
        let message = MessageRow {
            id: message_id.to_string(),
            conversation_id: out.get("conversationId").and_then(Value::as_str).unwrap_or_default().to_string(),
            session_id: target.to_string(),
            root_id: out.get("rootId").and_then(Value::as_str).map(str::to_string).or_else(|| self.root_for(target)),
            author_kind: Some("session".to_string()),
            body: sent.get("text").and_then(Value::as_str).map(str::to_string),
            card: sent.get("sealedCard").cloned(),
            author_session_id: Some(self.session_id.clone()),
            created_at: Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
            ..Default::default()
        };
        service.deliver_direct(&deliver_to, &message);
    }
}

impl GatewayTransform for SealingTransform {
    fn request(&self, tool: &str, args: Value) -> Result<Value, String> {
        let op = args.get("op").and_then(Value::as_str).unwrap_or_default().to_string();
        match (tool, op.as_str()) {
            (SEND_TOOL, _) => self.seal_send(args, None),
            (SHOW_TOOL, _) => self.seal_show(args),
            (SESSIONS_TOOL, "end") => self.seal_end(args),
            (SELF_TOOL, "suggest") => self.seal_suggest(args),
            _ => Ok(args),
        }
    }

    fn request_with(&self, tool: &str, args: Value, upstream: &dyn GatewayUpstream) -> Result<Value, String> {
        let op = args.get("op").and_then(Value::as_str).unwrap_or_default().to_string();
        match (tool, op.as_str()) {
            (SEND_TOOL, _) => {
                let args = if args.get("answer").is_some_and(Value::is_object) && args.get("sessionId").is_some() {
                    self.seal_answer(args, upstream)?
                } else {
                    args
                };
                self.seal_send(args, Some(upstream))
            }
            (SESSIONS_TOOL, "escalate") => self.seal_escalate(args, upstream),
            _ => self.request(tool, args),
        }
    }

    fn result(&self, tool: &str, args_sent: &Value, mut result: Value) -> Value {
        let failed = result.get("isError") == Some(&Value::Bool(true));
        if let Some(items) = result.get_mut("content").and_then(Value::as_array_mut) {
            for item in items.iter_mut() {
                if item.get("type").and_then(Value::as_str) != Some("text") {
                    continue;
                }
                let Some(text) = item.get("text").and_then(Value::as_str).map(str::to_string) else { continue };
                if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
                    if !parsed.is_object() && !parsed.is_array() && !parsed.is_string() {
                        continue;
                    }
                    self.learn(&parsed);
                    if !failed && tool == SEND_TOOL {
                        self.after_send(args_sent, &parsed);
                    }
                    let opened = self.open_tree(parsed, &self.root_id, 0);
                    let rendered = if text.contains('\n') {
                        serde_json::to_string_pretty(&opened)
                    } else {
                        serde_json::to_string(&opened)
                    };
                    if let Ok(rendered) = rendered {
                        item["text"] = Value::String(rendered);
                    }
                } else if text.starts_with(SEALED_BODY_PREFIX) {
                    item["text"] = self.open_tree(Value::String(text), &self.root_id, 0);
                }
            }
        }
        if let Some(structured) = result.get_mut("structuredContent") {
            let taken = std::mem::take(structured);
            *structured = self.open_tree(taken, &self.root_id, 0);
        }
        result
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Listener {
    root_id: String,
    /// Null for a keyless tree (its listeners get the neutral line).
    epoch: Option<u32>,
    public_key: Option<String>,
}

/// `sessions.sendTarget`: null root = the send creates it; null key = keyless.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendTarget {
    root_id: Option<String>,
    epoch: Option<u32>,
    public_key: Option<String>,
    brief: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct EscalateTarget {
    root_id: String,
}

/// EXP-1472: what to do with content that could not be sealed. Without a
/// key holder in this process it goes as given (`Ok`: plaintext stays legal
/// in a keyless tree, the server refuses it in a keyed one); otherwise the
/// call fails with the reason and nothing is sent.
fn unsealed(what: &str, err: &SealError) -> Result<(), String> {
    match err {
        SealError::NoService => Ok(()),
        err => Err(refused(what, err)),
    }
}

/// The agent-facing error of content refused unsealed (logged).
fn refused(what: &str, err: &dyn std::fmt::Display) -> String {
    log::warn!("gateway: {what} not sent: it could not be sealed: {err}");
    format!("{what} was not sent: it could not be sealed ({err}). Nothing left this device; try again in a moment.")
}

/// Why a session's root is unknown: no key holder at all, or a lookup miss.
fn no_root() -> SealError {
    if sealing::service().is_some() { SealError::UnknownRoot } else { SealError::NoService }
}

fn card_kind(message: &Value) -> Option<&str> {
    message.get("card")?.get("kind")?.as_str()
}

/// An MCP call result's JSON: `structuredContent`, else the first text
/// content parsed.
fn result_json(result: &Value) -> Option<Value> {
    if let Some(structured) = result.get("structuredContent").filter(|v| !v.is_null()) {
        return Some(structured.clone());
    }
    let text = result.get("content")?.as_array()?.iter().find_map(|item| item.get("text")?.as_str())?;
    serde_json::from_str(text).ok()
}

/// Opens a card envelope (`sealing::open_card`: a sealed picture uploaded
/// after sealing takes its clear entry's attachmentId, addendum 4).
fn open_card_value(root: &str, card: &Value) -> (OpenStatus, Value) {
    let opened = sealing::open_card(root, card);
    (opened.status, opened.card)
}

/// The agent-facing card: opened, or its clear part with the placeholder.
fn open_card_for_agent(root: &str, card: &Value) -> Value {
    let (status, mut out) = open_card_value(root, card);
    if !matches!(status, OpenStatus::Opened | OpenStatus::Plain) {
        out["locked"] = Value::String(sealing::LOCKED_TEXT.to_string());
    }
    out
}

/// The show arguments without the gateway-only picture `attachmentId`s
/// (the server's plaintext schema is strict).
fn strip_picture_ids(mut args: Value) -> Value {
    if let Some(pictures) = args.get_mut("pictures").and_then(Value::as_array_mut) {
        for picture in pictures.iter_mut().filter_map(Value::as_object_mut) {
            picture.remove("attachmentId");
        }
    }
    args
}

/// The `#IDENT` issue references of a text (`#EXP-12`), in order, once.
pub fn issue_refs(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let boundary = i == 0 || !(chars[i - 1].is_alphanumeric() || matches!(chars[i - 1], '_' | '/' | '#' | '&'));
        if chars[i] == '#' && boundary {
            let start = i + 1;
            let mut j = start;
            while j < chars.len() && (chars[j].is_ascii_uppercase() || (j > start && chars[j].is_ascii_digit())) {
                j += 1;
            }
            if j > start && j < chars.len() && chars[j] == '-' {
                let digits = j + 1;
                let mut k = digits;
                while k < chars.len() && chars[k].is_ascii_digit() {
                    k += 1;
                }
                let tail_ok = k == chars.len() || !(chars[k].is_alphanumeric() || chars[k] == '_');
                if k > digits && tail_ok {
                    let ident: String = chars[start..k].iter().collect();
                    if !out.contains(&ident) {
                        out.push(ident);
                    }
                    i = k;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

/// `sessions.send`'s cap on the issues a message names.
const REF_ISSUES_MAX: usize = 30;

/// The text's `#IDENT` refs resolved to issue ids (`issues get`); refs
/// that do not resolve are skipped.
fn resolve_issue_refs(upstream: &dyn GatewayUpstream, text: &str) -> Vec<String> {
    issue_refs(text)
        .into_iter()
        .take(REF_ISSUES_MAX)
        .filter_map(|ident| {
            let result = upstream.call("exponential_issues", json!({ "op": "get", "id": ident })).ok()?;
            if result.get("isError") == Some(&Value::Bool(true)) {
                return None;
            }
            let issue = result_json(&result)?;
            issue.get("id").or_else(|| issue.pointer("/issue/id")).and_then(Value::as_str).map(str::to_string)
        })
        .collect()
}

/// `suggestions` / `replies` cards of `sessions_suggest {tasks, replies}`
/// (the shapes `postSuggestions` / `postReplies` post, ids minted here).
fn suggestion_cards(args: &Value) -> Result<Vec<Value>, String> {
    let mut cards = Vec::new();
    if let Some(tasks) = args.get("tasks").and_then(Value::as_array).filter(|t| !t.is_empty()) {
        if tasks.len() > 10 {
            return Err("Suggest 1 to 10 tasks".into());
        }
        let mut items = Vec::new();
        for task in tasks {
            let title = task.get("title").and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty());
            let text = task.get("text").and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty());
            let (Some(title), Some(text)) = (title, text) else {
                return Err("every task needs a title and a text".into());
            };
            let mut item = json!({ "id": uuid::Uuid::new_v4().to_string(), "title": title, "text": text });
            if let Some(ids) = task.get("issueIds").and_then(Value::as_array).filter(|i| !i.is_empty()) {
                item["issueIds"] = Value::Array(ids.clone());
            }
            items.push(item);
        }
        cards.push(json!({ "kind": "suggestions", "items": items }));
    }
    if let Some(replies) = args.get("replies").and_then(Value::as_array).filter(|r| !r.is_empty()) {
        if replies.len() > 6 {
            return Err("Offer 1 to 6 replies".into());
        }
        let items: Vec<Value> = replies
            .iter()
            .filter_map(|r| r.as_str().map(str::trim).filter(|r| !r.is_empty()))
            .map(|text| json!({ "id": uuid::Uuid::new_v4().to_string(), "text": text }))
            .collect();
        cards.push(json!({ "kind": "replies", "items": items }));
    }
    Ok(cards)
}

const ASK_QUESTION_LEAD: &str = "Asks: ";
const ASK_PLAN_LEAD: &str = "Proposes a plan:";
const ASK_PLAN_MAX: usize = 2000;

/// `askText` of `lib/sessions/bus.ts`: what a listener's agent reads for
/// the asking session's open card.
pub fn ask_text(session_id: &str, card: &Value) -> String {
    let send = format!("exponential_sessions_send({{sessionId: \"{session_id}\", answer: {{…}}}})");
    if card["kind"] == "question" {
        let options: Vec<&str> = card.get("options").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
        let mut flags = Vec::new();
        if card.get("multi") == Some(&Value::Bool(true)) {
            flags.push("several allowed");
        }
        if card.get("custom") != Some(&Value::Bool(false)) {
            flags.push("free text allowed");
        }
        let mut lines = vec![format!("{ASK_QUESTION_LEAD}{}", card["question"].as_str().unwrap_or_default())];
        if !options.is_empty() {
            let flags = if flags.is_empty() { String::new() } else { format!(" ({})", flags.join(", ")) };
            lines.push(format!("Options: {}{flags}", options.join(" | ")));
        }
        lines.push(format!(
            "Answer with {send} where answer = {{answers: [\"<option>\"]}} or {{answer: \"<text>\"}} if you know it; otherwise end your turn, a person answers in the card."
        ));
        return lines.join("\n");
    }
    let plan = card["plan"].as_str().unwrap_or_default();
    let clipped = if plan.chars().count() > ASK_PLAN_MAX {
        format!("{}…", plan.chars().take(ASK_PLAN_MAX - 1).collect::<String>().trim_end())
    } else {
        plan.to_string()
    };
    let questions: Vec<String> = card
        .get("questions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|q| {
            let labels: Vec<&str> =
                q.get("options").and_then(Value::as_array).into_iter().flatten().filter_map(|o| o.get("label")?.as_str()).collect();
            format!("- {}: {} ({})", q["id"].as_str().unwrap_or_default(), q["prompt"].as_str().unwrap_or_default(), labels.join(" | "))
        })
        .collect();
    let mut blocks = vec![ASK_PLAN_LEAD.to_string(), clipped];
    if !questions.is_empty() {
        blocks.push(format!("Questions:\n{}", questions.join("\n")));
    }
    blocks.push(format!(
        "Decide with {send} where answer = {{decision: \"approved\" | \"rejected\", answers: {{\"<question id>\": [\"<option>\"]}}, feedback: \"<text>\"}} if it is yours to decide; otherwise end your turn, a person decides in the card."
    ));
    blocks.join("\n\n")
}

// --- the answered card (port of lib/sessions/card-answer.ts) --------------

const ANSWER_JOINER: &str = ", ";

fn answer_issue_copy(issue: &str) -> String {
    match issue {
        "decision" => "Pick a decision",
        "missing" => "Answer every question to approve",
        "multi" => "This question takes one answer",
        _ => "Pick one of the options",
    }
    .to_string()
}

fn picks(labels: &Value) -> Vec<String> {
    labels
        .as_array()
        .map(|labels| labels.iter().filter_map(Value::as_str).map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect())
        .unwrap_or_default()
}

fn option_issue(labels: &[String], multi: bool, custom: bool, picked: &[String]) -> Option<&'static str> {
    if !multi && picked.len() > 1 {
        return Some("multi");
    }
    if !custom && picked.iter().any(|p| !labels.iter().any(|l| l.trim() == p)) {
        return Some("unknown");
    }
    None
}

/// `answerIssue` of `@exp/domain-contract/plan-card`.
fn answer_issue(card: &Value, input: &Map<String, Value>) -> Option<&'static str> {
    let flag = |v: &Value, key: &str| v.get(key) == Some(&Value::Bool(true));
    if card["kind"] == "plan" {
        let decision = input.get("decision").and_then(Value::as_str);
        if !matches!(decision, Some("approved" | "rejected")) {
            return Some("decision");
        }
        let empty = Map::new();
        let answers = input.get("answers").and_then(Value::as_object).unwrap_or(&empty);
        let questions = card.get("questions").and_then(Value::as_array).cloned().unwrap_or_default();
        for id in answers.keys() {
            if !questions.iter().any(|q| q.get("id").and_then(Value::as_str) == Some(id)) {
                return Some("unknown");
            }
        }
        for question in &questions {
            let id = question.get("id").and_then(Value::as_str).unwrap_or_default();
            let picked = answers.get(id).map(picks).unwrap_or_default();
            if picked.is_empty() {
                if decision == Some("approved") {
                    return Some("missing");
                }
                continue;
            }
            let labels: Vec<String> = question
                .get("options")
                .and_then(Value::as_array)
                .map(|o| o.iter().filter_map(|o| o.get("label")?.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            if let Some(issue) = option_issue(&labels, flag(question, "multi"), question.get("custom") != Some(&Value::Bool(false)), &picked) {
                return Some(issue);
            }
        }
        return None;
    }
    let picked = match input.get("answers") {
        Some(answers) if answers.is_array() => picks(answers),
        _ => input.get("answer").and_then(Value::as_str).map(str::trim).filter(|a| !a.is_empty()).map(|a| vec![a.to_string()]).unwrap_or_default(),
    };
    if picked.is_empty() {
        return Some("missing");
    }
    let labels: Vec<String> = card
        .get("options")
        .and_then(Value::as_array)
        .map(|o| o.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    option_issue(&labels, flag(card, "multi"), card.get("custom") != Some(&Value::Bool(false)), &picked)
}

/// `answeredCard`: the card with its answer (question) or decision +
/// answers + feedback (plan) in it.
pub fn answered_card(card: &Value, input: &Map<String, Value>) -> Result<Value, String> {
    let kind = card.get("kind").and_then(Value::as_str);
    if !matches!(kind, Some("question" | "plan")) {
        return Err("This message asks nothing".into());
    }
    if let Some(issue) = answer_issue(card, input) {
        return Err(answer_issue_copy(issue));
    }
    let surface_data = input.get("surfaceData").filter(|v| !v.is_null()).cloned();
    if surface_data.as_ref().is_some_and(|d| d.to_string().len() > PLAN_CARD_SURFACE_MAX_BYTES) {
        return Err(format!("The form data is larger than {PLAN_CARD_SURFACE_MAX_BYTES} bytes"));
    }
    let mut out = card.clone();
    if kind == Some("question") {
        let labels = input.get("answers").filter(|a| a.is_array()).map(picks).unwrap_or_default();
        let answers =
            if labels.is_empty() { vec![input.get("answer").and_then(Value::as_str).unwrap_or_default().trim().to_string()] } else { labels.clone() };
        out["answer"] = json!(if labels.is_empty() { answers[0].clone() } else { labels.join(ANSWER_JOINER) });
        out["answers"] = json!(answers);
    } else {
        let decision = input.get("decision").and_then(Value::as_str).unwrap_or_default();
        out["decision"] = json!(decision);
        out["fresh"] = json!(decision == "approved" && input.get("fresh") == Some(&Value::Bool(true)));
        let feedback = input.get("feedback").and_then(Value::as_str).map(str::trim).filter(|f| !f.is_empty());
        out["feedback"] = feedback.map(|f| json!(f)).unwrap_or(Value::Null);
        if let Some(answers) = input.get("answers").and_then(Value::as_object) {
            let mut picked = Map::new();
            for (id, labels) in answers {
                let labels = picks(labels);
                if !labels.is_empty() {
                    picked.insert(id.clone(), json!(labels));
                }
            }
            out["answers"] = Value::Object(picked);
        }
    }
    if let Some(data) = surface_data {
        out["surfaceData"] = data;
    }
    Ok(out)
}

// --- the show card (port of lib/sessions/plan.ts + the show tool) --------

pub const PLAN_CARD_PICTURES_MAX: usize = 10;
pub const PLAN_CARD_DESCRIPTION_MAX: usize = 2000;
pub const PLAN_CARD_SURFACE_MAX_BYTES: usize = 65536;
pub const PLAN_CARD_SURFACE_MAX_COMPONENTS: usize = 200;
pub const PLAN_CARD_PROMPT_MAX: usize = 500;
pub const PLAN_CARD_LABEL_MAX: usize = 500;
pub const PLAN_CARD_LABEL_KEY_MAX: usize = 64;
pub const PLAN_CARD_QUESTIONS_MAX: usize = 10;
pub const PLAN_CARD_OPTIONS_MAX: usize = 12;
pub const QUESTION_CARD_OPTIONS_MAX: usize = 50;
pub const MESSAGE_BODY_MAX: usize = 65536;
pub const SURFACE_CARD_TITLE_MAX: usize = 200;
const PLAN_CARD_UPLOAD_SCHEME: &str = "upload:";

pub const SHOW_CARD_ROOT_ERROR: &str = "surface root must not be a Card: the chat already draws each surface in a card. Use a Stack/List root with real components (Row, Badge, StatTile, Table, Button…).";
pub const SHOW_TEXT_ONLY_ERROR: &str = "this surface is text only. Send it as a plain message instead.";

#[derive(Debug, Clone, Default)]
pub struct PictureInput {
    pub label: String,
    pub caption: Option<String>,
    /// Gateway-only: a picture the agent already uploaded
    /// (`exponential_attachments`); a sealed card needs every picture's id.
    pub attachment_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct OptionInput {
    pub label: String,
    pub description: Option<String>,
    pub picture: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct QuestionInput {
    pub id: String,
    pub prompt: String,
    pub multi: Option<bool>,
    pub custom: Option<bool>,
    pub options: Vec<OptionInput>,
}

#[derive(Debug, Clone, Default)]
pub struct AskInput {
    pub question: String,
    pub options: Vec<OptionInput>,
    pub multi: Option<bool>,
    pub custom: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct PlanInput {
    pub text: String,
    pub questions: Option<Vec<QuestionInput>>,
}

/// `exponential_show`'s arguments, checked like the tool's strict zod
/// schema (unknown keys, types, caps).
#[derive(Debug, Clone, Default)]
pub struct ShowInput {
    pub surface: Option<Value>,
    pub title: Option<String>,
    pub replace: Option<bool>,
    pub ask: Option<AskInput>,
    pub plan: Option<PlanInput>,
    pub pictures: Option<Vec<PictureInput>>,
    pub description: Option<String>,
    pub story: Option<bool>,
}

fn strict<'a>(value: &'a Value, path: &str, keys: &[&str]) -> Result<&'a Map<String, Value>, String> {
    let obj = value.as_object().ok_or_else(|| format!("{path}: expected an object"))?;
    if let Some(key) = obj.keys().find(|k| !keys.contains(&k.as_str())) {
        return Err(format!("{path}: unrecognized key \"{key}\""));
    }
    Ok(obj)
}

fn opt_str(obj: &Map<String, Value>, key: &str, path: &str) -> Result<Option<String>, String> {
    match obj.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(format!("{path}.{key}: expected a string")),
    }
}

fn opt_bool(obj: &Map<String, Value>, key: &str, path: &str) -> Result<Option<bool>, String> {
    match obj.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(format!("{path}.{key}: expected a boolean")),
    }
}

/// A `.trim().min(1).max(max)` string.
fn trimmed(value: Option<String>, path: &str, max: usize) -> Result<Option<String>, String> {
    let Some(value) = value else { return Ok(None) };
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(format!("{path}: must not be empty"));
    }
    if value.chars().count() > max {
        return Err(format!("{path}: at most {max} characters"));
    }
    Ok(Some(value))
}

fn required(value: Option<String>, path: &str) -> Result<String, String> {
    value.ok_or_else(|| format!("{path}: required"))
}

fn picture_label(label: &str, path: &str) -> Result<(), String> {
    let mut chars = label.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphanumeric());
    let rest_ok = chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if label.is_empty() || label.chars().count() > PLAN_CARD_LABEL_KEY_MAX || !first_ok || !rest_ok {
        return Err(format!("{path}: a picture label is letters, digits, '.', '_' or '-'"));
    }
    Ok(())
}

fn parse_options(value: Option<&Value>, path: &str, max: usize) -> Result<Vec<OptionInput>, String> {
    let items = value.and_then(Value::as_array).ok_or_else(|| format!("{path}: expected an array"))?;
    if items.len() > max {
        return Err(format!("{path}: at most {max} options"));
    }
    items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let at = format!("{path}[{i}]");
            let obj = strict(item, &at, &["label", "description", "picture"])?;
            let label = required(trimmed(opt_str(obj, "label", &at)?, &format!("{at}.label"), PLAN_CARD_LABEL_MAX)?, &format!("{at}.label"))?;
            let description = opt_str(obj, "description", &at)?;
            if description.as_ref().is_some_and(|d| d.chars().count() > PLAN_CARD_DESCRIPTION_MAX) {
                return Err(format!("{at}.description: at most {PLAN_CARD_DESCRIPTION_MAX} characters"));
            }
            let picture = opt_str(obj, "picture", &at)?;
            if let Some(picture) = &picture {
                picture_label(picture, &format!("{at}.picture"))?;
            }
            Ok(OptionInput { label, description, picture })
        })
        .collect()
}

impl ShowInput {
    pub fn parse(args: &Value) -> Result<ShowInput, String> {
        let obj = strict(args, "arguments", &["surface", "title", "replace", "ask", "plan", "pictures", "description", "story"])?;
        let surface = match obj.get("surface") {
            None | Some(Value::Null) => None,
            Some(surface) => {
                let s = strict(surface, "surface", &["root", "data"])?;
                if !s.get("root").is_some_and(Value::is_object) {
                    return Err("surface.root: expected an object".into());
                }
                if s.get("data").is_some_and(|d| !d.is_object() && !d.is_null()) {
                    return Err("surface.data: expected an object".into());
                }
                Some(surface.clone())
            }
        };
        let title = trimmed(opt_str(obj, "title", "arguments")?, "title", SURFACE_CARD_TITLE_MAX)?;
        let ask = match obj.get("ask") {
            None | Some(Value::Null) => None,
            Some(ask) => {
                let a = strict(ask, "ask", &["question", "options", "multi", "custom"])?;
                Some(AskInput {
                    question: required(trimmed(opt_str(a, "question", "ask")?, "ask.question", PLAN_CARD_PROMPT_MAX)?, "ask.question")?,
                    options: parse_options(a.get("options"), "ask.options", QUESTION_CARD_OPTIONS_MAX)?,
                    multi: opt_bool(a, "multi", "ask")?,
                    custom: opt_bool(a, "custom", "ask")?,
                })
            }
        };
        let plan = match obj.get("plan") {
            None | Some(Value::Null) => None,
            Some(plan) => {
                let p = strict(plan, "plan", &["text", "questions"])?;
                let questions = match p.get("questions") {
                    None | Some(Value::Null) => None,
                    Some(questions) => {
                        let items = questions.as_array().ok_or("plan.questions: expected an array")?;
                        if items.len() > PLAN_CARD_QUESTIONS_MAX {
                            return Err(format!("plan.questions: at most {PLAN_CARD_QUESTIONS_MAX} questions"));
                        }
                        let parsed: Result<Vec<QuestionInput>, String> = items
                            .iter()
                            .enumerate()
                            .map(|(i, item)| {
                                let at = format!("plan.questions[{i}]");
                                let q = strict(item, &at, &["id", "prompt", "multi", "custom", "options"])?;
                                Ok(QuestionInput {
                                    id: required(trimmed(opt_str(q, "id", &at)?, &format!("{at}.id"), PLAN_CARD_LABEL_KEY_MAX)?, &format!("{at}.id"))?,
                                    prompt: required(
                                        trimmed(opt_str(q, "prompt", &at)?, &format!("{at}.prompt"), PLAN_CARD_PROMPT_MAX)?,
                                        &format!("{at}.prompt"),
                                    )?,
                                    multi: opt_bool(q, "multi", &at)?,
                                    custom: opt_bool(q, "custom", &at)?,
                                    options: parse_options(q.get("options"), &format!("{at}.options"), PLAN_CARD_OPTIONS_MAX)?,
                                })
                            })
                            .collect();
                        Some(parsed?)
                    }
                };
                Some(PlanInput {
                    text: required(trimmed(opt_str(p, "text", "plan")?, "plan.text", MESSAGE_BODY_MAX)?, "plan.text")?,
                    questions,
                })
            }
        };
        let pictures = match obj.get("pictures") {
            None | Some(Value::Null) => None,
            Some(pictures) => {
                let items = pictures.as_array().ok_or("pictures: expected an array")?;
                if items.len() > PLAN_CARD_PICTURES_MAX {
                    return Err(format!("pictures: at most {PLAN_CARD_PICTURES_MAX} pictures"));
                }
                let parsed: Result<Vec<PictureInput>, String> = items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| {
                        let at = format!("pictures[{i}]");
                        let p = strict(item, &at, &["label", "caption", "attachmentId"])?;
                        let label = required(opt_str(p, "label", &at)?, &format!("{at}.label"))?;
                        picture_label(&label, &format!("{at}.label"))?;
                        let caption = opt_str(p, "caption", &at)?;
                        if caption.as_ref().is_some_and(|c| c.chars().count() > PLAN_CARD_DESCRIPTION_MAX) {
                            return Err(format!("{at}.caption: at most {PLAN_CARD_DESCRIPTION_MAX} characters"));
                        }
                        let attachment_id = opt_str(p, "attachmentId", &at)?;
                        Ok(PictureInput { label, caption, attachment_id })
                    })
                    .collect();
                Some(parsed?)
            }
        };
        Ok(ShowInput {
            surface,
            title,
            replace: opt_bool(obj, "replace", "arguments")?,
            ask,
            plan,
            pictures,
            description: opt_str(obj, "description", "arguments")?,
            story: opt_bool(obj, "story", "arguments")?,
        })
    }
}

fn duplicate<'a>(values: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let mut seen = HashSet::new();
    values.into_iter().find(|v| !seen.insert(*v))
}

/// `cardPictures`.
fn card_pictures(pictures: Option<&[PictureInput]>) -> Result<Vec<Value>, String> {
    let list = pictures.unwrap_or_default();
    if let Some(twice) = duplicate(list.iter().map(|p| p.label.as_str())) {
        return Err(format!("pictures: the label \"{twice}\" is declared twice"));
    }
    Ok(list
        .iter()
        .map(|p| {
            let caption = p.caption.as_deref().map(str::trim).filter(|c| !c.is_empty());
            json!({ "label": p.label, "attachmentId": p.attachment_id, "width": null, "height": null, "caption": caption })
        })
        .collect())
}

fn assert_declared(declared: &HashSet<String>, label: &str, place: &str) -> Result<(), String> {
    if declared.contains(label) {
        return Ok(());
    }
    Err(format!("{place} names the picture \"{label}\", which is not in pictures: declare it as {{label: \"{label}\"}}"))
}

fn check_options(options: &[OptionInput], custom: bool, declared: &HashSet<String>, place: &str) -> Result<(), String> {
    if let Some(twice) = duplicate(options.iter().map(|o| o.label.trim())) {
        return Err(format!("{place}: the option \"{twice}\" is listed twice"));
    }
    if options.is_empty() && !custom {
        return Err(format!("{place} has no options and custom: false, so it cannot be answered: add options or allow custom"));
    }
    for (index, option) in options.iter().enumerate() {
        if let Some(picture) = &option.picture {
            assert_declared(declared, picture, &format!("{place}.options[{index}]"))?;
        }
    }
    Ok(())
}

/// Every `](upload:<label>)` embed in a markdown text.
pub fn upload_embeds(markdown: &str) -> Vec<String> {
    let needle = format!("]({PLAN_CARD_UPLOAD_SCHEME}");
    let mut out = Vec::new();
    let mut rest = markdown;
    while let Some(at) = rest.find(&needle) {
        let after = &rest[at + needle.len()..];
        let label: String = after.chars().take_while(|c| *c != ')' && !c.is_whitespace()).collect();
        let closed = after[label.len()..].starts_with(')');
        if closed {
            out.push(label.clone());
        }
        rest = &after[label.len()..];
    }
    out
}

fn count_nodes(node: &eui_core::UiNode) -> usize {
    1 + node.children.iter().map(count_nodes).sum::<usize>() + node.slots.iter().flat_map(|s| s.values()).map(count_nodes).sum::<usize>()
}

/// `validateCardSurface`: the core catalog only, within the byte and
/// component caps, with no reducer issue. `Null` when absent.
pub fn validate_card_surface(surface: Option<&Value>) -> Result<Value, String> {
    let Some(surface) = surface else { return Ok(Value::Null) };
    let bytes = serde_json::to_string(surface).map(|s| s.len()).unwrap_or(usize::MAX);
    if bytes > PLAN_CARD_SURFACE_MAX_BYTES {
        return Err(format!("surface is {bytes} bytes; the cap is {PLAN_CARD_SURFACE_MAX_BYTES}: simplify it"));
    }
    let root = surface.get("root").cloned().unwrap_or(Value::Null);
    if !root.get("id").is_some_and(Value::is_string) || !root.get("component").is_some_and(Value::is_string) {
        return Err("surface.root must be an Exponential UI node: {id, component, props?, children?}".into());
    }
    let nested: eui_core::NestedNode =
        serde_json::from_value(root.clone()).map_err(|e| format!("surface.root is not a valid Exponential UI tree: {e}"))?;
    let result = eui_core::reducer::reduce_nested(&nested, &eui_core::reducer::ReduceOptions::new(eui_core::CORE_CATALOG_ID));
    let count = count_nodes(&result.root) + result.templates.iter().flat_map(|t| t.values()).map(count_nodes).sum::<usize>();
    if count > PLAN_CARD_SURFACE_MAX_COMPONENTS {
        return Err(format!("surface has {count} components; the cap is {PLAN_CARD_SURFACE_MAX_COMPONENTS}"));
    }
    if !result.issues.is_empty() {
        let listed: Vec<String> = result.issues.iter().take(10).map(|i| format!("{}: {}", i.id, i.message)).collect();
        let more = if result.issues.len() > 10 { format!("; and {} more", result.issues.len() - 10) } else { String::new() };
        return Err(format!("surface is not valid in the Exponential UI core catalog: {}{more}", listed.join("; ")));
    }
    Ok(json!({ "root": root, "data": surface.get("data").cloned().unwrap_or(Value::Null) }))
}

fn option_json(options: &[OptionInput]) -> Vec<Value> {
    options
        .iter()
        .map(|o| {
            let description = o.description.as_deref().map(str::trim).filter(|d| !d.is_empty());
            json!({ "label": o.label.trim(), "description": description, "picture": o.picture })
        })
        .collect()
}

/// `buildPlanCard`.
pub fn build_plan_card(plan: &str, questions: Option<&[QuestionInput]>, pictures: Option<&[PictureInput]>, surface: Option<&Value>) -> Result<Value, String> {
    let plan = plan.trim();
    if plan.is_empty() {
        return Err("plan is required: the plan as markdown".into());
    }
    let pictures = card_pictures(pictures)?;
    let declared: HashSet<String> = pictures.iter().filter_map(|p| p["label"].as_str().map(str::to_string)).collect();
    for label in upload_embeds(plan) {
        assert_declared(&declared, &label, &format!("plan's ![…](upload:{label})"))?;
    }
    let questions = questions.unwrap_or_default();
    if let Some(twice) = duplicate(questions.iter().map(|q| q.id.trim())) {
        return Err(format!("questions: the id \"{twice}\" is used twice"));
    }
    let mut built = Vec::new();
    for (index, question) in questions.iter().enumerate() {
        let custom = question.custom != Some(false);
        check_options(&question.options, custom, &declared, &format!("questions[{index}]"))?;
        built.push(json!({
            "id": question.id.trim(),
            "prompt": question.prompt.trim(),
            "multi": question.multi == Some(true),
            "custom": custom,
            "options": option_json(&question.options),
        }));
    }
    Ok(json!({
        "kind": "plan",
        "plan": plan,
        "pictures": pictures,
        "questions": built,
        "surface": validate_card_surface(surface)?,
        "decision": null,
        "fresh": null,
        "feedback": null,
        "answers": null,
        "surfaceData": null,
    }))
}

/// `buildQuestionCard`: the sidecars ride only when some option carries a
/// description / a picture.
pub fn build_question_card(ask: &AskInput, pictures: Option<&[PictureInput]>, surface: Option<&Value>) -> Result<Value, String> {
    let question = ask.question.trim();
    if question.is_empty() {
        return Err("question is required".into());
    }
    let pictures = card_pictures(pictures)?;
    let declared: HashSet<String> = pictures.iter().filter_map(|p| p["label"].as_str().map(str::to_string)).collect();
    let custom = ask.custom != Some(false);
    check_options(&ask.options, custom, &declared, "options")?;
    let descriptions: Vec<Value> =
        ask.options.iter().map(|o| o.description.as_deref().map(str::trim).filter(|d| !d.is_empty()).map_or(Value::Null, |d| json!(d))).collect();
    let option_pictures: Vec<Value> = ask.options.iter().map(|o| o.picture.as_ref().map_or(Value::Null, |p| json!(p))).collect();
    let mut card = Map::new();
    card.insert("kind".into(), json!("question"));
    card.insert("question".into(), json!(question));
    card.insert("options".into(), json!(ask.options.iter().map(|o| o.label.trim()).collect::<Vec<_>>()));
    if descriptions.iter().any(|d| !d.is_null()) {
        card.insert("optionDescriptions".into(), json!(descriptions));
    }
    if option_pictures.iter().any(|p| !p.is_null()) {
        card.insert("optionPictures".into(), json!(option_pictures));
    }
    card.insert("pictures".into(), json!(pictures));
    card.insert("multi".into(), json!(ask.multi == Some(true)));
    card.insert("custom".into(), json!(custom));
    card.insert("surface".into(), validate_card_surface(surface)?);
    card.insert("answer".into(), Value::Null);
    card.insert("answers".into(), Value::Null);
    card.insert("surfaceData".into(), Value::Null);
    Ok(Value::Object(card))
}

/// `buildSurfaceCard`.
pub fn build_surface_card(
    surface: &Value,
    title: Option<&str>,
    pictures: Option<&[PictureInput]>,
    story: Option<bool>,
    description: Option<&str>,
) -> Result<Value, String> {
    let surface = validate_card_surface(Some(surface))?;
    let title = title.map(str::trim).filter(|t| !t.is_empty());
    if let Some(title) = title {
        let length = title.chars().count();
        if length > SURFACE_CARD_TITLE_MAX {
            return Err(format!("title is {length} characters; the cap is {SURFACE_CARD_TITLE_MAX}"));
        }
    }
    let pictures = card_pictures(pictures)?;
    let description = description.map(str::trim).filter(|d| !d.is_empty());
    if let Some(description) = description {
        let length = description.chars().count();
        if length > PLAN_CARD_DESCRIPTION_MAX {
            return Err(format!("description is {length} characters; the cap is {PLAN_CARD_DESCRIPTION_MAX}"));
        }
    }
    let mut card = Map::new();
    card.insert("kind".into(), json!("surface"));
    card.insert("surface".into(), surface);
    card.insert("title".into(), title.map_or(Value::Null, |t| json!(t)));
    if let Some(description) = description {
        card.insert("description".into(), json!(description));
    }
    if !pictures.is_empty() {
        card.insert("pictures".into(), json!(pictures));
    }
    if let Some(story) = story {
        card.insert("story".into(), json!(story));
    }
    Ok(Value::Object(card))
}

/// `picturesOnlySurface`: a Card holding the title (else the picture
/// labels) above the pictures.
pub fn pictures_only_surface(title: Option<&str>, pictures: &[PictureInput]) -> Value {
    let text = title
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| pictures.iter().map(|p| p.label.as_str()).collect::<Vec<_>>().join(", "));
    json!({ "root": { "id": "c", "component": "Card", "children": [{ "id": "h", "component": "Heading", "props": { "text": text } }] } })
}

fn component_group(component: &str) -> Option<String> {
    eui_core::catalog::CORE.components.get(component).map(|def| def.group.clone())
}

fn text_only_node(node: &Value) -> bool {
    let Some(obj) = node.as_object() else { return true };
    let Some(component) = obj.get("component").and_then(Value::as_str) else { return false };
    if obj.contains_key("template") {
        return false;
    }
    let group = component_group(component);
    if group.as_deref() == Some("text") {
        return true;
    }
    if !(group.as_deref() == Some("layout") || component == "Section" || component == "Group") {
        return false;
    }
    let children = obj.get("children").and_then(Value::as_array).into_iter().flatten();
    let slots = obj.get("slots").and_then(Value::as_object).into_iter().flat_map(|s| s.values());
    children.chain(slots).all(text_only_node)
}

/// `showSurfaceProblem`: a Card root, or (as the whole card) text only.
pub fn show_surface_problem(surface: &Value, alone: bool) -> Option<&'static str> {
    let root = surface.get("root")?;
    if root.get("component").and_then(Value::as_str) == Some("Card") {
        return Some(SHOW_CARD_ROOT_ERROR);
    }
    if alone && text_only_node(root) {
        return Some(SHOW_TEXT_ONLY_ERROR);
    }
    None
}

/// The show tool's card: the same choices and checks as its handler.
pub fn build_show_card(input: &ShowInput) -> Result<Value, String> {
    if input.ask.is_some() && input.plan.is_some() {
        return Err("Pass ask or plan, not both: one card per call.".into());
    }
    let has_pictures = input.pictures.as_ref().is_some_and(|p| !p.is_empty());
    if input.ask.is_none() && input.plan.is_none() && input.surface.is_none() && !has_pictures {
        return Err("Pass surface, ask, plan or pictures.".into());
    }
    let pictures = input.pictures.as_deref();
    let mut card = if let Some(plan) = &input.plan {
        build_plan_card(&plan.text, plan.questions.as_deref(), pictures, input.surface.as_ref())?
    } else if let Some(ask) = &input.ask {
        build_question_card(ask, pictures, input.surface.as_ref())?
    } else {
        let shown = input.surface.clone().unwrap_or_else(|| pictures_only_surface(input.title.as_deref(), pictures.unwrap_or_default()));
        build_surface_card(&shown, input.title.as_deref(), pictures, input.story, input.description.as_deref())?
    };
    if let Some(surface) = &input.surface {
        if let Some(problem) = show_surface_problem(surface, input.ask.is_none() && input.plan.is_none() && !has_pictures) {
            return Err(problem.to_string());
        }
    }
    if let Some(story) = input.story {
        card["story"] = json!(story);
    }
    Ok(card)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sealing::test_support::{install_fake, serial};
    use crate::sealing::Keyring;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/sealed-content.json")).unwrap()
    }

    fn hex32(v: &Value) -> [u8; 32] {
        let s = v.as_str().unwrap();
        (0..32).map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap()).collect::<Vec<_>>().try_into().unwrap()
    }

    fn fixture_into(keyring: &Keyring, root: &str) {
        let fx = fixture();
        for (epoch, secret) in fx["keys"].as_object().unwrap() {
            keyring.insert_secret(root, epoch.parse().unwrap(), hex32(secret));
        }
        let public = sealing::public_key_from_b64(fx["tree"]["publicKey"].as_str().unwrap()).unwrap();
        keyring.set_current(root, 1, public);
    }

    /// A fake Exponential: `/api/mcp` records every tools/call and answers
    /// from `answer(tool, args)`; `/api/trpc/*` answers the listeners and
    /// escalation lookups.
    struct FakeServer {
        url: String,
        calls: Arc<Mutex<Vec<(String, Value)>>>,
    }

    type Answer = Arc<dyn Fn(&str, &Value) -> Value + Send + Sync>;

    fn fake_server(answer: Answer, trpc: Value) -> FakeServer {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let record = calls.clone();
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        std::thread::spawn(move || {
            for mut request in server.incoming_requests() {
                let url = request.url().to_string();
                let mut body = String::new();
                let _ = request.as_reader().read_to_string(&mut body);
                let json_header = tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap();
                if let Some(path) = url.strip_prefix("/api/trpc/") {
                    let procedure = path.split('?').next().unwrap_or_default();
                    let data = trpc.get(procedure).cloned().unwrap_or(Value::Null);
                    record.lock().unwrap().push((format!("trpc:{procedure}"), Value::String(url.clone())));
                    let _ = request.respond(tiny_http::Response::from_string(json!({ "result": { "data": data } }).to_string()).with_header(json_header));
                    continue;
                }
                // A client's GET stream or session DELETE: not served.
                let Ok(message) = serde_json::from_str::<Value>(&body) else {
                    let _ = request.respond(tiny_http::Response::empty(405));
                    continue;
                };
                let Some(id) = message.get("id").cloned() else {
                    let _ = request.respond(tiny_http::Response::empty(202));
                    continue;
                };
                let reply = match message["method"].as_str().unwrap_or_default() {
                    "tools/call" => {
                        let tool = message["params"]["name"].as_str().unwrap_or_default().to_string();
                        let args = message["params"]["arguments"].clone();
                        record.lock().unwrap().push((tool.clone(), args.clone()));
                        let data = answer(&tool, &args);
                        json!({ "jsonrpc": "2.0", "id": id, "result": { "content": [{ "type": "text", "text": serde_json::to_string_pretty(&data).unwrap() }] } })
                    }
                    // A whole `initialize` answer: the transform's own reads
                    // go through emcp's client, which reads it strictly.
                    _ => json!({ "jsonrpc": "2.0", "id": id, "result": {
                        "protocolVersion": "2025-06-18", "capabilities": { "tools": {} }, "serverInfo": { "name": "fake", "version": "1" },
                    } }),
                };
                let _ = request.respond(tiny_http::Response::from_string(reply.to_string()).with_header(json_header));
            }
        });
        FakeServer { url: format!("http://127.0.0.1:{port}"), calls }
    }

    /// The gateway in front of `server` for session `own` (root `root`).
    fn gateway(server: &FakeServer, own: &str, root: &str) -> codemode::Grant {
        let trpc = Arc::new(TrpcClient::new(&server.url, Arc::new(api::StaticToken("t".into()))));
        let transform = Arc::new(SealingTransform::new(own, root, Some(trpc)));
        let upstream = codemode::Upstream {
            name: "exponential".into(),
            url: format!("{}/api/mcp", server.url),
            headers: vec![("Authorization".into(), "Bearer expu_k".into()), ("X-Exp-Session-Id".into(), own.into())],
        };
        codemode::gateway_grant(own, "test", upstream, transform).unwrap()
    }

    /// One tools/call through the gateway → the result's text parsed.
    fn call(grant: &codemode::Grant, tool: &str, args: Value) -> Value {
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": tool, "arguments": args } });
        let text = reqwest::blocking::Client::new()
            .post(format!("{}/api/mcp", grant.url))
            .header("Authorization", format!("Bearer {}", grant.token))
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .unwrap()
            .text()
            .unwrap();
        let reply: Value = serde_json::from_str(&text).unwrap();
        let result = &reply["result"];
        let text = result["content"][0]["text"].as_str().unwrap_or_default().to_string();
        let mut parsed = serde_json::from_str::<Value>(&text).unwrap_or(Value::String(text));
        if result["isError"] == true {
            parsed = json!({ "error": parsed });
        }
        parsed
    }

    fn calls_of(server: &FakeServer, tool: &str) -> Vec<Value> {
        server.calls.lock().unwrap().iter().filter(|(t, _)| t == tool).map(|(_, a)| a.clone()).collect()
    }

    #[test]
    fn sessions_send_show_end_and_suggest_leave_sealed_and_results_come_back_opened() {
        let _serial = serial();
        let fx = fixture();
        let fake = install_fake();
        fixture_into(&fake.keyring, "root-1");
        fake.keyring.note_root("child-1", "root-1");
        let listener_tree = enet_core::sealed::generate_tree_key();
        let body_envelope = fx["body"]["envelope"].as_str().unwrap().to_string();
        let report_envelope = fx["card"]["splits"].as_array().unwrap().iter().find(|s| s["name"] == "report").unwrap()["envelope"].clone();
        assert_eq!(report_envelope["kind"], "report");
        let tree_body = body_envelope.clone();
        let answer: Answer = Arc::new(move |tool, args| match tool {
            SEND_TOOL => json!({
                "sessionId": args.get("sessionId").or(args.get("parentId")).cloned().unwrap_or(json!("new-root")),
                "messageId": "m-1",
                "status": "running",
                "deliverTo": { "installId": "install-2", "endpointId": "ab12" },
            }),
            "exponential_issues" if args["id"] == "EXP-12" => json!({ "id": "00000000-0000-4000-8000-000000000012", "identifier": "EXP-12" }),
            SESSIONS_TOOL if args["op"] == "get" => json!({
                "root": { "id": "root-1", "title": "Session", "sealedTitle": tree_body },
                "children": [{ "id": "child-1", "rootId": "root-1" }],
                "messages": [
                    { "id": "m-0", "sessionId": "child-1", "body": tree_body, "card": report_envelope },
                ],
            }),
            _ => json!({ "ok": true, "messageId": "m-2" }),
        });
        let server = fake_server(
            answer,
            json!({ "sessions.listeners": [{ "rootId": "root-9", "epoch": 3, "publicKey": listener_tree.public_key() }] }),
        );
        let grant = gateway(&server, "child-1", "root-1");

        // sessions_send into the own tree: text + name sealed, delivered direct.
        let out = call(&grant, SEND_TOOL, json!({ "sessionId": "root-1", "text": "secret plan for #EXP-12", "name": "Fix login" }));
        assert_eq!(out["messageId"], "m-1");
        let sent = calls_of(&server, SEND_TOOL).pop().unwrap();
        let text = sent["text"].as_str().unwrap();
        assert!(text.starts_with("exp-sealed:v1:1:"), "{sent}");
        assert!(sent.get("name").is_none() && sent["sealedName"].as_str().unwrap().starts_with("exp-sealed:"));
        assert_eq!(sealing::open_body_text("root-1", text).as_deref(), Some("secret plan for #EXP-12"));
        assert_eq!(sent["refIssueIds"], json!(["00000000-0000-4000-8000-000000000012"]));
        let delivered = fake.delivered.lock().unwrap().pop().unwrap();
        assert_eq!(delivered.0, "ab12");
        assert_eq!(delivered.1.id, "m-1");
        assert_eq!(delivered.1.body.as_deref(), Some(text));
        assert_eq!(delivered.1.author_session_id.as_deref(), Some("child-1"));
        assert_eq!(delivered.1.root_id.as_deref(), Some("root-1"));

        // show: the card is built and sealed; the uploaded pictures' ids
        // stay clear; the ask text rides sealed for the own tree and for
        // the listener's tree.
        let attachment = "00000000-0000-4000-8000-0000000000a1";
        call(
            &grant,
            SHOW_TOOL,
            json!({ "ask": { "question": "Which database?", "options": [{ "label": "Postgres" }, { "label": "SQLite" }] }, "pictures": [{ "label": "web-db", "caption": "the schema", "attachmentId": attachment }], "replace": true }),
        );
        let sent = calls_of(&server, SHOW_TOOL).pop().unwrap();
        assert_eq!(sent["kind"], "question");
        assert_eq!(sent["replace"], true);
        assert_eq!(sent["sealedCard"]["pictures"], json!([{ "label": "web-db", "attachmentId": attachment }]));
        let raw = sent.to_string();
        assert!(!raw.contains("Which database") && !raw.contains("Postgres") && !raw.contains("the schema"), "{raw}");
        let opened = sealing::open_card("root-1", &sent["sealedCard"]).card;
        assert_eq!(opened["question"], "Which database?");
        let ask = sealing::open_body_text("root-1", sent["askText"].as_str().unwrap()).unwrap();
        assert!(ask.starts_with("Asks: Which database?\nOptions: Postgres | SQLite (free text allowed)\n"), "{ask}");
        assert_eq!(sent["listenerCopies"][0]["rootId"], "root-9");
        let copy = enet_core::sealed::open_body(sent["listenerCopies"][0]["body"].as_str().unwrap(), &|_| Some(listener_tree.secret));
        assert_eq!(copy.text.as_deref(), Some(ask.as_str()));
        // Pictures still to upload: sealed too, their labels clear so the
        // result's upload curls file each on its clear entry.
        call(&grant, SHOW_TOOL, json!({ "pictures": [{ "label": "web-a", "caption": "the dock" }], "title": "Shots" }));
        let sent = calls_of(&server, SHOW_TOOL).pop().unwrap();
        assert_eq!(sent["kind"], "surface");
        assert_eq!(sent["sealedCard"]["pictures"], json!([{ "label": "web-a" }]));
        assert!(sent.get("pictures").is_none() && sent.get("title").is_none(), "{sent}");
        let raw = sent.to_string();
        assert!(!raw.contains("Shots") && !raw.contains("the dock"), "{raw}");
        // The server filed the upload on the clear entry: opening fills it.
        let mut filed = sent["sealedCard"].clone();
        filed["pictures"] = json!([{ "label": "web-a", "attachmentId": attachment }]);
        let opened = sealing::open_card("root-1", &filed).card;
        assert_eq!(opened["pictures"][0]["attachmentId"], attachment);
        assert_eq!(opened["pictures"][0]["caption"], "the dock");
        // A refused card never reaches the upstream.
        let before = calls_of(&server, SHOW_TOOL).len();
        let refused = call(&grant, SHOW_TOOL, json!({ "surface": { "root": { "id": "c", "component": "Card" } } }));
        assert_eq!(refused["error"], SHOW_CARD_ROOT_ERROR);
        assert_eq!(calls_of(&server, SHOW_TOOL).len(), before);

        // end: a sealed report + a copy for the listener's tree.
        call(&grant, SESSIONS_TOOL, json!({ "op": "end", "summary": "Merged it" }));
        let sent = calls_of(&server, SESSIONS_TOOL).pop().unwrap();
        assert!(sent.get("summary").is_none() && !sent.to_string().contains("Merged it"), "{sent}");
        let report = sealing::open_card("root-1", &sent["sealedReport"]).card;
        assert_eq!(report, json!({ "kind": "report", "sessionId": "child-1", "status": "ended", "final": true, "summary": "Merged it" }));
        let copy = &sent["copies"][0];
        assert_eq!(copy["rootId"], "root-9");
        let reopened = enet_core::sealed::open_card(&copy["sealedReport"], &|epoch| (epoch == 3).then_some(listener_tree.secret));
        assert_eq!(reopened.card["summary"], "Merged it");

        // suggest: one card envelope per card.
        call(&grant, SELF_TOOL, json!({ "op": "suggest", "replies": ["Ship it"], "tasks": [{ "title": "Tests", "text": "Cover it" }] }));
        let sent = calls_of(&server, SELF_TOOL).pop().unwrap();
        assert!(sent.get("replies").is_none() && sent.get("tasks").is_none());
        let cards = sent["sealed"].as_array().unwrap();
        assert_eq!((cards[0]["kind"].as_str(), cards[1]["kind"].as_str()), (Some("suggestions"), Some("replies")));
        assert!(!sent.to_string().contains("Ship it"));
        let replies = sealing::open_card("root-1", &cards[1]).card;
        assert_eq!(replies["items"][0]["text"], "Ship it");
        assert!(replies["items"][0]["id"].is_string());

        // Results: the fixture's envelopes open for the agent.
        let tree = call(&grant, SESSIONS_TOOL, json!({ "op": "get" }));
        assert_eq!(tree["root"]["title"], fx["body"]["text"]);
        assert!(tree["root"].get("sealedTitle").is_none());
        assert_eq!(tree["messages"][0]["body"], fx["body"]["text"]);
        assert_eq!(tree["messages"][0]["card"]["summary"], "Merged #1400; tests green");
        codemode::revoke("child-1");
        sealing::uninstall();
    }

    #[test]
    fn a_new_thread_carries_its_key_and_another_tree_uses_its_current_key() {
        let _serial = serial();
        let install = enet_core::PushKeypair::generate();
        let fake = Arc::new(crate::sealing::test_support::FakeService { install_public: Some(install.public()), ..Default::default() });
        sealing::install(fake.clone());
        let other = enet_core::sealed::generate_tree_key();
        fake.keyring.set_current("root-other", 1, other.public);
        fake.roots.lock().unwrap().insert("s-other".into(), "root-other".into());
        let answer: Answer = Arc::new(|_, args| {
            json!({ "sessionId": args.get("sessionId").cloned().unwrap_or(json!("root-new")), "messageId": "m", "status": "queued", "deliverTo": null })
        });
        let server = fake_server(answer, json!({}));
        let grant = gateway(&server, "s-own", "root-own");

        call(&grant, SEND_TOOL, json!({ "text": "start this", "deviceId": "d1" }));
        let sent = calls_of(&server, SEND_TOOL).pop().unwrap();
        let seal = &sent["seal"];
        assert_eq!(seal["installId"], "install-1");
        assert_eq!(seal["epoch"], 1);
        assert_eq!(seal["wraps"][0]["installId"], "install-1");
        let secret = sealing::unwrap_tree_key(&install.secret(), seal["wraps"][0]["wrapped"].as_str().unwrap()).unwrap();
        assert!(seal.get("secret").is_none());
        // Adopted under the new root's id: its content opens here.
        assert_eq!(fake.keyring.secret("root-new", 1), Some(secret));
        assert_eq!(sealing::open_body_text("root-new", sent["text"].as_str().unwrap()).as_deref(), Some("start this"));
        assert!(fake.delivered.lock().unwrap().is_empty(), "deliverTo null = no direct delivery");

        // Another tree: sealed with ITS key, never a new one.
        call(&grant, SEND_TOOL, json!({ "sessionId": "s-other", "text": "hello neighbour" }));
        let sent = calls_of(&server, SEND_TOOL).pop().unwrap();
        let opened = enet_core::sealed::open_body(sent["text"].as_str().unwrap(), &|_| Some(other.secret));
        assert_eq!(opened.text.as_deref(), Some("hello neighbour"));
        assert!(fake.created.lock().unwrap().is_empty());

        // An agent conversation the server cannot resolve: refused, never
        // sent in clear (EXP-1472).
        let sends = calls_of(&server, SEND_TOOL).len();
        let out = call(&grant, SEND_TOOL, json!({ "agentId": "a1", "text": "never in clear" }));
        assert!(out["error"].as_str().is_some_and(|e| e.contains("could not be sealed")), "{out}");
        assert_eq!(calls_of(&server, SEND_TOOL).len(), sends);
        assert!(server.calls.lock().unwrap().iter().any(|(t, url)| t == "trpc:sessions.sendTarget" && url.as_str().unwrap_or_default().contains("a1")));
        codemode::revoke("s-own");
        sealing::uninstall();
    }

    #[test]
    fn a_send_into_a_keyless_tree_waits_for_its_key_and_never_goes_out_plaintext() {
        let _serial = serial();
        let fake = install_fake();
        sealing::set_send_key_wait(std::time::Duration::from_millis(800));
        fake.roots.lock().unwrap().insert("s-a".into(), "root-a".into());
        let answer: Answer = Arc::new(|_, _| json!({ "sessionId": "s-a", "messageId": "m", "status": "running", "deliverTo": null }));
        let server = fake_server(answer, json!({}));
        let grant = gateway(&server, "s-own", "root-own");

        // Nobody keys root-a: the call fails and nothing reaches the server.
        let out = call(&grant, SEND_TOOL, json!({ "sessionId": "s-a", "text": "PONG-7731" }));
        assert!(out["error"].as_str().is_some_and(|e| e.contains("no key yet")), "{out}");
        assert!(calls_of(&server, SEND_TOOL).is_empty());
        assert!(!server.calls.lock().unwrap().iter().any(|(_, args)| args.to_string().contains("PONG-7731")));

        // Its device keys it meanwhile (the claim): the send waits and seals.
        let tree = enet_core::sealed::generate_tree_key();
        let keyring_owner = fake.clone();
        let keyer = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(150));
            keyring_owner.keyring.set_current("root-a", 1, tree.public);
        });
        call(&grant, SEND_TOOL, json!({ "sessionId": "s-a", "text": "PONG-7731" }));
        keyer.join().unwrap();
        let sent = calls_of(&server, SEND_TOOL).pop().unwrap();
        let opened = enet_core::sealed::open_body(sent["text"].as_str().unwrap(), &|_| Some(tree.secret));
        assert_eq!(opened.text.as_deref(), Some("PONG-7731"));
        assert!(!fake.created.lock().unwrap().iter().any(|root| root == "root-a"), "another tree's key is never created here");

        sealing::set_send_key_wait(sealing::SEND_KEY_WAIT);
        codemode::revoke("s-own");
        sealing::uninstall();
    }

    #[test]
    fn a_send_to_an_agent_or_app_seals_to_its_conversation() {
        let _serial = serial();
        let install = enet_core::PushKeypair::generate();
        let fake = Arc::new(crate::sealing::test_support::FakeService { install_public: Some(install.public()), ..Default::default() });
        sealing::install(fake.clone());
        let answer: Answer = Arc::new(|_, _| json!({ "sessionId": "conv-1", "messageId": "m", "status": "idle", "deliverTo": null }));

        // A keyed agent conversation: sealed to ITS current key, no new key.
        let conversation = enet_core::sealed::generate_tree_key();
        let server = fake_server(
            answer.clone(),
            json!({ "sessions.sendTarget": { "rootId": "conv-1", "epoch": 2, "publicKey": conversation.public_key(), "brief": null } }),
        );
        let grant = gateway(&server, "s-own", "root-own");
        call(&grant, SEND_TOOL, json!({ "agentId": "a1", "text": "review #EXP-1" }));
        let sent = calls_of(&server, SEND_TOOL).pop().unwrap();
        assert!(sent.get("seal").is_none(), "{sent}");
        let text = sent["text"].as_str().unwrap();
        assert!(text.starts_with("exp-sealed:v1:2:"), "{text}");
        let opened = enet_core::sealed::open_body(text, &|epoch| (epoch == 2).then_some(conversation.secret));
        assert_eq!(opened.text.as_deref(), Some("review #EXP-1"));
        assert_eq!(sent["agentId"], "a1");
        assert!(fake.created.lock().unwrap().is_empty());
        codemode::revoke("s-own");

        // A new app editor session: a fresh key rides as `seal`, the brief
        // is composed in, and the key is adopted under the new root.
        let server = fake_server(
            answer,
            json!({ "sessions.sendTarget": { "rootId": null, "epoch": null, "publicKey": null, "brief": "You edit the app Notes." } }),
        );
        let grant = gateway(&server, "s-own", "root-own");
        call(&grant, SEND_TOOL, json!({ "appId": "app-1", "text": " make it blue " }));
        let sent = calls_of(&server, SEND_TOOL).pop().unwrap();
        assert_eq!(sent["seal"]["installId"], "install-1");
        let secret = sealing::unwrap_tree_key(&install.secret(), sent["seal"]["wraps"][0]["wrapped"].as_str().unwrap()).unwrap();
        assert_eq!(fake.keyring.secret("conv-1", 1), Some(secret));
        assert_eq!(
            sealing::open_body_text("conv-1", sent["text"].as_str().unwrap()).as_deref(),
            Some("You edit the app Notes.\n\nmake it blue")
        );
        assert!(!sent.to_string().contains("blue"));
        codemode::revoke("s-own");
        sealing::uninstall();
    }

    #[test]
    fn an_answer_reseals_the_whole_card_and_an_escalation_seals_for_the_target() {
        let _serial = serial();
        let fx = fixture();
        let fake = install_fake();
        fixture_into(&fake.keyring, "root-1");
        let agent_tree = enet_core::sealed::generate_tree_key();
        fake.keyring.set_current("root-agent", 1, agent_tree.public);
        let open_question = fx["card"]["splits"][0]["envelope"].clone();
        let answer: Answer = Arc::new(move |tool, args| match tool {
            SESSIONS_TOOL if args["op"] == "get" => json!({
                "root": { "id": "root-1" },
                "children": [{ "id": "child-1" }],
                "messages": [{ "id": "q-1", "sessionId": "child-1", "card": open_question, "answeredAt": null }],
            }),
            _ => json!({ "sessionId": "child-1", "messageId": null, "status": "running" }),
        });
        let server = fake_server(answer, json!({ "sessions.escalateTarget": { "rootId": "root-agent" } }));
        let grant = gateway(&server, "root-1", "root-1");

        call(&grant, SEND_TOOL, json!({ "sessionId": "child-1", "answer": { "answers": ["Postgres"] } }));
        let sent = calls_of(&server, SEND_TOOL).pop().unwrap();
        let sealed_card = sent["answer"]["sealedCard"].clone();
        assert_eq!(sent["answer"].as_object().unwrap().keys().collect::<Vec<_>>(), ["messageId", "sealedCard"]);
        assert_eq!(sent["answer"]["messageId"], "q-1");
        assert_eq!(sealed_card["answered"], true);
        assert!(!sent.to_string().contains("Postgres"), "{sent}");
        let card = sealing::open_card("root-1", &sealed_card).card;
        assert_eq!(card["answer"], "Postgres");
        assert_eq!(card["answers"], json!(["Postgres"]));
        assert_eq!(card["question"], "Which database?");
        // An invalid answer is refused before the upstream.
        let refused = call(&grant, SEND_TOOL, json!({ "sessionId": "child-1", "answer": { "answers": ["Postgres", "SQLite"] } }));
        assert_eq!(refused["error"], "This question takes one answer");

        call(&grant, SESSIONS_TOOL, json!({ "op": "escalate", "messageId": "q-1" }));
        let sent = calls_of(&server, SESSIONS_TOOL).into_iter().rev().find(|a| a["op"] == "escalate").unwrap();
        assert_eq!(sent["messageId"], "q-1");
        let opened = enet_core::sealed::open_card(&sent["sealedCard"], &|_| Some(agent_tree.secret));
        assert_eq!(opened.card["question"], "Which database?");
        assert!(server.calls.lock().unwrap().iter().any(|(t, _)| t == "trpc:sessions.escalateTarget"));
        codemode::revoke("root-1");
        sealing::uninstall();
    }

    #[test]
    fn opening_merges_filed_pictures_and_locks_what_cannot_open() {
        let _serial = serial();
        let fake = install_fake();
        let card = json!({ "kind": "plan", "plan": "p", "pictures": [
            { "label": "web-a", "caption": "Before", "attachmentId": null },
            { "label": "web-b", "caption": "After", "attachmentId": "att-b", "width": 4 },
        ] });
        let transform = SealingTransform::new("s", "r", None);
        let sealed = transform.seal_card("r", &card).unwrap();
        assert_eq!(
            sealed["pictures"],
            json!([{ "label": "web-a" }, { "label": "web-b", "attachmentId": "att-b" }]),
            "the projection only: a picture still to upload by its label"
        );
        let none = transform.seal_card("r", &json!({ "kind": "plan", "plan": "p", "pictures": [{ "caption": "x", "attachmentId": null }] })).unwrap();
        assert!(none.get("pictures").is_none());
        let (status, opened) = open_card_value("r", &sealed);
        assert_eq!(status, OpenStatus::Opened);
        assert_eq!(opened["pictures"], card["pictures"], "the sealed pictures stay whole");
        // web-a uploaded later (the server filed it on its clear entry; a
        // clear id never replaces a sealed one).
        let mut filed = sealed.clone();
        filed["pictures"] = json!([{ "label": "web-a", "attachmentId": "att-a" }, { "label": "web-b", "attachmentId": "att-x" }]);
        let (_, opened) = open_card_value("r", &filed);
        assert_eq!(opened["pictures"][0]["attachmentId"], "att-a");
        assert_eq!(opened["pictures"][0]["caption"], "Before");
        assert_eq!(opened["pictures"][1]["attachmentId"], "att-b");
        // Another tree's content: locked, the clear part kept.
        let other = enet_core::sealed::generate_tree_key();
        let foreign = enet_core::sealed::seal_card(&other.public, 1, &json!({ "kind": "report", "summary": "x" })).unwrap();
        let body = enet_core::sealed::seal_body(&other.public, 1, "hidden").unwrap();
        let result = transform.result(
            "exponential_sessions",
            &json!({}),
            json!({ "content": [{ "type": "text", "text": json!({ "id": "m", "rootId": "r2", "body": body, "card": foreign }).to_string() }] }),
        );
        let out: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(out["body"], sealing::LOCKED_TEXT);
        assert_eq!(out["card"], json!({ "kind": "report", "locked": sealing::LOCKED_TEXT }));
        assert_eq!(fake.keyring.root_of("m").as_deref(), Some("r2"), "id + rootId pairs are learned");
        sealing::uninstall();
    }

    // --- card builders vs the TS reference --------------------------------

    fn stack() -> Value {
        json!({ "root": { "id": "s", "component": "Stack", "children": [
            { "id": "r", "component": "Row", "props": { "title": "Tests" }, "slots": { "trailing": { "id": "g", "component": "Badge", "props": { "text": "Green" } } } }
        ] } })
    }

    #[test]
    fn the_plan_card_matches_build_plan_card() {
        let input = ShowInput::parse(&json!({
            "plan": { "text": "  ## Plan\n![a](upload:web-a) ", "questions": [{ "id": " scope ", "prompt": "Scope?", "options": [{ "label": " All ", "description": " every board " }, { "label": "One" }] }] },
            "pictures": [{ "label": "web-a", "caption": "  " }],
        }))
        .unwrap();
        let card = build_show_card(&input).unwrap();
        assert_eq!(
            card,
            json!({
                "kind": "plan",
                "plan": "## Plan\n![a](upload:web-a)",
                "pictures": [{ "label": "web-a", "attachmentId": null, "width": null, "height": null, "caption": null }],
                "questions": [{ "id": "scope", "prompt": "Scope?", "multi": false, "custom": true, "options": [
                    { "label": "All", "description": "every board", "picture": null },
                    { "label": "One", "description": null, "picture": null },
                ] }],
                "surface": null,
                "decision": null, "fresh": null, "feedback": null, "answers": null, "surfaceData": null,
            })
        );
        let undeclared = ShowInput::parse(&json!({ "plan": { "text": "![x](upload:web-b)" } })).unwrap();
        assert_eq!(
            build_show_card(&undeclared).unwrap_err(),
            "plan's ![…](upload:web-b) names the picture \"web-b\", which is not in pictures: declare it as {label: \"web-b\"}"
        );
        let twice = ShowInput::parse(&json!({ "plan": { "text": "p", "questions": [
            { "id": "a", "prompt": "A", "options": [{ "label": "x" }] }, { "id": "a", "prompt": "B", "options": [{ "label": "y" }] }
        ] } }))
        .unwrap();
        assert_eq!(build_show_card(&twice).unwrap_err(), "questions: the id \"a\" is used twice");
    }

    #[test]
    fn the_question_card_matches_build_question_card() {
        let input = ShowInput::parse(&json!({
            "ask": { "question": "Which?", "options": [{ "label": "A", "picture": "web-a" }, { "label": "B" }], "multi": true },
            "pictures": [{ "label": "web-a", "caption": "Look" }],
            "story": false,
        }))
        .unwrap();
        let card = build_show_card(&input).unwrap();
        assert_eq!(
            card,
            json!({
                "kind": "question", "question": "Which?", "options": ["A", "B"],
                "optionPictures": ["web-a", null],
                "pictures": [{ "label": "web-a", "attachmentId": null, "width": null, "height": null, "caption": "Look" }],
                "multi": true, "custom": true, "surface": null, "answer": null, "answers": null, "surfaceData": null,
                "story": false,
            })
        );
        let closed = ShowInput::parse(&json!({ "ask": { "question": "Q", "options": [], "custom": false } })).unwrap();
        assert_eq!(
            build_show_card(&closed).unwrap_err(),
            "options has no options and custom: false, so it cannot be answered: add options or allow custom"
        );
        let both = ShowInput::parse(&json!({ "ask": { "question": "Q", "options": [] }, "plan": { "text": "p" } })).unwrap();
        assert_eq!(build_show_card(&both).unwrap_err(), "Pass ask or plan, not both: one card per call.");
        assert!(ShowInput::parse(&json!({ "bogus": 1 })).unwrap_err().contains("unrecognized key \"bogus\""));
        assert!(ShowInput::parse(&json!({ "pictures": [{ "label": "web chats" }] })).is_err());
    }

    #[test]
    fn the_surface_card_matches_build_surface_card_and_the_show_checks() {
        let input = ShowInput::parse(&json!({ "surface": stack(), "title": " Results ", "description": " Before and after " })).unwrap();
        let card = build_show_card(&input).unwrap();
        let mut surface = stack();
        surface["data"] = Value::Null;
        assert_eq!(card, json!({ "kind": "surface", "surface": surface, "title": "Results", "description": "Before and after" }));

        // Pictures alone: a Card holding the title.
        let pictures = ShowInput::parse(&json!({ "pictures": [{ "label": "web-chats" }, { "label": "ios-chats" }] })).unwrap();
        let card = build_show_card(&pictures).unwrap();
        assert_eq!(card["surface"]["root"]["children"][0]["props"]["text"], "web-chats, ios-chats");
        assert_eq!(card["pictures"].as_array().unwrap().len(), 2);
        assert!(card.get("story").is_none());

        // Text only alone is refused; a Card root is refused.
        let text = ShowInput::parse(&json!({ "surface": { "root": { "id": "s", "component": "Stack", "children": [{ "id": "t", "component": "Text", "props": { "text": "hi" } }] } } })).unwrap();
        assert_eq!(build_show_card(&text).unwrap_err(), SHOW_TEXT_ONLY_ERROR);
        // Not a node / an unknown component: the reducer's verdict.
        let bad = ShowInput::parse(&json!({ "surface": { "root": { "component": "Stack" } } })).unwrap();
        assert_eq!(build_show_card(&bad).unwrap_err(), "surface.root must be an Exponential UI node: {id, component, props?, children?}");
        let unknown = ShowInput::parse(&json!({ "surface": { "root": { "id": "s", "component": "Stack", "children": [{ "id": "x", "component": "Blink" }] } } })).unwrap();
        assert!(build_show_card(&unknown).unwrap_err().starts_with("surface is not valid in the Exponential UI core catalog: "));
        let empty = ShowInput::parse(&json!({})).unwrap();
        assert_eq!(build_show_card(&empty).unwrap_err(), "Pass surface, ask, plan or pictures.");
    }

    #[test]
    fn answered_cards_match_card_answer() {
        let plan = json!({ "kind": "plan", "plan": "p", "questions": [{ "id": "scope", "options": [{ "label": "All" }], "custom": false }] });
        let input = json!({ "decision": "approved", "fresh": true, "feedback": " go ", "answers": { "scope": [" All "] } });
        let out = answered_card(&plan, input.as_object().unwrap()).unwrap();
        assert_eq!(out["decision"], "approved");
        assert_eq!(out["fresh"], true);
        assert_eq!(out["feedback"], "go");
        assert_eq!(out["answers"], json!({ "scope": ["All"] }));
        let missing = json!({ "decision": "approved" });
        assert_eq!(answered_card(&plan, missing.as_object().unwrap()).unwrap_err(), "Answer every question to approve");
        let question = json!({ "kind": "question", "question": "Q", "options": ["A", "B"], "multi": true });
        let out = answered_card(&question, json!({ "answers": ["A", "B"] }).as_object().unwrap()).unwrap();
        assert_eq!(out["answer"], "A, B");
        let free = answered_card(&question, json!({ "answer": " typed " }).as_object().unwrap()).unwrap();
        assert_eq!(free["answer"], "typed");
        assert_eq!(free["answers"], json!(["typed"]));
    }

    #[test]
    fn issue_refs_finds_each_identifier_once() {
        assert_eq!(issue_refs("Fix #EXP-12 and #APP-3, again #EXP-12; not a#EXP-1 or #exp-2 or #EXP-"), ["EXP-12", "APP-3"]);
    }

    #[test]
    fn the_plan_ask_text_matches_bus_ask_text() {
        let card = json!({ "kind": "plan", "plan": "1. One", "questions": [{ "id": "scope", "prompt": "Scope?", "options": [{ "label": "All" }, { "label": "One" }] }] });
        assert_eq!(
            ask_text("s-1", &card),
            "Proposes a plan:\n\n1. One\n\nQuestions:\n- scope: Scope? (All | One)\n\nDecide with exponential_sessions_send({sessionId: \"s-1\", answer: {…}}) where answer = {decision: \"approved\" | \"rejected\", answers: {\"<question id>\": [\"<option>\"]}, feedback: \"<text>\"} if it is yours to decide; otherwise end your turn, a person decides in the card."
        );
    }

    #[test]
    fn upload_embeds_finds_every_label() {
        assert_eq!(upload_embeds("![a](upload:web-a) and ![b](upload:ios-b) ![c](upload:bad label)"), ["web-a", "ios-b"]);
    }
}
