//! Typed `actions.*` client (EXP-253 — team action prompts).
//!
//! The pinned wire shape:
//! `actions.list({teamId})` — **query**, member-read — →
//! `{actions: [{id, teamId, repositoryId, name, description, body,
//! sortOrder, createdAt, updatedAt}]}` ordered by `sortOrder`, then `name`.
//! `actions.get({id})` → `{action}` — the fetch-fresh path runners hash.
//!
//! SECURITY: an action's `body` is a **DB-stored prompt an interactive claude
//! session executes locally** — the one place server data drives local
//! execution. The mandatory compensating control is the client-side
//! per-device trust gate: before every run, re-fetch via [`get`], hash the
//! FRESH body with [`body_hash`], and check it against
//! [`crate::trust_store::TrustStore`]. Never run a listed/cached body.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::patch::Patch;
use crate::trpc::TrpcClient;
use crate::trust_store::{hex, sha256};

/// The server-defined virtual "Create action" row's id (EXP-257) — one of
/// the two non-UUID ids `actions.list` may carry. `actions.get/update/delete`
/// reject both; clients construct the rows locally and skip the trust gate
/// (their content is server-shipped, never owner-authored).
pub const BUILTIN_CREATE_ACTION_ID: &str = domain::contract::BUILTIN_CREATE_ACTION_ID;

/// The server-defined virtual "Fix merge conflicts" row's id (EXP-259) — the
/// second builtin: takes a `pr` input (an issue-linked open PR), rebases its
/// branch onto the default branch in a worktree, resolves the conflicts,
/// pushes, and merges via the `exponential_pr_merge` MCP tool.
pub const BUILTIN_FIX_CONFLICTS_ID: &str = domain::contract::BUILTIN_FIX_CONFLICTS_ID;

/// Whether `id` is a server-defined virtual builtin action id.
pub fn is_builtin_action_id(id: &str) -> bool {
    id == BUILTIN_CREATE_ACTION_ID || id == BUILTIN_FIX_CONFLICTS_ID
}

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
    /// The markdown prompt the run executes. Trust-hash THIS, freshly
    /// fetched — see the module docs.
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

/// `actions.get` — query, member-read. The run path MUST use this (fresh
/// body) rather than a listed row before hashing for the trust gate.
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

/// The trust-gate hash: SHA-256 over the raw body bytes, hex-encoded. Any
/// body change (even whitespace) yields a new hash, which un-trusts the
/// action on this device until the trust dialog confirms the new body.
pub fn body_hash(body: &str) -> String {
    hex(&sha256(body.as_bytes()))
}

/// The trust-gate hash for a FULL action (EXP-257): identical to
/// [`body_hash`] while the inputs schema is empty (existing trust records
/// stay valid), else SHA-256 over the body plus a canonical JSON of the
/// schema (key/label/type/required in definition order, NUL-separated from
/// the body). Input labels are owner-authored text injected into the run's
/// prompt, so any schema change must re-prompt the trust dialog.
pub fn trust_hash(action: &Action) -> String {
    if action.inputs.is_empty() {
        return body_hash(&action.body);
    }
    // Hand-built canonical form: a fixed field order regardless of serde_json
    // map ordering, string fields JSON-escaped.
    let mut canonical = String::from("[");
    for (ix, input) in action.inputs.iter().enumerate() {
        if ix > 0 {
            canonical.push(',');
        }
        canonical.push_str(&format!(
            r#"{{"key":{},"label":{},"type":{},"required":{}}}"#,
            serde_json::to_string(&input.key).unwrap_or_default(),
            serde_json::to_string(&input.label).unwrap_or_default(),
            serde_json::to_string(&input.input_type).unwrap_or_default(),
            input.required,
        ));
    }
    canonical.push(']');
    let mut bytes = action.body.as_bytes().to_vec();
    bytes.push(0); // unambiguous body/schema boundary
    bytes.extend_from_slice(canonical.as_bytes());
    hex(&sha256(&bytes))
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
    fn body_hash_is_the_sha256_hex_of_the_raw_body() {
        // FIPS vector: sha256("abc").
        assert_eq!(
            body_hash("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // Whitespace-only edits still change the hash — the gate re-fires.
        assert_ne!(body_hash("a b"), body_hash("a  b"));
    }

    fn action_with_inputs(inputs: Vec<ActionInput>) -> Action {
        Action {
            id: "act-1".to_string(),
            team_id: "team-1".to_string(),
            repository_id: None,
            name: "Groom".to_string(),
            description: None,
            body: "do it".to_string(),
            builtin: false,
            inputs,
            sort_order: 0.0,
            created_at: None,
            updated_at: None,
        }
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
    fn trust_hash_matches_body_hash_while_inputs_are_empty() {
        // Existing per-device trust records predate the inputs schema and
        // MUST stay valid for input-less actions.
        let action = action_with_inputs(Vec::new());
        assert_eq!(trust_hash(&action), body_hash("do it"));
    }

    #[test]
    fn trust_hash_covers_the_inputs_schema() {
        let input = ActionInput {
            key: "scope".to_string(),
            label: "Scope".to_string(),
            input_type: "text".to_string(),
            required: false,
            placeholder: None,
        };
        let with_inputs = action_with_inputs(vec![input.clone()]);
        // A schema diverges from the bare body hash…
        assert_ne!(trust_hash(&with_inputs), body_hash("do it"));
        // …and every schema field re-fires the gate: label (prompt-injected
        // owner text), type, required, and definition order.
        let mut relabeled = with_inputs.clone();
        relabeled.inputs[0].label = "Scope!".to_string();
        assert_ne!(trust_hash(&with_inputs), trust_hash(&relabeled));
        let mut retyped = with_inputs.clone();
        retyped.inputs[0].input_type = "repo".to_string();
        assert_ne!(trust_hash(&with_inputs), trust_hash(&retyped));
        let mut required = with_inputs.clone();
        required.inputs[0].required = true;
        assert_ne!(trust_hash(&with_inputs), trust_hash(&required));
        // The placeholder is presentation-only — it never re-prompts.
        let mut placeholder = with_inputs.clone();
        placeholder.inputs[0].placeholder = Some("hint".to_string());
        assert_eq!(trust_hash(&with_inputs), trust_hash(&placeholder));
        // Same inputs, same body → stable.
        assert_eq!(trust_hash(&with_inputs), trust_hash(&with_inputs.clone()));
    }
}
