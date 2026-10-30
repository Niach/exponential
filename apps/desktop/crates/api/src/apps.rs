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
    /// VAPP-120 (update only): the exact `exponential.*` function names the
    /// app may run as the viewer without asking (`grantedTools`; the creator
    /// or an owner). `None` leaves the grants as they are.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub granted_tools: Option<Vec<String>>,
    /// VAPP-156: the head version the owner saw (`headVersion`), sent with
    /// every grant; a newer head refuses with CONFLICT.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_version: Option<i64>,
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

/// VAPP-121 `apps.validateImport` / `apps.import` input: the file's text.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportInput<'a> {
    team_id: &'a str,
    text: &'a str,
}

/// One server issue of an import: a JSON pointer and its message.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct ImportIssue {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub message: String,
}

/// `apps.validateImport` output: the issues (empty = importable), the app
/// name and the record count the file carries.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct ValidateImportOutput {
    #[serde(default)]
    pub issues: Vec<ImportIssue>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub records: i64,
}

/// `apps.import` output: the new app and how many records it got.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOutput {
    #[serde(default, alias = "txid")]
    pub tx_id: Option<i64>,
    pub app: App,
    #[serde(default)]
    pub records: i64,
}

/// VAPP-121 `apps.validateImport`: the server's check of an app file (no
/// write; the one authority ×4).
pub fn validate_import(trpc: &TrpcClient, team_id: &str, text: &str) -> Result<ValidateImportOutput, ApiError> {
    trpc.mutation("apps.validateImport", &ImportInput { team_id, text })
}

/// VAPP-121 `apps.import`: a new app out of an app file or a bare package.
pub fn import(trpc: &TrpcClient, team_id: &str, text: &str) -> Result<ImportOutput, ApiError> {
    trpc.mutation("apps.import", &ImportInput { team_id, text })
}

/// `apps.update`: rename / re-icon / re-describe / re-grant (VAPP-120).
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
        let grant = AppInput {
            id: Some("a".into()),
            name: "Dash".into(),
            icon: "puzzle".into(),
            color: "#6366f1".into(),
            granted_tools: Some(vec!["exponential.issues.update".into()]),
            head_version: Some(3),
            ..AppInput::default()
        };
        assert_eq!(
            serde_json::to_value(&grant).unwrap(),
            serde_json::json!({"id": "a", "name": "Dash", "icon": "puzzle", "color": "#6366f1", "grantedTools": ["exponential.issues.update"], "headVersion": 3})
        );
        assert_eq!(
            serde_json::to_value(ImportInput { team_id: "t", text: "{}" }).unwrap(),
            serde_json::json!({"teamId": "t", "text": "{}"})
        );
        let checked: ValidateImportOutput = serde_json::from_value(
            serde_json::json!({"issues": [{"path": "/package", "message": "is required"}], "name": null, "records": 0}),
        )
        .unwrap();
        assert_eq!(checked.issues, vec![ImportIssue { path: "/package".into(), message: "is required".into() }]);
        let imported: ImportOutput = serde_json::from_value(
            serde_json::json!({"txid": 3, "app": {"id": "a", "slug": "todos", "name": "Todos", "headVersion": 1}, "records": 2}),
        )
        .unwrap();
        assert_eq!((imported.tx_id, imported.app.slug.as_str(), imported.records), (Some(3), "todos", 2));
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
