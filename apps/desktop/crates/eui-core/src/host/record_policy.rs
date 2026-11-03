//! eApps R3 (docs/spec/apps.md §3.4, §9): who may read, create, update and
//! delete a kind's records, under the member's app access. A line-by-line
//! port of `src/host/record-policy.ts`; `fixtures/record-policy.json` is
//! the spec (`tests/record_policy.rs` replays it).

use super::package::PackageIssue;
use serde_json::Value;

pub const RECORD_POLICY_OPS: &[&str] = &["read", "create", "update", "delete"];
pub const RECORD_POLICY_LEVELS: &[&str] = &["member", "author", "owners"];
pub const APP_ACCESS_LEVELS: &[&str] = &["none", "read", "write"];

/// One op of a kind's policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordOp {
    Read,
    Create,
    Update,
    Delete,
}

impl RecordOp {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "read" => Self::Read,
            "create" => Self::Create,
            "update" => Self::Update,
            "delete" => Self::Delete,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
        }
    }
}

/// A kind's effective policy: a level (`member`, `author`, `owners`) per op.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordPolicy {
    pub read: &'static str,
    pub create: &'static str,
    pub update: &'static str,
    pub delete: &'static str,
}

pub const DEFAULT_RECORD_POLICY: RecordPolicy = RecordPolicy { read: "member", create: "member", update: "author", delete: "author" };

impl RecordPolicy {
    pub fn level(&self, op: RecordOp) -> &'static str {
        match op {
            RecordOp::Read => self.read,
            RecordOp::Create => self.create,
            RecordOp::Update => self.update,
            RecordOp::Delete => self.delete,
        }
    }
    fn set(&mut self, op: RecordOp, level: &'static str) {
        match op {
            RecordOp::Read => self.read = level,
            RecordOp::Create => self.create = level,
            RecordOp::Update => self.update = level,
            RecordOp::Delete => self.delete = level,
        }
    }
    pub fn to_json(&self) -> Value {
        serde_json::json!({"read": self.read, "create": self.create, "update": self.update, "delete": self.delete})
    }
}

pub const COPY_NONE: &str = "You have no access to this app";
pub const COPY_READ_ONLY: &str = "You have read-only access to this app";
pub const COPY_AUTHOR_READ: &str = "Only the record's author or a team owner can read it";
pub const COPY_AUTHOR_CHANGE: &str = "Only the record's author or a team owner can change it";
pub const COPY_OWNERS_READ: &str = "Only team owners can read these records";
pub const COPY_OWNERS_CREATE: &str = "Only team owners can add these records";
pub const COPY_OWNERS_CHANGE: &str = "Only team owners can change these records";

/// The refusal texts as `record-policy.json` `copy` spells them.
pub fn record_policy_copy() -> Value {
    serde_json::json!({
        "none": COPY_NONE,
        "readOnly": COPY_READ_ONLY,
        "author": {"read": COPY_AUTHOR_READ, "update": COPY_AUTHOR_CHANGE, "delete": COPY_AUTHOR_CHANGE},
        "owners": {"read": COPY_OWNERS_READ, "create": COPY_OWNERS_CREATE, "update": COPY_OWNERS_CHANGE, "delete": COPY_OWNERS_CHANGE},
    })
}

/// Who is asking: the member id (`None` = signed out), their team role
/// (`None` = not a member of the app's team) and their access level for the
/// app (`None` = write).
#[derive(Clone, Copy, Debug, Default)]
pub struct RecordViewer<'a> {
    pub id: Option<&'a str>,
    pub role: Option<&'a str>,
    pub access: Option<&'a str>,
}

fn level_of(v: &str) -> Option<&'static str> {
    RECORD_POLICY_LEVELS.iter().copied().find(|l| *l == v)
}

/// The issues of a kind's `policy` (each under `at`).
pub fn validate_record_policy(policy: &Value, at: &str) -> Vec<PackageIssue> {
    let Some(map) = policy.as_object() else {
        return vec![PackageIssue { path: at.into(), message: "policy is {read?, create?, update?, delete?}".into() }];
    };
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort_by(|a, b| super::utf16_cmp(a, b));
    let mut issues = Vec::new();
    for key in keys {
        let value = &map[key];
        let issue = |message: String| PackageIssue { path: format!("{at}/{key}"), message };
        if RecordOp::parse(key).is_none() {
            issues.push(issue(format!("unknown policy op {key}: read, create, update or delete")));
        } else if key == "create" && value.as_str() == Some("author") {
            issues.push(issue("create is member or owners: a new record has no author yet".into()));
        } else if value.as_str().and_then(level_of).is_none() {
            issues.push(issue(format!("{key} is member, author or owners")));
        }
    }
    issues
}

/// A kind's effective policy: its `policy` over the defaults (an invalid
/// entry keeps the default).
pub fn record_policy(kind: Option<&Value>) -> RecordPolicy {
    let mut out = DEFAULT_RECORD_POLICY;
    let Some(policy) = kind.and_then(|k| k.get("policy")).and_then(Value::as_object) else { return out };
    for name in RECORD_POLICY_OPS {
        let op = RecordOp::parse(name).unwrap();
        if let Some(level) = policy.get(*name).and_then(Value::as_str).and_then(level_of) {
            if !(op == RecordOp::Create && level == "author") {
                out.set(op, level);
            }
        }
    }
    out
}

/// A record's author (`authorId ?? createdBy`), or `None`.
pub fn record_author_of(row: Option<&Value>) -> Option<&str> {
    let row = row?;
    row.get("authorId").and_then(Value::as_str).or_else(|| row.get("createdBy").and_then(Value::as_str))
}

/// Why `viewer` may NOT `op` a record of a kind with `policy` (`row` = the
/// record for read/update/delete), or `None` when they may.
pub fn record_refusal(policy: &RecordPolicy, op: RecordOp, viewer: &RecordViewer, row: Option<&Value>) -> Option<&'static str> {
    let access = viewer.access.unwrap_or("write");
    if viewer.id.is_none() || viewer.role.is_none() || access == "none" {
        return Some(COPY_NONE);
    }
    if access == "read" && op != RecordOp::Read {
        return Some(COPY_READ_ONLY);
    }
    if viewer.role == Some("owner") {
        return None;
    }
    let level = policy.level(op);
    if level == "member" {
        return None;
    }
    if level == "owners" || op == RecordOp::Create {
        return Some(match op {
            RecordOp::Read => COPY_OWNERS_READ,
            RecordOp::Create => COPY_OWNERS_CREATE,
            _ => COPY_OWNERS_CHANGE,
        });
    }
    match record_author_of(row) {
        Some(author) if Some(author) == viewer.id => None,
        _ => Some(if op == RecordOp::Read { COPY_AUTHOR_READ } else { COPY_AUTHOR_CHANGE }),
    }
}

/// May `viewer` `op` the record?
pub fn record_allowed(policy: &RecordPolicy, op: RecordOp, viewer: &RecordViewer, row: Option<&Value>) -> bool {
    record_refusal(policy, op, viewer, row).is_none()
}

/// The rows (`{kind, authorId?, createdBy?, …}`) `viewer` may read, each by
/// its kind's policy (`kinds` = the package's `records`), in order.
pub fn readable_rows<'r>(rows: &'r [Value], kinds: Option<&Value>, viewer: &RecordViewer) -> Vec<&'r Value> {
    rows.iter()
        .filter(|r| {
            let kind = r.get("kind").and_then(Value::as_str).unwrap_or("");
            record_allowed(&record_policy(kinds.and_then(|k| k.get(kind))), RecordOp::Read, viewer, Some(r))
        })
        .collect()
}
