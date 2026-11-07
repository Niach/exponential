//! #EDE-55: the executor through the real `api` wrappers: what a screen
//! calls on an instance it calls on a Local desktop, and decodes the same.

use api::issues::IssuesCreateInput;
use api::patch::Patch;
use elocal::Desktops;
use std::sync::Arc;

fn desktop() -> (tempfile::TempDir, elocal::LocalDesktop) {
    let dir = tempfile::tempdir().unwrap();
    let desktops = Desktops::open(dir.path()).unwrap();
    let id = desktops.current().unwrap().to_string();
    let desktop = desktops.open_desktop(&id).unwrap();
    (dir, desktop)
}

#[test]
fn boards_statuses_labels_issues_comments() {
    let (_dir, desktop) = desktop();
    let client = desktop.client();
    let team = desktop.team().unwrap();
    let team_id = team["id"].as_str().unwrap().to_string();
    assert_eq!(client.base_url(), format!("local://{team_id}"));

    let board = api::boards::boards_create(&client, &api::boards::BoardsCreateInput { team_id: team_id.clone(), name: "Work".into(), prefix: "wk".into(), icon: None, color: None, repository: None, default_branch: None }).unwrap();
    assert_eq!(board.board.prefix.as_deref(), Some("WK"));
    assert_eq!(board.board.slug.as_deref(), Some("work"));

    let created = api::issues::issues_create(&client, &IssuesCreateInput::new(&board.board.id, "First")).unwrap();
    assert_eq!(created.issue.identifier.as_deref(), Some("WK-1"));
    assert_eq!(created.issue.status.as_deref(), Some("backlog"));
    assert_eq!(created.issue.assignee_id.as_deref(), Some(desktop.executor.identity().user_id.as_str()));
    let second = api::issues::issues_create(&client, &IssuesCreateInput::new(&board.board.id, "Second")).unwrap();
    assert_eq!(second.issue.identifier.as_deref(), Some("WK-2"));

    let label = api::labels::labels_create(&client, &team_id, "Bug", Some("#ff0000")).unwrap();
    api::labels::issue_labels_add(&client, &created.issue.id, &label.label.id).unwrap();
    assert_eq!(desktop.store.rows("issue_labels").unwrap().len(), 1);
    assert!(matches!(api::labels::labels_create(&client, &team_id, "bug", None), Err(api::ApiError::Http { status: 409, .. })));

    let status = api::statuses::statuses_create(&client, &team_id, "started", "Blocked", "#ff0000").unwrap();
    let mut patch = api::issues::IssuesUpdateInput::new(&created.issue.id);
    patch.status_id = Some(status.status.id.clone());
    patch.description = Patch::Set("body".into());
    let updated = api::issues::issues_update(&client, &patch).unwrap();
    assert_eq!(updated.issue.status.as_deref(), Some("in_progress"));
    let row = desktop.store.row("issues", &created.issue.id).unwrap().unwrap();
    assert_eq!(row["description"], serde_json::json!("body"));
    assert_eq!(row["status_id"], serde_json::json!(status.status.id));

    let comment = api::comments::comments_create(&client, &created.issue.id, "hi", None, None, None).unwrap();
    assert_eq!(comment.comment.body.as_deref(), Some("hi"));
    let reply = api::comments::comments_create(&client, &created.issue.id, "re", None, Some(&comment.comment.id), None).unwrap();
    assert_eq!(reply.comment.parent_id.as_deref(), Some(comment.comment.id.as_str()));

    let hits = api::issues::search(&client, &team_id, "fir", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].identifier, "WK-1");
    let got = api::issues::issues_get(&client, "WK-2").unwrap();
    assert_eq!(got.issue.id, second.issue.id);

    api::issues::issues_delete(&client, &created.issue.id).unwrap();
    assert_eq!(desktop.store.rows("comments").unwrap().len(), 0);
    assert_eq!(desktop.store.rows("issue_labels").unwrap().len(), 0);
    assert_eq!(desktop.store.rows("issues").unwrap().len(), 1);

    api::statuses::statuses_delete(&client, &team_id, &status.status.id, None).unwrap();
    assert_eq!(desktop.store.rows("issue_statuses").unwrap().len(), 6);
}

#[test]
fn sessions_agents_prompts_records_and_refusals() {
    let (_dir, desktop) = desktop();
    let client = desktop.client();
    let team_id = desktop.executor.team_id().to_string();

    let mut send = api::sessions::SendInput::default();
    send.team_id = Some(team_id.clone());
    send.text = Some("Fix the login\nplease".into());
    let out = api::sessions::send(&client, &send).unwrap();
    let session = desktop.store.row("sessions", &out.session_id).unwrap().unwrap();
    assert_eq!(session["title"], serde_json::json!("Fix the login"));
    assert_eq!(session["status"], serde_json::json!("queued"));
    assert_eq!(session["device_id"], serde_json::json!(desktop.executor.device_id()));
    let again = api::sessions::send(&client, &api::sessions::SendInput::message(&out.session_id, "more", vec![])).unwrap();
    assert_eq!(again.session_id, out.session_id);
    assert_eq!(desktop.store.rows("messages").unwrap().len(), 2);
    api::sessions::end(&client, &out.session_id).unwrap();
    assert_eq!(desktop.store.row("sessions", &out.session_id).unwrap().unwrap()["status"], serde_json::json!("ended"));

    let assistant = api::agents::ensure_assistant(&client, &team_id).unwrap().unwrap();
    assert_eq!(assistant.role.as_deref(), Some("assistant"));
    assert_eq!(api::agents::ensure_assistant(&client, &team_id).unwrap().unwrap().id, assistant.id);
    let mut update = api::agents::UpdateInput::new(&assistant.id);
    update.instructions = Some("Be kind".into());
    assert_eq!(api::agents::update(&client, &update).unwrap().instructions, "Be kind");
    assert_eq!(api::agents::get(&client, &assistant.id).unwrap().instructions, "Be kind");
    assert_eq!(api::agents::list(&client, &team_id).unwrap().len(), 1);

    let prompt = api::prompts::create(&client, &team_id, "Review", None, "Look at it").unwrap();
    assert_eq!(prompt.body, "Look at it");
    assert_eq!(api::prompts::get(&client, &prompt.id).unwrap().body, "Look at it");
    assert_eq!(api::prompts::list(&client, &team_id).unwrap()[0].name, "Review");

    desktop.store.write("apps", serde_json::json!({"id": "app-1", "team_id": team_id, "slug": "notes", "name": "Notes"}).as_object().cloned().unwrap()).unwrap();
    let record = api::apps::records_upsert(&client, &api::apps::RecordUpsertInput { id: "app-1".into(), kind: "note".into(), data: serde_json::json!({"text": "a"}), ..Default::default() }).unwrap();
    assert!(record.created);
    let patched = api::apps::records_upsert(&client, &api::apps::RecordUpsertInput { id: "app-1".into(), kind: "note".into(), data: serde_json::json!({"more": 1}), record_id: Some(record.id.clone()), ..Default::default() }).unwrap();
    assert!(!patched.created);
    assert_eq!(desktop.store.row("app_records", &record.id).unwrap().unwrap()["data"], serde_json::json!({"text": "a", "more": 1}));
    api::apps::records_delete(&client, "app-1", &record.id, None, None).unwrap();
    assert_eq!(desktop.store.rows("app_records").unwrap().len(), 0);

    let refused: Result<serde_json::Value, _> = client.mutation("devices.register", &serde_json::json!({"deviceId": "d"}));
    match refused {
        Err(api::ApiError::Http { status, message }) => {
            assert_eq!(status, 501);
            assert_eq!(message, "devices.register is not available on a Local desktop");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }

    let row = desktop.executor.attachment_create("note.txt", "text/plain", b"hello", None).unwrap();
    let id = row["id"].as_str().unwrap();
    let (bytes, content_type) = client.get_bytes(&format!("/api/attachments/{id}")).unwrap();
    assert_eq!(bytes, b"hello");
    assert_eq!(content_type.as_deref(), Some("text/plain"));
    let _: Arc<elocal::LocalExecutor> = desktop.executor.clone();
}
