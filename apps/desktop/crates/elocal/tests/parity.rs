//! #EDE-55: every case of fixtures/local-parity.json, replayed in Rust.

use elocal::parity::*;
use elocal::LocalIdentity;
use serde_json::{json, Map, Value};

fn fixture() -> Value {
    serde_json::from_str(LOCAL_PARITY_JSON).unwrap()
}

fn obj(v: &Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap()
}

fn identity(v: &Value) -> LocalIdentity {
    LocalIdentity { user_id: v["userId"].as_str().unwrap().into(), email: v["email"].as_str().unwrap().into(), name: v["name"].as_str().unwrap_or("").into() }
}

fn ids(list: &Value) -> impl FnMut() -> String {
    let list: Vec<String> = list.as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    let mut i = 0;
    move || {
        let id = list[i].clone();
        i += 1;
        id
    }
}

fn statuses(v: &Value) -> Vec<StatusRow> {
    v.as_array().unwrap().iter().map(|s| StatusRow::from_row(&obj(s))).collect()
}

#[test]
fn procedures() {
    for c in fixture()["procedures"]["cases"].as_array().unwrap() {
        let p = c["procedure"].as_str().unwrap();
        assert_eq!(is_local_procedure(p), c["local"].as_bool().unwrap(), "{p}");
        if let Some(err) = c["error"].as_str() {
            assert_eq!(refusal(p), err);
        }
    }
}

#[test]
fn identity_email() {
    for c in fixture()["identity"]["cases"].as_array().unwrap() {
        assert_eq!(local_email(c["login"].as_str().unwrap()), c["email"].as_str().unwrap());
    }
}

#[test]
fn seed() {
    for c in fixture()["seed"]["cases"].as_array().unwrap() {
        let mut next = ids(&c["ids"]);
        let rows = seed_rows(c["desktopId"].as_str().unwrap(), c["desktopName"].as_str().unwrap(), &identity(&c["identity"]), c["now"].as_str().unwrap(), &mut next);
        let got: Vec<Value> = rows.into_iter().map(|(table, row)| json!({"table": table, "row": row})).collect();
        assert_eq!(Value::Array(got), c["expected"], "{}", c["name"]);
    }
}

#[test]
fn numbers_and_identifiers() {
    let f = fixture();
    for c in f["issues"]["numberCases"].as_array().unwrap() {
        let numbers: Vec<i64> = c["numbers"].as_array().unwrap().iter().map(|n| n.as_i64().unwrap()).collect();
        assert_eq!(next_issue_number(&numbers), c["next"].as_i64().unwrap());
    }
    for c in f["issues"]["identifierCases"].as_array().unwrap() {
        assert_eq!(issue_identifier(c["prefix"].as_str().unwrap(), c["number"].as_i64().unwrap()), c["identifier"].as_str().unwrap());
    }
    for (category, anchor) in f["issues"]["anchorByCategory"].as_object().unwrap() {
        assert_eq!(anchor_for_category(category), anchor.as_str().unwrap());
    }
}

#[test]
fn issue_create_and_update() {
    let f = fixture();
    let team = &f["issues"]["team"];
    let who = identity(&team["identity"]);
    let statuses = statuses(&team["statuses"]);
    for c in f["issues"]["create"]["cases"].as_array().unwrap() {
        let numbers: Vec<i64> = c["numbers"].as_array().unwrap().iter().map(|n| n.as_i64().unwrap()).collect();
        let ctx = IssueContext { team_id: team["teamId"].as_str().unwrap(), board_prefix: team["boards"][0]["prefix"].as_str().unwrap(), numbers: &numbers, statuses: &statuses, identity: &who, id: c["id"].as_str().unwrap(), now: c["now"].as_str().unwrap() };
        assert_eq!(Value::Object(create_issue_row(&obj(&c["input"]), &ctx)), c["expected"], "{}", c["name"]);
    }
    for c in f["issues"]["update"]["cases"].as_array().unwrap() {
        assert_eq!(Value::Object(apply_issue_patch(&obj(&c["row"]), &obj(&c["patch"]), &statuses, c["now"].as_str().unwrap())), c["expected"], "{}", c["name"]);
    }
}

#[test]
fn drafts() {
    let f = fixture();
    let who = identity(&f["issues"]["team"]["identity"]);
    for c in f["drafts"]["upsert"]["cases"].as_array().unwrap() {
        let existing = c["existing"].as_object().cloned();
        assert_eq!(Value::Object(draft_row(&obj(&c["input"]), &who, existing.as_ref(), c["now"].as_str().unwrap())), c["expected"], "{}", c["name"]);
    }
    assert_eq!(draft_consumed_refusal("GEN-7"), "This draft was already created as GEN-7");
}

#[test]
fn composer_drafts() {
    let f = fixture();
    let who = identity(&f["issues"]["team"]["identity"]);
    for c in f["composerDrafts"]["upsert"]["cases"].as_array().unwrap() {
        let existing = c["existing"].as_object().cloned();
        let row = composer_draft_row(&obj(&c["input"]), &who, existing.as_ref(), c["id"].as_str().unwrap(), c["now"].as_str().unwrap());
        assert_eq!(Value::Object(row), c["expected"], "{}", c["name"]);
    }
    assert_eq!(composer_draft_empty_refusal(), "An empty draft is a delete");
}

#[test]
fn comments() {
    let f = fixture();
    let team = &f["issues"]["team"];
    let who = identity(&team["identity"]);
    for c in f["comments"]["cases"].as_array().unwrap() {
        let existing: Vec<(String, Option<String>)> = c["existing"].as_array().unwrap().iter().map(|e| (e["id"].as_str().unwrap().to_string(), e["parent_id"].as_str().map(str::to_string))).collect();
        assert_eq!(Value::Object(create_comment_row(&obj(&c["input"]), team["teamId"].as_str().unwrap(), &who, &existing, c["id"].as_str().unwrap(), c["now"].as_str().unwrap())), c["expected"], "{}", c["name"]);
    }
}

#[test]
fn sessions_send() {
    let f = fixture();
    let team = &f["issues"]["team"];
    let who = identity(&team["identity"]);
    for c in f["sessions"]["send"]["cases"].as_array().unwrap() {
        let existing = c.get("existing").and_then(|e| e.as_object()).map(|e| (obj(&e["session"]), obj(&e["conversation"])));
        let attachments = c.get("attachments").and_then(Value::as_array).map(|rows| rows.iter().map(obj).collect()).unwrap_or_default();
        let ctx = SendContext { team_id: team["teamId"].as_str().unwrap(), identity: &who, device_id: f["sessions"]["deviceId"].as_str().unwrap(), existing, attachments, now: c["now"].as_str().unwrap() };
        let mut next = ids(&c["ids"]);
        let (rows, (session_id, message_id)) = send_rows(&obj(&c["input"]), &ctx, &mut next);
        if let Some(expected) = c.get("expected") {
            let got: Vec<Value> = rows.iter().map(|(table, row)| json!({"table": table, "row": row})).collect();
            assert_eq!(Value::Array(got), *expected, "{}", c["name"]);
            assert_eq!(json!({"sessionId": session_id, "messageId": message_id}), c["output"]);
        }
        if let Some(title) = c.get("expectedTitle") {
            assert_eq!(rows[0].1["title"], *title, "{}", c["name"]);
        }
    }
}

#[test]
fn sessions_wake() {
    let f = fixture();
    let device = f["sessions"]["deviceId"].as_str().unwrap();
    let rows = |v: &Value| v.as_array().unwrap().iter().map(obj).collect::<Vec<_>>();
    for c in f["sessions"]["wake"]["cases"].as_array().unwrap() {
        let live: Vec<String> = c["live"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
        let refused: std::collections::HashMap<String, Vec<String>> = serde_json::from_value(c["refused"].clone()).unwrap();
        let got: Vec<Value> = wake_plan(device, &rows(&c["sessions"]), &rows(&c["messages"]), &live, &refused)
            .into_iter()
            .map(|w| json!({"sessionId": w.session_id, "mode": if w.live { "send" } else { "wake" }, "messageIds": w.message_ids}))
            .collect();
        assert_eq!(Value::Array(got), c["expected"], "{}", c["name"]);
    }
}

#[test]
fn assistant() {
    let f = fixture();
    let team = &f["issues"]["team"];
    let a = &f["agents"]["assistant"];
    assert_eq!(Value::Object(assistant_row(team["teamId"].as_str().unwrap(), &identity(&team["identity"]), f["sessions"]["deviceId"].as_str().unwrap(), a["id"].as_str().unwrap(), a["now"].as_str().unwrap())), a["expected"]);
}
