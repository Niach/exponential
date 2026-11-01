//! `reduce(state, event)`: a pure port of `packages/eagent/src/reduce.ts`,
//! Exponential's server behaviour as a function. No I/O, no clock (the
//! envelope's `at` is now); a refusal leaves the state as it was.

use std::collections::{BTreeMap, HashSet};

use ecore::{is_valid_id, ErrorCode, Failure};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::copy::{self, fill, rules, text};
use crate::js;
use crate::model::*;

const MIN: i64 = 60_000;
const HOUR: i64 = 60 * MIN;

type Step<T> = Result<T, Failure>;

fn refuse<T>(code: ErrorCode, message: &str) -> Step<T> {
    Err(Failure::new(code, message))
}

struct Ctx {
    state: HubState,
    effects: Vec<Effect>,
    now: i64,
}

impl Ctx {
    fn row(&self, id: &str) -> &HubSession {
        &self.state.sessions[id]
    }

    fn row_mut(&mut self, id: &str) -> &mut HubSession {
        self.state.sessions.get_mut(id).expect("a known session")
    }
}

// ── shared rules ─────────────────────────────────────────────────────────────

pub fn is_live(session: &HubSession) -> bool {
    session.status != Status::Ended
}

fn is_working(session: &HubSession) -> bool {
    matches!(session.status, Status::Running | Status::Idle)
}

/// A tree's acting agent: its conversation root's agent, else the agent that started it.
pub fn tree_agent_id(root: &HubSession) -> Option<String> {
    root.agent_id.clone().or_else(|| root.started_by_agent_id.clone())
}

/// A reader of `listener` may not see `origin`.
pub fn ask_hidden(listener: &HubSession, origin: &HubSession) -> bool {
    !origin.shared && (listener.shared || listener.user_id != origin.user_id)
}

/// The fields of a root thread `reuse_refusal` reads.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReuseRoot {
    pub status: Status,
    pub needs_input: bool,
    pub ended_at: Option<i64>,
    pub last_turn_at: Option<i64>,
    pub last_activity_at: Option<i64>,
    pub updated_at: i64,
}

impl From<&HubSession> for ReuseRoot {
    fn from(row: &HubSession) -> Self {
        Self {
            status: row.status,
            needs_input: row.needs_input,
            ended_at: row.ended_at,
            last_turn_at: row.last_turn_at,
            last_activity_at: row.last_activity_at,
            updated_at: row.updated_at,
        }
    }
}

/// Whether an AGENT's message may reuse this root thread: None = reuse, else the reason.
pub fn reuse_refusal(root: &ReuseRoot, children: usize, now: i64, awaits_caller: bool) -> Option<String> {
    if root.needs_input || awaits_caller {
        return None;
    }
    let reuse = &rules().reuse;
    let age = |at: i64| now - at;
    if root.status == Status::Ended && age(root.ended_at.unwrap_or(root.updated_at)) >= reuse.max_ended_hours * HOUR {
        return Some(fill(text(&["reuseEnded"]), &[("hours", &reuse.max_ended_hours.to_string())]));
    }
    if age(root.last_turn_at.or(root.last_activity_at).unwrap_or(root.updated_at)) >= reuse.max_idle_hours * HOUR {
        return Some(fill(text(&["reuseIdle"]), &[("hours", &reuse.max_idle_hours.to_string())]));
    }
    if children >= reuse.max_children {
        return Some(fill(text(&["reuseCrowded"]), &[("children", &children.to_string())]));
    }
    None
}

/// The heartbeat's live list: valid ids only, each once; `dropped` = the invalid entries.
pub fn clean_live_session_ids(ids: &[Value]) -> (Vec<String>, usize) {
    let valid: Vec<&str> = ids.iter().filter_map(Value::as_str).filter(|id| is_valid_id(id)).collect();
    let mut seen = HashSet::new();
    let unique = valid.iter().filter(|id| seen.insert(**id)).map(|id| id.to_string()).collect();
    (unique, ids.len() - valid.len())
}

/// Every session id, oldest first (createdAt, then id).
fn ordered(state: &HubState) -> Vec<String> {
    let mut rows: Vec<&HubSession> = state.sessions.values().collect();
    rows.sort_by(|a, b| a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id)));
    rows.into_iter().map(|row| row.id.clone()).collect()
}

fn session(state: &HubState, id: Option<&str>) -> Step<HubSession> {
    match id.and_then(|id| state.sessions.get(id)) {
        Some(row) => Ok(row.clone()),
        None => refuse(ErrorCode::NotFound, text(&["notFound"])),
    }
}

fn is_pending_input(message: &HubMessage) -> bool {
    message.author_kind == AuthorKind::User
        || message.author_session_id.as_ref().is_some_and(|author| *author != message.session_id)
}

fn open_cards(state: &HubState, session_id: &str) -> bool {
    state.messages.iter().any(|m| {
        m.session_id == session_id && !m.answered && matches!(m.card, Some(Card::Question { .. } | Card::Plan { .. }))
    })
}

fn blocked_kind(row: &HubSession) -> Option<BlockedKind> {
    row.blocked.as_ref().map(|b| b.kind)
}

/// `blocked` cleared (only a flag of `kind`, when named) and needsInput re-derived from open cards.
fn clear_blocked(ctx: &mut Ctx, id: &str, kind: Option<BlockedKind>) {
    if kind.is_some() && blocked_kind(ctx.row(id)) != kind {
        return;
    }
    let needs_input = open_cards(&ctx.state, id);
    let now = ctx.now;
    let row = ctx.row_mut(id);
    row.blocked = None;
    row.needs_input = needs_input;
    row.updated_at = now;
}

fn interruption(kind: BlockedKind, now: i64, resume: bool, resume_since: Option<i64>) -> Blocked {
    Blocked {
        kind,
        since: now,
        window: Field::Null,
        resets_at: Field::Null,
        resume: Field::Value(resume),
        resume_since: Field::from_option(resume_since),
        extra: Map::new(),
    }
}

fn flag(kind: BlockedKind, now: i64) -> Blocked {
    Blocked {
        kind,
        since: now,
        window: Field::Null,
        resets_at: Field::Null,
        resume: Field::Missing,
        resume_since: Field::Missing,
        extra: Map::new(),
    }
}

struct Post {
    author_kind: AuthorKind,
    author_session_id: Option<String>,
    body: String,
    card: Option<Card>,
    sealed: bool,
    waking: bool,
}

fn post(ctx: &mut Ctx, to: &str, message: Post) -> String {
    ctx.state.seq += 1;
    let id = format!("m{}", ctx.state.seq);
    ctx.state.messages.push(HubMessage {
        id: id.clone(),
        session_id: to.to_string(),
        author_kind: message.author_kind,
        author_session_id: message.author_session_id.clone(),
        body: message.body.clone(),
        card: message.card.clone(),
        sealed: message.sealed,
        pending: message.waking,
        answered: false,
        at: ctx.now,
    });
    ctx.effects.push(Effect::Post {
        message_id: id.clone(),
        to: to.to_string(),
        author_kind: message.author_kind,
        author_session_id: message.author_session_id,
        body: message.body,
        card: message.card,
        sealed: message.sealed,
        waking: message.waking,
    });
    id
}

fn system_card(ctx: &mut Ctx, to: &str, card: Card) -> String {
    post(
        ctx,
        to,
        Post { author_kind: AuthorKind::System, author_session_id: None, body: String::new(), card: Some(card), sealed: false, waking: false },
    )
}

// ── the bus ──────────────────────────────────────────────────────────────────

/// Who a transition of `child` reaches: its parent (whatever its status), then every live watcher by id.
pub fn bus_listeners(state: &HubState, child: &HubSession) -> Vec<HubSession> {
    let parent = child.parent_id.as_ref().and_then(|id| state.sessions.get(id));
    // `sessions` is a BTreeMap: its values already come by id.
    let watchers = state.sessions.values().filter(|row| {
        is_live(row) && row.id != child.id && Some(&row.id) != child.parent_id.as_ref() && row.wake_on.contains(&child.id)
    });
    parent.into_iter().chain(watchers).cloned().collect()
}

#[derive(Clone)]
struct Line {
    body: String,
    card: Option<Card>,
    sealed: bool,
}

impl Line {
    fn new(body: String, card: Option<Card>) -> Self {
        Self { body, card, sealed: false }
    }
}

/// Sealed to the child's tree: in-tree listeners get a byte copy, other trees their `copies[rootId]`, else `hidden`.
struct Sealed {
    in_tree: Option<Line>,
    copies: BTreeMap<String, String>,
}

struct Deliver<'a> {
    author_kind: AuthorKind,
    body: String,
    card: Option<Card>,
    hidden: Option<Line>,
    waking: bool,
    fallback_to_self: bool,
    skip: Option<&'a dyn Fn(&HubState, &HubSession) -> bool>,
    sealed: Option<Sealed>,
}

impl Deliver<'_> {
    fn new(author_kind: AuthorKind, body: String, card: Option<Card>, waking: bool) -> Self {
        Self { author_kind, body, card, hidden: None, waking, fallback_to_self: false, skip: None, sealed: None }
    }
}

fn deliver(ctx: &mut Ctx, child: &HubSession, message: Deliver) {
    let all = bus_listeners(&ctx.state, child);
    let listeners: Vec<&HubSession> = all.iter().filter(|l| !message.skip.is_some_and(|skip| skip(&ctx.state, l))).collect();
    for listener in listeners {
        let plain = Line::new(message.body.clone(), message.card.clone());
        let hide = message.hidden.is_some() && ask_hidden(listener, child);
        let mut line = if hide { message.hidden.clone().unwrap() } else { plain.clone() };
        if let (Some(sealed), false) = (&message.sealed, hide) {
            line = if listener.root_id == child.root_id {
                sealed.in_tree.clone().unwrap_or(Line { sealed: true, ..plain })
            } else if let Some(copy) = sealed.copies.get(&listener.root_id) {
                Line { body: copy.clone(), card: message.card.clone(), sealed: true }
            } else {
                message.hidden.clone().unwrap_or(Line::new(String::new(), message.card.clone()))
            };
        }
        post(
            ctx,
            &listener.id,
            Post {
                author_kind: message.author_kind,
                author_session_id: Some(child.id.clone()),
                body: line.body,
                card: line.card,
                sealed: line.sealed,
                waking: message.waking && is_live(listener),
            },
        );
    }
    if all.is_empty() && message.fallback_to_self {
        post(
            ctx,
            &child.id,
            Post { author_kind: message.author_kind, author_session_id: None, body: message.body, card: message.card, sealed: false, waking: false },
        );
    }
}

fn bus_line(ctx: &mut Ctx, child: &HubSession, event: &str, body: String, waking: bool, pull_request_id: Option<&str>, self_fallback: bool) {
    let card = EventCard::session(event, &child.id);
    let silent = rules().assistant_silent_events.iter().any(|e| e == event);
    let skip = |state: &HubState, listener: &HubSession| {
        listener.agent_id.as_ref().is_some_and(|agent| {
            listener.id == listener.root_id && state.agents.get(agent).is_some_and(|a| a.role == AgentRole::Assistant)
        })
    };
    let posted = match pull_request_id.filter(|id| !id.is_empty()) {
        Some(id) => EventCard { pull_request_id: Some(id.to_string()), ..card.clone() },
        None => card.clone(),
    };
    let mut message = Deliver::new(AuthorKind::System, body, Some(Card::Event(posted)), waking);
    if event == "pr_opened" {
        message.hidden = Some(Line::new(copy::private_text(&child.id, text(&["privatePrLead"])), Some(Card::Event(card))));
    }
    message.fallback_to_self = self_fallback;
    if silent {
        message.skip = Some(&skip);
    }
    deliver(ctx, child, message);
}

fn bus_relay_turn(ctx: &mut Ctx, child: &HubSession, body: &str, sealed: bool, copies: BTreeMap<String, String>) {
    let text_ = js::trim(body);
    if text_.is_empty() {
        return;
    }
    // A team agent's conversation that only WATCHES the child hears its ask, wall, PR and report, never each turn.
    let skip = |_: &HubState, listener: &HubSession| listener.agent_id.is_some() && Some(&listener.id) != child.parent_id.as_ref();
    let mut message = Deliver::new(AuthorKind::Agent, text_.to_string(), None, true);
    message.hidden = Some(Line::new(copy::private_text(&child.id, text(&["privateReplyLead"])), None));
    message.skip = Some(&skip);
    if sealed {
        message.sealed = Some(Sealed { in_tree: None, copies });
    }
    deliver(ctx, child, message);
}

fn bus_ask(ctx: &mut Ctx, child: &HubSession, message_id: &str, card: &Card) {
    let mirror = Card::Ask { session_id: child.id.clone(), message_id: message_id.to_string() };
    let mut message = Deliver::new(AuthorKind::Agent, copy::ask_text(&child.id, card), Some(mirror.clone()), true);
    message.hidden = Some(Line::new(copy::ask_hidden_text(&child.id, card), Some(mirror)));
    deliver(ctx, child, message);
}

fn report_summary(ctx: &Ctx, child: &HubSession, summary: Option<&str>, summary_sealed: bool) -> String {
    let mut text_ = if summary_sealed { String::new() } else { js::trim(summary.unwrap_or("")).to_string() };
    if text_.is_empty() {
        let last = ctx
            .state
            .messages
            .iter()
            .filter(|m| m.session_id == child.id && m.author_kind == AuthorKind::Agent && m.card.is_none() && m.author_session_id.is_none())
            .last();
        text_ = match last {
            Some(m) if !m.sealed => js::trim(&m.body).to_string(),
            _ => String::new(),
        };
    }
    let text_ = if text_.is_empty() { rules().report.fallback.clone() } else { text_ };
    js::slice(&text_, rules().report.max)
}

fn bus_report(ctx: &mut Ctx, child: &HubSession, summary: Option<&str>, summary_sealed: bool) {
    let text_ = report_summary(ctx, child, summary, summary_sealed);
    let card = Card::Report { session_id: child.id.clone(), summary: text_.clone(), is_final: true };
    let lead = text(&["privateReportLead"]);
    let mut message = Deliver::new(AuthorKind::Agent, text_, Some(card), true);
    message.hidden = Some(Line::new(
        copy::private_text(&child.id, lead),
        Some(Card::Report { session_id: child.id.clone(), summary: lead.to_string(), is_final: true }),
    ));
    deliver(ctx, child, message);
    for row in ctx.state.sessions.values_mut() {
        row.wake_on.retain(|id| *id != child.id);
    }
}

/// `watcher` watches `ids`: live sessions of its team its account may see, never itself.
fn watch_sessions(ctx: &mut Ctx, watcher_id: &str, ids: &[String]) -> Vec<String> {
    let Some(watcher) = ctx.state.sessions.get(watcher_id) else { return vec![] };
    let mut seen = HashSet::new();
    let live: Vec<String> = ids
        .iter()
        .filter(|id| seen.insert(id.as_str()))
        .filter(|id| *id != watcher_id)
        .filter_map(|id| ctx.state.sessions.get(id))
        .filter(|row| is_live(row) && row.team_id == watcher.team_id && (row.shared || row.user_id == watcher.user_id))
        .map(|row| row.id.clone())
        .collect();
    let watcher = ctx.row_mut(watcher_id);
    let mut seen = HashSet::new();
    watcher.wake_on = watcher.wake_on.iter().chain(&live).filter(|id| seen.insert(id.as_str())).cloned().collect();
    live
}

// ── the tree ─────────────────────────────────────────────────────────────────

fn assert_tree_room(state: &HubState, root_id: &str, depth: i64) -> Step<()> {
    let caps = &rules().caps;
    if depth > caps.tree_max_depth {
        return refuse(ErrorCode::Invalid, &fill(text(&["tooDeep"]), &[("max", &caps.tree_max_depth.to_string())]));
    }
    let live = state.sessions.values().filter(|row| row.root_id == root_id && is_live(row)).count();
    if live >= caps.tree_max_sessions {
        return refuse(ErrorCode::Limit, &fill(text(&["tooMany"]), &[("max", &caps.tree_max_sessions.to_string())]));
    }
    Ok(())
}

struct NewSession {
    id: String,
    parent_id: Option<String>,
    root_id: String,
    depth: i64,
    team_id: String,
    user_id: String,
    shared: bool,
    device_id: String,
    started_by_agent_id: Option<String>,
}

fn insert_session(ctx: &mut Ctx, row: NewSession) -> Step<HubSession> {
    if !is_valid_id(&row.id) {
        return refuse(ErrorCode::Invalid, "Invalid session id");
    }
    if ctx.state.sessions.contains_key(&row.id) {
        return refuse(ErrorCode::Conflict, &format!("Session {} exists", row.id));
    }
    let created = HubSession {
        id: row.id,
        parent_id: row.parent_id,
        root_id: row.root_id,
        depth: row.depth,
        team_id: row.team_id,
        user_id: row.user_id,
        shared: row.shared,
        agent_id: None,
        started_by_agent_id: row.started_by_agent_id,
        device_id: row.device_id,
        status: Status::Queued,
        busy: false,
        background: false,
        needs_input: false,
        blocked: None,
        wake_on: vec![],
        created_at: ctx.now,
        started_at: None,
        ended_at: None,
        last_turn_at: None,
        last_activity_at: None,
        updated_at: ctx.now,
    };
    ctx.state.sessions.insert(created.id.clone(), created.clone());
    Ok(created)
}

/// A send's message: the caller session's input (agent-authored) or a person's; blank = none.
fn post_send(ctx: &mut Ctx, to: &str, text_: &str, sealed: bool, caller: Option<&HubSession>) -> Option<String> {
    if js::trim(text_).is_empty() {
        return None;
    }
    Some(post(
        ctx,
        to,
        Post {
            author_kind: if caller.is_some() { AuthorKind::Agent } else { AuthorKind::User },
            author_session_id: caller.map(|c| c.id.clone()),
            body: text_.to_string(),
            card: None,
            sealed,
            waking: true,
        },
    ))
}

fn caller_tree_agent(state: &HubState, caller: Option<&HubSession>) -> Option<String> {
    caller.and_then(|c| tree_agent_id(state.sessions.get(&c.root_id).unwrap_or(c)))
}

fn default_session(key: &str) -> String {
    rules().defaults["session"][key].as_str().unwrap_or_default().to_string()
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendData {
    id: Option<String>,
    parent_id: Option<String>,
    session_id: Option<String>,
    user_id: Option<String>,
    device_id: Option<String>,
    shared: Option<bool>,
    text: Option<String>,
    sealed: Option<bool>,
    /// The id a fresh thread or a continuation takes.
    fresh_id: Option<String>,
}

fn sent(id: String, message_id: Option<String>) -> StepResult {
    StepResult { session_id: Some(id), message_id: Field::from_option(message_id), ..StepResult::default() }
}

fn start_root(ctx: &mut Ctx, data: &SendData, caller: Option<&HubSession>) -> Step<StepResult> {
    let caller_root = caller.and_then(|c| ctx.state.sessions.get(&c.root_id).cloned());
    let device_id = data.device_id.clone().or_else(|| {
        caller_root.as_ref().filter(|root| tree_agent_id(root).is_some()).map(|root| root.device_id.clone())
    });
    let Some(device_id) = device_id else { return refuse(ErrorCode::Invalid, "Pick a device for the new session") };
    let id = data.id.clone().or_else(|| data.fresh_id.clone()).unwrap_or_default();
    let root = insert_session(
        ctx,
        NewSession {
            id: id.clone(),
            parent_id: None,
            root_id: id,
            depth: 0,
            team_id: caller.map_or_else(|| default_session("teamId"), |c| c.team_id.clone()),
            user_id: data.user_id.clone().or_else(|| caller.map(|c| c.user_id.clone())).unwrap_or_else(|| default_session("userId")),
            shared: data.shared.unwrap_or(true),
            device_id,
            started_by_agent_id: caller_tree_agent(&ctx.state, caller),
        },
    )?;
    let message_id = post_send(ctx, &root.id, data.text.as_deref().unwrap_or(""), data.sealed == Some(true), caller);
    if let Some(caller) = caller {
        watch_sessions(ctx, &caller.id, std::slice::from_ref(&root.id));
        system_card(ctx, &caller.id, Card::ThreadStarted { session_id: root.id.clone() });
    }
    Ok(sent(root.id, message_id))
}

fn start_child(ctx: &mut Ctx, data: &SendData, caller: Option<&HubSession>) -> Step<StepResult> {
    let parent = session(&ctx.state, data.parent_id.as_deref())?;
    assert_tree_room(&ctx.state, &parent.root_id, parent.depth + 1)?;
    let root = session(&ctx.state, Some(&parent.root_id))?;
    let child = insert_session(
        ctx,
        NewSession {
            id: data.id.clone().unwrap_or_default(),
            parent_id: Some(parent.id.clone()),
            root_id: parent.root_id.clone(),
            depth: parent.depth + 1,
            team_id: parent.team_id.clone(),
            user_id: data.user_id.clone().or_else(|| caller.map(|c| c.user_id.clone())).unwrap_or_else(|| parent.user_id.clone()),
            shared: root.shared,
            device_id: data.device_id.clone().unwrap_or_else(|| parent.device_id.clone()),
            started_by_agent_id: None,
        },
    )?;
    let message_id = post_send(ctx, &child.id, data.text.as_deref().unwrap_or(""), data.sealed == Some(true), caller);
    system_card(ctx, &parent.id, Card::Session { session_id: child.id.clone() });
    Ok(sent(child.id, message_id))
}

/// #FEED-172: the target's tree has the last word with the caller: its newest agent-authored line in the caller's
/// conversation is newer than the caller's newest message into the target's tree. A reply never forks.
pub fn awaits_caller(state: &HubState, caller: &HubSession, target: &HubSession) -> bool {
    let in_tree = |id: Option<&String>| id.and_then(|id| state.sessions.get(id)).is_some_and(|row| row.root_id == target.root_id);
    let mut from: Option<i64> = None;
    let mut to: Option<i64> = None;
    for m in &state.messages {
        if m.session_id == caller.id && m.author_kind == AuthorKind::Agent && in_tree(m.author_session_id.as_ref()) {
            from = from.max(Some(m.at));
        }
        if m.author_session_id.as_ref() == Some(&caller.id) && in_tree(Some(&m.session_id)) {
            to = to.max(Some(m.at));
        }
    }
    match (from, to) {
        (Some(from), Some(to)) => from > to,
        (Some(_), None) => true,
        _ => false,
    }
}

fn agent_fresh_thread(ctx: &mut Ctx, data: &SendData, caller: &HubSession, target: &HubSession) -> Step<Option<StepResult>> {
    if caller.root_id == target.root_id || target.parent_id.is_some() || target.agent_id.is_some() {
        return Ok(None);
    }
    if js::trim(data.text.as_deref().unwrap_or("")).is_empty() || data.sealed == Some(true) || target.needs_input {
        return Ok(None);
    }
    if caller_tree_agent(&ctx.state, Some(caller)).is_none() {
        return Ok(None);
    }
    let children = ctx.state.sessions.values().filter(|row| row.root_id == target.id && row.id != target.id).count();
    let awaits = awaits_caller(&ctx.state, caller, target);
    let Some(reason) = reuse_refusal(&ReuseRoot::from(target), children, ctx.now, awaits) else { return Ok(None) };
    let fresh = SendData {
        id: data.fresh_id.clone(),
        device_id: data.device_id.clone().or_else(|| Some(target.device_id.clone())),
        ..data.clone()
    };
    let outcome = start_root(ctx, &fresh, Some(caller))?;
    Ok(Some(StepResult { warning: Some(fill(text(&["freshThread"]), &[("id", &target.id), ("reason", &reason)])), ..outcome }))
}

fn send_message(ctx: &mut Ctx, data: &SendData, caller: Option<&HubSession>) -> Step<StepResult> {
    let target = session(&ctx.state, data.session_id.as_deref())?;
    if let Some(caller) = caller {
        if let Some(fresh) = agent_fresh_thread(ctx, data, caller, &target)? {
            return Ok(fresh);
        }
    }
    let text_ = data.text.as_deref().unwrap_or("");
    let sealed = data.sealed == Some(true);
    if is_live(&target) || target.agent_id.is_some() {
        let message_id = post_send(ctx, &target.id, text_, sealed, caller);
        return Ok(sent(target.id, message_id));
    }
    if js::trim(text_).is_empty() {
        return refuse(ErrorCode::Invalid, text(&["endedNeedsMessage"]));
    }
    let Some(parent_id) = target.parent_id.clone() else {
        // An ended thread ROOT revives in place: the same row idles with the message pending.
        let now = ctx.now;
        let row = ctx.row_mut(&target.id);
        row.status = Status::Idle;
        row.ended_at = None;
        row.busy = false;
        row.background = false;
        row.needs_input = false;
        row.blocked = None;
        row.updated_at = now;
        let message_id = post_send(ctx, &target.id, text_, sealed, caller);
        return Ok(sent(target.id, message_id));
    };
    // An ended CHILD continues as its sibling: same parent and depth.
    let depth = if target.depth == 0 { 1 } else { target.depth };
    assert_tree_room(&ctx.state, &target.root_id, depth)?;
    let root = session(&ctx.state, Some(&target.root_id))?;
    let child = insert_session(
        ctx,
        NewSession {
            id: data.fresh_id.clone().unwrap_or_default(),
            parent_id: Some(parent_id.clone()),
            root_id: target.root_id.clone(),
            depth,
            team_id: target.team_id.clone(),
            user_id: data.user_id.clone().or_else(|| caller.map(|c| c.user_id.clone())).unwrap_or_else(|| target.user_id.clone()),
            shared: root.shared,
            device_id: data.device_id.clone().unwrap_or_else(|| target.device_id.clone()),
            started_by_agent_id: None,
        },
    )?;
    let message_id = post_send(ctx, &child.id, text_, sealed, caller);
    system_card(ctx, &parent_id, Card::Session { session_id: child.id.clone() });
    Ok(sent(child.id, message_id))
}

/// The session and its live descendants (a root = its whole tree), deepest first, oldest first.
fn live_subtree(state: &HubState, top: &HubSession) -> Vec<String> {
    let mut rows: Vec<&HubSession> = if top.parent_id.is_none() {
        state.sessions.values().filter(|row| row.root_id == top.id).collect()
    } else {
        let mut out = vec![];
        let mut stack = vec![top.id.as_str()];
        while let Some(id) = stack.pop() {
            out.push(&state.sessions[id]);
            stack.extend(state.sessions.values().filter(|kid| kid.parent_id.as_deref() == Some(id)).map(|kid| kid.id.as_str()));
        }
        out
    };
    rows.retain(|row| is_live(row));
    rows.sort_by(|a, b| b.depth.cmp(&a.depth).then(a.created_at.cmp(&b.created_at)).then_with(|| a.id.cmp(&b.id)));
    rows.into_iter().map(|row| row.id.clone()).collect()
}

struct Named<'a> {
    id: &'a str,
    summary: Option<&'a str>,
    summary_sealed: bool,
}

/// Each row ends (never an agent's conversation), closes its ask mirrors, says `session_ended` on itself and
/// reports into its parent and watchers; `named` carries the summary.
fn end_rows(ctx: &mut Ctx, ids: &[String], named: Option<Named>) -> Vec<String> {
    let mut ended = vec![];
    let now = ctx.now;
    for id in ids {
        let row = ctx.row(id);
        if row.agent_id.is_some() || !is_live(row) {
            continue;
        }
        let row = ctx.row_mut(id);
        row.status = Status::Ended;
        row.ended_at = Some(now);
        row.busy = false;
        row.background = false;
        row.needs_input = false;
        row.blocked = None;
        row.updated_at = now;
        let row = row.clone();
        for m in &mut ctx.state.messages {
            if matches!(&m.card, Some(Card::Ask { session_id, .. }) if *session_id == row.id) {
                m.answered = true;
            }
        }
        ended.push(row.id.clone());
        system_card(ctx, &row.id, Card::Event(EventCard::session("session_ended", &row.id)));
        match &named {
            Some(named) if named.id == row.id => bus_report(ctx, &row, named.summary, named.summary_sealed),
            _ => bus_report(ctx, &row, None, false),
        }
    }
    for id in &ended {
        ctx.effects.push(Effect::Kill { session_id: id.clone() });
    }
    ended
}

// ── the device ───────────────────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaimData {
    session_id: Option<String>,
    device_id: Option<String>,
}

fn claim(ctx: &mut Ctx, data: ClaimData) -> Step<StepResult> {
    let row = session(&ctx.state, data.session_id.as_deref())?;
    if data.device_id.as_ref() != Some(&row.device_id) {
        return refuse(ErrorCode::Conflict, text(&["otherDevice"]));
    }
    if !is_live(&row) {
        return refuse(ErrorCode::Conflict, text(&["claimEnded"]));
    }
    let queued = row.status == Status::Queued;
    let first = queued && row.started_at.is_none();
    let now = ctx.now;
    {
        let row = ctx.row_mut(&row.id);
        if queued {
            row.status = Status::Running;
        }
        row.started_at = row.started_at.or(Some(now));
        if blocked_kind(row) == Some(BlockedKind::Unclaimed) {
            row.blocked = None;
            row.needs_input = false;
        }
        row.updated_at = now;
    }
    if first {
        // The brief's Request IS the first input: the claim delivers it, so the device never feeds it twice.
        let input = ctx.state.messages.iter_mut().find(|m| m.session_id == row.id && m.card.is_none() && is_pending_input(m));
        let (message_id, sealed) = match input {
            Some(input) => {
                input.pending = false;
                (Some(input.id.clone()), input.sealed)
            }
            None => (None, false),
        };
        ctx.effects.push(Effect::Brief { session_id: row.id.clone(), message_id, sealed });
    }
    if queued {
        let row = ctx.row(&row.id).clone();
        bus_line(ctx, &row, "session_claimed", text(&["claimed"]).to_string(), false, None, false);
    }
    Ok(StepResult::session(&row.id))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Wall {
    kind: BlockedKind,
    since: Option<i64>,
    #[serde(default)]
    handled: Option<bool>,
    #[serde(default)]
    window: Field<String>,
    #[serde(default)]
    resets_at: Field<String>,
    #[serde(default)]
    resume: Field<bool>,
    #[serde(default)]
    resume_since: Field<i64>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReportData {
    session_id: Option<String>,
    #[serde(default)]
    device_id: Field<String>,
    status: Option<Status>,
    busy: Option<bool>,
    background: Option<bool>,
    needs_input: Option<bool>,
    #[serde(default)]
    blocked: Field<Wall>,
    lost: Option<String>,
    mid_turn: Option<bool>,
}

fn report(ctx: &mut Ctx, data: ReportData) -> Step<StepResult> {
    let Some(row) = data.session_id.as_ref().and_then(|id| ctx.state.sessions.get(id)).cloned() else {
        return Ok(StepResult::default());
    };
    // An ended row stays ended; a device the session moved away from is ignored.
    let moved = match &data.device_id {
        Field::Missing => false,
        Field::Null => true,
        Field::Value(device) => *device != row.device_id,
    };
    if !is_live(&row) || moved {
        return Ok(StepResult::default());
    }
    let now = ctx.now;
    let mid_turn = data.mid_turn.unwrap_or(row.busy || row.status == Status::Running);
    if data.lost.as_deref().is_some_and(|lost| !lost.is_empty()) {
        // The process went away without the agent finishing: never "done", never ended. Mid-turn = the silent
        // `restarted` interruption (its device continues it; the watchdog's resume rule catches one it does not).
        let kind = blocked_kind(&row);
        let flaggable = kind.is_none_or(|k| k == BlockedKind::Stuck || k.is_interruption());
        if kind == Some(BlockedKind::Stuck) {
            clear_blocked(ctx, &row.id, Some(BlockedKind::Stuck));
        }
        let row = ctx.row_mut(&row.id);
        row.status = Status::Idle;
        row.busy = false;
        row.background = false;
        if mid_turn && flaggable {
            row.blocked = Some(interruption(BlockedKind::Restarted, now, true, Some(now)));
        }
        row.last_activity_at = Some(now);
        row.updated_at = now;
        return Ok(StepResult::session(&row.id));
    }
    // A thread ROOT that finished a turn and an agent's conversation survive their process (idle);
    // a child, or a root that never finished a turn (a refused launch), ends.
    let status = match data.status {
        Some(Status::Ended) if row.agent_id.is_some() || (row.parent_id.is_none() && row.last_turn_at.is_some()) => Some(Status::Idle),
        status => status,
    };
    if status == Some(Status::Ended) {
        let ended_ids = end_rows(ctx, std::slice::from_ref(&row.id), None);
        return Ok(StepResult { ended_ids: Some(ended_ids), ..StepResult::session(&row.id) });
    }
    let (blocked, handled) = match data.blocked {
        Field::Missing => (Field::Missing, false),
        Field::Null => (Field::Null, false),
        Field::Value(wall) => (
            Field::Value(Blocked {
                kind: wall.kind,
                since: wall.since.unwrap_or(now),
                window: wall.window,
                resets_at: wall.resets_at,
                resume: wall.resume,
                resume_since: wall.resume_since,
                extra: wall.extra,
            }),
            wall.handled == Some(true),
        ),
    };
    let newly_blocked = blocked.value().is_some() && row.blocked.is_none() && !handled;
    let next_blocked = match &blocked {
        Field::Missing => match &row.blocked {
            Some(b) if b.kind == BlockedKind::Stuck || b.kind.is_interruption() => None,
            other => other.clone(),
        },
        other => other.value().cloned(),
    };
    let turn_ended = (data.busy == Some(false) && row.busy) || status == Some(Status::Idle);
    let was_blocked = row.blocked.is_some();
    let needs_input = if next_blocked.is_some() {
        Some(true)
    } else if data.needs_input.is_some() {
        data.needs_input
    } else if was_blocked {
        Some(false)
    } else {
        None
    };
    let cleared = was_blocked && next_blocked.is_none();
    {
        let row = ctx.row_mut(&row.id);
        if let Some(status) = status {
            row.status = status;
        }
        if let Some(busy) = data.busy {
            row.busy = busy;
        } else if status == Some(Status::Idle) {
            row.busy = false;
        }
        if data.busy == Some(false) || (data.busy.is_none() && status == Some(Status::Idle)) {
            row.background = false;
        } else if let Some(background) = data.background {
            row.background = background;
        }
        if let Some(needs_input) = needs_input {
            row.needs_input = needs_input;
        }
        row.blocked = next_blocked;
        if turn_ended {
            row.last_turn_at = Some(now);
        }
        row.last_activity_at = Some(now);
        row.updated_at = now;
    }
    if cleared {
        clear_blocked(ctx, &row.id, None);
    }
    if newly_blocked {
        let wall = blocked.value().unwrap();
        let body = copy::blocked_text(wall.window.value().map(String::as_str), wall.resets_at.value().map(String::as_str));
        let row = ctx.row(&row.id).clone();
        bus_line(ctx, &row, "session_blocked", body, true, None, true);
    }
    Ok(StepResult::session(&row.id))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReplyData {
    session_id: Option<String>,
    body: Option<String>,
    card: Option<Card>,
    sealed: Option<bool>,
    copies: Option<BTreeMap<String, String>>,
}

fn reply(ctx: &mut Ctx, data: ReplyData) -> Step<StepResult> {
    let Some(id) = data.session_id.filter(|id| ctx.state.sessions.contains_key(id)) else { return Ok(StepResult::default()) };
    let body = data.body.unwrap_or_default();
    if data.card.is_none() && js::trim(&body).is_empty() {
        return refuse(ErrorCode::Invalid, text(&["replyEmpty"]));
    }
    if data.card.as_ref().is_some_and(|card| !matches!(card, Card::Question { .. } | Card::Plan { .. })) {
        return refuse(ErrorCode::Invalid, "A reply's card is a question or a plan");
    }
    let sealed = data.sealed == Some(true);
    let message_id = post(
        ctx,
        &id,
        Post { author_kind: AuthorKind::Agent, author_session_id: None, body: body.clone(), card: data.card.clone(), sealed, waking: false },
    );
    let now = ctx.now;
    let row = ctx.row_mut(&id);
    row.last_activity_at = Some(now);
    if data.card.is_some() {
        row.needs_input = true;
        row.updated_at = now;
    }
    let row = row.clone();
    match &data.card {
        Some(card) => bus_ask(ctx, &row, &message_id, card),
        None => bus_relay_turn(ctx, &row, &body, sealed, data.copies.unwrap_or_default()),
    }
    Ok(StepResult { message_id: Field::Value(message_id), ..StepResult::session(&id) })
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConsumeData {
    session_id: Option<String>,
    message_ids: Option<Vec<String>>,
}

fn consume(ctx: &mut Ctx, data: ConsumeData) -> Step<StepResult> {
    let ids: HashSet<String> = data.message_ids.unwrap_or_default().into_iter().collect();
    for m in &mut ctx.state.messages {
        if Some(&m.session_id) == data.session_id.as_ref() && ids.contains(&m.id) && m.pending {
            m.pending = false;
        }
    }
    Ok(StepResult { session_id: data.session_id, ..StepResult::default() })
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BeatData {
    device_id: Option<String>,
    live_session_ids: Option<Vec<Value>>,
    activity: Option<Vec<Activity>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Activity {
    #[serde(default)]
    session_id: Value,
    #[serde(default)]
    at: Value,
}

fn heartbeat(ctx: &mut Ctx, data: BeatData) -> Step<StepResult> {
    let now = ctx.now;
    let device_id = data.device_id.unwrap_or_default();
    let live = data.live_session_ids.map(|ids| clean_live_session_ids(&ids).0);
    ctx.state.devices.insert(device_id.clone(), HubDevice { id: device_id.clone(), last_seen_at: now, live_session_ids: live.clone() });
    let hosted: Vec<String> = ordered(&ctx.state).into_iter().filter(|id| ctx.row(id).device_id == device_id).collect();
    // An offline-parked session: a listed one survived (busy again), an unlisted mid-turn one waits for its
    // device to continue it from now on (`resumeSince`), anything else is simply live again.
    for id in &hosted {
        let Some(blocked) = ctx.row(id).blocked.clone().filter(|b| b.kind == BlockedKind::DeviceOffline) else { continue };
        let resume = blocked.resume == Field::Value(true);
        match &live {
            Some(live) if resume && live.contains(id) => {
                let row = ctx.row_mut(id);
                row.status = Status::Running;
                row.busy = true;
                row.background = false;
                row.updated_at = now;
                clear_blocked(ctx, id, Some(BlockedKind::DeviceOffline));
            }
            Some(_) if resume => {
                if blocked.resume_since.is_nullish() {
                    let row = ctx.row_mut(id);
                    row.blocked = Some(Blocked { resume_since: Field::Value(now), ..blocked });
                    row.updated_at = now;
                }
            }
            _ => clear_blocked(ctx, id, Some(BlockedKind::DeviceOffline)),
        }
    }
    let mut latest: Vec<(String, i64)> = vec![];
    for entry in data.activity.unwrap_or_default() {
        let Some(id) = entry.session_id.as_str().filter(|id| is_valid_id(id)) else { continue };
        let Some(at) = entry.at.as_f64().filter(|at| at.is_finite()) else { continue };
        let at = at.min(now as f64) as i64;
        match latest.iter_mut().find(|(known, _)| known == id) {
            Some((_, prev)) => *prev = (*prev).max(at),
            None => latest.push((id.to_string(), at.max(0))),
        }
    }
    for (id, at) in latest {
        let Some(row) = ctx.state.sessions.get_mut(&id) else { continue };
        if row.device_id == device_id && is_working(row) && row.last_activity_at.is_none_or(|known| known < at) {
            row.last_activity_at = Some(at);
        }
    }
    if let Some(live) = &live {
        // A busy session the host no longer runs: its process died under a living device (silent).
        for id in &hosted {
            let row = ctx.row_mut(id);
            if row.status != Status::Running || !row.busy || (!live.is_empty() && live.contains(id)) {
                continue;
            }
            row.status = Status::Idle;
            row.busy = false;
            row.background = false;
            row.blocked = Some(interruption(BlockedKind::Restarted, now, true, Some(now)));
            row.updated_at = now;
        }
        for id in &hosted {
            let row = ctx.row(id);
            if live.contains(id) && is_working(row) && blocked_kind(row) == Some(BlockedKind::Stuck) {
                clear_blocked(ctx, id, Some(BlockedKind::Stuck));
            }
        }
    }
    Ok(StepResult::default())
}

// ── the watchdog ─────────────────────────────────────────────────────────────

fn online(ctx: &Ctx, device_id: &str) -> bool {
    ctx.state.devices.get(device_id).is_some_and(|d| d.last_seen_at >= ctx.now - rules().watchdog.online_window_seconds * 1000)
}

fn liveness(row: &HubSession) -> i64 {
    row.updated_at.max(row.last_activity_at.unwrap_or(row.updated_at))
}

fn tick(ctx: &mut Ctx) -> Step<StepResult> {
    let w = &rules().watchdog;
    let now = ctx.now;
    // Unclaimed: queued past unclaimedMinutes, flagged once.
    for id in ordered(&ctx.state) {
        let row = ctx.row(&id);
        if row.status != Status::Queued || row.started_at.is_some() || row.blocked.is_some() || row.created_at >= now - w.unclaimed_minutes * MIN {
            continue;
        }
        let row = ctx.row_mut(&id);
        row.blocked = Some(flag(BlockedKind::Unclaimed, now));
        row.needs_input = true;
        row.updated_at = now;
        let row = row.clone();
        bus_line(ctx, &row, "session_unclaimed", copy::unclaimed_text(), true, None, true);
    }
    // Stalled input: its host is nudged again every tick while it lasts.
    let mut nudged = HashSet::new();
    for id in ordered(&ctx.state) {
        let row = ctx.row(&id);
        if !is_working(row) || blocked_kind(row) == Some(BlockedKind::DeviceOffline) || !ctx.state.devices.contains_key(&row.device_id) {
            continue;
        }
        let stalled =
            ctx.state.messages.iter().any(|m| m.session_id == id && m.pending && m.at < now - w.nudge_minutes * MIN && is_pending_input(m));
        if stalled && nudged.insert(row.device_id.clone()) {
            ctx.effects.push(Effect::Nudge { device_id: row.device_id.clone() });
        }
    }
    // Stuck: busy with no sign of life, unless its online host lists it live.
    for id in ordered(&ctx.state) {
        let row = ctx.row(&id);
        let alive = liveness(row);
        if !is_working(row) || !row.busy || row.blocked.is_some() {
            continue;
        }
        if row.background && alive >= now - w.background_stuck_minutes * MIN {
            continue;
        }
        if alive >= now - w.stuck_minutes * MIN {
            continue;
        }
        let listed = ctx.state.devices.get(&row.device_id).and_then(|d| d.live_session_ids.as_ref()).is_some_and(|ids| ids.contains(&id));
        if online(ctx, &row.device_id) && listed {
            continue;
        }
        let row = ctx.row_mut(&id);
        row.blocked = Some(flag(BlockedKind::Stuck, now));
        row.needs_input = true;
        row.updated_at = now;
        let row = row.clone();
        bus_line(ctx, &row, "session_stuck", copy::stuck_text(copy::silent_minutes(Some(alive), now)), true, None, true);
    }
    // Offline host: a busy run is interrupted (resume, silent); a claimed one between turns just reads idle.
    for id in ordered(&ctx.state) {
        let row = ctx.row(&id);
        if !is_working(row) || !row.busy || row.blocked.is_some() || online(ctx, &row.device_id) {
            continue;
        }
        let row = ctx.row_mut(&id);
        row.status = Status::Idle;
        row.busy = false;
        row.background = false;
        row.blocked = Some(interruption(BlockedKind::DeviceOffline, now, true, None));
        row.updated_at = now;
    }
    for id in ordered(&ctx.state) {
        let row = ctx.row(&id);
        if row.status == Status::Running && !row.busy && !online(ctx, &row.device_id) {
            let row = ctx.row_mut(&id);
            row.status = Status::Idle;
            row.updated_at = now;
        }
    }
    // Unresumed: an interruption its device has not continued resumeMinutes after it could = on a person, said once.
    for id in ordered(&ctx.state) {
        let row = ctx.row(&id);
        let Some(b) = &row.blocked else { continue };
        if !is_working(row) || !b.kind.is_interruption() || row.needs_input || b.resume != Field::Value(true) {
            continue;
        }
        if !b.resume_since.value().is_some_and(|since| *since < now - w.resume_minutes * MIN) {
            continue;
        }
        let row = ctx.row_mut(&id);
        row.needs_input = true;
        row.updated_at = now;
        let row = row.clone();
        bus_line(ctx, &row, "session_interrupted", copy::unresumed_text(), true, None, true);
    }
    // A usage wall whose reset passed: cleared, silently.
    for id in ordered(&ctx.state) {
        let row = ctx.row(&id);
        let Some(b) = row.blocked.as_ref().filter(|b| b.kind == BlockedKind::RateLimit) else { continue };
        let Some(resets_at) = b.resets_at.value().filter(|r| !r.is_empty()) else { continue };
        if is_working(row) && js::date_parse(resets_at).is_some_and(|at| at <= now) {
            clear_blocked(ctx, &id, Some(BlockedKind::RateLimit));
        }
    }
    Ok(StepResult::default())
}

// ── wake-ups: schedules and events ───────────────────────────────────────────

fn trees_moved(ctx: &Ctx, agent: &HubAgent, since: i64) -> bool {
    let rows: Vec<&HubSession> = ctx.state.sessions.values().filter(|row| row.team_id == agent.team_id).collect();
    let moved = |row: &&HubSession| row.last_activity_at.is_some_and(|at| at > since);
    if agent.role == AgentRole::Assistant {
        // An Assistant watches all its member's chats; its own conversation's turns are not news, its children are.
        return rows.iter().any(|row| {
            row.user_id == agent.owner_id && (row.agent_id.as_ref() != Some(&agent.id) || row.id != row.root_id) && moved(row)
        });
    }
    let started_by = |row: &HubSession| row.started_by_agent_id.as_ref() == Some(&agent.id);
    let roots: HashSet<&str> = rows
        .iter()
        .filter(|row| row.id == row.root_id && (started_by(row) || row.agent_id.as_ref() == Some(&agent.id)))
        .map(|row| row.id.as_str())
        .collect();
    let tree: Vec<&&HubSession> = rows
        .iter()
        .filter(|row| roots.contains(row.root_id.as_str()) && (started_by(&ctx.state.sessions[&row.root_id]) || row.id != row.root_id))
        .collect();
    tree.is_empty() || tree.into_iter().any(moved)
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScheduleData {
    agent_id: Option<String>,
    due: Option<Vec<Due>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Due {
    trigger_id: String,
    cron: String,
    tz: Option<String>,
    at: i64,
}

fn is_tick(card: &Option<Card>) -> bool {
    matches!(card, Some(Card::Event(e)) if e.event == "trigger_fired" && e.event_id == Field::Null && e.trigger_id != Field::Null)
}

fn schedule(ctx: &mut Ctx, data: ScheduleData) -> Step<StepResult> {
    let Some(agent) = data.agent_id.as_ref().and_then(|id| ctx.state.agents.get(id)).cloned() else {
        return refuse(ErrorCode::NotFound, "Agent not found");
    };
    let schedules: Vec<&Trigger> = agent.triggers.iter().filter(|t| t.kind == TriggerKind::Schedule && t.enabled != Some(false)).collect();
    let due: Vec<Due> = data.due.unwrap_or_default().into_iter().filter(|d| schedules.iter().any(|t| t.id == d.trigger_id)).collect();
    let last_fired = schedules.iter().filter_map(|t| agent.fired_at.get(&t.id)).max().copied();
    if due.is_empty() || last_fired.is_some_and(|since| !trees_moved(ctx, &agent, since)) {
        return Ok(StepResult::default());
    }
    let caught_up = rules().trigger.caught_up_after_minutes * MIN;
    let body = due.iter().map(|d| copy::schedule_text(&d.cron, d.tz.as_deref(), ctx.now - d.at > caught_up)).collect::<Vec<_>>().join("\n");
    let conversation = session(&ctx.state, Some(&agent.conversation_id))?;
    // Never stacks: an unread tick takes the newer text; a read one nobody answered is re-armed; else a new card.
    let tick = ctx.state.messages.iter().rposition(|m| m.session_id == conversation.id && is_tick(&m.card));
    let answered = tick.is_some_and(|i| {
        let t = &ctx.state.messages[i];
        ctx.state.messages.iter().any(|m| m.session_id == conversation.id && m.author_kind == AuthorKind::User && m.id != t.id && m.at > t.at)
    });
    let message_id = match tick.filter(|i| ctx.state.messages[*i].pending || !answered) {
        Some(i) => {
            let tick = &mut ctx.state.messages[i];
            let rearm = !tick.pending;
            tick.body = body.clone();
            tick.pending = true;
            let id = tick.id.clone();
            ctx.effects.push(Effect::Edit { message_id: id.clone(), body, waking: true });
            if rearm {
                ctx.effects.push(Effect::Nudge { device_id: conversation.device_id.clone() });
            }
            id
        }
        None => {
            let card = EventCard {
                event: "trigger_fired".into(),
                session_id: None,
                pull_request_id: None,
                agent_id: Some(agent.id.clone()),
                trigger_id: Field::Value(due[0].trigger_id.clone()),
                event_id: Field::Null,
                name: Field::Null,
            };
            let id = post(
                ctx,
                &conversation.id,
                Post { author_kind: AuthorKind::User, author_session_id: None, body, card: Some(Card::Event(card)), sealed: false, waking: true },
            );
            ctx.effects.push(Effect::Nudge { device_id: conversation.device_id.clone() });
            id
        }
    };
    let fired = &mut ctx.state.agents.get_mut(&agent.id).expect("the agent").fired_at;
    for d in &due {
        fired.insert(d.trigger_id.clone(), d.at);
    }
    Ok(StepResult { message_id: Field::Value(message_id), ..StepResult::session(&conversation.id) })
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmittedData {
    name: Option<String>,
    team_id: Option<String>,
    session_id: Option<String>,
    emitted_by: Option<String>,
    payload: Option<Value>,
}

fn emitted(ctx: &mut Ctx, data: EmittedData, event_id: &str) -> Step<StepResult> {
    let name = data.name.unwrap_or_default();
    let team_id = data.team_id.unwrap_or_else(|| default_session("teamId"));
    let origin = data.session_id.as_ref().filter(|id| !id.is_empty()).and_then(|id| ctx.state.sessions.get(id));
    let emitter = origin.and_then(|origin| tree_agent_id(ctx.state.sessions.get(&origin.root_id).unwrap_or(origin)));
    let payload = data.payload.unwrap_or_else(|| Value::Object(Map::new()));
    let agents: Vec<HubAgent> = ctx.state.agents.values().cloned().collect();
    for agent in agents {
        if agent.team_id != team_id || Some(&agent.id) == emitter.as_ref() {
            continue;
        }
        for trigger in &agent.triggers {
            if trigger.kind != TriggerKind::Event || trigger.enabled == Some(false) || trigger.name.as_ref() != Some(&name) {
                continue;
            }
            let conversation = session(&ctx.state, Some(&agent.conversation_id))?;
            let card = EventCard {
                event: "trigger_fired".into(),
                session_id: None,
                pull_request_id: None,
                agent_id: Some(agent.id.clone()),
                trigger_id: Field::Value(trigger.id.clone()),
                event_id: Field::Value(event_id.to_string()),
                name: Field::Value(name.clone()),
            };
            post(
                ctx,
                &conversation.id,
                Post {
                    author_kind: AuthorKind::User,
                    author_session_id: None,
                    body: copy::event_text(&name, data.emitted_by.as_deref().unwrap_or("server"), &payload),
                    card: Some(Card::Event(card)),
                    sealed: false,
                    waking: true,
                },
            );
            ctx.effects.push(Effect::Nudge { device_id: conversation.device_id.clone() });
        }
    }
    Ok(StepResult::default())
}

// ── the reducer ──────────────────────────────────────────────────────────────

fn parse<T: DeserializeOwned>(data: &Value, kind: &str) -> Step<T> {
    serde_json::from_value(data.clone()).map_err(|e| Failure::new(ErrorCode::Invalid, &format!("Invalid {kind} data: {e}")))
}

fn data_str<'a>(data: &'a Value, key: &str) -> Option<&'a str> {
    data.get(key).and_then(Value::as_str)
}

fn step(ctx: &mut Ctx, event: &HubEvent, data: &Value) -> Step<StepResult> {
    let caller = |ctx: &Ctx| -> Step<Option<HubSession>> {
        match data_str(data, "callerId").filter(|id| !id.is_empty()) {
            Some(id) => session(&ctx.state, Some(id)).map(Some),
            None => Ok(None),
        }
    };
    match event.kind.as_str() {
        "send.start" => {
            let send: SendData = parse(data, &event.kind)?;
            let caller = caller(ctx)?;
            if send.parent_id.as_deref().is_some_and(|id| !id.is_empty()) {
                start_child(ctx, &send, caller.as_ref())
            } else {
                start_root(ctx, &send, caller.as_ref())
            }
        }
        "send.message" => {
            let send: SendData = parse(data, &event.kind)?;
            let caller = caller(ctx)?;
            send_message(ctx, &send, caller.as_ref())
        }
        "send.watch" => {
            let watcher = data_str(data, "watcherId").unwrap_or_default().to_string();
            let ids: Vec<String> = data.get("ids").and_then(|ids| serde_json::from_value(ids.clone()).ok()).unwrap_or_default();
            watch_sessions(ctx, &watcher, &ids);
            Ok(StepResult { session_id: data_str(data, "watcherId").map(str::to_string), ..StepResult::default() })
        }
        "session.end" => {
            let top = session(&ctx.state, data_str(data, "sessionId"))?;
            let subtree = live_subtree(&ctx.state, &top);
            let named = Named {
                id: &top.id,
                summary: data_str(data, "summary"),
                summary_sealed: data.get("summarySealed") == Some(&Value::Bool(true)),
            };
            let ended_ids = end_rows(ctx, &subtree, Some(named));
            Ok(StepResult { ended_ids: Some(ended_ids), ..StepResult::session(&top.id) })
        }
        "session.pr_opened" => {
            let opener = session(&ctx.state, data_str(data, "sessionId"))?;
            let field = |key: &str| js::to_string(data.get(key).unwrap_or(&Value::String("undefined".into())));
            let body = copy::pr_opened_text(&field("number"), &field("url"));
            bus_line(ctx, &opener, "pr_opened", body, true, data_str(data, "pullRequestId"), true);
            Ok(StepResult::session(&opener.id))
        }
        "device.claim" => claim(ctx, parse(data, &event.kind)?),
        "device.report" => report(ctx, parse(data, &event.kind)?),
        "device.reply" => reply(ctx, parse(data, &event.kind)?),
        "device.consume" => consume(ctx, parse(data, &event.kind)?),
        "device.heartbeat" => heartbeat(ctx, parse(data, &event.kind)?),
        "watchdog.tick" => tick(ctx),
        "trigger.schedule" => schedule(ctx, parse(data, &event.kind)?),
        "trigger.event" => emitted(ctx, parse(data, &event.kind)?, &event.id),
        kind => refuse(ErrorCode::Invalid, &format!("Unknown event {kind}")),
    }
}

/// One event → the next state, the effects to apply and the step's result (or an eCore refusal, state unchanged).
pub fn reduce(state: &HubState, event: &HubEvent) -> Outcome {
    let mut ctx = Ctx { state: state.clone(), effects: vec![], now: event.at };
    let data = if event.data.is_null() { Value::Object(Map::new()) } else { event.data.clone() };
    match step(&mut ctx, event, &data) {
        Ok(result) => Outcome { state: ctx.state, effects: ctx.effects, result: Some(result), failure: None },
        Err(failure) => Outcome { state: state.clone(), effects: vec![], result: None, failure: Some(failure) },
    }
}

fn merged(layers: &[&Value]) -> Value {
    let mut out = Map::new();
    for layer in layers {
        if let Value::Object(map) = layer {
            for (key, value) in map {
                if key != "$comment" {
                    out.insert(key.clone(), value.clone());
                }
            }
        }
    }
    Value::Object(out)
}

fn list<'a>(given: &'a Value, key: &str) -> &'a [Value] {
    given.get(key).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// A fixture's `given` with rules.json `defaults` filled in: rootId and depth follow parentId, a child
/// shares its root's `shared` unless it names its own.
pub fn hydrate(given: &Value) -> Result<HubState, String> {
    let d = &rules().defaults;
    let mut state = HubState::default();
    let rows = list(given, "sessions");
    let by_id: BTreeMap<&str, &Value> = rows.iter().filter_map(|row| Some((row.get("id")?.as_str()?, row))).collect();
    fn place(by_id: &BTreeMap<&str, &Value>, row: &Value, depth_left: usize) -> Result<(String, i64), String> {
        let id = row["id"].as_str().ok_or("given: a session without an id")?;
        let Some(parent_id) = row.get("parentId").and_then(Value::as_str).filter(|p| !p.is_empty()) else {
            return Ok((id.to_string(), 0));
        };
        let parent = by_id.get(parent_id).ok_or(format!("given: {id}'s parent {parent_id} is missing"))?;
        if depth_left == 0 {
            return Err(format!("given: {id}'s parents loop"));
        }
        let (root, depth) = place(by_id, parent, depth_left - 1)?;
        Ok((root, depth + 1))
    }
    for row in rows {
        let (root_id, depth) = place(&by_id, row, rows.len())?;
        let shared = row
            .get("shared")
            .filter(|s| !s.is_null())
            .or_else(|| by_id.get(root_id.as_str()).and_then(|root| root.get("shared")).filter(|s| !s.is_null()))
            .cloned()
            .unwrap_or_else(|| d["session"]["shared"].clone());
        let placed = serde_json::json!({ "rootId": root_id, "depth": depth, "shared": shared });
        let session: HubSession = serde_json::from_value(merged(&[&d["session"], &placed, row])).map_err(|e| format!("given session: {e}"))?;
        state.sessions.insert(session.id.clone(), session);
    }
    for m in list(given, "messages") {
        state.messages.push(serde_json::from_value(merged(&[&d["message"], m])).map_err(|e| format!("given message: {e}"))?);
    }
    for device in list(given, "devices") {
        let device: HubDevice = serde_json::from_value(merged(&[&d["device"], device])).map_err(|e| format!("given device: {e}"))?;
        state.devices.insert(device.id.clone(), device);
    }
    for agent in list(given, "agents") {
        let team = serde_json::json!({ "teamId": d["session"]["teamId"] });
        let agent: HubAgent = serde_json::from_value(merged(&[&d["agent"], &team, agent])).map_err(|e| format!("given agent: {e}"))?;
        state.agents.insert(agent.id.clone(), agent);
    }
    Ok(state)
}
