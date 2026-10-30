//! Typed `sessionKeys.*` tRPC mirrors (EXP-1458; shapes verified against
//! `apps/web/src/lib/trpc/session-keys.ts`, the spec is
//! `packages/domain-contract/fixtures/sealed-content.json`).
//!
//! Each session tree has one X25519 keypair per epoch; the public key sits
//! on the root row, the secret reaches every reader install wrapped. Every
//! call names an `installId` the caller owns. Server-only table: no shape.
//! The key holder (cache, the `pending` loop, key creation) is
//! `steer::seal_keys`.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// `pending`'s ceiling per call (`SESSION_KEYS_PENDING_MAX`).
pub const PENDING_MAX: usize = 200;

/// One wrap of a tree secret for a reader install.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallWrap {
    pub install_id: String,
    pub wrapped: String,
}

/// `sessionKeys.create` input: a tree's first key (epoch 1) or a rotation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateKey {
    pub root_id: String,
    pub install_id: String,
    pub epoch: u32,
    pub public_key: String,
    pub wraps: Vec<InstallWrap>,
}

/// One wrap this install holds (`mine`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeldKey {
    pub root_id: String,
    pub epoch: u32,
    pub wrapped: String,
}

/// A reader still missing the current epoch's wrap (`pending`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingWrap {
    pub root_id: String,
    pub epoch: u32,
    pub public_key: String,
    pub target_install_id: String,
    pub target_x25519_public: String,
}

/// One answer to a [`PendingWrap`] (`wrap`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairWrap {
    pub root_id: String,
    pub epoch: u32,
    pub target_install_id: String,
    pub wrapped: String,
}

/// A keyed root's current public key (`publicKeys`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootKey {
    pub root_id: String,
    pub epoch: u32,
    pub public_key: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WrapOutput {
    pub written: u32,
    pub skipped: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MineInput<'a> {
    install_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    root_ids: Option<&'a [String]>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PendingInput<'a> {
    install_id: &'a str,
    limit: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WrapInput<'a> {
    install_id: &'a str,
    wraps: &'a [PairWrap],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RootsInput<'a> {
    root_ids: &'a [String],
}

/// `sessionKeys.create`. `CONFLICT` (409) = another writer keyed the tree
/// first (read `publicKeys` and use that key).
pub fn create(trpc: &TrpcClient, input: &CreateKey) -> Result<(), ApiError> {
    let _: serde_json::Value = trpc.mutation("sessionKeys.create", input)?;
    Ok(())
}

/// `sessionKeys.mine`: every wrap this install holds (all epochs), or only
/// those of `root_ids`.
pub fn mine(trpc: &TrpcClient, install_id: &str, root_ids: Option<&[String]>) -> Result<Vec<HeldKey>, ApiError> {
    if root_ids.is_some_and(<[String]>::is_empty) {
        return Ok(Vec::new());
    }
    trpc.query_with_input("sessionKeys.mine", &MineInput { install_id, root_ids })
}

/// `sessionKeys.pending`: readers without the current epoch's wrap on trees
/// this install holds. Automatic: answer every pair with [`wrap`].
pub fn pending(trpc: &TrpcClient, install_id: &str) -> Result<Vec<PendingWrap>, ApiError> {
    trpc.query_with_input("sessionKeys.pending", &PendingInput { install_id, limit: PENDING_MAX })
}

/// `sessionKeys.wrap`: only pairs `pending` lists.
pub fn wrap(trpc: &TrpcClient, install_id: &str, wraps: &[PairWrap]) -> Result<WrapOutput, ApiError> {
    trpc.mutation("sessionKeys.wrap", &WrapInput { install_id, wraps })
}

/// `sessionKeys.publicKeys`: the current key of each keyed root the caller
/// may read or send to (keyless and invisible roots are omitted).
pub fn public_keys(trpc: &TrpcClient, root_ids: &[String]) -> Result<Vec<RootKey>, ApiError> {
    if root_ids.is_empty() {
        return Ok(Vec::new());
    }
    trpc.query_with_input("sessionKeys.publicKeys", &RootsInput { root_ids })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_input_is_camel_case() {
        let input = CreateKey {
            root_id: "r".into(),
            install_id: "i".into(),
            epoch: 1,
            public_key: "k".into(),
            wraps: vec![InstallWrap { install_id: "i".into(), wrapped: "w".into() }],
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"rootId":"r","installId":"i","epoch":1,"publicKey":"k","wraps":[{"installId":"i","wrapped":"w"}]})
        );
    }

    #[test]
    fn mine_skips_absent_roots_and_pending_decodes() {
        let json = serde_json::to_value(MineInput { install_id: "i", root_ids: None }).unwrap();
        assert_eq!(json, serde_json::json!({"installId":"i"}));
        let rows: Vec<PendingWrap> = serde_json::from_str(
            r#"[{"rootId":"r","epoch":2,"publicKey":"k","targetInstallId":"t","targetX25519Public":"x"}]"#,
        )
        .unwrap();
        assert_eq!(rows[0].epoch, 2);
        assert_eq!(rows[0].target_install_id, "t");
    }
}
