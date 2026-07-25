//! Typed `actions.*` client (EXP-253 — team action prompts).
//!
//! Listing rides the Electric `actions` shape since EXP-268 (body EXCLUDED
//! from sync — the synced row is the list projection); this client stays
//! load-bearing for `actions.get` — the ONLY body path, fetched fresh right
//! before a run or on editor open — and the owner-only CRUD. `actions.list`
//! survives for pre-shape builds and tests.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::patch::Patch;
use crate::trpc::TrpcClient;

/// The virtual "Create action" row's id (EXP-257) — the ONE non-UUID action
/// id. `actions.get/update/delete` reject it; clients construct the row
/// locally ([`builtin_create_action`]) — its content is product-shipped,
/// never owner-authored.
pub const BUILTIN_CREATE_ACTION_ID: &str = domain::contract::BUILTIN_CREATE_ACTION_ID;

/// One typed run-time input definition on an action (EXP-257 — filled in the
/// unified launch dialog, resolved server-side for remote starts).
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ActionInput {
    pub key: String,
    pub label: String,
    /// `text` | `repo` | `board` (contract `actionInputType`). An UNKNOWN
    /// value must block the run with "needs a newer app version" — never a
    /// silent text fallback.
    #[serde(rename = "type")]
    pub input_type: String,
    /// Absent on the wire = optional (the contract's `required` default).
    #[serde(default)]
    pub required: bool,
    /// Text-field placeholder, when the owner set one.
    #[serde(default)]
    pub placeholder: Option<String>,
}

/// One `actions` row as the wire carries it.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub id: String,
    pub team_id: String,
    /// Execution context: `Some` = run in this repo's trunk clone on the
    /// default branch; `None` = repo-less (scratch dir).
    #[serde(default)]
    pub repository_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// The markdown prompt the run executes — EMPTY on shape-synced rows
    /// (EXP-268: the proxy excludes it); [`get`] returns the real body.
    #[serde(default)]
    pub body: String,
    /// The server-appended virtual "Create action" row (EXP-257). Clients
    /// pin it FIRST by this flag (never by sort order) and hide the owner
    /// edit/delete affordances on it.
    #[serde(default)]
    pub builtin: bool,
    /// The typed run-time inputs schema (EXP-257; empty on input-less
    /// actions and on rows from a pre-inputs server).
    #[serde(default)]
    pub inputs: Vec<ActionInput>,
    #[serde(default)]
    pub sort_order: f64,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Deserialize)]
struct ListResponse {
    actions: Vec<Action>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListInput<'a> {
    team_id: &'a str,
}

/// `actions.list` — query, ordered by `sortOrder` then `name` server-side.
pub fn list(trpc: &TrpcClient, team_id: &str) -> Result<Vec<Action>, ApiError> {
    let response: ListResponse = trpc.query_with_input("actions.list", &ListInput { team_id })?;
    Ok(response.actions)
}

#[derive(Deserialize)]
struct ActionResponse {
    action: Action,
}

/// `actions.get` — query, member-read. The ONLY body path: synced rows carry
/// no body, so the run path and the editor fetch the fresh row here.
pub fn get(trpc: &TrpcClient, id: &str) -> Result<Action, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
    }
    let response: ActionResponse = trpc.query_with_input("actions.get", &Input { id })?;
    Ok(response.action)
}

// ---------------------------------------------------------------------------
// Owner-only CRUD (the desktop actions panel's raw editor; the server
// re-validates everything — name/description/body limits, repo-in-team —
// and owns the (teamId, name) CONFLICT)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateInput<'a> {
    team_id: &'a str,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repository_id: Option<&'a str>,
    body: &'a str,
}

/// `actions.create` — mutation, owner-only. The server appends to the end of
/// the sort order.
pub fn create(
    trpc: &TrpcClient,
    team_id: &str,
    name: &str,
    description: Option<&str>,
    repository_id: Option<&str>,
    body: &str,
) -> Result<Action, ApiError> {
    let response: ActionResponse = trpc.mutation(
        "actions.create",
        &CreateInput {
            team_id,
            name,
            description,
            repository_id,
            body,
        },
    )?;
    Ok(response.action)
}

/// `actions.update` input. Omitted fields stay unchanged; `repository_id`
/// is the server's `.nullable().optional()` tri-state ([`Patch`]): `Null`
/// clears the action to repo-less.
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionUpdate {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub repository_id: Patch<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<f64>,
}

impl ActionUpdate {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }
}

/// `actions.update` — mutation, owner-only.
pub fn update(trpc: &TrpcClient, input: &ActionUpdate) -> Result<Action, ApiError> {
    let response: ActionResponse = trpc.mutation("actions.update", input)?;
    Ok(response.action)
}

/// `actions.delete` — mutation, owner-only.
pub fn delete(trpc: &TrpcClient, id: &str) -> Result<(), ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
    }
    #[derive(Deserialize)]
    struct Ok_ {
        #[allow(dead_code)]
        ok: bool,
    }
    let _: Ok_ = trpc.mutation("actions.delete", &Input { id })?;
    Ok(())
}

/// Hydrate the list projection from a synced `actions` shape row (EXP-268).
/// `body` is deliberately empty — the shape excludes it; [`get`] is the body
/// path. Rows with an unparseable inputs payload degrade to input-less (the
/// unknown-`type` run block still guards launches).
pub fn from_row(row: &domain::rows::ActionRow) -> Action {
    let inputs = row
        .inputs
        .clone()
        .and_then(|value| serde_json::from_value::<Vec<ActionInput>>(value).ok())
        .unwrap_or_default();
    Action {
        id: row.id.clone(),
        team_id: row.team_id.clone().unwrap_or_default(),
        repository_id: row.repository_id.clone(),
        name: row.name.clone().unwrap_or_default(),
        description: row.description.clone(),
        body: String::new(),
        builtin: false,
        inputs,
        sort_order: row.sort_order.unwrap_or_default(),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

/// The client-constructed virtual "Create action" row (EXP-257/EXP-268):
/// synced rows can't carry it (it isn't a DB row), so every client prepends
/// its own local copy — pinned FIRST by the `builtin` flag, owner
/// edit/delete hidden. Mirrors the web's `builtinCreateAction`.
pub fn builtin_create_action(team_id: &str) -> Action {
    Action {
        id: BUILTIN_CREATE_ACTION_ID.to_string(),
        team_id: team_id.to_string(),
        repository_id: None,
        name: "Create action".to_string(),
        description: Some("Describe a new action and let Claude author it for the team".to_string()),
        body: String::new(),
        builtin: true,
        inputs: vec![
            ActionInput {
                key: "description".to_string(),
                label: "Description".to_string(),
                input_type: "text".to_string(),
                required: true,
                placeholder: Some("What should this action do?".to_string()),
            },
            ActionInput {
                key: "repo".to_string(),
                label: "Repository".to_string(),
                input_type: "repo".to_string(),
                required: false,
                placeholder: None,
            },
        ],
        sort_order: 1e9,
        created_at: None,
        updated_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trpc::tests::one_shot_server;
    use crate::StaticToken;
    use std::sync::Arc;
    use std::time::Duration;

    fn client(base: &str) -> TrpcClient {
        TrpcClient::new(base, Arc::new(StaticToken("tok".to_string())))
    }

    #[test]
    fn list_decodes_actions_and_uses_get() {
        let (base, captured) = one_shot_server(
            200,
            r##"{"result":{"data":{"actions":[
                {"id":"act-1","teamId":"team-1","repositoryId":"repo-1",
                 "name":"Code review","description":"Review + file issues",
                 "body":"# Review\n","sortOrder":1,
                 "createdAt":"2026-07-24T00:00:00.000Z","updatedAt":"2026-07-24T00:00:00.000Z"}]}}}"##,
        );
        let actions = list(&client(&base), "team-1").unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].name, "Code review");
        assert_eq!(actions[0].repository_id.as_deref(), Some("repo-1"));
        assert_eq!(actions[0].body, "# Review\n");
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/actions.list?input="));
    }

    #[test]
    fn list_tolerates_null_repo_and_description() {
        let (base, _captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"actions":[
                {"id":"act-1","teamId":"team-1","repositoryId":null,
                 "name":"Groom","description":null,"body":"do it","sortOrder":0}]}}}"#,
        );
        let actions = list(&client(&base), "team-1").unwrap();
        assert_eq!(actions[0].repository_id, None);
        assert_eq!(actions[0].description, None);
    }

    #[test]
    fn get_unwraps_the_action_envelope() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"action":
                {"id":"act-1","teamId":"team-1","repositoryId":null,
                 "name":"Groom","description":null,"body":"fresh body","sortOrder":0}}}}"#,
        );
        let action = get(&client(&base), "act-1").unwrap();
        assert_eq!(action.body, "fresh body");
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/actions.get?input="));
    }

    #[test]
    fn create_posts_and_omits_absent_optionals() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"action":{"id":"act-1","teamId":"team-1",
                "repositoryId":null,"name":"Groom","description":null,
                "body":"do it","sortOrder":1}}}}"#,
        );
        let action = create(&client(&base), "team-1", "Groom", None, None, "do it").unwrap();
        assert_eq!(action.id, "act-1");
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/actions.create HTTP/1.1"));
        assert!(request.contains(r#""teamId":"team-1""#));
        assert!(request.contains(r#""body":"do it""#));
        // Omitted optionals stay off the wire (zod .optional()).
        assert!(!request.contains(r#""description""#));
        assert!(!request.contains(r#""repositoryId""#));
    }

    #[test]
    fn update_serializes_the_repository_tristate() {
        // Omit = unchanged, Null = clear to repo-less.
        let mut input = ActionUpdate::new("act-1");
        input.name = Some("Renamed".to_string());
        input.repository_id = Patch::Null;
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains(r#""repositoryId":null"#));
        assert!(json.contains(r#""name":"Renamed""#));
        assert!(!json.contains(r#""body""#));

        let omitted = ActionUpdate::new("act-1");
        let json = serde_json::to_string(&omitted).unwrap();
        assert!(!json.contains("repositoryId"));
    }

    #[test]
    fn list_decodes_inputs_and_builtin() {
        // EXP-257: the server appends the virtual builtin row and real rows
        // may carry a typed inputs schema (`type` on the wire, `required`
        // absent = optional).
        let (base, _captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"actions":[
                {"id":"act-1","teamId":"team-1","repositoryId":null,
                 "name":"Groom","description":null,"body":"do it","sortOrder":0,
                 "inputs":[{"key":"scope","label":"Scope","type":"text","required":true,
                            "placeholder":"e.g. backlog"},
                           {"key":"repo","label":"Repository","type":"repo"}]},
                {"id":"builtin:create-action","teamId":"team-1","repositoryId":null,
                 "name":"Create action","description":"Describe a new action","body":"",
                 "builtin":true,"sortOrder":1000000000,
                 "inputs":[{"key":"description","label":"Description","type":"text","required":true},
                           {"key":"repo","label":"Repository","type":"repo"}]}]}}}"#,
        );
        let actions = list(&client(&base), "team-1").unwrap();
        assert_eq!(actions.len(), 2);
        assert!(!actions[0].builtin);
        assert_eq!(
            actions[0].inputs,
            vec![
                ActionInput {
                    key: "scope".to_string(),
                    label: "Scope".to_string(),
                    input_type: "text".to_string(),
                    required: true,
                    placeholder: Some("e.g. backlog".to_string()),
                },
                ActionInput {
                    key: "repo".to_string(),
                    label: "Repository".to_string(),
                    input_type: "repo".to_string(),
                    // Absent on the wire = optional.
                    required: false,
                    placeholder: None,
                },
            ]
        );
        assert!(actions[1].builtin);
        assert_eq!(actions[1].id, BUILTIN_CREATE_ACTION_ID);
        assert!(actions[1].inputs[0].required);
    }

    #[test]
    fn pre_inputs_rows_decode_with_defaults() {
        // A row from a pre-EXP-257 server (or an input-less action) carries
        // neither field — both default.
        let (base, _captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"actions":[
                {"id":"act-1","teamId":"team-1","repositoryId":null,
                 "name":"Groom","description":null,"body":"do it","sortOrder":0}]}}}"#,
        );
        let actions = list(&client(&base), "team-1").unwrap();
        assert!(!actions[0].builtin);
        assert!(actions[0].inputs.is_empty());
    }

    #[test]
    fn from_row_hydrates_the_synced_list_projection() {
        // EXP-268: a synced actions row (no body on the wire) hydrates to the
        // list projection — body empty, inputs re-parsed from the stored
        // JSON, never builtin.
        let row: domain::rows::ActionRow = serde_json::from_value(serde_json::json!({
            "id": "act-1",
            "team_id": "team-1",
            "repository_id": "repo-1",
            "name": "Code review",
            "description": "Review + file issues",
            "inputs": r#"[{"key":"scope","label":"Scope","type":"text","required":true}]"#,
            "sort_order": "1.5"
        }))
        .unwrap();
        let action = from_row(&row);
        assert_eq!(action.id, "act-1");
        assert_eq!(action.team_id, "team-1");
        assert_eq!(action.repository_id.as_deref(), Some("repo-1"));
        assert_eq!(action.name, "Code review");
        assert!(action.body.is_empty());
        assert!(!action.builtin);
        assert_eq!(action.sort_order, 1.5);
        assert_eq!(action.inputs.len(), 1);
        assert_eq!(action.inputs[0].key, "scope");
        assert!(action.inputs[0].required);
    }

    #[test]
    fn builtin_create_action_matches_the_web_factory() {
        let builtin = builtin_create_action("team-1");
        assert_eq!(builtin.id, BUILTIN_CREATE_ACTION_ID);
        assert!(builtin.builtin);
        assert!(builtin.body.is_empty());
        assert_eq!(builtin.name, "Create action");
        assert_eq!(builtin.inputs.len(), 2);
        assert!(builtin.inputs[0].required);
        assert_eq!(builtin.inputs[1].input_type, "repo");
        // Pinned first by flag; the huge sortOrder only keeps naive
        // sortOrder-asc renderers from interleaving it.
        assert_eq!(builtin.sort_order, 1e9);
    }
}
