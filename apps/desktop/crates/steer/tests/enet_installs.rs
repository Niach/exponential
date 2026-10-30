//! EXP-1457 step 1: real eNet nodes (`NetConfig::local_only()`) started the
//! way the desktop app and the daemon start them, against an in-memory
//! install registry that checks every registration signature like the
//! server does (`apps/web/src/lib/trpc/installs.ts`).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use api::error::ApiError;
use api::installs::{InstallEntry, RegisterInstall};
use enet_core::{NetConfig, NetError};
use steer::enet::{self, InstallKind, InstallNode, InstallRegistry, InstallSpec};

const WAIT: Duration = Duration::from_secs(20);

/// The server's rules, minus Postgres: a fresh signature by the endpoint's
/// key over the caller's user id, one account per endpoint, revoked = never.
#[derive(Default)]
struct FakeRegistry {
    /// team id → member user ids.
    teams: BTreeMap<String, Vec<String>>,
    installs: Mutex<Vec<(InstallEntry, bool)>>,
}

/// One caller's view of the registry (tRPC = authed as `user_id`).
struct As {
    user_id: String,
    registry: Arc<FakeRegistry>,
}

fn verify(input: &RegisterInstall, user_id: &str) -> bool {
    let Ok(public) = data_encoding::HEXLOWER.decode(input.endpoint_id.as_bytes()) else {
        return false;
    };
    let Ok(signature) = data_encoding::BASE64URL_NOPAD.decode(input.signature.as_bytes()) else {
        return false;
    };
    let message = format!(
        "exp-install-v1\n{user_id}\n{}\n{}\n{}",
        input.endpoint_id, input.x25519_public, input.signed_at
    );
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public)
        .verify(message.as_bytes(), &signature)
        .is_ok()
}

impl InstallRegistry for As {
    fn register(&self, input: &RegisterInstall) -> Result<String, ApiError> {
        if !verify(input, &self.user_id) {
            return Err(ApiError::Http { status: 400, message: "Invalid install signature".into() });
        }
        let mut installs = self.registry.installs.lock().unwrap();
        if let Some((row, revoked)) = installs.iter_mut().find(|(row, _)| row.endpoint_id == input.endpoint_id) {
            if row.user_id != self.user_id {
                return Err(ApiError::Http { status: 409, message: "Endpoint belongs to another account".into() });
            }
            if *revoked {
                return Err(ApiError::Http { status: 403, message: "This install was revoked".into() });
            }
            return Ok(row.install_id.clone());
        }
        let install_id = format!("install-{}", installs.len());
        installs.push((
            InstallEntry {
                install_id: install_id.clone(),
                user_id: self.user_id.clone(),
                kind: input.kind.clone(),
                label: input.label.clone(),
                platform: input.platform.clone(),
                endpoint_id: input.endpoint_id.clone(),
                x25519_public: input.x25519_public.clone(),
                last_seen_at: None,
                device_id: input.device_id.clone(),
            },
            false,
        ));
        Ok(install_id)
    }

    fn list(&self, team_id: &str) -> Result<Vec<InstallEntry>, ApiError> {
        let members = self.registry.teams.get(team_id).cloned().unwrap_or_default();
        if !members.contains(&self.user_id) {
            return Err(ApiError::Http { status: 403, message: "not a member".into() });
        }
        Ok(self
            .registry
            .installs
            .lock()
            .unwrap()
            .iter()
            .filter(|(row, revoked)| !revoked && members.contains(&row.user_id))
            .map(|(row, _)| row.clone())
            .collect())
    }

    fn revoke(&self, install_id: &str) -> Result<(), ApiError> {
        let mut installs = self.registry.installs.lock().unwrap();
        let row = installs
            .iter_mut()
            .find(|(row, _)| row.install_id == install_id && row.user_id == self.user_id)
            .ok_or(ApiError::Http { status: 404, message: "Install not found".into() })?;
        row.1 = true;
        Ok(())
    }
}

fn start(
    registry: &Arc<FakeRegistry>,
    dir: &std::path::Path,
    user: &str,
    team: &str,
    kind: InstallKind,
) -> InstallNode {
    let spec = InstallSpec {
        kind,
        user_id: user.into(),
        device_id: Some(format!("device-{user}")),
        label: Some(format!("{user}'s box")),
        identity_path: enet::identity_path(dir, &format!("account-{user}"), kind),
        net: NetConfig::local_only(),
    };
    let team = team.to_string();
    InstallNode::start(
        &tokio::runtime::Handle::current(),
        spec,
        Arc::new(As { user_id: user.into(), registry: Arc::clone(registry) }),
        Arc::new(move || vec![team.clone()]),
    )
}

async fn up(install: &InstallNode, passes: u64) {
    tokio::time::timeout(WAIT, install.passes_at_least(passes)).await.expect("allowlist pass in time");
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("enet-installs-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[tokio::test(flavor = "multi_thread")]
async fn team_installs_exchange_an_acked_ping_and_a_foreign_endpoint_is_refused() {
    let dir = temp_dir("team");
    let registry = Arc::new(FakeRegistry {
        teams: BTreeMap::from([
            ("team-1".to_string(), vec!["alice".to_string(), "bob".to_string()]),
            ("team-2".to_string(), vec!["mallory".to_string()]),
        ]),
        ..Default::default()
    });
    let desktop = start(&registry, &dir, "alice", "team-1", InstallKind::Desktop);
    let daemon = start(&registry, &dir, "bob", "team-1", InstallKind::Daemon);
    let foreign = start(&registry, &dir, "mallory", "team-2", InstallKind::Daemon);
    up(&desktop, 1).await;
    up(&daemon, 1).await;
    up(&foreign, 1).await;
    // Both registered (signatures verified); a team change re-reads the list.
    assert!(desktop.install_id().is_some() && daemon.install_id().is_some());
    desktop.refresh();
    daemon.refresh();
    up(&desktop, 2).await;
    up(&daemon, 2).await;

    let a = desktop.node().expect("desktop node");
    let b = daemon.node().expect("daemon node");
    let m = foreign.node().expect("foreign node");
    assert_eq!(a.access().allowed_peers(), vec![b.endpoint_id()]);
    assert_eq!(b.access().allowed_peers(), vec![a.endpoint_id()]);
    assert!(m.access().allowed_peers().is_empty(), "nothing crosses teams");

    // Local-only nodes have no relay: introduce the direct addresses.
    a.add_ticket(&b.ticket());
    b.add_ticket(&a.ticket());
    m.add_ticket(&a.ticket());

    let ack = a.send_chat(b.endpoint_id(), enet::PING_BODY.to_vec()).await.expect("desktop → daemon acked");
    assert_eq!(ack.body, enet::PING_BODY);
    b.send_chat(a.endpoint_id(), enet::PING_BODY.to_vec()).await.expect("daemon → desktop acked");

    let refused = m.send_chat(a.endpoint_id(), enet::PING_BODY.to_vec()).await;
    assert!(matches!(refused, Err(NetError::Refused(_))), "foreign endpoint refused, got {refused:?}");

    // Sign-out stops the node.
    desktop.stop();
    tokio::time::timeout(WAIT, async {
        while desktop.node().is_some() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("node stopped");
    daemon.stop();
    foreign.stop();
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn identity_survives_restarts_and_a_revoke_mints_a_new_one() {
    let dir = temp_dir("revoke");
    let registry = Arc::new(FakeRegistry {
        teams: BTreeMap::from([("team-1".to_string(), vec!["alice".to_string()])]),
        ..Default::default()
    });
    let first = start(&registry, &dir, "alice", "team-1", InstallKind::Desktop);
    up(&first, 1).await;
    let endpoint = first.node().unwrap().endpoint_id();
    let install_id = first.install_id().unwrap();
    first.stop();

    // A restart reuses the identity and the registry row.
    let again = start(&registry, &dir, "alice", "team-1", InstallKind::Desktop);
    up(&again, 1).await;
    assert_eq!(again.node().unwrap().endpoint_id(), endpoint);
    assert_eq!(again.install_id().unwrap(), install_id);

    // Explicit sign-out: revoked + forgotten; the next start is a new install.
    let revoker = again.clone();
    let as_alice = As { user_id: "alice".into(), registry: Arc::clone(&registry) };
    tokio::task::spawn_blocking(move || revoker.revoke_blocking(&as_alice)).await.unwrap();
    let path = enet::identity_path(&dir, "account-alice", InstallKind::Desktop);
    assert!(!path.exists());
    let fresh = start(&registry, &dir, "alice", "team-1", InstallKind::Desktop);
    up(&fresh, 1).await;
    assert_ne!(fresh.node().unwrap().endpoint_id(), endpoint);
    assert_ne!(fresh.install_id().unwrap(), install_id);

    // Revoked from elsewhere (the web): the registry answers FORBIDDEN and
    // the install mints a new identity on its own.
    let revoked_endpoint = fresh.node().map(|n| n.endpoint_id());
    fresh.stop();
    As { user_id: "alice".into(), registry: Arc::clone(&registry) }
        .revoke(&fresh.install_id().unwrap())
        .unwrap();
    let healed = start(&registry, &dir, "alice", "team-1", InstallKind::Desktop);
    up(&healed, 1).await;
    assert!(healed.install_id().is_some());
    assert_ne!(Some(healed.node().unwrap().endpoint_id()), revoked_endpoint);
    healed.stop();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn identity_file_is_0600_and_reloads() {
    let dir = temp_dir("file");
    let path = enet::identity_path(&dir, "acc", InstallKind::Daemon);
    assert!(path.ends_with("accounts/acc/cli-enet-identity"));
    let minted = enet::load_or_create_identity(&path).unwrap();
    let loaded = enet::load_or_create_identity(&path).unwrap();
    assert_eq!(minted.to_bytes(), loaded.to_bytes());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }
    // A malformed file is replaced, not trusted.
    std::fs::write(&path, "not hex\n").unwrap();
    let replaced = enet::load_or_create_identity(&path).unwrap();
    assert_ne!(replaced.to_bytes(), minted.to_bytes());
    assert_eq!(enet::load_or_create_identity(&path).unwrap().to_bytes(), replaced.to_bytes());
    enet::forget_identity(&path);
    assert!(!path.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn registration_signature_verifies_and_is_bound_to_the_user() {
    let identity = enet_core::Identity::from_bytes([7; 32]);
    let input = enet::Registration {
        kind: InstallKind::Desktop,
        user_id: "user-1",
        device_id: Some("device-1"),
        label: Some("Mac"),
        platform: Some("macos"),
    }
    .sign(&identity, "2026-10-30T12:00:00.000Z");
    assert_eq!(input.endpoint_id.len(), 64);
    assert_eq!(input.x25519_public.len(), 43);
    assert_eq!(input.signature.len(), 86);
    assert!(verify(&input, "user-1"));
    assert!(!verify(&input, "user-2"), "a proof never registers under another account");
}

/// The shared vector (`packages/domain-contract/fixtures/install-register.json`)
/// the server and every other platform replay: this host's registration
/// produces exactly its bytes.
#[test]
fn registration_replays_the_install_register_fixture() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/domain-contract/fixtures/install-register.json");
    let fixture: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let field = |key: &str| fixture[key].as_str().unwrap().to_string();
    let secret: [u8; 32] = data_encoding::HEXLOWER.decode(field("identity").as_bytes()).unwrap().try_into().unwrap();
    let identity = enet_core::Identity::from_bytes(secret);
    let user_id = field("userId");
    let input = enet::Registration {
        kind: InstallKind::Daemon,
        user_id: &user_id,
        device_id: None,
        label: None,
        platform: None,
    }
    .sign(&identity, &field("signedAt"));
    assert_eq!(input.endpoint_id, field("endpointId"));
    assert_eq!(input.x25519_public, field("x25519Public"));
    assert_eq!(
        enet::install_message(&user_id, &input.endpoint_id, &input.x25519_public, &input.signed_at),
        field("message")
    );
    assert_eq!(input.signature, field("signature"));
    assert_eq!(
        data_encoding::HEXLOWER.encode(&data_encoding::BASE64URL_NOPAD.decode(input.signature.as_bytes()).unwrap()),
        field("signatureHex")
    );
    assert!(verify(&input, &user_id));
}

/// An install that registers AFTER a teammate's last list pass is refused
/// once, and the refusal makes the teammate re-read the list: the retry gets
/// in without waiting out the ten-minute refresh.
#[tokio::test(flavor = "multi_thread")]
async fn a_late_teammate_gets_in_after_one_refusal() {
    let dir = temp_dir("late");
    let registry = Arc::new(FakeRegistry {
        teams: BTreeMap::from([("team-1".to_string(), vec!["alice".to_string(), "bob".to_string()])]),
        ..Default::default()
    });
    let early = start(&registry, &dir, "alice", "team-1", InstallKind::Daemon);
    up(&early, 1).await;
    let late = start(&registry, &dir, "bob", "team-1", InstallKind::Desktop);
    up(&late, 1).await;
    let a = early.node().unwrap();
    let b = late.node().unwrap();
    a.add_ticket(&b.ticket());
    b.add_ticket(&a.ticket());
    assert!(a.access().allowed_peers().is_empty(), "alice listed before bob registered");

    let first = b.send_chat(a.endpoint_id(), enet::PING_BODY.to_vec()).await;
    assert!(matches!(first, Err(NetError::Refused(_))), "got {first:?}");
    up(&early, 2).await;
    b.send_chat(a.endpoint_id(), enet::PING_BODY.to_vec()).await.expect("let in after the refresh");
    early.stop();
    late.stop();
    let _ = std::fs::remove_dir_all(&dir);
}
