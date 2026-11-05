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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DraftInput<'a> {
    team_id: &'a str,
}

/// VAPP-176: `apps.create {teamId}` with NO other field = a DRAFT named
/// "Untitled app" (no icon, no colour, no version); New app ×4.
pub fn create_draft(trpc: &TrpcClient, team_id: &str) -> Result<AppOutput, ApiError> {
    trpc.mutation("apps.create", &DraftInput { team_id })
}

/// `apps.discardDraft` output: whether the draft went.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscardOutput {
    #[serde(default, alias = "txid")]
    pub tx_id: Option<i64>,
    #[serde(default)]
    pub discarded: bool,
}

/// VAPP-176: `apps.discardDraft {id}`: hard-deletes the caller's still-EMPTY
/// draft (no version, no editor session); anything else = a no-op.
pub fn discard_draft(trpc: &TrpcClient, id: &str) -> Result<DiscardOutput, ApiError> {
    trpc.mutation("apps.discardDraft", &IdInput { id })
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

/// #VAPP-254: one upstream a checked file declares, with the team server its
/// exact name resolves to.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportUpstream {
    pub name: String,
    #[serde(default)]
    pub server_id: Option<String>,
    #[serde(default)]
    pub server_name: Option<String>,
}

/// #VAPP-254: an owner's pick on the import step (`apps.import`
/// `upstreamMappings`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamMapping {
    pub upstream: String,
    pub server_id: String,
}

/// `apps.import` input: [`ImportInput`] plus the picks (omitted when none).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportAppInput<'a> {
    team_id: &'a str,
    text: &'a str,
    #[serde(skip_serializing_if = "<[UpstreamMapping]>::is_empty")]
    upstream_mappings: &'a [UpstreamMapping],
}

/// `apps.validateImport` output: the issues (empty = importable), the app
/// name and the record count the file carries; #VAPP-254: the upstreams it
/// declares in name order and what an owner may pick (empty for a member).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct ValidateImportOutput {
    #[serde(default)]
    pub issues: Vec<ImportIssue>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub records: i64,
    #[serde(default)]
    pub upstreams: Vec<ImportUpstream>,
    #[serde(default)]
    pub servers: Vec<crate::app_access::AppServer>,
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

/// VAPP-121 `apps.import`: a new app out of an app file or a bare package;
/// #VAPP-254: `upstream_mappings` = an owner's picks.
pub fn import(trpc: &TrpcClient, team_id: &str, text: &str, upstream_mappings: &[UpstreamMapping]) -> Result<ImportOutput, ApiError> {
    trpc.mutation("apps.import", &ImportAppInput { team_id, text, upstream_mappings })
}

/// VAPP-234 `apps.importTemplate` input.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportTemplateInput<'a> {
    team_id: &'a str,
    template: &'a str,
    include_records: bool,
}

/// VAPP-234 `apps.importTemplate`: a new app out of a signed gallery
/// template (fixtures/app-templates.json), its sample records when asked.
pub fn import_template(trpc: &TrpcClient, team_id: &str, template: &str, include_records: bool) -> Result<ImportOutput, ApiError> {
    trpc.mutation("apps.importTemplate", &ImportTemplateInput { team_id, template, include_records })
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

/// eApps V1 (#VAPP-210) `apps.publish` output.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishOutput {
    #[serde(default, alias = "txid")]
    pub tx_id: Option<i64>,
    pub published: i64,
    #[serde(default)]
    pub head: i64,
}

/// eApps V1: `apps.publish {id, number}` = members and the app's tools run
/// version `number` from now on (an older one = a rollback).
pub fn publish(trpc: &TrpcClient, id: &str, number: i64) -> Result<PublishOutput, ApiError> {
    trpc.mutation("apps.publish", &RestoreInput { id, number })
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
    /// VAPP-188: write ON BEHALF of this member (owners only; `None` = the
    /// writer, omitted on the wire).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// VAPP-215: where the write came from (`editor` = the Data tab, `app`
    /// = an app's own UI; `None` = omitted, the server says `app`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    /// VAPP-235: a create's CLIENT id (the outbox's; `None` = the server
    /// picks; a taken id = CONFLICT).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_id: Option<String>,
    /// VAPP-235: the outbox entry's idempotency key (a replay answers the
    /// first write's result).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub write_id: Option<String>,
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
    /// VAPP-215: the write's `app_record_events` row (the Undo toast's).
    #[serde(default)]
    pub event_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordIdInput<'a> {
    id: &'a str,
    record_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    via: Option<&'a str>,
    /// VAPP-235: the outbox entry's idempotency key (delete only).
    #[serde(skip_serializing_if = "Option::is_none")]
    write_id: Option<&'a str>,
}

/// `apps.records.delete` output: the write's event (VAPP-215).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordDeleteOutput {
    #[serde(default, alias = "txid")]
    pub tx_id: Option<i64>,
    #[serde(default)]
    pub event_id: Option<String>,
}

/// VAPP-215 `apps.records.restore` / `undo` output: the new writes' events.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordEventsOutput {
    #[serde(default, alias = "txid")]
    pub tx_id: Option<i64>,
    #[serde(default)]
    pub event_ids: Vec<String>,
}

/// VAPP-215 `apps.records.history` output: the record's events, newest
/// first, raw (`domain::app_records::RecordEvent::from_value` reads one).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct RecordHistoryOutput {
    #[serde(default)]
    pub events: Vec<Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordRestoreInput<'a> {
    id: &'a str,
    record_id: &'a str,
    event_id: &'a str,
    via: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordUndoInput<'a> {
    id: &'a str,
    event_ids: &'a [String],
    via: &'a str,
}

/// VAPP-215 `apps.records.history {recordId}`: the record's events (its
/// read policy; read on demand, no synced shape).
pub fn records_history(trpc: &TrpcClient, app_id: &str, record_id: &str) -> Result<RecordHistoryOutput, ApiError> {
    trpc.query_with_input("apps.records.history", &RecordIdInput { id: app_id, record_id, via: None, write_id: None })
}

/// VAPP-215 `apps.records.restore {recordId, eventId}`: the record as that
/// event left it, as a new write.
pub fn records_restore(
    trpc: &TrpcClient,
    app_id: &str,
    record_id: &str,
    event_id: &str,
    via: &str,
) -> Result<RecordEventsOutput, ApiError> {
    trpc.mutation("apps.records.restore", &RecordRestoreInput { id: app_id, record_id, event_id, via })
}

/// VAPP-215 `apps.records.undo {eventIds}`: each event's `before` as a new
/// write, newest first, in one transaction.
pub fn records_undo(trpc: &TrpcClient, app_id: &str, event_ids: &[String], via: &str) -> Result<RecordEventsOutput, ApiError> {
    trpc.mutation("apps.records.undo", &RecordUndoInput { id: app_id, event_ids, via })
}

/// VAPP-122 `apps.records.upsert`: create a record, or shallow-patch the
/// team's row `record_id` names (the server validates the kind's schema,
/// the size cap and the plan quota).
pub fn records_upsert(trpc: &TrpcClient, input: &RecordUpsertInput) -> Result<RecordUpsertOutput, ApiError> {
    trpc.mutation("apps.records.upsert", input)
}

/// VAPP-122 `apps.records.delete`: the record's trash. VAPP-235:
/// `write_id` = the outbox entry's idempotency key.
pub fn records_delete(
    trpc: &TrpcClient,
    app_id: &str,
    record_id: &str,
    via: Option<&str>,
    write_id: Option<&str>,
) -> Result<RecordDeleteOutput, ApiError> {
    trpc.mutation("apps.records.delete", &RecordIdInput { id: app_id, record_id, via, write_id })
}

/// VAPP-194 `apps.records.batch` input: a package action's writes, passed
/// through unchanged (`{op: "upsert", kind, data, id?|newId?}` | `{op:
/// "delete", id}`; VAPP-235: `newId` = a create's client id).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordsBatchInput<'a> {
    pub id: &'a str,
    pub writes: &'a [Value],
    /// VAPP-235: the outbox entry's idempotency key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub write_id: Option<&'a str>,
}

/// `apps.records.batch` output: each write's record id, in order.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordsBatchOutput {
    #[serde(default, alias = "txid")]
    pub tx_id: Option<i64>,
    #[serde(default)]
    pub ids: Vec<String>,
    /// VAPP-215: the batch's events (the Undo toast's; empty = no change).
    #[serde(default)]
    pub event_ids: Vec<String>,
}

/// VAPP-194 `apps.records.batch`: the writes as ONE transaction (all land
/// or none; the server checks every one against its kind again).
pub fn records_batch(
    trpc: &TrpcClient,
    app_id: &str,
    writes: &[Value],
    write_id: Option<&str>,
) -> Result<RecordsBatchOutput, ApiError> {
    trpc.mutation("apps.records.batch", &RecordsBatchInput { id: app_id, writes, write_id })
}

/// VAPP-230: the upstream read-binding contract (`upstream.json`
/// `binding`), for the hosts that call [`upstream_read`].
pub use emcp::contract::upstream as upstream_binding;
pub use emcp::contract::upstream::UpstreamReadAnswer;

#[derive(Serialize)]
struct UpstreamReadInput<'a> {
    id: &'a str,
    source: &'a str,
}

/// VAPP-230 `apps.upstreamRead {id, source}`: one `upstream:<server>/<tool>?…`
/// binding read by the server broker as the viewer, `{ok: true, value, meta}`
/// or `{ok: false, error, server, message}` (a refusal is an answer, not an
/// error; an `Err` = offline or the app unreachable).
pub fn upstream_read(trpc: &TrpcClient, app_id: &str, source: &str) -> Result<UpstreamReadAnswer, ApiError> {
    trpc.query_with_input("apps.upstreamRead", &UpstreamReadInput { id: app_id, source })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_read_speaks_the_web_wire() {
        assert_eq!(
            serde_json::to_value(UpstreamReadInput { id: "app", source: "upstream:crm/search?q=a" }).unwrap(),
            serde_json::json!({"id": "app", "source": "upstream:crm/search?q=a"})
        );
        let ok: UpstreamReadAnswer =
            serde_json::from_value(serde_json::json!({"ok": true, "value": {"mrr": 1}, "meta": {"via": "upstream"}})).unwrap();
        assert_eq!((ok.ok, ok.value), (true, serde_json::json!({"mrr": 1})));
        let refused: UpstreamReadAnswer = serde_json::from_value(
            serde_json::json!({"ok": false, "error": "upstreamDisconnected", "server": "crm", "message": "Connect crm"}),
        )
        .unwrap();
        assert_eq!(upstream_binding::upstream_banners(&[refused]), vec!["Connect crm".to_string()]);
    }

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
        // An older server sends no upstreams.
        assert!(checked.upstreams.is_empty() && checked.servers.is_empty());
        // #VAPP-254: the owner's picker, and the picks going back.
        let mapped: ValidateImportOutput = serde_json::from_value(serde_json::json!({
            "issues": [], "name": "CRM", "records": 0,
            "upstreams": [{"name": "crm", "serverId": null, "serverName": null}, {"name": "mail", "serverId": "s1", "serverName": "mail"}],
            "servers": [{"id": "s1", "name": "mail"}, {"id": "s2", "name": "hubspot"}],
        }))
        .unwrap();
        assert_eq!(mapped.upstreams[0], ImportUpstream { name: "crm".into(), server_id: None, server_name: None });
        assert_eq!(mapped.upstreams[1].server_name.as_deref(), Some("mail"));
        assert_eq!(mapped.servers[1], crate::app_access::AppServer { id: "s2".into(), name: "hubspot".into() });
        assert_eq!(
            serde_json::to_value(ImportAppInput { team_id: "t", text: "{}", upstream_mappings: &[] }).unwrap(),
            serde_json::json!({"teamId": "t", "text": "{}"})
        );
        assert_eq!(
            serde_json::to_value(ImportAppInput {
                team_id: "t",
                text: "{}",
                upstream_mappings: &[UpstreamMapping { upstream: "crm".into(), server_id: "s2".into() }],
            })
            .unwrap(),
            serde_json::json!({"teamId": "t", "text": "{}", "upstreamMappings": [{"upstream": "crm", "serverId": "s2"}]})
        );
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
            author: None,
            via: None,
            new_id: None,
            write_id: None,
        };
        assert_eq!(
            serde_json::to_value(&upsert).unwrap(),
            serde_json::json!({"id": "app", "kind": "entry", "data": {"hours": 3}})
        );
        // VAPP-235: the outbox's replay carries the client id and the write id.
        let replay = RecordUpsertInput { new_id: Some("n1".into()), write_id: Some("w1".into()), ..upsert.clone() };
        assert_eq!(
            serde_json::to_value(&replay).unwrap(),
            serde_json::json!({"id": "app", "kind": "entry", "data": {"hours": 3}, "newId": "n1", "writeId": "w1"})
        );
        assert_eq!(
            serde_json::to_value(RecordIdInput { id: "app", record_id: "r1", via: None, write_id: Some("w2") }).unwrap(),
            serde_json::json!({"id": "app", "recordId": "r1", "writeId": "w2"})
        );
        assert_eq!(
            serde_json::to_value(RecordsBatchInput { id: "app", writes: &[], write_id: Some("w3") }).unwrap(),
            serde_json::json!({"id": "app", "writes": [], "writeId": "w3"})
        );
        let on_behalf = RecordUpsertInput { record_id: Some("r1".into()), author: Some("u2".into()), ..upsert };
        assert_eq!(
            serde_json::to_value(&on_behalf).unwrap(),
            serde_json::json!({"id": "app", "kind": "entry", "data": {"hours": 3}, "recordId": "r1", "author": "u2"})
        );
        assert_eq!(
            serde_json::to_value(RecordIdInput { id: "app", record_id: "r1", via: None, write_id: None }).unwrap(),
            serde_json::json!({"id": "app", "recordId": "r1"})
        );
        // VAPP-215: the Data tab says `via: "editor"`; the write's event comes back.
        assert_eq!(
            serde_json::to_value(RecordIdInput { id: "app", record_id: "r1", via: Some("editor"), write_id: None }).unwrap(),
            serde_json::json!({"id": "app", "recordId": "r1", "via": "editor"})
        );
        assert_eq!(
            serde_json::to_value(RecordUndoInput { id: "app", event_ids: &["e1".to_string()], via: "editor" }).unwrap(),
            serde_json::json!({"id": "app", "eventIds": ["e1"], "via": "editor"})
        );
        assert_eq!(
            serde_json::to_value(RecordRestoreInput { id: "app", record_id: "r1", event_id: "e1", via: "editor" }).unwrap(),
            serde_json::json!({"id": "app", "recordId": "r1", "eventId": "e1", "via": "editor"})
        );
        let out: RecordUpsertOutput =
            serde_json::from_value(serde_json::json!({"txid": 3, "id": "r1", "created": true, "eventId": "e1"})).unwrap();
        assert_eq!((out.tx_id, out.id.as_str(), out.created, out.event_id.as_deref()), (Some(3), "r1", true, Some("e1")));
        let writes = vec![
            serde_json::json!({"op": "upsert", "kind": "entry", "data": {"hours": 2}}),
            serde_json::json!({"op": "delete", "id": "r1"}),
        ];
        assert_eq!(
            serde_json::to_value(RecordsBatchInput { id: "app", writes: &writes, write_id: None }).unwrap(),
            serde_json::json!({"id": "app", "writes": [{"op": "upsert", "kind": "entry", "data": {"hours": 2}}, {"op": "delete", "id": "r1"}]})
        );
        let out: RecordsBatchOutput = serde_json::from_value(serde_json::json!({"txid": 4, "ids": ["n1", "r1"]})).unwrap();
        assert_eq!((out.tx_id, out.ids), (Some(4), vec!["n1".to_string(), "r1".to_string()]));
    }
}
