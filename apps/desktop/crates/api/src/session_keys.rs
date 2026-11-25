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
    /// #EXP-1950: the maker's install's signature over the wrap
    /// (`enet_core::wrap_sig`); omitted = unsigned (old servers strip it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
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
    /// #EXP-1950: who signed it (both None = unsigned; old servers omit both).
    #[serde(default)]
    pub signature: Option<String>,
    #[serde(default)]
    pub signer_endpoint_id: Option<String>,
}

/// A reader still missing a wrap of an epoch this install holds (`pending`):
/// the current one, or (#EXP-2119) a superseded one. `public_key` is empty
/// for a superseded epoch the server knows no key of.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingWrap {
    pub root_id: String,
    pub epoch: u32,
    pub public_key: String,
    pub target_install_id: String,
    pub target_x25519_public: String,
    /// #EXP-1950: the target's ed25519 key, the recipient a wrap signature
    /// names (None from a server older than #3394: the wrap goes unsigned).
    #[serde(default)]
    pub target_endpoint_id: Option<String>,
}

/// One answer to a [`PendingWrap`] (`wrap`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairWrap {
    pub root_id: String,
    pub epoch: u32,
    pub target_install_id: String,
    pub wrapped: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// #EXP-1950 (wrap-signature.json `resign`): a fresh signed wrap of a root's
/// current epoch for `install_id` (the TARGET), replacing its unsigned row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResignWrap {
    pub root_id: String,
    pub epoch: u32,
    pub install_id: String,
    pub wrapped: String,
    pub signature: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResignOutput {
    pub replaced: u32,
    pub skipped: u32,
}

/// `resign`'s ceiling per call (`resign.maxWraps`).
pub const RESIGN_MAX: usize = 200;

/// A keyed root's current public key (`publicKeys`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootKey {
    pub root_id: String,
    pub epoch: u32,
    pub public_key: String,
    /// #EXP-1950: a signed wrap of this epoch made for another install,
    /// which names the key (install-trust.json R3); absent on an older server.
    #[serde(default)]
    pub vouch: Option<serde_json::Value>,
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
    /// #EXP-1941: only these roots (a call answers the wraps of ITS root:
    /// one pass over all roots is capped and sorted by root id).
    #[serde(skip_serializing_if = "Option::is_none")]
    root_ids: Option<&'a [String]>,
    /// #EXP-2119 (`rotation` OLDER EPOCHS): also pairs of a superseded epoch
    /// the server knows no public key of (an older server strips it).
    older_epochs: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WrapInput<'a> {
    install_id: &'a str,
    wraps: &'a [PairWrap],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResignInput<'a> {
    install_id: &'a str,
    wraps: &'a [ResignWrap],
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

/// `sessionKeys.pending`: readers without the wrap of an epoch (current, and
/// #EXP-2119 older ones) on trees this install holds. Automatic: answer every pair with [`wrap`].
pub fn pending(trpc: &TrpcClient, install_id: &str, root_ids: Option<&[String]>) -> Result<Vec<PendingWrap>, ApiError> {
    trpc.query_with_input("sessionKeys.pending", &PendingInput { install_id, limit: PENDING_MAX, root_ids, older_epochs: true })
}

/// `sessionKeys.wrap`: only pairs `pending` lists.
pub fn wrap(trpc: &TrpcClient, install_id: &str, wraps: &[PairWrap]) -> Result<WrapOutput, ApiError> {
    trpc.mutation("sessionKeys.wrap", &WrapInput { install_id, wraps })
}

/// #EXP-1950 `sessionKeys.resign`: replaces readers' UNSIGNED rows of a
/// root's current epoch with these signed wraps (a signed row is never
/// touched). NOT_FOUND (404) = a server without the procedure.
pub fn resign(trpc: &TrpcClient, install_id: &str, wraps: &[ResignWrap]) -> Result<ResignOutput, ApiError> {
    trpc.mutation("sessionKeys.resign", &ResignInput { install_id, wraps })
}

/// `sessionKeys.publicKeys`: the current key of each keyed root the caller
/// may read or send to (keyless and invisible roots are omitted).
pub fn public_keys(trpc: &TrpcClient, root_ids: &[String]) -> Result<Vec<RootKey>, ApiError> {
    if root_ids.is_empty() {
        return Ok(Vec::new());
    }
    trpc.query_with_input("sessionKeys.publicKeys", &RootsInput { root_ids })
}

/// #EXP-1951 `sessionKeys.due`: a root this install rotates now (`epoch` =
/// the one to supersede, `create {epoch + 1}`). #EXP-2123 (install-trust.json
/// `keyMaker`): `stranded` = no approved reader holds its current epoch:
/// a joined reader rotates it WITHOUT the old secret.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DueRoot {
    pub root_id: String,
    pub epoch: u32,
    #[serde(default)]
    pub stranded: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DueInput<'a> {
    install_id: &'a str,
    limit: usize,
}

/// `sessionKeys.due`: flagged roots this install holds the current epoch of,
/// and the stranded ones it reads. NOT_FOUND (404) = an older server.
pub fn due(trpc: &TrpcClient, install_id: &str) -> Result<Vec<DueRoot>, ApiError> {
    trpc.query_with_input("sessionKeys.due", &DueInput { install_id, limit: PENDING_MAX })
}

/// #EXP-1960: `sessionKeys.requestKey`'s answer (`clearTree.requestKeyCases`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum RequestedKey {
    #[serde(rename_all = "camelCase")]
    Keyed { epoch: u32, public_key: String },
    Clear,
    Requested,
    NoHost,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RootInput<'a> {
    root_id: &'a str,
}

/// `sessionKeys.requestKey`: a keyless tree this install may not key asks
/// its devices to key it (a `tree_key` command each). NOT_FOUND (404) =
/// the caller may not see the root.
pub fn request_key(trpc: &TrpcClient, root_id: &str) -> Result<RequestedKey, ApiError> {
    trpc.mutation("sessionKeys.requestKey", &RootInput { root_id })
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
            wraps: vec![
                InstallWrap { install_id: "i".into(), wrapped: "w".into(), signature: None },
                InstallWrap { install_id: "j".into(), wrapped: "v".into(), signature: Some("s".into()) },
            ],
        };
        let json = serde_json::to_value(&input).unwrap();
        // #EXP-1950: an unsigned wrap carries no `signature` key at all.
        assert_eq!(
            json,
            serde_json::json!({"rootId":"r","installId":"i","epoch":1,"publicKey":"k","wraps":[{"installId":"i","wrapped":"w"},{"installId":"j","wrapped":"v","signature":"s"}]})
        );
    }

    #[test]
    fn mine_skips_absent_roots_and_pending_decodes() {
        let json = serde_json::to_value(MineInput { install_id: "i", root_ids: None }).unwrap();
        assert_eq!(json, serde_json::json!({"installId":"i"}));
        // #EXP-1941: `pending` by root; without roots the input is today's.
        let json = serde_json::to_value(PendingInput { install_id: "i", limit: PENDING_MAX, root_ids: None, older_epochs: true }).unwrap();
        assert_eq!(json, serde_json::json!({"installId":"i","limit":200,"olderEpochs":true}));
        let roots = ["r".to_string()];
        let json = serde_json::to_value(PendingInput { install_id: "i", limit: PENDING_MAX, root_ids: Some(&roots), older_epochs: true }).unwrap();
        assert_eq!(json, serde_json::json!({"installId":"i","limit":200,"rootIds":["r"],"olderEpochs":true}));
        let rows: Vec<PendingWrap> = serde_json::from_str(
            r#"[{"rootId":"r","epoch":2,"publicKey":"k","targetInstallId":"t","targetX25519Public":"x"}]"#,
        )
        .unwrap();
        assert_eq!(rows[0].epoch, 2);
        assert_eq!(rows[0].target_install_id, "t");
        // #EXP-1950: an older server names no endpoint; a newer one does.
        assert_eq!(rows[0].target_endpoint_id, None);
        let rows: Vec<PendingWrap> = serde_json::from_str(
            r#"[{"rootId":"r","epoch":2,"publicKey":"k","targetInstallId":"t","targetX25519Public":"x","targetEndpointId":"e"}]"#,
        )
        .unwrap();
        assert_eq!(rows[0].target_endpoint_id.as_deref(), Some("e"));
        let held: Vec<HeldKey> = serde_json::from_str(r#"[{"rootId":"r","epoch":1,"wrapped":"w"},{"rootId":"r","epoch":2,"wrapped":"w","signature":"s","signerEndpointId":"e"}]"#).unwrap();
        assert_eq!((held[0].signature.as_deref(), held[1].signature.as_deref()), (None, Some("s")));
        let pair = PairWrap { root_id: "r".into(), epoch: 1, target_install_id: "t".into(), wrapped: "w".into(), signature: None };
        assert_eq!(serde_json::to_value(&pair).unwrap(), serde_json::json!({"rootId":"r","epoch":1,"targetInstallId":"t","wrapped":"w"}));
        let wraps = [ResignWrap { root_id: "r".into(), epoch: 1, install_id: "t".into(), wrapped: "w".into(), signature: "s".into() }];
        assert_eq!(
            serde_json::to_value(ResignInput { install_id: "i", wraps: &wraps }).unwrap(),
            serde_json::json!({"installId":"i","wraps":[{"rootId":"r","epoch":1,"installId":"t","wrapped":"w","signature":"s"}]})
        );
    }

    #[test]
    fn due_rows_default_to_not_stranded() {
        let rows: Vec<DueRoot> = serde_json::from_str(r#"[{"rootId":"r","epoch":1},{"rootId":"s","epoch":3,"stranded":true}]"#).unwrap();
        assert_eq!(rows[0], DueRoot { root_id: "r".into(), epoch: 1, stranded: false });
        assert_eq!(rows[1], DueRoot { root_id: "s".into(), epoch: 3, stranded: true });
        assert_eq!(serde_json::to_value(DueInput { install_id: "i", limit: PENDING_MAX }).unwrap(), serde_json::json!({"installId":"i","limit":200}));
    }

    #[test]
    fn request_key_answers_parse() {
        let keyed: RequestedKey = serde_json::from_str(r#"{"state":"keyed","epoch":1,"publicKey":"k"}"#).unwrap();
        assert_eq!(keyed, RequestedKey::Keyed { epoch: 1, public_key: "k".into() });
        let requested: RequestedKey = serde_json::from_str(r#"{"state":"requested"}"#).unwrap();
        assert_eq!(requested, RequestedKey::Requested);
        let none: RequestedKey = serde_json::from_str(r#"{"state":"noHost"}"#).unwrap();
        assert_eq!(none, RequestedKey::NoHost);
    }
}
