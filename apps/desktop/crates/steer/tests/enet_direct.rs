//! EXP-1458 (contract addendum 3): agent-to-agent over eNet. Two REAL local
//! eNet nodes started the way the apps start them, each with its key holder
//! (a fake `sessionKeys.*`): node A's holder delivers a sealed message the
//! server accepted to node B's install, B's direct inbox holds it still
//! sealed, and the pickup merge dedups it against the Electric copy.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use api::error::ApiError;
use api::installs::{InstallEntry, RegisterInstall};
use api::session_keys::{CreateKey, HeldKey, PairWrap, PendingWrap, RootKey};
use coding::sealing::{DeliverTo, SealService};
use domain::rows::MessageRow;
use enet_core::NetConfig;
use steer::enet::{self, InstallKind, InstallNode, InstallRegistry, InstallSpec};
use steer::seal_keys::KeyApi;

const WAIT: Duration = Duration::from_secs(20);

/// One team, no signature checks (`enet_installs.rs` covers those).
#[derive(Default)]
struct Registry {
    installs: Mutex<Vec<InstallEntry>>,
}

struct As {
    user_id: String,
    registry: Arc<Registry>,
}

impl InstallRegistry for As {
    fn register(&self, input: &RegisterInstall) -> Result<String, ApiError> {
        let mut installs = self.registry.installs.lock().unwrap();
        if let Some(row) = installs.iter().find(|row| row.endpoint_id == input.endpoint_id) {
            return Ok(row.install_id.clone());
        }
        let install_id = format!("install-{}", installs.len());
        installs.push(InstallEntry {
            install_id: install_id.clone(),
            user_id: self.user_id.clone(),
            kind: input.kind.clone(),
            label: input.label.clone(),
            platform: input.platform.clone(),
            endpoint_id: input.endpoint_id.clone(),
            x25519_public: input.x25519_public.clone(),
            last_seen_at: None,
            device_id: input.device_id.clone(),
        });
        Ok(install_id)
    }
    fn list(&self, _team_id: &str) -> Result<Vec<InstallEntry>, ApiError> {
        Ok(self.registry.installs.lock().unwrap().clone())
    }
    fn revoke(&self, _install_id: &str) -> Result<(), ApiError> {
        Ok(())
    }
    fn keys(self: Arc<Self>) -> Option<Arc<dyn KeyApi>> {
        Some(Arc::new(NoKeys))
    }
}

/// A `sessionKeys.*` with nothing to hand out: only starts the holders.
struct NoKeys;

impl KeyApi for NoKeys {
    fn create(&self, _input: &CreateKey) -> Result<(), ApiError> {
        Ok(())
    }
    fn mine(&self, _install_id: &str, _root_ids: Option<&[String]>) -> Result<Vec<HeldKey>, ApiError> {
        Ok(Vec::new())
    }
    fn pending(&self, _install_id: &str) -> Result<Vec<PendingWrap>, ApiError> {
        Ok(Vec::new())
    }
    fn wrap(&self, _install_id: &str, _wraps: &[PairWrap]) -> Result<(), ApiError> {
        Ok(())
    }
    fn public_keys(&self, _root_ids: &[String]) -> Result<Vec<RootKey>, ApiError> {
        Ok(Vec::new())
    }
}

fn start(registry: &Arc<Registry>, dir: &std::path::Path, user: &str, kind: InstallKind) -> InstallNode {
    let spec = InstallSpec {
        kind,
        user_id: user.into(),
        device_id: Some(format!("device-{user}")),
        label: None,
        identity_path: enet::identity_path(dir, &format!("account-{user}"), kind),
        net: NetConfig::local_only(),
    };
    InstallNode::start(
        &tokio::runtime::Handle::current(),
        spec,
        Arc::new(As { user_id: user.into(), registry: Arc::clone(registry) }),
        Arc::new(|| vec!["team-1".to_string()]),
    )
}

async fn up(install: &InstallNode, passes: u64) {
    tokio::time::timeout(WAIT, install.passes_at_least(passes)).await.expect("allowlist pass in time");
}

fn sealed_row(id: &str) -> (MessageRow, String) {
    let tree = enet_core::sealed::generate_tree_key();
    let body = enet_core::sealed::seal_body(&tree.public, 1, "the child's report").unwrap();
    let card = enet_core::sealed::seal_card(&tree.public, 1, &serde_json::json!({"kind": "report", "text": "done"})).unwrap();
    let row = MessageRow {
        id: id.into(),
        conversation_id: "conv-parent".into(),
        session_id: "parent".into(),
        root_id: Some("root".into()),
        author_kind: Some("agent".into()),
        author_session_id: Some("child".into()),
        body: Some(body.clone()),
        card: Some(card),
        created_at: Some("2026-10-30T12:00:00.000Z".into()),
        ..Default::default()
    };
    (row, body)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sealed_agent_message_reaches_the_target_install_and_dedups_against_electric() {
    let dir = std::env::temp_dir().join(format!("enet-direct-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let registry = Arc::new(Registry::default());
    let sender = start(&registry, &dir, "alice", InstallKind::Desktop);
    let target = start(&registry, &dir, "bob", InstallKind::Daemon);
    up(&sender, 1).await;
    up(&target, 1).await;
    // The second registers after the first's list: re-read both.
    sender.refresh();
    target.refresh();
    up(&sender, 2).await;
    up(&target, 2).await;
    let a = sender.node().expect("sender node");
    let b = target.node().expect("target node");
    // Local-only nodes have no relay: introduce the direct addresses.
    a.add_ticket(&b.ticket());
    b.add_ticket(&a.ticket());
    // The ping keeps working beside the direct messages.
    a.send_chat(b.endpoint_id(), enet::PING_BODY.to_vec()).await.expect("ping acked");

    let (row, sealed_body) = sealed_row(&format!("direct-{}", std::process::id()));
    let deliver_to = DeliverTo {
        install_id: target.install_id().unwrap(),
        endpoint_id: enet::endpoint_hex(&b.endpoint_id()),
    };
    let holder = sender.key_holder().expect("sender key holder");
    tokio::task::spawn_blocking({
        let row = row.clone();
        move || holder.deliver_direct(&deliver_to, &row)
    })
    .await
    .unwrap();

    let arrived = tokio::time::timeout(WAIT, async {
        loop {
            if let Some(found) = coding::direct_inbox::pending().into_iter().find(|m| m.id == row.id) {
                return found;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the direct message reached the target's inbox");
    assert_eq!(arrived.session_id, "parent");
    assert_eq!(arrived.root_id.as_deref(), Some("root"));
    assert_eq!(arrived.author_session_id.as_deref(), Some("child"));
    assert_eq!(arrived.created_at, row.created_at);
    assert_eq!(arrived.body.as_deref(), Some(sealed_body.as_str()), "the body stays sealed");
    assert_eq!(arrived.card, row.card, "the card stays sealed");
    assert!(arrived.body.as_deref().unwrap().starts_with(enet_core::sealed::SEALED_BODY_PREFIX));
    assert!(arrived.consumed_at.is_none());
    assert!(coding::pickup::is_pending_input(&arrived), "the pickup feeds it as the parent's input");

    // The pickup merge: alone it adds the row; once Electric has the same
    // id the inbox copy is gone for good.
    let mut before_sync = Vec::new();
    coding::direct_inbox::merge(&mut before_sync);
    assert_eq!(before_sync.iter().filter(|m| m.id == row.id).count(), 1);
    let electric = MessageRow { consumed_at: Some("2026-10-30T12:00:01.000Z".into()), ..row.clone() };
    let mut synced = vec![electric.clone()];
    coding::direct_inbox::merge(&mut synced);
    assert_eq!(synced, vec![electric]);
    assert!(!coding::direct_inbox::pending().iter().any(|m| m.id == row.id));

    sender.stop();
    target.stop();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_payload_round_trips_with_the_contract_field_names() {
    let (row, _) = sealed_row("m1");
    let bytes = enet::direct_payload(&row).expect("a direct message");
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let mut keys: Vec<&str> = json.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["authorSessionId", "body", "card", "createdAt", "messageId", "rootId", "sessionId", "t", "v"]
    );
    assert_eq!(json["t"], enet::DIRECT_TAG);
    assert_eq!(json["v"], 1);
    let back = enet::parse_direct_payload(&bytes).expect("decodes");
    assert_eq!(back.id, "m1");
    assert_eq!(back.session_id, "parent");
    assert_eq!(back.body, row.body);
    assert_eq!(back.card, row.card);
    assert_eq!(back.author_kind.as_deref(), Some("agent"));

    // No card: the key is omitted.
    let bare = MessageRow { card: None, ..row.clone() };
    let json: serde_json::Value = serde_json::from_slice(&enet::direct_payload(&bare).unwrap()).unwrap();
    assert!(json.get("card").is_none());
}

#[test]
fn only_valid_sealed_agent_messages_decode() {
    let (row, _) = sealed_row("m1");
    assert!(enet::parse_direct_payload(enet::PING_BODY).is_none(), "the ping is no message");
    assert!(enet::parse_direct_payload(b"{not json").is_none());
    // Nothing to send for a person's row or a session's own reply.
    assert!(enet::direct_payload(&MessageRow { author_session_id: None, ..row.clone() }).is_none());
    assert!(enet::direct_payload(&MessageRow { author_session_id: Some("parent".into()), ..row.clone() }).is_none());
    assert!(enet::direct_payload(&MessageRow { session_id: String::new(), ..row.clone() }).is_none());
    assert!(enet::direct_payload(&MessageRow { root_id: None, ..row.clone() }).is_none());
    // Plaintext never rides the direct path.
    assert!(enet::direct_payload(&MessageRow { body: Some("hello".into()), ..row.clone() }).is_none());
    assert!(enet::direct_payload(&MessageRow { card: Some(serde_json::json!({"kind": "report"})), ..row.clone() }).is_none());
    let tampered = serde_json::json!({
        "t": "exp-msg", "v": 1, "messageId": "m1", "sessionId": "parent", "rootId": "root",
        "createdAt": null, "authorSessionId": "child", "body": "plaintext"
    });
    assert!(enet::parse_direct_payload(tampered.to_string().as_bytes()).is_none());
    let other_version = serde_json::json!({
        "t": "exp-msg", "v": 2, "messageId": "m1", "sessionId": "parent", "rootId": "root",
        "createdAt": null, "authorSessionId": "child", "body": row.body
    });
    assert!(enet::parse_direct_payload(other_version.to_string().as_bytes()).is_none());
}
