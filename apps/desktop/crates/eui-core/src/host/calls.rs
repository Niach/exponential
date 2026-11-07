//! Round 9 §6.5–§6.8 + §18.3: the ONE place a host's routed calls live, so
//! gpui, Swift and Compose run the same rules as the TS `ExponentialHost`
//! (`fixtures/a2ui10-functions.json` `sequences`): the agent channel, the
//! policy in front, reactive de-duplication and caching, pending calls and
//! their timeout, the once-per-lifetime errors of a surface and the cap of
//! [`MAX_SENT_ISSUES`] per message.
//!
//! Synchronous and clock-free for the host: what it must do comes back as
//! [`CallEffect`]s and outbound messages ([`CallHub::take_outbox`]); a
//! [`SharedCalls`] handle is what a [`crate::surface::Surface`] resolves
//! through ([`route_handle`]).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::contract::{call_agent_function_message, error_message_to, ErrorTarget};
use super::policy::{decide_route, routed_call_key, FunctionPolicy, RouteDecision, RouteDecisionInput};
use super::router::HostRouter;
use crate::catalog::A2UI_VERSION;
use crate::data::{Failure, Position, RouteAnswer, RoutedCall};
use crate::surface::Surface;
use crate::types::ReduceIssue;

/// §18.3: an issue past this many per message is summed up.
pub const MAX_SENT_ISSUES: usize = 32;

const EXPRESSION_ERROR: &str = "EXPRESSION_ERROR";
const EXECUTION_ERROR: &str = "EXECUTION_ERROR";
const INVALID_FUNCTION_CALL: &str = "INVALID_FUNCTION_CALL";
const UNKNOWN_FUNCTION: &str = "UNKNOWN_FUNCTION";

/// A routed REACTIVE call: in flight, or settled for the surface's lifetime.
#[derive(Debug, Clone, PartialEq)]
enum ReactiveRoute {
    Pending,
    Settled(RouteAnswer),
}

#[derive(Debug, Default)]
struct SurfaceCalls {
    catalog_id: Option<String>,
    send_data_model: bool,
    agent: bool,
    package_functions: Option<Vec<String>>,
    counter: u64,
    routes: HashMap<String, ReactiveRoute>,
    sent: HashSet<String>,
}

#[derive(Debug)]
enum Pending {
    Reactive { surface_id: String, key: String },
    Action { surface_id: String },
}

#[derive(Debug)]
struct PendingCall {
    name: String,
    deadline_ms: f64,
    kind: Pending,
}

/// What the host must do for a reactive call ([`CallHub::take_effects`]):
/// answer with [`CallHub::consent`] / [`CallHub::settle_local`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "effect")]
pub enum CallEffect {
    /// The consent hook decides.
    #[serde(rename_all = "camelCase")]
    Ask { ticket: u64, surface_id: String, name: String, catalog_id: String, args: Map<String, Value> },
    /// Run the registered function.
    #[serde(rename_all = "camelCase")]
    RunLocal { ticket: u64, surface_id: String, name: String, catalog_id: String, args: Map<String, Value> },
}

/// Where a press goes ([`CallHub::press`]). `Ask`: call again with the consent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "route")]
pub enum PressRoute {
    /// The policy refused (the error is in the outbox).
    Deny,
    /// No function, no agent (the error is in the outbox).
    NotFound,
    /// Ask the consent hook, then press again.
    Ask,
    /// Run the registered (or built-in) function; a failure goes to [`CallHub::local_failed`].
    Local,
    /// Sent to the agent: wait for this id in [`CallHub::observe`] / [`CallHub::expire`].
    #[serde(rename_all = "camelCase")]
    Agent { function_call_id: String },
}

/// An ACTION call the agent answered, or that timed out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettledCall {
    pub function_call_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    /// The failure's message (its `error` is already in the outbox).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The host's routed calls, one per renderer lifetime.
#[derive(Debug, Default)]
pub struct CallHub {
    /// The transport's far end reads renderer → agent messages.
    agent: bool,
    policy: Option<FunctionPolicy>,
    registered: HashSet<String>,
    available: Vec<String>,
    surfaces: HashMap<String, SurfaceCalls>,
    pending: IndexMap<String, PendingCall>,
    /// Tickets of reactive calls waiting for the host: (surface, key, call).
    tickets: HashMap<u64, (String, String, RoutedCall)>,
    next_ticket: u64,
    outbox: Vec<Value>,
    effects: Vec<CallEffect>,
    dirty: Vec<String>,
    /// A fixed clock (tests); `None` = the wall clock.
    pub clock_ms: Option<f64>,
}

/// The handle a host and its surfaces share.
pub type SharedCalls = Arc<Mutex<CallHub>>;

/// `ResolveContext::route` for one surface's reactive positions.
pub fn route_handle(calls: &SharedCalls, surface_id: &str) -> Arc<dyn Fn(RoutedCall) -> RouteAnswer + Send + Sync> {
    let (calls, surface_id) = (calls.clone(), surface_id.to_string());
    Arc::new(move |call| calls.lock().unwrap_or_else(|e| e.into_inner()).reactive(&surface_id, call))
}

impl CallHub {
    pub fn new(agent: bool) -> Self {
        CallHub { agent, next_ticket: 1, ..Default::default() }
    }

    pub fn shared(agent: bool) -> SharedCalls {
        Arc::new(Mutex::new(Self::new(agent)))
    }

    pub fn set_agent(&mut self, agent: bool) {
        self.agent = agent;
    }

    pub fn set_policy(&mut self, policy: Option<FunctionPolicy>) {
        self.policy = policy;
    }

    /// The functions the host registered (its registry changed).
    pub fn set_registered<S: AsRef<str>>(&mut self, names: &[S]) {
        self.registered = names.iter().map(|n| n.as_ref().to_string()).collect();
    }

    fn now(&self) -> f64 {
        self.clock_ms.unwrap_or_else(crate::format::now_ms)
    }

    /// Round 9 §6.5: does the surface have an agent?
    pub fn has_agent(&self, surface_id: &str) -> bool {
        self.agent && self.surfaces.get(surface_id).is_some_and(|s| s.agent)
    }

    /// Every op the router answered, AFTER `route` returned (a template's
    /// surface knows its package by then): follows the surfaces and settles
    /// the `respond` ops. The ACTION calls that settled come back.
    pub fn observe(&mut self, ops: &[Value], router: &HostRouter) -> Vec<SettledCall> {
        self.available = router.available_catalog_ids();
        let mut settled = Vec::new();
        for op in ops {
            let sid = op.get("surfaceId").and_then(Value::as_str);
            match (op.get("op").and_then(Value::as_str), sid) {
                (Some("create"), Some(sid)) => {
                    let info = router.surface(sid);
                    let package = router.package_id_of(sid).and_then(|id| router.package(&id).cloned());
                    let functions = package.and_then(|p| p.get("functions").and_then(Value::as_array).map(|l| l.iter().filter_map(|f| f.as_str().map(str::to_string)).collect()));
                    self.surfaces.insert(
                        sid.to_string(),
                        SurfaceCalls {
                            catalog_id: info.map(|i| i.catalog_id.clone()).filter(|c| !c.is_empty()),
                            send_data_model: op.get("sendDataModel") == Some(&Value::Bool(true)),
                            agent: info.is_some_and(|i| i.agent),
                            package_functions: functions,
                            ..Default::default()
                        },
                    );
                }
                (Some("delete"), Some(sid)) => {
                    self.surfaces.remove(sid);
                }
                (Some("respond"), _) => settled.extend(self.respond(op)),
                _ => {}
            }
        }
        settled
    }

    /// §10: an `agentFunctionResponse` settles its call; an unknown or
    /// settled id is a host issue, never sent (`None`).
    fn respond(&mut self, op: &Value) -> Option<SettledCall> {
        let id = op.get("functionCallId")?.as_str()?.to_string();
        let answer = match op.get("error") {
            Some(e) => Err(Failure { function_call_id: Some(id.clone()), ..Failure::new(EXPRESSION_ERROR, e["message"].as_str().unwrap_or_default()) }),
            None => Ok(op.get("value").cloned().unwrap_or(Value::Null)),
        };
        let pending = self.pending.shift_remove(&id)?;
        self.finish(id, pending, answer)
    }

    fn finish(&mut self, id: String, pending: PendingCall, answer: RouteAnswer) -> Option<SettledCall> {
        match pending.kind {
            Pending::Reactive { surface_id, key } => {
                self.settle_reactive(&surface_id, &key, answer);
                None
            }
            Pending::Action { .. } => Some(match answer {
                Ok(value) => SettledCall { function_call_id: id, value: Some(value), error: None },
                Err(failure) => {
                    self.outbox.push(error_message_to(&failure.code, ErrorTarget::FunctionCall(&id), &failure.message, None));
                    SettledCall { function_call_id: id, value: None, error: Some(failure.message) }
                }
            }),
        }
    }

    fn settle_reactive(&mut self, surface_id: &str, key: &str, answer: RouteAnswer) {
        if let Some(s) = self.surfaces.get_mut(surface_id) {
            s.routes.insert(key.to_string(), ReactiveRoute::Settled(answer.clone()));
        }
        if let Err(failure) = &answer {
            self.report_failure(surface_id, failure, None);
        }
        if !self.dirty.iter().any(|d| d == surface_id) {
            self.dirty.push(surface_id.to_string());
        }
    }

    /// The calls whose deadline passed (§6.5: 30 s): a timeout is a failure.
    pub fn expire(&mut self) -> Vec<SettledCall> {
        let now = self.now();
        let expired: Vec<String> = self.pending.iter().filter(|(_, p)| p.deadline_ms <= now).map(|(id, _)| id.clone()).collect();
        let mut settled = Vec::new();
        for id in expired {
            if let Some(pending) = self.pending.shift_remove(&id) {
                let failure = Failure { function_call_id: Some(id.clone()), ..Failure::new(EXPRESSION_ERROR, format!("{} timed out", pending.name)) };
                settled.extend(self.finish(id, pending, Err(failure)));
            }
        }
        settled
    }

    /// The next deadline, for the host's one timer.
    pub fn next_deadline_ms(&self) -> Option<f64> {
        self.pending.values().map(|p| p.deadline_ms).reduce(f64::min)
    }

    fn input(&self, surface_id: &str, name: &str, catalog_id: Option<&str>, position: Position) -> RouteDecisionInput {
        let s = self.surfaces.get(surface_id);
        RouteDecisionInput {
            name: name.to_string(),
            catalog_id: catalog_id.map(str::to_string),
            surface_catalog_id: s.and_then(|s| s.catalog_id.clone()),
            available: self.available.clone(),
            position,
            policy: self.policy.clone(),
            package_functions: s.and_then(|s| s.package_functions.clone()),
            registered: self.registered.contains(name),
            agent: self.has_agent(surface_id),
        }
    }

    /// One `callAgentFunction`, its pending entry and its timeout.
    fn call_agent(&mut self, surface_id: &str, name: &str, catalog_id: &str, args: &Map<String, Value>, kind: Pending) -> String {
        let id = {
            let s = self.surfaces.entry(surface_id.to_string()).or_default();
            s.counter += 1;
            format!("{surface_id}:{}", s.counter)
        };
        let mut all = Map::new();
        let declared = crate::functions::declared_function(catalog_id, name).and_then(|f| f.get("args")).and_then(Value::as_object);
        for (k, schema) in declared.into_iter().flatten() {
            if let (Some(default), false) = (schema.get("default"), args.contains_key(k)) {
                all.insert(k.clone(), default.clone());
            }
        }
        all.extend(args.clone());
        self.pending.insert(id.clone(), PendingCall { name: name.to_string(), deadline_ms: self.now() + crate::limits::AGENT_FUNCTION_TIMEOUT_MS as f64, kind });
        self.outbox.push(call_agent_function_message(surface_id, &id, name, catalog_id, all));
        id
    }

    /// `ResolveOptions.route` of one surface's REACTIVE positions: identical
    /// calls share one in-flight call, a settled answer stays for the
    /// surface's lifetime, a pending position reads `null`.
    pub fn reactive(&mut self, surface_id: &str, call: RoutedCall) -> RouteAnswer {
        let Some(s) = self.surfaces.get(surface_id) else { return Err(Failure::new(UNKNOWN_FUNCTION, format!("no function {}", call.name))) };
        let key = routed_call_key(&call.catalog_id, &call.name, &Value::Object(call.args.clone()));
        match s.routes.get(&key) {
            Some(ReactiveRoute::Settled(answer)) => return answer.clone(),
            Some(ReactiveRoute::Pending) => return Ok(Value::Null),
            None => {}
        }
        let mut input = self.input(surface_id, &call.name, Some(&call.catalog_id), Position::Reactive);
        input.catalog_id = Some(call.catalog_id.clone());
        let route = decide_route(&input);
        if let RouteDecision::Fail { code, message } = route {
            return Err(Failure::new(&code, message));
        }
        if let Some(s) = self.surfaces.get_mut(surface_id) {
            s.routes.insert(key.clone(), ReactiveRoute::Pending);
        }
        match route {
            RouteDecision::Ask { .. } | RouteDecision::Local { .. } => {
                let ask = matches!(route, RouteDecision::Ask { .. });
                let ticket = self.next_ticket.max(1);
                self.next_ticket = ticket + 1;
                let (name, catalog_id, args) = (call.name.clone(), call.catalog_id.clone(), call.args.clone());
                self.tickets.insert(ticket, (surface_id.to_string(), key, call));
                let surface_id = surface_id.to_string();
                self.effects.push(if ask { CallEffect::Ask { ticket, surface_id, name, catalog_id, args } } else { CallEffect::RunLocal { ticket, surface_id, name, catalog_id, args } });
            }
            _ => {
                self.call_agent(surface_id, &call.name, &call.catalog_id, &call.args, Pending::Reactive { surface_id: surface_id.to_string(), key });
            }
        }
        Ok(Value::Null)
    }

    /// The consent hook's answer for an `Ask` effect.
    pub fn consent(&mut self, ticket: u64, allowed: bool) {
        let Some((surface_id, key, call)) = self.tickets.remove(&ticket) else { return };
        if !allowed {
            let failure = Failure::new(INVALID_FUNCTION_CALL, format!("{} was not allowed", call.name));
            return self.settle_reactive(&surface_id, &key, Err(failure));
        }
        if self.registered.contains(&call.name) {
            let (name, catalog_id, args) = (call.name.clone(), call.catalog_id.clone(), call.args.clone());
            self.tickets.insert(ticket, (surface_id.clone(), key, call));
            self.effects.push(CallEffect::RunLocal { ticket, surface_id, name, catalog_id, args });
        } else {
            self.call_agent(&surface_id, &call.name, &call.catalog_id, &call.args, Pending::Reactive { surface_id: surface_id.clone(), key });
        }
    }

    /// The registered function's answer for a `RunLocal` effect.
    pub fn settle_local(&mut self, ticket: u64, answer: Result<Value, String>) {
        let Some((surface_id, key, _)) = self.tickets.remove(&ticket) else { return };
        self.settle_reactive(&surface_id, &key, answer.map_err(|message| Failure::new(EXPRESSION_ERROR, message)));
    }

    /// An ACTION `functionCall` to a host function: the policy, the consent
    /// (`consent` = its answer on the second press), then the local function
    /// or a NEW `callAgentFunction` (never shared or cached).
    pub fn press(&mut self, surface_id: &str, name: &str, catalog_id: Option<&str>, args: &Map<String, Value>, consent: Option<bool>) -> PressRoute {
        let route = decide_route(&self.input(surface_id, name, catalog_id, Position::Action));
        let denied = |hub: &mut Self| {
            hub.outbox.push(error_message_to(INVALID_FUNCTION_CALL, ErrorTarget::Surface(surface_id), &format!("{name} was not allowed"), None));
            PressRoute::Deny
        };
        let target = match route {
            RouteDecision::Fail { code, .. } if code == INVALID_FUNCTION_CALL => return denied(self),
            RouteDecision::Fail { code, message } => {
                let code = if code == UNKNOWN_FUNCTION { UNKNOWN_FUNCTION.to_string() } else { code };
                self.outbox.push(error_message_to(&code, ErrorTarget::Surface(surface_id), &message, None));
                return PressRoute::NotFound;
            }
            RouteDecision::Ask { catalog_id } => match consent {
                None => return PressRoute::Ask,
                Some(false) => return denied(self),
                Some(true) if self.registered.contains(name) => return PressRoute::Local,
                Some(true) => catalog_id,
            },
            RouteDecision::Agent { catalog_id } => catalog_id,
            RouteDecision::Local { .. } | RouteDecision::Catalog { .. } | RouteDecision::Index => return PressRoute::Local,
        };
        let function_call_id = self.call_agent(surface_id, name, &target, args, Pending::Action { surface_id: surface_id.to_string() });
        PressRoute::Agent { function_call_id }
    }

    /// §6.8: a local function of an action threw: it stops, EXECUTION_ERROR.
    pub fn local_failed(&mut self, surface_id: &str, message: &str) {
        self.outbox.push(error_message_to(EXECUTION_ERROR, ErrorTarget::Surface(surface_id), message, None));
    }

    /// §6.8: a reactive evaluation failed; sent once (like a reducer issue).
    pub fn report_failure(&mut self, surface_id: &str, failure: &Failure, path: Option<&str>) {
        let Some(s) = self.surfaces.get_mut(surface_id) else { return };
        let path = path.or(failure.path.as_deref());
        let key = format!("{}\u{0}{}\u{0}{}", failure.code, failure.function_call_id.as_deref().or(path).unwrap_or(""), failure.message);
        if !s.sent.insert(key) {
            return;
        }
        self.outbox.push(match &failure.function_call_id {
            Some(id) => error_message_to(&failure.code, ErrorTarget::FunctionCall(id), &failure.message, None),
            None => error_message_to(&failure.code, ErrorTarget::Surface(surface_id), &failure.message, path),
        });
    }

    /// §5.2 + §18.3: the surface's coded reducer issues, each once for its
    /// lifetime, at most [`MAX_SENT_ISSUES`] per message.
    pub fn send_issues(&mut self, surface_id: &str, issues: &[ReduceIssue]) {
        let Some(s) = self.surfaces.get_mut(surface_id) else { return };
        let (mut sent, mut dropped) = (0, 0);
        let mut out = Vec::new();
        for issue in issues.iter().filter(|i| !i.code.is_empty()) {
            let key = format!("{}\u{0}{}\u{0}{}", issue.code, issue.path, issue.message);
            if s.sent.contains(&key) {
                continue;
            }
            if sent >= MAX_SENT_ISSUES {
                dropped += 1;
                continue;
            }
            s.sent.insert(key);
            sent += 1;
            out.push(error_message_to(&issue.code, ErrorTarget::Surface(surface_id), &issue.message, Some(&issue.path)));
        }
        if dropped > 0 {
            out.push(error_message_to("VALIDATION_FAILED", ErrorTarget::Surface(surface_id), &format!("{dropped} more issues were dropped"), Some("/")));
        }
        self.outbox.extend(out);
    }

    /// The one call a host makes after a surface reduced or laid out: its
    /// issues and reactive failures go to the outbox.
    pub fn flush_surface(&mut self, surface_id: &str, surface: &mut Surface) {
        self.send_issues(surface_id, &surface.issues().to_vec());
        for failure in surface.take_failures() {
            self.report_failure(surface_id, &failure, None);
        }
    }

    /// §1.3: the metadata beside a message about a `sendDataModel` surface;
    /// `data` = the surface's data model.
    pub fn data_model_metadata(&self, message: &Value, data: &dyn Fn(&str) -> Option<Value>) -> Option<Value> {
        let about = ["action", "callAgentFunction", "error"].iter().find_map(|k| message.get(*k).and_then(|b| b.get("surfaceId")).and_then(Value::as_str))?;
        if !self.surfaces.get(about).is_some_and(|s| s.send_data_model) {
            return None;
        }
        let mut surfaces = Map::new();
        surfaces.insert(about.to_string(), data(about).unwrap_or(json!({})));
        Some(json!({"a2uiRendererDataModel": {"version": A2UI_VERSION, "surfaces": surfaces}}))
    }

    /// What must leave on the transport, in order.
    pub fn take_outbox(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.outbox)
    }

    /// What the host must run for reactive calls.
    pub fn take_effects(&mut self) -> Vec<CallEffect> {
        std::mem::take(&mut self.effects)
    }

    /// The surfaces whose routed value settled: bind again.
    pub fn take_dirty(&mut self) -> Vec<String> {
        std::mem::take(&mut self.dirty)
    }
}
