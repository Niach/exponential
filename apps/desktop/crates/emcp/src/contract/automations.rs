//! An app's automations (#VAPP-220, `packages/emcp/fixtures/automations.json`;
//! its `$comment` is the spec): validation, the schedule spacing, trigger
//! matching with the loop guard, the sponsor, the rate cap, template
//! resolution and the audit names. Read as JSON: a malformed automation is
//! a validation error, never a parse failure.

use std::collections::HashSet;

use serde_json::{json, Map, Value};

use super::exposes::{self, ExposeContext, ExposeError};

pub const NAME_MAX: usize = 40;
pub const MAX_AUTOMATIONS: usize = 16;
pub const MIN_SCHEDULE_GAP_MINUTES: u32 = 15;
pub const EVENT_MAX: usize = 128;
pub const RUNS_PER_MINUTE: u32 = 30;
pub const CLIENT_PREFIX: &str = "automation:";
pub const TRIGGER_KINDS: &[&str] = &["schedule", "record", "event", "webhook"];
pub const RECORD_TRIGGER_OPS: &[&str] = &["create", "update", "delete"];

fn segment_ok(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase()) && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn name_ok(name: &str) -> bool {
    segment_ok(name) && name.encode_utf16().count() <= NAME_MAX
}

fn event_ok(name: &str) -> bool {
    name.split('.').all(segment_ok) && name.encode_utf16().count() <= EVENT_MAX
}

/// The kind of an automation's `on` (exactly one key), else `None`.
pub fn trigger_kind(on: &Value) -> Option<&'static str> {
    let on = on.as_object()?;
    let keys: Vec<&'static str> = TRIGGER_KINDS.iter().copied().filter(|k| on.contains_key(*k)).collect();
    if keys.len() == 1 {
        Some(keys[0])
    } else {
        None
    }
}

fn digits(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// One cron field's values (`*`, lists, ranges, steps; digits only), sorted.
pub fn cron_field_values(raw: &str, min: u32, max: u32) -> Option<Vec<u32>> {
    let mut out = Vec::new();
    for part in raw.split(',') {
        let mut pieces = part.split('/');
        let range = pieces.next().filter(|r| !r.is_empty())?;
        let step = pieces.next();
        if pieces.next().is_some() {
            return None;
        }
        let by = match step {
            Some(step) => digits(step).filter(|n| *n >= 1)?,
            None => 1,
        };
        let (lo, hi) = if range == "*" {
            (min, max)
        } else {
            let bounds: Vec<&str> = range.split('-').collect();
            if bounds.len() > 2 {
                return None;
            }
            let lo = digits(bounds[0])?;
            let hi = if bounds.len() == 2 { digits(bounds[1])? } else if step.is_some() { max } else { lo };
            (lo, hi)
        };
        if lo < min || hi > max || lo > hi {
            return None;
        }
        let mut v = lo;
        while v <= hi {
            out.push(v);
            v += by;
        }
    }
    out.sort_unstable();
    out.dedup();
    Some(out)
}

/// A schedule's check: `None` = fine, else `schedule` | `scheduleTooOften`.
pub fn schedule_error(schedule: &Value) -> Option<&'static str> {
    let Some(cron) = schedule.get("cron").and_then(Value::as_str) else { return Some("schedule") };
    if !schedule.is_object() {
        return Some("schedule");
    }
    if let Some(tz) = schedule.get("tz") {
        if tz.as_str().is_none_or(|tz| tz.trim().is_empty()) {
            return Some("schedule");
        }
    }
    let fields: Vec<&str> = cron.split_whitespace().collect();
    let field_ok = |f: &&str| f.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '*' | ',' | '/' | '-'));
    if fields.len() != 5 || !fields.iter().all(field_ok) {
        return Some("schedule");
    }
    let (Some(minutes), Some(hours)) = (cron_field_values(fields[0], 0, 59), cron_field_values(fields[1], 0, 23)) else {
        return Some("schedule");
    };
    let day: Vec<u32> = hours.iter().flat_map(|h| minutes.iter().map(move |m| h * 60 + m)).collect();
    if day.len() < 2 {
        return None;
    }
    let mut gap = 1440 - day[day.len() - 1] + day[0];
    for pair in day.windows(2) {
        gap = gap.min(pair[1] - pair[0]);
    }
    if gap < MIN_SCHEDULE_GAP_MINUTES {
        Some("scheduleTooOften")
    } else {
        None
    }
}

fn roots(kind: &str) -> &'static [&'static str] {
    match kind {
        "record" => &["record", "firedAt"],
        "event" | "webhook" => &["event", "firedAt"],
        _ => &["firedAt"],
    }
}

fn ident_ok(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn placeholder_ok(inner: &str, allowed: &[&str]) -> bool {
    let mut parts = inner.split('.');
    let root = parts.next().unwrap_or("");
    let rest: Vec<&str> = parts.collect();
    if !ident_ok(root) || !allowed.contains(&root) || !rest.iter().all(|p| ident_ok(p)) {
        return false;
    }
    if root == "firedAt" {
        rest.is_empty()
    } else {
        !rest.is_empty()
    }
}

/// Every `${…}` of a value is a root the trigger carries.
pub fn templates_valid(value: &Value, kind: &str) -> bool {
    match value {
        Value::String(text) => {
            let mut at = 0;
            while let Some(start) = text[at..].find("${") {
                let open = at + start + 2;
                let Some(len) = text[open..].find('}') else { return true };
                if !placeholder_ok(&text[open..open + len], roots(kind)) {
                    return false;
                }
                at = open + len + 1;
            }
            true
        }
        Value::Array(items) => items.iter().all(|item| templates_valid(item, kind)),
        Value::Object(map) => map.values().all(|item| templates_valid(item, kind)),
        _ => true,
    }
}

/// `${record.…}`, `${event.…}`, `${firedAt}` → `${input.<root>…}`.
pub fn to_input_templates(value: &Value) -> Value {
    match value {
        Value::String(text) => {
            let mut out = text.clone();
            for root in ["record", "event", "firedAt"] {
                for next in [".", "}"] {
                    out = out.replace(&format!("${{{root}{next}"), &format!("${{input.{root}{next}"));
                }
            }
            Value::String(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(to_input_templates).collect()),
        Value::Object(map) => Value::Object(map.iter().map(|(k, v)| (k.clone(), to_input_templates(v))).collect::<Map<_, _>>()),
        other => other.clone(),
    }
}

/// The run with its templates filled from `{record?, event?, firedAt}`.
pub fn resolve_automation_run(run: &Value, input: &Value) -> Value {
    exposes::resolve_args(&to_input_templates(run), input).unwrap_or(Value::Null)
}

/// Every error of an app's `automations`, in order (empty = valid).
pub fn validate_automations(list: &Value, ctx: &ExposeContext) -> Vec<ExposeError> {
    let Some(list) = list.as_array() else { return vec![ExposeError { index: None, code: "list" }] };
    let mut errors = Vec::new();
    if list.len() > MAX_AUTOMATIONS {
        errors.push(ExposeError { index: None, code: "tooMany" });
    }
    let mut seen = HashSet::new();
    let mut hooks = HashSet::new();
    for (index, item) in list.iter().enumerate() {
        let mut fail = |code: &'static str| errors.push(ExposeError { index: Some(index), code });
        if !item.is_object() {
            fail("shape");
            continue;
        }
        match item.get("name").and_then(Value::as_str) {
            Some(name) if name_ok(name) => {
                if !seen.insert(name.to_string()) {
                    fail("duplicate");
                }
            }
            _ => fail("name"),
        }
        let on = &item["on"];
        let Some(kind) = trigger_kind(on) else {
            fail("on");
            continue;
        };
        match kind {
            "schedule" => {
                if let Some(code) = schedule_error(&on["schedule"]) {
                    fail(code);
                }
            }
            "record" => {
                let record = &on["record"];
                let op_ok = record.is_object() && record["op"].as_str().is_some_and(|op| RECORD_TRIGGER_OPS.contains(&op));
                if !op_ok {
                    fail("on");
                    continue;
                }
                if !record["kind"].as_str().is_some_and(|k| ctx.kinds.iter().any(|d| d == k)) {
                    fail("kind");
                }
            }
            "webhook" => {
                // #VAPP-237 (A3): a hook name, unique among the app's webhooks.
                match on["webhook"].get("name").and_then(Value::as_str) {
                    Some(hook) if name_ok(hook) && hooks.insert(hook.to_string()) => {}
                    _ => fail("webhook"),
                }
            }
            _ => {
                if !on["event"].as_str().is_some_and(event_ok) {
                    fail("event");
                }
            }
        }
        if let Some(sponsor) = item.get("sponsor") {
            if !sponsor.as_str().is_some_and(|s| s.contains('@')) {
                fail("sponsor");
            }
        }
        let run = &item["run"];
        if !run.is_object() {
            fail("run");
            continue;
        }
        if !templates_valid(run, kind) {
            fail("template");
        }
        if run["action"].as_str().is_some_and(|a| a.starts_with("exponential.apps.")) {
            fail("chain");
            continue;
        }
        let expose = automation_expose("automation", run);
        for error in exposes::validate_exposes(&[expose], ctx) {
            if error.code != "template" && error.code != "accessTooLow" {
                fail(error.code);
            }
        }
    }
    errors
}

/// The run as an exposes binding (the broker runs it like a tool).
pub fn automation_expose(name: &str, run: &Value) -> Value {
    json!({
        "name": name,
        "title": name,
        "description": name,
        "input": { "type": "object", "properties": { "record": {}, "event": {}, "firedAt": {} } },
        "access": "write",
        "run": to_input_templates(run),
    })
}

/// What fired: a record write (`kind`, `op`) or an event (its name).
#[derive(Clone, Debug)]
pub enum FiredBy<'a> {
    Record { kind: &'a str, op: &'a str },
    Event(&'a str),
    Webhook(&'a str),
}

/// The automations a record write or event fires, in package order. LOOP
/// GUARD: a write or event caused by an automation run fires nothing.
pub fn match_automations(list: &Value, fired_by: &FiredBy<'_>, from_automation: bool) -> Vec<String> {
    match_automations_by(list, fired_by, from_automation, None)
}

/// [`match_automations`] for a write or event caused from inside an
/// agent's session tree (`by_agent` = its slug): no hand-off to that agent
/// fires (#VAPP-236, the agent loop guard).
pub fn match_automations_by(list: &Value, fired_by: &FiredBy<'_>, from_automation: bool, by_agent: Option<&str>) -> Vec<String> {
    let Some(list) = list.as_array() else { return Vec::new() };
    if from_automation {
        return Vec::new();
    }
    let mut out = Vec::new();
    for item in list {
        let Some(name) = item.get("name").and_then(Value::as_str) else { continue };
        if by_agent.is_some_and(|agent| item["run"]["agent"].as_str() == Some(agent)) {
            continue;
        }
        let on = &item["on"];
        let hit = match (trigger_kind(on), fired_by) {
            (Some("record"), FiredBy::Record { kind, op }) => on["record"]["kind"] == *kind && on["record"]["op"] == *op,
            (Some("event"), FiredBy::Event(event)) => on["event"] == *event,
            (Some("webhook"), FiredBy::Webhook(hook)) => on["webhook"]["name"] == *hook,
            _ => false,
        };
        if hit {
            out.push(name.to_string());
        }
    }
    out
}

/// A team member as the sponsor rule reads them.
#[derive(Clone, Debug)]
pub struct SponsorMember {
    pub id: String,
    pub email: String,
    pub role: String,
    /// The member's app access: `none` | `read` | `write`.
    pub level: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sponsor {
    User(String),
    Paused(&'static str),
}

/// C23: who an automation acts as, or why it is paused.
pub fn resolve_sponsor(declared: Option<&str>, published_by: Option<&str>, members: &[SponsorMember]) -> Sponsor {
    let publisher = published_by.and_then(|id| members.iter().find(|m| m.id == id));
    let mut sponsor = publisher;
    let declared = declared.map(|d| d.trim().to_lowercase()).filter(|d| !d.is_empty());
    if let Some(declared) = declared {
        if publisher.is_none_or(|p| p.email.to_lowercase() != declared) {
            let Some(publisher) = publisher else { return Sponsor::Paused("sponsorGone") };
            if publisher.role != "owner" {
                return Sponsor::Paused("sponsorNotAllowed");
            }
            sponsor = members.iter().find(|m| m.email.to_lowercase() == declared);
        }
    }
    match sponsor {
        None => Sponsor::Paused("sponsorGone"),
        Some(member) if member.level != "write" => Sponsor::Paused("sponsorReadOnly"),
        Some(member) => Sponsor::User(member.id.clone()),
    }
}

/// Whether another run may start (the app's runs of the last 60 s).
pub fn rate_allows(runs_last_minute: u32) -> bool {
    runs_last_minute < RUNS_PER_MINUTE
}

/// The audit row's `client_id`.
pub fn automation_client_id(name: &str) -> String {
    format!("{CLIENT_PREFIX}{name}")
}

/// The audit row's `tool`.
pub fn automation_tool(run: &Value) -> String {
    match exposes::run_via(run) {
        "records" => format!(
            "records.{}:{}",
            run["records"]["op"].as_str().unwrap_or(""),
            run["records"]["kind"].as_str().unwrap_or("")
        ),
        "action" => run["action"].as_str().unwrap_or("").to_string(),
        "agent" => format!("{}{}", exposes::AGENT_AUDIT_PREFIX, run["agent"].as_str().unwrap_or("")),
        _ => format!("{}/{}", run["upstream"].as_str().unwrap_or(""), run["tool"].as_str().unwrap_or("")),
    }
}
