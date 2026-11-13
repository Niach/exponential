//! #EXP-1800: a Local desktop keeps every composer's draft like the server
//! (composer-draft.json, local-parity.json `composerDrafts`): the desktop's
//! own writer path (`domain::composer_draft::write_for` → the `api`
//! wrappers) over the local executor, the rows in the store's
//! `composer_drafts` table with the shape's columns.

use api::composer_drafts::{composer_drafts_delete, composer_drafts_upsert};
use domain::composer_draft::{parse_key, write_for, Write};
use domain::rows::ComposerDraftRow;
use elocal::{Desktops, LocalDesktop};
use serde_json::Value;

const COMPOSER_DRAFT: &str = include_str!("../../../../../packages/domain-contract/fixtures/composer-draft.json");

fn fixture() -> Value {
    serde_json::from_str(COMPOSER_DRAFT).unwrap()
}

fn desktop() -> (tempfile::TempDir, LocalDesktop) {
    let dir = tempfile::tempdir().unwrap();
    let desktops = Desktops::open(dir.path()).unwrap();
    let id = desktops.current().unwrap().to_string();
    let desktop = desktops.open_desktop(&id).unwrap();
    (dir, desktop)
}

/// The store's rows, decoded as the composer's shape row.
fn drafts(desktop: &LocalDesktop) -> Vec<ComposerDraftRow> {
    desktop.store.rows("composer_drafts").unwrap().into_iter().map(|row| serde_json::from_value(Value::Object(row)).unwrap()).collect()
}

fn draft(desktop: &LocalDesktop, key: &str) -> Option<ComposerDraftRow> {
    drafts(desktop).into_iter().find(|d| d.key.as_deref() == Some(key))
}

/// What the desktop's writer does with the field's text.
fn write(desktop: &LocalDesktop, team_id: &str, key: &str, text: &str) -> Result<(), api::ApiError> {
    let client = desktop.client();
    match write_for(text) {
        Write::Upsert(text) => composer_drafts_upsert(&client, team_id, key, &text).map(|_| ()),
        Write::Delete => composer_drafts_delete(&client, key).map(|_| ()),
    }
}

fn status(result: Result<impl std::fmt::Debug, api::ApiError>) -> u16 {
    match result {
        Err(api::ApiError::Http { status, .. }) => status,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn the_shape_table_carries_the_proxy_columns() {
    let spec = sync::shapes::shape_by_name("composer_drafts").unwrap();
    assert_eq!(spec.columns, &["id", "user_id", "team_id", "key", "text", "created_at", "updated_at"]);
}

#[test]
fn every_key_kind_keeps_one_row_per_key() {
    let (_dir, desktop) = desktop();
    let team_id = desktop.executor.team_id().to_string();
    let me = desktop.executor.identity().user_id;
    let f = fixture();
    for c in f["keys"].as_array().unwrap() {
        let key = c["key"].as_str().unwrap();
        write(&desktop, &team_id, key, "half a thought").unwrap();
        let row = draft(&desktop, key).unwrap_or_else(|| panic!("{}", c["name"]));
        assert_eq!(row.user_id.as_deref(), Some(me.as_str()), "{}", c["name"]);
        assert_eq!(row.team_id.as_deref(), Some(team_id.as_str()), "{}", c["name"]);
        assert_eq!(row.text.as_deref(), Some("half a thought"), "{}", c["name"]);
        assert!(parse_key(row.key.as_deref().unwrap()).is_some());
    }
    assert_eq!(drafts(&desktop).len(), f["keys"].as_array().unwrap().len());
}

#[test]
fn writes_replay_composer_draft_json() {
    let (_dir, desktop) = desktop();
    let team_id = desktop.executor.team_id().to_string();
    let key = "comment:6f1c2a40-0000-4000-8000-000000000001";
    for c in fixture()["writes"].as_array().unwrap() {
        // Start from a saved draft each time, so a delete has a row to drop.
        write(&desktop, &team_id, key, "earlier").unwrap();
        write(&desktop, &team_id, key, c["text"].as_str().unwrap()).unwrap();
        let expected = &c["expected"];
        match expected["op"].as_str().unwrap() {
            "upsert" => assert_eq!(draft(&desktop, key).and_then(|d| d.text), expected["text"].as_str().map(str::to_string), "{}", c["name"]),
            "delete" => assert!(draft(&desktop, key).is_none(), "{}", c["name"]),
            other => panic!("unknown op {other}"),
        }
    }
}

#[test]
fn a_resave_keeps_the_row_and_a_send_clears_it() {
    let (_dir, desktop) = desktop();
    let client = desktop.client();
    let team_id = desktop.executor.team_id().to_string();
    let key = "session:6f1c2a40-0000-4000-8000-000000000003";
    composer_drafts_upsert(&client, &team_id, key, "first").unwrap();
    let first = draft(&desktop, key).unwrap();
    composer_drafts_upsert(&client, &team_id, key, "first, more").unwrap();
    let second = draft(&desktop, key).unwrap();
    assert_eq!(second.id, first.id);
    assert_eq!(second.created_at, first.created_at);
    assert_eq!(second.text.as_deref(), Some("first, more"));
    assert_eq!(drafts(&desktop).len(), 1);

    // A send (or a discard) deletes it; deleting nothing is fine.
    composer_drafts_delete(&client, key).unwrap();
    assert!(drafts(&desktop).is_empty());
    composer_drafts_delete(&client, key).unwrap();
}

#[test]
fn refusals_match_the_server() {
    let (_dir, desktop) = desktop();
    let client = desktop.client();
    let team_id = desktop.executor.team_id().to_string();
    // An empty text is a delete, never an empty row.
    assert_eq!(status(composer_drafts_upsert(&client, &team_id, "newChat:x", " \n")), 400);
    for key in fixture()["unparsable"].as_array().unwrap() {
        let key = key.as_str().unwrap();
        assert_eq!(status(composer_drafts_upsert(&client, &team_id, key, "text")), 400, "{key:?}");
        assert_eq!(status(composer_drafts_delete(&client, key)), 400, "{key:?}");
    }
    assert_eq!(status(composer_drafts_upsert(&client, "another-team", "newChat:x", "text")), 404);
    assert!(drafts(&desktop).is_empty());
}
