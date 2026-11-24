//! #EXP-2118, on its own (the expectation is process-global): an account
//! switch stops one node and starts the next. The old node's task ends LATER
//! and never clears what the next one expects, or its sessions' sealed
//! writes would fail instead of waiting for the key holder.

use std::sync::Arc;
use std::time::Duration;

use api::error::ApiError;
use api::installs::{InstallEntry, RegisterInstall};
use enet_core::NetConfig;
use steer::enet::{identity_path, InstallKind, InstallNode, InstallRegistry, InstallSpec};

/// Never registers (the devices row is missing): a holder stays expected.
struct Refusing;

impl InstallRegistry for Refusing {
    fn register(&self, _input: &RegisterInstall) -> Result<String, ApiError> {
        Err(ApiError::Http { status: 400, message: "Unknown device".into() })
    }

    fn list(&self, _team_id: &str) -> Result<Vec<InstallEntry>, ApiError> {
        Ok(Vec::new())
    }

    fn revoke(&self, _install_id: &str) -> Result<(), ApiError> {
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stopped_nodes_end_never_clears_the_next_nodes_expected_holder() {
    let dir = std::env::temp_dir().join(format!("enet-expected-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let start = |account: &str| {
        let spec = InstallSpec {
            kind: InstallKind::Desktop,
            user_id: account.into(),
            device_id: Some("device".into()),
            label: None,
            identity_path: identity_path(&dir, account, InstallKind::Desktop),
            net: NetConfig::local_only(),
        };
        InstallNode::start(&tokio::runtime::Handle::current(), spec, Arc::new(Refusing), Arc::new(Vec::new))
    };
    let old = start("a");
    tokio::time::timeout(Duration::from_secs(20), old.passes_at_least(1)).await.expect("the old node is up");
    old.stop();
    assert!(!coding::sealing::service_expected());
    let next = start("b");
    tokio::time::timeout(Duration::from_secs(20), async {
        while old.node().is_some() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        next.passes_at_least(1).await;
    })
    .await
    .expect("the old node ends, the next one is up");
    // The old task's last step follows its node's shutdown.
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(coding::sealing::service_expected(), "the next node still brings a holder");
    next.stop();
    let _ = std::fs::remove_dir_all(&dir);
}
