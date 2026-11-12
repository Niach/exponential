//! #EXP-1799: a Local desktop's issue delete goes through the trash with the
//! server's semantics (issue-lifecycle.json `trash`, #EXP-1797): the three
//! columns stamped, nothing cascades, every read hides it, restore brings
//! everything back, purge (Delete forever, the 30-day sweep) drops it, and
//! the team stream gets `issue.deleted` / `issue.restored` / `issue.purged`.

use api::issues::IssuesCreateInput;
use elocal::{Desktops, LocalDesktop};
use serde_json::{json, Value};

const LIFECYCLE: &str = include_str!("../../../../../packages/domain-contract/fixtures/issue-lifecycle.json");

fn trash_fixture() -> Value {
    serde_json::from_str::<Value>(LIFECYCLE).unwrap()["trash"].clone()
}

fn desktop(dir: &std::path::Path) -> LocalDesktop {
    let desktops = Desktops::open(dir).unwrap();
    let id = desktops.current().unwrap().to_string();
    desktops.open_desktop(&id).unwrap()
}

fn ms(text: &str) -> i64 {
    domain::instant::parse_instant(text).unwrap()
}

fn events(desktop: &LocalDesktop, name: &str) -> Vec<serde_json::Map<String, Value>> {
    desktop.store.rows("events").unwrap().into_iter().filter(|e| e["name"] == json!(name)).collect()
}

struct Board {
    id: String,
    first: String,
    second: String,
}

/// A board with two related issues; the first has a comment, a label, a file
/// and a session link.
fn seed(desktop: &LocalDesktop) -> Board {
    let client = desktop.client();
    let team_id = desktop.executor.team_id().to_string();
    let board = api::boards::boards_create(&client, &api::boards::BoardsCreateInput { team_id: team_id.clone(), name: "Work".into(), prefix: "wk".into(), icon: None, color: None, repository: None, default_branch: None }).unwrap().board.id;
    let first = api::issues::issues_create(&client, &IssuesCreateInput::new(&board, "First")).unwrap().issue.id;
    let second = api::issues::issues_create(&client, &IssuesCreateInput::new(&board, "Second")).unwrap().issue.id;
    api::comments::comments_create(&client, &first, "hi", None, None, None).unwrap();
    let label = api::labels::labels_create(&client, &team_id, "Bug", None).unwrap().label.id;
    api::labels::issue_labels_add(&client, &first, &label).unwrap();
    desktop.executor.attachment_create("a.txt", "text/plain", b"bytes", Some(&first)).unwrap();
    let obj = |v: Value| v.as_object().cloned().unwrap();
    desktop.store.write("issue_relations", obj(json!({"id": "r1", "issue_id": first, "related_issue_id": second, "team_id": team_id, "type": "related"}))).unwrap();
    let mut send = api::sessions::SendInput::default();
    send.team_id = Some(team_id.clone());
    send.text = Some("work on it".into());
    send.issue_ids = Some(vec![first.clone()]);
    api::sessions::send(&client, &send).unwrap();
    Board { id: board, first, second }
}

fn counts(desktop: &LocalDesktop) -> [usize; 6] {
    ["issues", "comments", "issue_labels", "attachments", "issue_relations", "session_issues"].map(|t| desktop.store.rows(t).unwrap().len())
}

#[test]
fn delete_trashes_hides_and_restore_brings_everything_back() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = desktop(dir.path());
    let client = desktop.client();
    let team_id = desktop.executor.team_id().to_string();
    let board = seed(&desktop);
    let before = counts(&desktop);
    assert_eq!(before, [2, 1, 1, 1, 1, 1]);
    let attachment = desktop.store.rows("attachments").unwrap()[0]["id"].as_str().unwrap().to_string();

    api::issues::issues_delete(&client, &board.first).unwrap();
    // Hidden from every read: the tables, issues.get, issues.search.
    assert_eq!(counts(&desktop), [1, 0, 0, 0, 0, 0]);
    assert!(matches!(api::issues::issues_get(&client, "WK-1"), Err(api::ApiError::Http { status: 404, .. })));
    assert!(api::issues::search(&client, &team_id, "first", 10).unwrap().is_empty());
    // Nothing cascades: the bytes stay.
    assert!(desktop.blobs.get(&attachment).is_some());
    // A second delete of a trashed issue answers not found.
    assert!(matches!(api::issues::issues_delete(&client, &board.first), Err(api::ApiError::Http { status: 404, .. })));

    let trash = api::issues::issues_list_deleted(&client, &team_id).unwrap();
    assert_eq!(trash.len(), 1);
    let row = &trash[0];
    assert_eq!((row.id.as_str(), row.identifier.as_str(), row.title.as_str()), (board.first.as_str(), "WK-1", "First"));
    assert_eq!(row.board_id.as_deref(), Some(board.id.as_str()));
    assert_eq!(row.deleted_by.as_deref(), Some(desktop.executor.identity().user_id.as_str()));
    assert_eq!(row.deleted_by_agent_id, None);
    let retention = trash_fixture()["retentionDays"].as_i64().unwrap();
    assert_eq!(ms(row.purge_at.as_deref().unwrap()) - ms(row.deleted_at.as_deref().unwrap()), retention * 86_400_000);

    let deleted = events(&desktop, "issue.deleted");
    assert_eq!(deleted.len(), 1);
    assert_eq!(deleted[0]["emitted_by"], json!("user"));
    assert_eq!(deleted[0]["user_id"], json!(desktop.executor.identity().user_id));
    let payload = deleted[0]["payload"].as_object().unwrap();
    let mut keys: Vec<&str> = payload.keys().map(String::as_str).collect();
    let fixture = trash_fixture();
    let mut expected: Vec<&str> = fixture["eventPayload"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    keys.sort();
    expected.sort();
    assert_eq!(keys, expected);
    assert_eq!(payload["identifier"], json!("WK-1"));
    assert_eq!(payload["viaMcp"], json!(false));

    let out = api::issues::issues_restore(&client, &board.first).unwrap();
    assert_eq!(out.identifier.as_deref(), Some("WK-1"));
    assert_eq!(counts(&desktop), before);
    let issue = desktop.store.row("issues", &board.first).unwrap().unwrap();
    assert_eq!(issue["identifier"], json!("WK-1"));
    assert!(!issue.contains_key("deleted_at"));
    assert!(api::issues::issues_list_deleted(&client, &team_id).unwrap().is_empty());
    assert_eq!(events(&desktop, "issue.restored").len(), 1);
    assert!(matches!(api::issues::issues_restore(&client, &board.first), Err(api::ApiError::Http { status: 404, .. })));
}

#[test]
fn a_relation_stays_hidden_while_either_side_is_trashed() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = desktop(dir.path());
    let client = desktop.client();
    let board = seed(&desktop);
    let out = api::issues::issues_bulk_delete(&client, &[board.first.clone(), board.second.clone()]).unwrap();
    assert_eq!(out.deleted, Some(2));
    assert_eq!(counts(&desktop)[0], 0);
    api::issues::issues_restore(&client, &board.first).unwrap();
    assert_eq!(desktop.store.rows("issue_relations").unwrap().len(), 0, "the other side is still trashed");
    api::issues::issues_restore(&client, &board.second).unwrap();
    assert_eq!(desktop.store.rows("issue_relations").unwrap().len(), 1);
}

#[test]
fn purge_and_the_sweep_drop_it_for_good() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = desktop(dir.path());
    let client = desktop.client();
    let team_id = desktop.executor.team_id().to_string();
    let board = seed(&desktop);
    let attachment = desktop.store.rows("attachments").unwrap()[0]["id"].as_str().unwrap().to_string();

    // Delete forever: only from the trash.
    assert!(matches!(api::issues::issues_purge(&client, &board.first), Err(api::ApiError::Http { status: 404, .. })));
    api::issues::issues_delete(&client, &board.first).unwrap();
    api::issues::issues_purge(&client, &board.first).unwrap();
    assert!(api::issues::issues_list_deleted(&client, &team_id).unwrap().is_empty());
    assert!(desktop.blobs.get(&attachment).is_none());
    assert!(matches!(api::issues::issues_restore(&client, &board.first), Err(api::ApiError::Http { status: 404, .. })));
    let purged = events(&desktop, "issue.purged");
    assert_eq!((purged.len(), &purged[0]["emitted_by"]), (1, &json!("user")));

    // The sweep replays purgeDueCases against the trashed issue's deleted_at.
    api::issues::issues_delete(&client, &board.second).unwrap();
    let deleted_at = ms(api::issues::issues_list_deleted(&client, &team_id).unwrap()[0].deleted_at.as_deref().unwrap());
    let mut cases: Vec<Value> = trash_fixture()["purgeDueCases"].as_array().unwrap().iter().filter(|c| !c["input"]["deletedAt"].is_null()).cloned().collect();
    cases.sort_by_key(|c| c["expected"].as_bool());
    for case in cases {
        let elapsed = ms(case["input"]["now"].as_str().unwrap()) - ms(case["input"]["deletedAt"].as_str().unwrap());
        let swept = desktop.executor.purge_due_issues(deleted_at + elapsed);
        let due = case["expected"].as_bool().unwrap();
        assert_eq!(swept == vec![board.second.clone()], due, "{}", case["name"]);
        if due {
            break;
        }
    }
    let swept = events(&desktop, "issue.purged");
    let last = swept.iter().find(|e| e["payload"]["issueId"] == json!(board.second)).unwrap();
    assert_eq!(last["emitted_by"], json!("server"));
    assert_eq!(last["payload"]["actorUserId"], Value::Null);
}

#[test]
fn removing_a_board_purges_its_trashed_issues() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = desktop(dir.path());
    let client = desktop.client();
    let board = seed(&desktop);
    api::issues::issues_delete(&client, &board.first).unwrap();
    api::boards::boards_delete(&client, &board.id).unwrap();
    assert!(desktop.executor.meta().issue_trash.is_empty());
    assert_eq!(events(&desktop, "issue.purged").len(), 1);
}

/// An existing desktop (its sidecar from before the trash) opens, reads an
/// empty trash, and trashes from then on; the trash survives a reopen.
#[test]
fn an_existing_desktop_upgrades_into_the_trash() {
    let dir = tempfile::tempdir().unwrap();
    let (desktop_dir, first) = {
        let desktop = desktop(dir.path());
        let board = seed(&desktop);
        (desktop.dir.clone(), board.first)
    };
    let meta_path = desktop_dir.join("desktop.json");
    let mut meta: Value = serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
    meta.as_object_mut().unwrap().remove("issueTrash");
    std::fs::write(&meta_path, serde_json::to_string_pretty(&meta).unwrap()).unwrap();

    let desktop = self::desktop(dir.path());
    let client = desktop.client();
    let team_id = desktop.executor.team_id().to_string();
    assert!(api::issues::issues_list_deleted(&client, &team_id).unwrap().is_empty());
    api::issues::issues_delete(&client, &first).unwrap();
    drop(client);
    drop(desktop);

    let desktop = self::desktop(dir.path());
    let trash = api::issues::issues_list_deleted(&desktop.client(), &team_id).unwrap();
    assert_eq!(trash.len(), 1);
    api::issues::issues_restore(&desktop.client(), &first).unwrap();
    assert_eq!(counts(&desktop), [2, 1, 1, 1, 1, 1]);
}
