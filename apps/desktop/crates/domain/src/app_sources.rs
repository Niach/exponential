//! VAPP-119 — the `exp:` binding SOURCES beyond devices and records, the
//! Rust twin of `packages/domain-contract/src/app-sources.ts` (iOS
//! `AppSources.swift`, Android `AppSources.kt`), locked by
//! `fixtures/app-sources.json` (`cases` over `store`): what `exp:issues`,
//! `exp:boards`, `exp:members`, `exp:sessions`, `exp:comments` and
//! `exp:attachments` emit over one team's synced rows. Pure: the app host
//! feeds it the rows and the viewer. Text compares by code points (no
//! locale), ties by id, `count` counts before the limit.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;

use serde::Deserialize;
use serde_json::{json, Value};

const SOURCES_JSON: &str =
    include_str!("../../../../../packages/domain-contract/fixtures/app-sources.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct SourcesLimits {
    pub default: usize,
    pub max: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SourcesSpec {
    /// The names this module derives (`exp:<name>`).
    pub sources: Vec<String>,
    pub limits: SourcesLimits,
}

/// `fixtures/app-sources.json` minus its store and cases.
pub static APP_SOURCES: LazyLock<SourcesSpec> =
    LazyLock::new(|| serde_json::from_str(SOURCES_JSON).expect("app-sources.json parses"));

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceBoard {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub prefix: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub sort_order: f64,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceIssue {
    pub id: String,
    pub board_id: String,
    pub identifier: String,
    pub title: String,
    /// The anchor status.
    pub status: String,
    pub priority: Option<String>,
    pub assignee_id: Option<String>,
    pub creator_id: Option<String>,
    pub due_date: Option<String>,
    pub sort_order: f64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceMember {
    pub user_id: String,
    pub role: String,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceUser {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub image: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSession {
    pub id: String,
    pub parent_id: Option<String>,
    pub agent_id: Option<String>,
    pub app_id: Option<String>,
    pub user_id: Option<String>,
    pub status: String,
    pub busy: bool,
    pub needs_input: bool,
    pub title: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceComment {
    pub id: String,
    pub issue_id: String,
    pub author_id: Option<String>,
    pub parent_id: Option<String>,
    pub source: String,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceAttachment {
    pub id: String,
    pub issue_id: Option<String>,
    pub comment_id: Option<String>,
    pub uploader_id: Option<String>,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub created_at: String,
}

/// FEED-243: an issue's timeline row (`issue_events`), only who and when.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceIssueEvent {
    pub id: String,
    pub issue_id: String,
    pub actor_user_id: Option<String>,
    pub created_at: String,
}

/// FEED-243: a session linked to an issue (`session_issues`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSessionIssue {
    pub session_id: String,
    pub issue_id: String,
}

/// The synced rows of ONE team, as the host reads them.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceStore {
    pub boards: Vec<SourceBoard>,
    pub issues: Vec<SourceIssue>,
    pub members: Vec<SourceMember>,
    pub users: Vec<SourceUser>,
    pub sessions: Vec<SourceSession>,
    pub comments: Vec<SourceComment>,
    pub attachments: Vec<SourceAttachment>,
    #[serde(default)]
    pub issue_events: Vec<SourceIssueEvent>,
    #[serde(default)]
    pub session_issues: Vec<SourceSessionIssue>,
}

type Params = BTreeMap<String, String>;

/// A param as JS truthiness reads it (`undefined` / `""` = absent).
fn param<'a>(params: &'a Params, key: &str) -> Option<&'a str> {
    params.get(key).map(String::as_str).filter(|v| !v.is_empty())
}

/// JS `Number(text)`: whitespace-trimmed, empty = 0, else a decimal float.
fn js_number(text: &str) -> f64 {
    let t = text.trim();
    if t.is_empty() {
        return 0.0;
    }
    t.parse::<f64>().unwrap_or(f64::NAN)
}

fn limit_of(params: &Params) -> usize {
    let limits = APP_SOURCES.limits;
    match params.get("limit").map(|v| js_number(v)) {
        Some(n) if n.is_finite() && n.fract() == 0.0 && n > 0.0 => n.min(limits.max as f64) as usize,
        _ => limits.default,
    }
}

/// `a - b` as a sort key (NaN = equal, like JS's falsy `NaN`).
fn num_cmp(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}

fn user_name(users: &HashMap<&str, &SourceUser>, id: Option<&str>) -> String {
    id.and_then(|id| users.get(id)).map(|u| u.name.clone()).unwrap_or_default()
}

fn users_by_id(store: &SourceStore) -> HashMap<&str, &SourceUser> {
    // The first row wins, like `Array.find`.
    let mut map = HashMap::new();
    for u in &store.users {
        map.entry(u.id.as_str()).or_insert(u);
    }
    map
}

/// A board by id or slug.
fn board_of<'a>(store: &'a SourceStore, reference: Option<&str>) -> Option<&'a SourceBoard> {
    let reference = reference?;
    store.boards.iter().find(|b| b.id == reference || b.slug == reference)
}

/// An issue by identifier (case-insensitive) or id.
fn issue_of<'a>(store: &'a SourceStore, reference: Option<&str>) -> Option<&'a SourceIssue> {
    let reference = reference?;
    let upper = reference.to_uppercase();
    store.issues.iter().find(|i| i.id == reference || i.identifier.to_uppercase() == upper)
}

/// An ISO instant (RFC 3339) as epoch ms.
fn instant_ms(text: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(text.trim()).ok().map(|t| t.timestamp_millis())
}

/// A zone as its offset at an instant (minutes east): the team zone, else
/// the machine's.
type Offset = dyn Fn(i64) -> i64;

fn zone_offset_fn(time_zone: Option<&str>) -> Box<Offset> {
    match crate::team_zone::resolve_zone(time_zone) {
        Some(zone) => Box::new(move |ms| crate::team_zone::zone_offset(&zone, ms as f64) as i64),
        None => Box::new(|ms| {
            use chrono::{Offset as _, TimeZone as _};
            chrono::Local.timestamp_millis_opt(ms).single().map_or(0, |t| t.offset().fix().local_minus_utc() as i64 / 60)
        }),
    }
}

/// The instant a calendar day (`YYYY-MM-DD`) starts at under a zone (the
/// reference's two-pass `dayStart`).
fn day_start(day: &str, offset: &Offset) -> Option<i64> {
    let wall = instant_ms(&format!("{day}T00:00:00Z"))?;
    Some(wall - offset(wall - offset(wall) * 60_000) * 60_000)
}

/// `YYYY-MM-DD` exactly.
fn is_day(v: &str) -> bool {
    v.len() == 10 && v.bytes().enumerate().all(|(i, b)| if i == 4 || i == 7 { b == b'-' } else { b.is_ascii_digit() })
}

/// `YYYY-MM-DDTHH:MM:SS[.f](Z|±HH:MM)` exactly.
fn is_instant(v: &str) -> bool {
    let b = v.as_bytes();
    let digits = |r: std::ops::Range<usize>| r.into_iter().all(|i| b.get(i).is_some_and(u8::is_ascii_digit));
    if b.len() < 20 || !is_day(&v[..10]) || b[10] != b'T' || !digits(11..13) || b[13] != b':' || !digits(14..16) || b[16] != b':' || !digits(17..19) {
        return false;
    }
    let mut i = 19;
    if b[i] == b'.' {
        let start = i + 1;
        i = start;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    let rest = &b[i..];
    rest == b"Z" || (rest.len() == 6 && (rest[0] == b'+' || rest[0] == b'-') && digits(i + 1..i + 3) && rest[3] == b':' && digits(i + 4..i + 6))
}

/// FEED-243: a `since` / `until` bound (epoch ms, exclusive?). A date-only
/// bound reads in the zone: `since` = the day's start (inclusive), `until` =
/// the NEXT day's start (exclusive); an instant is inclusive; anything else =
/// no bound.
fn bound_of(raw: Option<&str>, end: bool, offset: &Offset) -> Option<(i64, bool)> {
    let v = raw?.trim();
    if is_day(v) {
        let day = chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d").ok()?;
        return if end {
            Some((day_start(&day.succ_opt()?.format("%Y-%m-%d").to_string(), offset)?, true))
        } else {
            Some((day_start(v, offset)?, false))
        };
    }
    if !is_instant(v) {
        return None;
    }
    instant_ms(v).map(|ms| (ms, false))
}

/// The latest touch of an issue in the window.
struct Touch {
    at: String,
    ms: i64,
    touches: Vec<&'static str>,
}

/// FEED-243: the issues `actor` touched inside [since, until]: an
/// `issue_events` row they acted, a comment they wrote, a session they
/// started linked to the issue (at its start and its last update).
fn touches_of(store: &SourceStore, actor: &str, params: &Params, time_zone: Option<&str>) -> HashMap<String, Touch> {
    let offset = zone_offset_fn(time_zone);
    let since = bound_of(param(params, "since"), false, &offset);
    let until = bound_of(param(params, "until"), true, &offset);
    let mut out: HashMap<String, Touch> = HashMap::new();
    let mut add = |issue_id: &str, kind: &'static str, at: &str| {
        let Some(ms) = instant_ms(at) else { return };
        if since.is_some_and(|(s, _)| ms < s) {
            return;
        }
        if until.is_some_and(|(u, open)| if open { ms >= u } else { ms > u }) {
            return;
        }
        match out.get_mut(issue_id) {
            None => {
                out.insert(issue_id.to_string(), Touch { at: at.to_string(), ms, touches: vec![kind] });
            }
            Some(t) => {
                if !t.touches.contains(&kind) {
                    t.touches.push(kind);
                }
                if ms > t.ms {
                    t.at = at.to_string();
                    t.ms = ms;
                }
            }
        }
    };
    for e in store.issue_events.iter().filter(|e| e.actor_user_id.as_deref() == Some(actor)) {
        add(&e.issue_id, "event", &e.created_at);
    }
    for c in store.comments.iter().filter(|c| c.author_id.as_deref() == Some(actor)) {
        add(&c.issue_id, "comment", &c.created_at);
    }
    let mut sessions: HashMap<&str, &SourceSession> = HashMap::new();
    for s in &store.sessions {
        sessions.entry(s.id.as_str()).or_insert(s);
    }
    for link in &store.session_issues {
        let Some(s) = sessions.get(link.session_id.as_str()).filter(|s| s.user_id.as_deref() == Some(actor)) else { continue };
        add(&link.issue_id, "session", &s.created_at);
        add(&link.issue_id, "session", &s.updated_at);
    }
    for t in out.values_mut() {
        t.touches.sort_unstable();
    }
    out
}

pub fn issues_value(store: &SourceStore, params: &Params, me: Option<&str>) -> Value {
    issues_value_in(store, params, me, None)
}

/// [`issues_value`] with the team zone date-only `since` / `until` read in
/// (`None` = the machine's).
pub fn issues_value_in(store: &SourceStore, params: &Params, me: Option<&str>, time_zone: Option<&str>) -> Value {
    let board_ref = param(params, "board");
    let board = board_of(store, board_ref);
    if board_ref.is_some() && board.is_none() {
        return json!({ "rows": [], "count": 0 });
    }
    let assignee_param = param(params, "assignee");
    let assignee = if assignee_param == Some("me") { me } else { assignee_param };
    let status = param(params, "status");
    let priority = param(params, "priority");
    let search = params.get("search").map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty());
    let touched: Option<HashMap<String, Touch>> = param(params, "touched").map(|t| {
        let actor = if t == "me" { me } else { Some(t) };
        actor.map(|a| touches_of(store, a, params, time_zone)).unwrap_or_default()
    });
    let mut rows: Vec<&SourceIssue> = store
        .issues
        .iter()
        .filter(|i| {
            if touched.as_ref().is_some_and(|t| !t.contains_key(&i.id)) {
                return false;
            }
            if board.is_some_and(|b| i.board_id != b.id) {
                return false;
            }
            if status.is_some_and(|s| i.status != s) {
                return false;
            }
            if priority.is_some_and(|p| i.priority.as_deref().unwrap_or("") != p) {
                return false;
            }
            if assignee_param == Some("none") && i.assignee_id.is_some() {
                return false;
            }
            if assignee_param.is_some_and(|a| a != "none") && i.assignee_id.as_deref() != assignee {
                return false;
            }
            if let Some(search) = &search {
                if !i.title.to_lowercase().contains(search.as_str())
                    && !i.identifier.to_lowercase().contains(search.as_str())
                {
                    return false;
                }
            }
            true
        })
        .collect();
    let sort = match params.get("sort").map(String::as_str) {
        Some("updatedAt") => "updatedAt",
        Some("createdAt") => "createdAt",
        Some("sortOrder") => "sortOrder",
        _ if touched.is_some() => "touchedAt",
        _ => "sortOrder",
    };
    let asc = match params.get("order").map(String::as_str) {
        Some("desc") => false,
        Some("asc") => true,
        _ => sort == "sortOrder",
    };
    rows.sort_by(|a, b| {
        let c = match sort {
            "updatedAt" => a.updated_at.cmp(&b.updated_at),
            "createdAt" => a.created_at.cmp(&b.created_at),
            "touchedAt" => {
                let t = touched.as_ref().expect("touchedAt sorts a touched query");
                t[&a.id].ms.cmp(&t[&b.id].ms)
            }
            _ => num_cmp(a.sort_order, b.sort_order),
        };
        let c = if asc { c } else { c.reverse() };
        c.then_with(|| a.id.cmp(&b.id))
    });
    let users = users_by_id(store);
    let boards: HashMap<&str, &SourceBoard> = store.boards.iter().map(|b| (b.id.as_str(), b)).collect();
    let count = rows.len();
    let rows: Vec<Value> = rows
        .into_iter()
        .take(limit_of(params))
        .map(|i| {
            let mut row = json!({
                "id": i.id,
                "identifier": i.identifier,
                "title": i.title,
                // A Select / ChipInput option (shape `option`).
                "value": i.identifier,
                "label": format!("{} {}", i.identifier, i.title),
                "status": i.status,
                "priority": i.priority,
                "assignee": i.assignee_id,
                "assigneeName": user_name(&users, i.assignee_id.as_deref()),
                "board": i.board_id,
                "boardSlug": boards.get(i.board_id.as_str()).map(|b| b.slug.as_str()).unwrap_or(""),
                "dueDate": i.due_date,
                "updatedAt": i.updated_at,
            });
            if let Some(t) = touched.as_ref().and_then(|t| t.get(&i.id)) {
                row["touchedAt"] = json!(t.at);
                row["touches"] = json!(t.touches);
            }
            row
        })
        .collect();
    json!({ "rows": rows, "count": count })
}

pub fn boards_value(store: &SourceStore, params: &Params) -> Value {
    let mut rows: Vec<&SourceBoard> = store.boards.iter().filter(|b| b.deleted_at.is_none()).collect();
    rows.sort_by(|a, b| {
        num_cmp(a.sort_order, b.sort_order).then_with(|| a.name.cmp(&b.name)).then_with(|| a.id.cmp(&b.id))
    });
    let count = rows.len();
    let rows: Vec<Value> = rows
        .into_iter()
        .take(limit_of(params))
        .map(|b| {
            json!({
                "id": b.id, "slug": b.slug, "name": b.name, "prefix": b.prefix,
                "icon": b.icon, "color": b.color, "value": b.id, "label": b.name,
            })
        })
        .collect();
    json!({ "rows": rows, "count": count })
}

pub fn members_value(store: &SourceStore, params: &Params, me: Option<&str>) -> Value {
    let users = users_by_id(store);
    let mut rows: Vec<(String, Value)> = store
        .members
        .iter()
        .map(|m| {
            let u = users.get(m.user_id.as_str());
            let name = u.map(|u| u.name.clone()).unwrap_or_default();
            let row = json!({
                "id": m.user_id,
                "name": name,
                "email": u.and_then(|u| u.email.clone()),
                "avatar": u.and_then(|u| u.image.clone()),
                "role": m.role,
                "value": m.user_id,
                "label": name,
            });
            (name, row)
        })
        .collect();
    rows.sort_by(|(an, a), (bn, b)| an.cmp(bn).then_with(|| a["id"].as_str().cmp(&b["id"].as_str())));
    let count = rows.len();
    let rows: Vec<Value> = rows.into_iter().take(limit_of(params)).map(|(_, r)| r).collect();
    json!({ "rows": rows, "count": count, "me": me })
}

pub fn sessions_value(store: &SourceStore, params: &Params, me: Option<&str>) -> Value {
    let status = param(params, "status");
    let mine = matches!(params.get("mine").map(String::as_str), Some("1" | "true"));
    let mut rows: Vec<&SourceSession> = store
        .sessions
        .iter()
        .filter(|s| s.parent_id.is_none() && s.app_id.is_none())
        .filter(|s| status.is_none_or(|st| s.status == st))
        .filter(|s| !mine || me.is_some_and(|me| s.user_id.as_deref() == Some(me)))
        .collect();
    rows.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| a.id.cmp(&b.id)));
    let count = rows.len();
    let rows: Vec<Value> = rows
        .into_iter()
        .take(limit_of(params))
        .map(|s| {
            json!({
                "id": s.id,
                "title": s.title.clone().unwrap_or_default(),
                "status": s.status,
                "busy": s.busy,
                "needsInput": s.needs_input,
                "kind": if s.agent_id.as_deref().is_some_and(|a| !a.is_empty()) { "agent" } else { "thread" },
                "updatedAt": s.updated_at,
            })
        })
        .collect();
    json!({ "rows": rows, "count": count })
}

pub fn comments_value(store: &SourceStore, params: &Params) -> Value {
    let Some(issue) = issue_of(store, param(params, "issue")) else { return Value::Null };
    let mut rows: Vec<&SourceComment> = store.comments.iter().filter(|c| c.issue_id == issue.id).collect();
    rows.sort_by(|a, b| a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id)));
    let users = users_by_id(store);
    let count = rows.len();
    let rows: Vec<Value> = rows
        .into_iter()
        .take(limit_of(params))
        .map(|c| {
            json!({
                "id": c.id,
                "body": c.body,
                "author": c.author_id,
                "authorName": user_name(&users, c.author_id.as_deref()),
                "parent": c.parent_id,
                "source": c.source,
                "createdAt": c.created_at,
            })
        })
        .collect();
    json!({ "rows": rows, "count": count })
}

pub fn attachments_value(store: &SourceStore, params: &Params) -> Value {
    let Some(issue) = issue_of(store, param(params, "issue")) else { return Value::Null };
    let mut rows: Vec<&SourceAttachment> =
        store.attachments.iter().filter(|a| a.issue_id.as_deref() == Some(issue.id.as_str())).collect();
    rows.sort_by(|a, b| a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id)));
    let users = users_by_id(store);
    let count = rows.len();
    let rows: Vec<Value> = rows
        .into_iter()
        .take(limit_of(params))
        .map(|a| {
            json!({
                "id": a.id,
                "name": a.filename,
                "type": a.content_type,
                "size": a.size_bytes,
                "url": format!("/api/attachments/{}", a.id),
                "width": a.width,
                "height": a.height,
                "uploader": a.uploader_id,
                "uploaderName": user_name(&users, a.uploader_id.as_deref()),
                "createdAt": a.created_at,
            })
        })
        .collect();
    json!({ "rows": rows, "count": count })
}

/// Whether `name` is one this module derives (`exp:<name>`).
pub fn is_source(name: &str) -> bool {
    APP_SOURCES.sources.iter().any(|s| s == name)
}

/// VAPP-206: the viewer, so a package's `visible` reads their team role and
/// app access (`None` = write).
pub fn me_value(store: &SourceStore, me: Option<&str>, access: Option<&str>) -> Value {
    let member = me.and_then(|me| store.members.iter().find(|m| m.user_id == me));
    let user = me.and_then(|me| store.users.iter().find(|u| u.id == me));
    json!({
        "id": me,
        "name": user.map(|u| u.name.clone()).unwrap_or_default(),
        "email": user.and_then(|u| u.email.clone()),
        "avatar": user.and_then(|u| u.image.clone()),
        "role": member.map(|m| m.role.clone()),
        "access": access.unwrap_or("write"),
    })
}

/// One `exp:<name>?params` over the store: the value the host emits at the
/// bound path (null for a name this module does not derive, or a comments /
/// attachments source without a resolvable issue).
pub fn source_value(name: &str, params: &Params, store: &SourceStore, me: Option<&str>) -> Value {
    source_value_as(name, params, store, me, None)
}

/// [`source_value`] with the viewer's app access (`exp:me`).
pub fn source_value_as(name: &str, params: &Params, store: &SourceStore, me: Option<&str>, access: Option<&str>) -> Value {
    source_value_in(name, params, store, me, access, None)
}

/// [`source_value_as`] in the team zone (FEED-243: date-only `since` /
/// `until`; `None` = the machine's).
pub fn source_value_in(
    name: &str,
    params: &Params,
    store: &SourceStore,
    me: Option<&str>,
    access: Option<&str>,
    time_zone: Option<&str>,
) -> Value {
    match name {
        "me" => me_value(store, me, access),
        "issues" => issues_value_in(store, params, me, time_zone),
        "boards" => boards_value(store, params),
        "members" => members_value(store, params, me),
        "sessions" => sessions_value(store, params, me),
        "comments" => comments_value(store, params),
        "attachments" => attachments_value(store, params),
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(SOURCES_JSON).unwrap()
    }

    fn store() -> SourceStore {
        serde_json::from_value(fixture()["store"].clone()).expect("the fixture store parses")
    }

    fn params_of(value: &Value) -> Params {
        value
            .as_object()
            .map(|o| o.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string())).collect())
            .unwrap_or_default()
    }

    #[test]
    fn every_case_replays_the_fixture() {
        let fixture = fixture();
        let store = store();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(cases.len() >= 17);
        for case in cases {
            let name = case["source"].as_str().unwrap();
            let got = source_value_in(
                name,
                &params_of(&case["params"]),
                &store,
                case["me"].as_str(),
                case["access"].as_str(),
                case["timeZone"].as_str(),
            );
            assert_eq!(got, case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn the_fixture_names_every_source_the_reference_derives() {
        let store = store();
        assert_eq!(APP_SOURCES.limits, SourcesLimits { default: 50, max: 500 });
        for name in &APP_SOURCES.sources {
            assert!(is_source(name));
            let value = source_value(name, &Params::new(), &store, None);
            if name != "comments" && name != "attachments" {
                assert!(value.is_object(), "{name}");
            }
        }
        assert_eq!(source_value("devices", &Params::new(), &store, None), Value::Null);
        assert!(!is_source("devices") && !is_source("records"));
    }

    #[test]
    fn bounds_parse_days_and_instants_only() {
        assert!(is_day("2026-10-23") && !is_day("2026-10-2") && !is_day("yesterday"));
        assert!(is_instant("2026-10-23T08:00:00Z") && is_instant("2026-10-23T08:00:00.120+02:00"));
        assert!(!is_instant("2026-10-23T08:00Z") && !is_instant("2026-10-23 08:00:00Z") && !is_instant("2026-10-23T08:00:00.Z"));
        let utc = |_| 0;
        assert_eq!(bound_of(Some("2026-10-23"), true, &utc), Some((instant_ms("2026-10-24T00:00:00Z").unwrap(), true)));
        assert_eq!(bound_of(Some("2026-10-23T08:00:00+02:00"), false, &utc), Some((instant_ms("2026-10-23T06:00:00Z").unwrap(), false)));
    }

    #[test]
    fn limits_follow_js_number() {
        let p = |v: &str| Params::from([("limit".to_string(), v.to_string())]);
        assert_eq!(limit_of(&Params::new()), 50);
        assert_eq!(limit_of(&p("")), 50);
        assert_eq!(limit_of(&p("0")), 50);
        assert_eq!(limit_of(&p("2.5")), 50);
        assert_eq!(limit_of(&p(" 3 ")), 3);
        assert_eq!(limit_of(&p("9999")), 500);
    }
}
