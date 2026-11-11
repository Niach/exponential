use super::*;

fn api_keys_rows() -> Vec<api::users::PersonalKeyMeta> {
    serde_json::from_value(serde_json::json!([
        {"id": "k1", "name": "Device: studio-mac", "start": "expu_d1", "createdAt": "2026-07-01", "lastRequest": "2026-07-03"},
        {"id": "k2", "name": "Device: build-box", "start": "expu_d2", "createdAt": "2026-06-12"},
        {"id": "k3", "name": "CI deploys", "start": "expu_ab", "createdAt": "2026-05-20", "lastRequest": "2026-07-02"},
        {"id": "k4", "name": "MCP client", "start": "expu_cd", "createdAt": "2026-06-30", "scope": {"teams": [{"id": "t", "name": "Acme"}], "boards": []}},
    ]))
    .expect("api key rows")
}

fn api_keys(name: &str, load: impl FnOnce() -> KeysLoad + 'static, minted: bool) {
    shoot_pane(name, ApiKeysPane::for_shots, move |pane, cx| {
        let minted = minted.then(|| serde_json::from_value(serde_json::json!({"key": "expu_ab12cd34ef56gh78ij90kl12mn34op56qr78st90", "id": "k5"})).expect("a minted key"));
        pane.show(load(), minted, cx)
    });
}

#[test]
#[ignore]
fn settings_api_keys_rows() {
    api_keys("api-keys-rows", || KeysLoad::Ready(api_keys_rows(), Some("k1".into())), false);
}

#[test]
#[ignore]
fn settings_api_keys_minted() {
    api_keys("api-keys-minted", || KeysLoad::Ready(api_keys_rows(), Some("k1".into())), true);
}

#[test]
#[ignore]
fn settings_api_keys_empty() {
    api_keys("api-keys-empty", || KeysLoad::Ready(Vec::new(), None), false);
}

#[test]
#[ignore]
fn settings_api_keys_failed() {
    api_keys("api-keys-failed", || KeysLoad::Failed, false);
}

use crate::settings::api_keys::RevokeIdentity;

/// The Revoke alert's body: the key's name and its visible prefix.
#[test]
#[ignore]
fn settings_api_keys_revoke_key() {
    shoot_pane("api-keys-revoke-key", |window, cx| RevokeIdentity::for_shots("CI deploys", Some("expu_ab\u{2026}"), window, cx), |_, _| {});
}

/// The Disconnect alert's body: a login session, named by its host only.
#[test]
#[ignore]
fn settings_api_keys_revoke_device() {
    shoot_pane("api-keys-revoke-device", |window, cx| RevokeIdentity::for_shots("studio-mac", None, window, cx), |_, _| {});
}

use crate::scope_picker::{ScopeBoard, ScopeSelection, ScopeTeam};
use crate::settings::api_keys::MintKeyContent;

/// The scope picker's tree (the web test's).
fn scope_tree() -> Vec<ScopeTeam> {
    let board = |id: &str, name: &str, prefix: &str| ScopeBoard { id: id.into(), name: name.into(), prefix: prefix.into() };
    vec![
        ScopeTeam { id: "t-acme".into(), name: "Acme".into(), boards: vec![board("b-web", "Web", "WEB"), board("b-mob", "Mobile", "MOB")] },
        ScopeTeam { id: "t-lab".into(), name: "Lab".into(), boards: vec![] },
    ]
}

fn scoped(team_ids: &[&str], board_ids: &[&str]) -> ScopeSelection {
    ScopeSelection { all_teams: false, team_ids: team_ids.iter().map(|id| id.to_string()).collect(), board_ids: board_ids.iter().map(|id| id.to_string()).collect() }
}

/// `refused`: Create pressed once the body is up (the slot measures the
/// refusal on the next frame).
fn create_body(name: &'static str, value: ScopeSelection, typed: &'static str, refused: bool) {
    let act = move |body: &mut MintKeyContent, _: &mut Window, cx: &mut gpui::Context<MintKeyContent>| {
        if refused {
            body.refuse(cx);
        }
    };
    shoot_pane_acted(name, move |window, cx| MintKeyContent::for_shots(scope_tree(), value, typed, window, cx), |_, _| {}, None, &[], act);
}

/// The Create API key alert's body as it opens: no name, Everything on.
#[test]
#[ignore]
fn settings_api_keys_create_empty() {
    create_body("api-keys-create-empty", ScopeSelection::everything(), "", false);
}

/// Named, scoped to a team and a board: the tree and the MCP-only note.
#[test]
#[ignore]
fn settings_api_keys_create_scoped() {
    create_body("api-keys-create-scoped", scoped(&["t-lab"], &["b-web"]), "CI deploys", false);
}

/// Create pressed on an empty pick: the refusal under the picker.
#[test]
#[ignore]
fn settings_api_keys_create_refused() {
    create_body("api-keys-create-refused", scoped(&[], &[]), "", true);
}
