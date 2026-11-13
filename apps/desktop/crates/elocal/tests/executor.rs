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

    // #EXP-1799: a delete is the trash's; the rows leave every read.
    api::issues::issues_delete(&client, &created.issue.id).unwrap();
    assert_eq!(desktop.store.rows("comments").unwrap().len(), 0);
    assert_eq!(desktop.store.rows("issue_labels").unwrap().len(), 0);
    assert_eq!(desktop.store.rows("issues").unwrap().len(), 1);
    assert_eq!(desktop.executor.meta().issue_trash.len(), 1);

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

/// #EDE-65 lane L: a send names its stored attachments: their ids ride the
/// message and their lines join the body (local-parity.json sessions.send).
#[test]
fn a_send_with_attachments_appends_their_lines() {
    let (_dir, desktop) = desktop();
    let client = desktop.client();
    let team_id = desktop.team().unwrap()["id"].as_str().unwrap().to_string();
    let row = desktop.executor.attachment_create("shot.png", "image/png", b"PNG!", None).unwrap();
    let id = row["id"].as_str().unwrap().to_string();
    let input = api::sessions::SendInput { team_id: Some(team_id), text: Some("look".into()), attachment_ids: Some(vec![id.clone()]), ..Default::default() };
    let out = api::sessions::send(&client, &input).unwrap();
    let message = desktop.store.rows_where("messages", "root_id", &out.session_id).unwrap().into_iter().find(|m| m["author_kind"] == "user").unwrap();
    assert_eq!(message["body"], format!("look\n\n![shot.png](/api/attachments/{id})"));
    assert_eq!(message["attachment_ids"], serde_json::json!([id]));
}

/// #EXP-1727: a git working copy linked as the board's repository.
/// F21: a send whose `clientMessageId` this desktop already stored (an
/// outbox retry, also after a restart) answers that message again and
/// writes nothing; another key, or none, sends anew.
#[test]
fn a_retried_send_with_its_client_message_id_stores_one_message() {
    let dir = tempfile::tempdir().unwrap();
    let desktops = Desktops::open(dir.path()).unwrap();
    let id = desktops.current().unwrap().to_string();
    let desktop = desktops.open_desktop(&id).unwrap();
    let client = desktop.client();
    let user_rows = |d: &elocal::LocalDesktop| d.store.rows("messages").unwrap().into_iter().filter(|m| m["author_kind"] == "user").count();

    let mut start = api::sessions::SendInput::default();
    start.team_id = Some(id.clone());
    start.text = Some("Fix the login".into());
    start.client_message_id = Some("c-1".into());
    let first = api::sessions::send(&client, &start).unwrap();
    let again = api::sessions::send(&client, &start).unwrap();
    assert_eq!((again.session_id.as_str(), again.message_id.as_deref()), (first.session_id.as_str(), first.message_id.as_deref()));
    assert_eq!(desktop.store.rows("sessions").unwrap().len(), 1);
    assert_eq!(user_rows(&desktop), 1);

    let mut follow = api::sessions::SendInput::message(&first.session_id, "And the logout", vec![]);
    follow.client_message_id = Some("c-2".into());
    let second = api::sessions::send(&client, &follow).unwrap();
    assert_ne!(second.message_id, first.message_id);
    assert_eq!(user_rows(&desktop), 2);

    // The keys outlive the process (the sidecar): a retry after a restart.
    drop(desktop);
    let reopened = desktops.open_desktop(&id).unwrap();
    let retried = api::sessions::send(&reopened.client(), &follow).unwrap();
    assert_eq!(retried.message_id, second.message_id);
    assert_eq!(user_rows(&reopened), 2);

    // Without a key every send is its own message.
    follow.client_message_id = None;
    api::sessions::send(&reopened.client(), &follow).unwrap();
    assert_eq!(user_rows(&reopened), 3);
}

#[test]
fn a_linked_folder_is_the_boards_repository() {
    let (dir, desktop) = desktop();
    let client = desktop.client();
    let team_id = desktop.team().unwrap()["id"].as_str().unwrap().to_string();
    let board = desktop.store.rows_where("boards", "team_id", &team_id).unwrap()[0]["id"].as_str().unwrap().to_string();
    let not_git = dir.path().join("plain");
    std::fs::create_dir_all(&not_git).unwrap();
    assert!(api::repositories::link_folder(&client, not_git.to_str().unwrap(), None).is_err());
    let repo = dir.path().join("my-app");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/trunk\n").unwrap();
    let linked = api::repositories::link_folder(&client, repo.to_str().unwrap(), Some(&board)).unwrap();
    assert_eq!((linked.full_name.as_str(), linked.default_branch.as_str()), ("my-app", "trunk"));
    // Linking the same folder again is the same repository.
    assert_eq!(api::repositories::link_folder(&client, repo.to_str().unwrap(), None).unwrap().id, linked.id);
    assert_eq!(api::repositories::list_local(&client).unwrap().len(), 1);
    let issue = api::issues::issues_create(&client, &IssuesCreateInput::new(&board, "Wire it")).unwrap();
    let for_issue = api::repositories::for_issue(&client, &issue.issue.id).unwrap().expect("the board's folder");
    assert_eq!(for_issue.repository_id, linked.id);
    assert_eq!(desktop.executor.board_repository(&board).unwrap().path, repo);
}

/// #EXP-1727: a session on an issue of a board with a linked folder runs in
/// its own git worktree of that folder; a second session of the tree reuses it.
#[test]
fn a_session_on_a_linked_board_gets_a_worktree() {
    if std::process::Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let (dir, desktop) = desktop();
    let client = desktop.client();
    let team_id = desktop.team().unwrap()["id"].as_str().unwrap().to_string();
    let board = desktop.store.rows_where("boards", "team_id", &team_id).unwrap()[0]["id"].as_str().unwrap().to_string();
    let repo = dir.path().join("app");
    let git = |args: &[&str]| assert!(std::process::Command::new("git").arg("-C").arg(&repo).args(args).output().unwrap().status.success(), "git {args:?}");
    std::fs::create_dir_all(&repo).unwrap();
    git(&["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("README.md"), "hi\n").unwrap();
    git(&["add", "."]);
    git(&["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "init"]);
    api::repositories::link_folder(&client, repo.to_str().unwrap(), Some(&board)).unwrap();
    let issue = api::issues::issues_create(&client, &IssuesCreateInput::new(&board, "Fix it")).unwrap();
    let mut input = api::sessions::SendInput::default();
    input.text = Some("Fix it".into());
    input.issue_ids = Some(vec![issue.issue.id.clone()]);
    let sent = api::sessions::send(&client, &input).unwrap();
    let (cwd, linked, branch) = elocal::runs::worktree_for(&desktop, &sent.session_id).expect("a worktree");
    assert_eq!(linked.name, "app");
    assert!(cwd.join("README.md").exists(), "the worktree is checked out");
    assert!(branch.starts_with("exp/"));
    assert_eq!(elocal::runs::worktree_for(&desktop, &sent.session_id).unwrap().0, cwd);
}

/// #EDE-65 J3b: the IDE's new-issue page autosaves its draft on a Local
/// desktop too; filing the issue consumes the draft, and a late autosave
/// of it is refused instead of resurrecting it (local-parity.json `drafts`).
#[test]
fn drafts_save_and_are_consumed_by_their_issue() {
    let (_dir, desktop) = desktop();
    let client = desktop.client();
    let team_id = desktop.executor.team_id().to_string();
    let board = desktop.store.rows("boards").unwrap()[0]["id"].as_str().unwrap().to_string();
    let draft_id = api::issue_drafts::new_draft_id();

    let mut save = api::issue_drafts::IssueDraftUpsertInput::new(&draft_id, &team_id, &board);
    save.title = "Half a thought".into();
    api::issue_drafts::issue_drafts_upsert(&client, &save).unwrap();
    let first = desktop.store.row("issue_drafts", &draft_id).unwrap().unwrap();
    assert_eq!(first["title"], "Half a thought");
    assert_eq!(first["user_id"].as_str(), Some(desktop.executor.identity().user_id.as_str()));
    save.title = "Half a thought, more".into();
    api::issue_drafts::issue_drafts_upsert(&client, &save).unwrap();
    let again = desktop.store.row("issue_drafts", &draft_id).unwrap().unwrap();
    assert_eq!(again["title"], "Half a thought, more");
    assert_eq!(again["created_at"], first["created_at"]);
    assert!(api::issue_drafts::issue_drafts_list_attachments(&client, &draft_id).unwrap().is_empty());

    let mut create = IssuesCreateInput::new(&board, "Half a thought, more");
    create.draft_id = Some(draft_id.clone());
    let filed = api::issues::issues_create(&client, &create).unwrap();
    assert!(desktop.store.row("issue_drafts", &draft_id).unwrap().is_none());
    assert_eq!(desktop.store.row("issues", &filed.issue.id).unwrap().unwrap()["draft_id"].as_str(), Some(draft_id.as_str()));
    match api::issue_drafts::issue_drafts_upsert(&client, &save) {
        Err(api::ApiError::Http { status: 409, message }) => assert_eq!(message, format!("This draft was already created as {}", filed.issue.identifier.unwrap())),
        other => panic!("a consumed draft must be refused: {other:?}"),
    }
    assert!(!api::issue_drafts::issue_drafts_delete(&client, &draft_id).unwrap().deleted);

    // Another board's team: refused like the server.
    let other = api::issue_drafts::IssueDraftUpsertInput::new(api::issue_drafts::new_draft_id(), "another-team", &board);
    assert!(matches!(api::issue_drafts::issue_drafts_upsert(&client, &other), Err(api::ApiError::Http { status: 400, .. })));
}
