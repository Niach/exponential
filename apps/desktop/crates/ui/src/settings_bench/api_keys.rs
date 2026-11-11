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

use crate::scope_picker::{ScopeBoard, ScopeSelection, ScopeTeam};
use crate::settings::api_keys::{MintKeyContent, RevokeIdentity};

fn scope_tree(teams: usize) -> Vec<ScopeTeam> {
    let mut tree = vec![
        ScopeTeam {
            id: "t-acme".into(),
            name: "Acme".into(),
            boards: vec![
                ScopeBoard { id: "b-web".into(), name: "Web".into(), prefix: "WEB".into() },
                ScopeBoard { id: "b-mob".into(), name: "Mobile".into(), prefix: "MOB".into() },
            ],
        },
        ScopeTeam { id: "t-lab".into(), name: "Lab".into(), boards: vec![] },
    ];
    for n in 2..teams {
        tree.push(ScopeTeam {
            id: format!("t-{n}"),
            name: format!("Team {n}"),
            boards: vec![ScopeBoard { id: format!("b-{n}"), name: format!("Board {n}"), prefix: format!("B{n}") }],
        });
    }
    tree
}

fn scoped(team_ids: &[&str], board_ids: &[&str]) -> ScopeSelection {
    ScopeSelection { all_teams: false, team_ids: team_ids.iter().map(|id| id.to_string()).collect(), board_ids: board_ids.iter().map(|id| id.to_string()).collect() }
}

/// Drawn, then the motion clock moved on: the switch eases into its first
/// state, and the headless clock never moves on its own.
fn mint(name: &'static str, teams: usize, value: ScopeSelection, typed: &'static str, error: Option<&'static str>) {
    shoot_pane_acted(name, move |window, cx| MintKeyContent::for_shots(scope_tree(teams), value, typed, error, window, cx), |_, _| {}, None, &[], |pane, _, cx| pane.settle(cx));
}

/// The Create dialog as it opens: no name, Everything on.
#[test]
#[ignore]
fn settings_api_keys_mint_everything() {
    mint("api-keys-mint-everything", 2, ScopeSelection::everything(), "", None);
}

#[test]
#[ignore]
fn settings_api_keys_mint_named() {
    mint("api-keys-mint-named", 2, ScopeSelection::everything(), "CI deploys", None);
}

/// Everything off, one board picked: the tree and the scoped note.
#[test]
#[ignore]
fn settings_api_keys_mint_scoped() {
    mint("api-keys-mint-scoped", 2, scoped(&[], &["b-web"]), "", None);
}

/// A whole team ticked: its boards read checked and disabled.
#[test]
#[ignore]
fn settings_api_keys_mint_whole_team() {
    mint("api-keys-mint-whole-team", 2, scoped(&["t-acme"], &[]), "", None);
}

/// An empty pick refused on Create: the error under the picker.
#[test]
#[ignore]
fn settings_api_keys_mint_error() {
    mint("api-keys-mint-error", 2, scoped(&[], &[]), "", Some("Select at least one team or board, or switch Everything on."));
}

#[test]
#[ignore]
fn settings_api_keys_mint_no_teams() {
    mint("api-keys-mint-no-teams", 0, scoped(&[], &[]), "", None);
}

/// More teams than the tree's 168 px cap: it scrolls.
#[test]
#[ignore]
fn settings_api_keys_mint_long() {
    mint("api-keys-mint-long", 8, scoped(&["t-3"], &["b-web"]), "", None);
}

#[test]
#[ignore]
fn settings_api_keys_revoke_key() {
    shoot_pane("api-keys-revoke-key", |window, cx| RevokeIdentity::for_shots("CI deploys", Some("expu_ab\u{2026}"), window, cx), |_, _| {});
}

#[test]
#[ignore]
fn settings_api_keys_revoke_device() {
    shoot_pane("api-keys-revoke-device", |window, cx| RevokeIdentity::for_shots("studio-mac", None, window, cx), |_, _| {});
}

/// Everything switched on by a press (from a scoped pick).
#[test]
#[ignore]
fn settings_api_keys_mint_toggled() {
    let build = move |window: &mut Window, cx: &mut gpui::Context<MintKeyContent>| MintKeyContent::for_shots(scope_tree(2), scoped(&[], &[]), "", None, window, cx);
    shoot_pane_acted("api-keys-mint-toggled", build, |pane, cx| pane.press("everything_switch", "change", cx), None, &[], |pane, _, cx| pane.settle(cx));
}
