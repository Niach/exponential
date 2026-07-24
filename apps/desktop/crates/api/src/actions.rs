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
}
