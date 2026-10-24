//! VAPP-110: the typed `apps.*` client (web `apps/web/src/lib/trpc/apps.ts`).
//!
//! An app is a team's declarative Exponential UI package. The list and the
//! head package ride the synced `apps` shape, the versions `app_versions`
//! (package-less); [`get`] with a `version` is the only path to an OLD
//! version's package. Create, rename and delete are the creator's or an
//! owner's; restore is any member's (a NEW head copying the version).
//!
//! Plain blocking calls: run them on a background executor only.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// The app row a write returns (the fields a client acts on).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct App {
    pub id: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppOutput {
    #[serde(default, alias = "txid")]
    pub tx_id: Option<i64>,
    pub app: App,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxOutput {
    #[serde(default, alias = "txid")]
    pub tx_id: Option<i64>,
}

/// `apps.restore` output: the new head `version`, a copy of `restored`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreOutput {
    #[serde(default, alias = "txid")]
    pub tx_id: Option<i64>,
    pub version: i64,
    pub restored: i64,
}

/// `apps.get` output: the version read and its package (`None` = no
/// version yet).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetOutput {
    #[serde(default)]
    pub version: i64,
    #[serde(default)]
    pub package: Option<Value>,
}

/// `apps.create` / `apps.update` input (identity + description).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub icon: String,
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Serialize)]
struct IdInput<'a> {
    id: &'a str,
}

#[derive(Serialize)]
struct VersionInput<'a> {
    id: &'a str,
    version: i64,
}

#[derive(Serialize)]
struct RestoreInput<'a> {
    id: &'a str,
    number: i64,
}

/// `apps.create`: the slug derives server-side (`slugify`, de-duplicated).
pub fn create(trpc: &TrpcClient, input: &AppInput) -> Result<AppOutput, ApiError> {
    trpc.mutation("apps.create", input)
}

/// `apps.update`: rename / re-icon / re-describe.
pub fn update(trpc: &TrpcClient, input: &AppInput) -> Result<AppOutput, ApiError> {
    trpc.mutation("apps.update", input)
}

/// `apps.delete`: the trash (`deleted_at`).
pub fn delete(trpc: &TrpcClient, id: &str) -> Result<TxOutput, ApiError> {
    trpc.mutation("apps.delete", &IdInput { id })
}

/// `apps.restore`: a NEW head copying version `number`.
pub fn restore(trpc: &TrpcClient, id: &str, number: i64) -> Result<RestoreOutput, ApiError> {
    trpc.mutation("apps.restore", &RestoreInput { id, number })
}

/// `apps.get {version}`: one version's package.
pub fn get_version(trpc: &TrpcClient, id: &str, version: i64) -> Result<GetOutput, ApiError> {
    trpc.query_with_input("apps.get", &VersionInput { id, version })
}

/// VAPP-122 `apps.records.upsert` input: `id` = the APP, `record_id` = the
/// record to patch (`None` = create).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordUpsertInput {
    pub id: String,
    pub kind: String,
    pub data: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_id: Option<String>,
}

/// `apps.records.upsert` output: the record's id, `created` = a new row.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordUpsertOutput {
    #[serde(default, alias = "txid")]
    pub tx_id: Option<i64>,
    pub id: String,
    #[serde(default)]
    pub created: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordIdInput<'a> {
    id: &'a str,
    record_id: &'a str,
}

/// VAPP-122 `apps.records.upsert`: create a record, or shallow-patch the
/// team's row `record_id` names (the server validates the kind's schema,
/// the size cap and the plan quota).
pub fn records_upsert(trpc: &TrpcClient, input: &RecordUpsertInput) -> Result<RecordUpsertOutput, ApiError> {
    trpc.mutation("apps.records.upsert", input)
}

/// VAPP-122 `apps.records.delete`: the record's trash.
pub fn records_delete(trpc: &TrpcClient, app_id: &str, record_id: &str) -> Result<TxOutput, ApiError> {
    trpc.mutation("apps.records.delete", &RecordIdInput { id: app_id, record_id })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_serialize_like_the_web() {
        let create = AppInput {
            team_id: Some("t".into()),
            name: "Dash".into(),
            icon: "puzzle".into(),
            color: "#6366f1".into(),
            ..AppInput::default()
        };
        assert_eq!(
            serde_json::to_value(&create).unwrap(),
            serde_json::json!({"teamId": "t", "name": "Dash", "icon": "puzzle", "color": "#6366f1"})
        );
        let restore: RestoreOutput = serde_json::from_value(serde_json::json!({"txid": 9, "version": 4, "restored": 1})).unwrap();
        assert_eq!((restore.tx_id, restore.version, restore.restored), (Some(9), 4, 1));
        let upsert = RecordUpsertInput {
            id: "app".into(),
            kind: "entry".into(),
            data: serde_json::json!({"hours": 3}),
            record_id: None,
        };
        assert_eq!(
            serde_json::to_value(&upsert).unwrap(),
            serde_json::json!({"id": "app", "kind": "entry", "data": {"hours": 3}})
        );
        assert_eq!(
            serde_json::to_value(RecordIdInput { id: "app", record_id: "r1" }).unwrap(),
            serde_json::json!({"id": "app", "recordId": "r1"})
        );
        let out: RecordUpsertOutput = serde_json::from_value(serde_json::json!({"txid": 3, "id": "r1", "created": true})).unwrap();
        assert_eq!((out.tx_id, out.id.as_str(), out.created), (Some(3), "r1", true));
    }
}
