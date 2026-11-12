//! The Rust twin of `packages/domain-contract/src/local-parity.ts`, locked
//! by `fixtures/local-parity.json` (`tests/parity.rs` replays every case):
//! which procedures a Local desktop serves and the refusal for the rest,
//! the identity's email, a fresh desktop's seed rows, issue numbers and
//! identifiers, the status anchor, what `issues.create`/`update`,
//! `comments.create`, `sessions.send` and `agents.ensureAssistant` write.

use std::sync::LazyLock;

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::identity::LocalIdentity;

pub const LOCAL_PARITY_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/local-parity.json");

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedStatus {
    pub key: String,
    pub category: String,
    pub name: String,
    pub color: String,
    /// The contract's literal (an integer), written as is.
    pub sort_order: Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedBoard {
    pub name: String,
    pub slug: String,
    pub prefix: String,
    pub color: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Seed {
    pub slug_prefix: String,
    pub board: SeedBoard,
    pub statuses: Vec<SeedStatus>,
}

#[derive(Debug, Clone, Deserialize)]
struct Procedures {
    refusal: String,
    local: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Identity {
    email_domain: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Issues {
    anchor_by_category: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Drafts {
    consumed_refusal: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ComposerDrafts {
    empty_refusal: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Spec {
    procedures: Procedures,
    identity: Identity,
    pub seed: Seed,
    issues: Issues,
    drafts: Drafts,
    #[serde(rename = "composerDrafts")]
    composer_drafts: ComposerDrafts,
}

pub static SPEC: LazyLock<Spec> = LazyLock::new(|| serde_json::from_str(LOCAL_PARITY_JSON).expect("local-parity.json parses"));

pub type Row = Map<String, Value>;

pub fn is_local_procedure(procedure: &str) -> bool {
    SPEC.procedures.local.iter().any(|p| p == procedure)
}

/// The refusal a Local desktop answers for a procedure it does not serve.
pub fn refusal(procedure: &str) -> String {
    SPEC.procedures.refusal.replace("{procedure}", procedure)
}

/// The identity's email from the machine login.
pub fn local_email(login: &str) -> String {
    let mut local = String::new();
    let mut dash = false;
    for c in login.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_' || c == '-' {
            local.push(c);
            dash = false;
        } else if !dash {
            local.push('-');
            dash = true;
        }
    }
    let local = local.trim_matches('-');
    format!("{}@{}", if local.is_empty() { "local" } else { local }, SPEC.identity.email_domain)
}

fn obj(v: Value) -> Row {
    v.as_object().cloned().unwrap_or_default()
}

/// A fresh desktop's rows, ids taken from `next_id` in order.
pub fn seed_rows(desktop_id: &str, desktop_name: &str, identity: &LocalIdentity, now: &str, next_id: &mut dyn FnMut() -> String) -> Vec<(&'static str, Row)> {
    let seed = &SPEC.seed;
    let slug = format!("{}{}", seed.slug_prefix, &desktop_id[..desktop_id.len().min(8)]);
    let mut out = vec![
        ("teams", obj(json!({"id": desktop_id, "name": desktop_name, "slug": slug, "icon_url": null, "pr_opened_automation": true, "pr_merged_automation": true, "end_sessions_on_merge": true, "yolo_mode": false, "merge_mode": "auto", "estimation_type": "none", "time_zone": null, "created_at": now, "updated_at": now}))),
        ("users", obj(json!({"id": identity.user_id, "name": identity.name, "email": identity.email, "image": null, "created_at": now, "updated_at": now}))),
        ("team_members", obj(json!({"id": next_id(), "team_id": desktop_id, "user_id": identity.user_id, "role": "owner", "created_at": now, "updated_at": now}))),
    ];
    for s in &seed.statuses {
        out.push(("issue_statuses", obj(json!({"id": next_id(), "team_id": desktop_id, "category": s.category, "name": s.name, "color": s.color, "sort_order": s.sort_order, "builtin_key": s.key, "created_at": now, "updated_at": now}))));
    }
    let b = &seed.board;
    out.push(("boards", obj(json!({"id": next_id(), "team_id": desktop_id, "name": b.name, "slug": b.slug, "prefix": b.prefix, "color": b.color, "icon": null, "repository_id": null, "default_branch": null, "sort_order": 0, "created_at": now, "updated_at": now}))));
    out
}

pub fn next_issue_number(numbers: &[i64]) -> i64 {
    numbers.iter().copied().max().unwrap_or(0) + 1
}

pub fn issue_identifier(prefix: &str, number: i64) -> String {
    format!("{prefix}-{number}")
}

pub fn anchor_for_category(category: &str) -> String {
    SPEC.issues.anchor_by_category.get(category).cloned().unwrap_or_else(|| "backlog".to_string())
}

/// A status row as the resolver needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusRow {
    pub id: String,
    pub category: String,
    pub builtin_key: Option<String>,
}

impl StatusRow {
    pub fn from_row(row: &Row) -> Self {
        Self {
            id: row.get("id").and_then(Value::as_str).unwrap_or_default().to_string(),
            category: row.get("category").and_then(Value::as_str).unwrap_or_default().to_string(),
            builtin_key: row.get("builtin_key").and_then(Value::as_str).map(str::to_string),
        }
    }
}

/// The status and anchor a write names: by id, by anchor (the builtin row
/// with that key), else the builtin backlog.
pub fn resolve_status(statuses: &[StatusRow], status_id: Option<&str>, anchor: Option<&str>) -> (String, Option<String>) {
    if let Some(id) = status_id {
        if let Some(row) = statuses.iter().find(|s| s.id == id) {
            return (row.builtin_key.clone().unwrap_or_else(|| anchor_for_category(&row.category)), Some(row.id.clone()));
        }
    }
    let key = anchor.unwrap_or("backlog");
    let builtin = statuses.iter().find(|s| s.builtin_key.as_deref() == Some(key));
    (key.to_string(), builtin.map(|s| s.id.clone()))
}

pub struct IssueContext<'a> {
    pub team_id: &'a str,
    pub board_prefix: &'a str,
    pub numbers: &'a [i64],
    pub statuses: &'a [StatusRow],
    pub identity: &'a LocalIdentity,
    pub id: &'a str,
    pub now: &'a str,
}

fn str_of<'a>(input: &'a Row, key: &str) -> Option<&'a str> {
    input.get(key).and_then(Value::as_str)
}

/// `issues.create` as the server would store it.
pub fn create_issue_row(input: &Row, ctx: &IssueContext) -> Row {
    let number = next_issue_number(ctx.numbers);
    let (status, status_id) = resolve_status(ctx.statuses, str_of(input, "statusId"), str_of(input, "status"));
    let agent = str_of(input, "assigneeAgentId").map(str::to_string);
    let assignee = if agent.is_some() { None } else { Some(str_of(input, "assigneeId").unwrap_or(&ctx.identity.user_id).to_string()) };
    obj(json!({
        "id": ctx.id,
        "board_id": input.get("boardId").cloned().unwrap_or(Value::Null),
        "team_id": ctx.team_id,
        "number": number,
        "identifier": issue_identifier(ctx.board_prefix, number),
        "title": input.get("title").cloned().unwrap_or(Value::Null),
        "description": str_of(input, "description"),
        "status": status,
        "status_id": status_id,
        "priority": str_of(input, "priority").unwrap_or("none"),
        "assignee_id": assignee,
        "assignee_agent_id": agent,
        "creator_id": ctx.identity.user_id,
        "creator_agent_id": null,
        "source": "user",
        "due_date": str_of(input, "dueDate"),
        "estimate": input.get("estimate").and_then(Value::as_i64),
        "sort_order": number,
        "completed_at": null,
        "duplicate_of_id": null,
        "draft_id": str_of(input, "draftId"),
        "via_app_id": null,
        "created_at": ctx.now,
        "updated_at": ctx.now,
    }))
}

/// `issueDrafts.upsert` refused: an issue was already filed from the draft.
pub fn draft_consumed_refusal(identifier: &str) -> String {
    SPEC.drafts.consumed_refusal.replace("{identifier}", identifier)
}

/// `issueDrafts.upsert`: the person's draft row, keyed by the client's id
/// (a re-save keeps `created_at`; an agent assignee clears the person).
pub fn draft_row(input: &Row, identity: &LocalIdentity, existing: Option<&Row>, now: &str) -> Row {
    let agent = str_of(input, "assigneeAgentId");
    obj(json!({
        "id": input.get("id").cloned().unwrap_or(Value::Null),
        "user_id": identity.user_id,
        "team_id": input.get("teamId").cloned().unwrap_or(Value::Null),
        "board_id": input.get("boardId").cloned().unwrap_or(Value::Null),
        "title": str_of(input, "title").unwrap_or(""),
        "description": str_of(input, "description").unwrap_or(""),
        "status_id": str_of(input, "statusId"),
        "priority": str_of(input, "priority").unwrap_or("none"),
        "assignee_id": if agent.is_some() { None } else { str_of(input, "assigneeId") },
        "assignee_agent_id": agent,
        "label_ids": input.get("labelIds").filter(|v| v.is_array()).cloned().unwrap_or(json!([])),
        "due_date": str_of(input, "dueDate"),
        "session_id": str_of(input, "sessionId"),
        "created_at": existing.and_then(|e| str_of(e, "created_at")).unwrap_or(now),
        "updated_at": now,
    }))
}

/// #EXP-1800: `composerDrafts.upsert` refused: an empty text is a delete.
pub fn composer_draft_empty_refusal() -> &'static str {
    &SPEC.composer_drafts.empty_refusal
}

/// #EXP-1800: `composerDrafts.upsert`: the identity's one row per `key`
/// (composer-draft.json); a new key takes `id`, a re-save keeps the row's id
/// and `created_at` and replaces team and text.
pub fn composer_draft_row(input: &Row, identity: &LocalIdentity, existing: Option<&Row>, id: &str, now: &str) -> Row {
    obj(json!({
        "id": existing.and_then(|e| str_of(e, "id")).unwrap_or(id),
        "user_id": identity.user_id,
        "team_id": input.get("teamId").cloned().unwrap_or(Value::Null),
        "key": str_of(input, "key").unwrap_or(""),
        "text": str_of(input, "text").unwrap_or(""),
        "created_at": existing.and_then(|e| str_of(e, "created_at")).unwrap_or(now),
        "updated_at": now,
    }))
}

const ISSUE_PATCH_COLUMNS: &[(&str, &str)] = &[
    ("title", "title"),
    ("priority", "priority"),
    ("assigneeId", "assignee_id"),
    ("assigneeAgentId", "assignee_agent_id"),
    ("description", "description"),
    ("dueDate", "due_date"),
    ("duplicateOfId", "duplicate_of_id"),
    ("estimate", "estimate"),
];

/// `issues.update`: a key present = set, null = cleared; a status write
/// dual-writes the anchor and keeps `completed_at` honest.
pub fn apply_issue_patch(row: &Row, patch: &Row, statuses: &[StatusRow], now: &str) -> Row {
    let mut out = row.clone();
    for (key, column) in ISSUE_PATCH_COLUMNS {
        if let Some(value) = patch.get(*key) {
            out.insert((*column).to_string(), value.clone());
        }
    }
    if patch.contains_key("statusId") || patch.contains_key("status") {
        let (status, status_id) = resolve_status(statuses, str_of(patch, "statusId"), str_of(patch, "status"));
        let completed = status == "done";
        let was = str_of(row, "status") == Some("done");
        out.insert("status".into(), json!(status));
        out.insert("status_id".into(), json!(status_id));
        if completed && !was {
            out.insert("completed_at".into(), json!(now));
        }
        if !completed {
            out.insert("completed_at".into(), Value::Null);
        }
    }
    out.insert("updated_at".into(), json!(now));
    out
}

/// `comments.create`: the identity's, one level deep.
pub fn create_comment_row(input: &Row, team_id: &str, identity: &LocalIdentity, existing: &[(String, Option<String>)], id: &str, now: &str) -> Row {
    let mut parent = str_of(input, "parentId").map(str::to_string);
    if let Some(p) = &parent {
        if let Some((_, Some(grand))) = existing.iter().find(|(cid, _)| cid == p) {
            parent = Some(grand.clone());
        }
    }
    obj(json!({
        "id": id,
        "issue_id": input.get("issueId").cloned().unwrap_or(Value::Null),
        "team_id": team_id,
        "author_id": identity.user_id,
        "parent_id": parent,
        "source": "user",
        "audience": "team",
        "author_agent_id": null,
        "via_app_id": null,
        "body": input.get("body").cloned().unwrap_or(Value::Null),
        "edited_at": null,
        "created_at": now,
        "updated_at": now,
    }))
}

/// The title of a new root: the name, else the text's first line cut at 80.
pub fn session_title(name: Option<&str>, text: Option<&str>) -> Option<String> {
    if let Some(n) = name.map(str::trim).filter(|n| !n.is_empty()) {
        return Some(n.to_string());
    }
    let line = text.unwrap_or("").lines().map(str::trim).find(|l| !l.is_empty())?;
    Some(line.chars().take(80).collect())
}

pub struct SendContext<'a> {
    pub team_id: &'a str,
    pub identity: &'a LocalIdentity,
    pub device_id: &'a str,
    /// the target session and its conversation when `sessionId` names one
    pub existing: Option<(Row, Row)>,
    /// #EDE-65 lane L: the rows of the send's `attachmentIds` (id, filename, content_type)
    pub attachments: Vec<Row>,
    pub now: &'a str,
}

const INLINE_IMAGE_TYPES: [&str; 5] = ["image/png", "image/jpeg", "image/webp", "image/gif", "image/avif"];

/// The server's `attachmentLine`: an embed for an inline image, else a link.
pub fn attachment_line(row: &Row) -> String {
    let url = format!("/api/attachments/{}", str_of(row, "id").unwrap_or_default());
    let name: String = str_of(row, "filename").unwrap_or_default().chars().filter(|c| !matches!(c, '[' | ']')).collect();
    if INLINE_IMAGE_TYPES.contains(&str_of(row, "content_type").unwrap_or_default()) {
        format!("![{name}]({url})")
    } else {
        format!("[{name}]({url})")
    }
}

/// A sent body with the line of every attachment it does not name yet (the
/// server's `claimMessageAttachments`).
pub fn body_with_attachments(text: &str, ids: &[String], attachments: &[Row]) -> String {
    let lines: Vec<String> = ids
        .iter()
        .filter(|id| !text.contains(&format!("/api/attachments/{id}")))
        .filter_map(|id| attachments.iter().find(|row| str_of(row, "id") == Some(id.as_str())))
        .map(attachment_line)
        .collect();
    if lines.is_empty() {
        return text.to_string();
    }
    let text = text.trim_end();
    std::iter::once(text.to_string()).filter(|t| !t.is_empty()).chain(lines).collect::<Vec<_>>().join("\n\n")
}

/// `sessions.send`: the rows written and the answer `(session_id, message_id)`.
pub fn send_rows(input: &Row, ctx: &SendContext, next_id: &mut dyn FnMut() -> String) -> (Vec<(&'static str, Row)>, (String, String)) {
    let mut rows = Vec::new();
    let s = |k: &str| str_of(input, k).map(str::to_string);
    let b = |k: &str| input.get(k).and_then(Value::as_bool).unwrap_or(false);
    let (session_id, conversation_id) = match &ctx.existing {
        Some((session, conversation)) => (str_of(session, "id").unwrap_or_default().to_string(), str_of(conversation, "id").unwrap_or_default().to_string()),
        None => {
            let session_id = next_id();
            let conversation_id = next_id();
            rows.push(("sessions", obj(json!({
                "id": session_id, "team_id": ctx.team_id, "parent_id": null, "root_id": session_id, "depth": 0,
                "agent_id": s("agentId"), "app_id": s("appId"), "user_id": ctx.identity.user_id, "device_id": ctx.device_id,
                "cli": s("cli"), "account": s("account"), "model": s("model"), "effort": s("effort"),
                "plan_mode": b("planMode"), "computer_use": b("computerUse"), "ultracode": b("ultracode"), "subagent_model": s("subagentModel"),
                "repository_id": null, "branch": null, "status": "queued", "busy": false, "needs_input": false, "blocked": null,
                "started_at": null, "last_turn_at": null, "ended_at": null, "agent_instructions_at": null, "started_by_agent_id": null,
                "shared": false, "title": session_title(str_of(input, "name"), str_of(input, "text")),
                "seal_public_key": null, "seal_epoch": null, "seal_rotate_epoch": null, "sealed_title": null, "guide": null, "via_app_id": null,
                "created_at": ctx.now, "updated_at": ctx.now,
            }))));
            rows.push(("conversations", obj(json!({"id": conversation_id, "team_id": ctx.team_id, "kind": "session", "session_id": session_id, "title": null, "digest": null, "created_at": ctx.now, "updated_at": ctx.now}))));
            (session_id, conversation_id)
        }
    };
    let message_id = next_id();
    let attachment_ids: Vec<String> = input.get("attachmentIds").and_then(Value::as_array).map(|ids| ids.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
    rows.push(("messages", obj(json!({
        "id": message_id, "conversation_id": conversation_id, "root_id": session_id, "team_id": ctx.team_id,
        "author_kind": "user", "author_id": ctx.identity.user_id, "author_session_id": null,
        "body": body_with_attachments(str_of(input, "text").unwrap_or(""), &attachment_ids, &ctx.attachments), "card": null,
        "attachment_ids": attachment_ids,
        "issue_brief": null, "answered_at": null, "consumed_at": null, "created_at": ctx.now, "updated_at": ctx.now,
    }))));
    if ctx.existing.is_none() {
        if let Some(issues) = input.get("issueIds").and_then(Value::as_array) {
            for (i, issue) in issues.iter().enumerate() {
                rows.push(("session_issues", obj(json!({"id": next_id(), "session_id": session_id, "issue_id": issue, "team_id": ctx.team_id, "source": "user", "added_by": ctx.identity.user_id, "added_by_session_id": null, "sort_order": i, "created_at": ctx.now, "updated_at": ctx.now}))));
            }
        }
    }
    (rows, (session_id, message_id))
}

/// `agents.ensureAssistant`: the identity's one assistant row.
pub fn assistant_row(team_id: &str, identity: &LocalIdentity, device_id: &str, id: &str, now: &str) -> Row {
    obj(json!({"id": id, "team_id": team_id, "name": "Assistant", "icon": null, "color": null, "triggers": [], "device_id": device_id, "cli": null, "account": null, "model": null, "skill_ids": [], "owner_id": identity.user_id, "sort_order": 0, "role": "assistant", "instructions_updated_at": null, "created_at": now, "updated_at": now}))
}
