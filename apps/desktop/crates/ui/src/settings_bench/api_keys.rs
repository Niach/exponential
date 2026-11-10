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
