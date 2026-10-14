//! Typed `actions.*` client (EXP-253 — team action prompts).
//!
//! EXP-1292: an action is a SAVED PROMPT — `name`, `icon`, markdown `body`,
//! typed PICK `inputs` and a composer hint. Starting one is
//! `sessions.send({actionId, inputs})` (the server renders the prompt into
//! the session brief); the builtins, the action triggers (now an agent's),
//! the action repository and its MCP list are gone from the device.
//!
//! Listing rides the Electric `actions` shape since EXP-268 (body EXCLUDED
//! from sync — the synced row is the list projection); this client stays
//! load-bearing for `actions.get` — the ONLY body path, fetched on editor
//! open — and the owner-only CRUD. `actions.list` survives for tests and
//! shape-less callers.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// EXP-825: the server's `actionPromptPlaceholderSchema` cap (web
/// `MAX_ACTION_PROMPT_PLACEHOLDER`) — the editor's single-line field stops
/// there so a save can never 400.
pub const MAX_PROMPT_PLACEHOLDER_CHARS: usize = 200;

/// One typed PICK input definition on an action (EXP-257 — filled in the
/// composer; EXP-1292: the values ride `sessions.send({inputs})` and the
/// server renders them into the session brief).
///
/// EXP-282: also `Serialize` — the desktop's action detail edits the input
/// DEFINITIONS and sends them back through [`ActionUpdate::inputs`] as a
/// whole-array replace (the server's `actionInputsSchema` contract: snake_case
/// `key`, ≤10 entries, unique keys).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ActionInput {
    pub key: String,
    pub label: String,
    /// `repo` | `board` | `pr` | `issue` (contract `actionInputType`; EXP-825
    /// retired `text`/`textarea` — the composer's free text reaches every run
    /// as the session's message instead). An UNKNOWN value must block the
    /// start with "needs a newer app version" — never a silent text fallback.
    #[serde(rename = "type")]
    pub input_type: String,
    /// Absent on the wire = optional (the contract's `required` default).
    #[serde(default)]
    pub required: bool,
    /// Text-field placeholder, when the owner set one. Skipped when absent:
    /// the server schema is `.optional()`, and a literal `null` would 400.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
}

/// One `actions` row as the wire carries it.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub id: String,
    pub team_id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// EXP-273: curated registry icon name (same set as `boards.icon`);
    /// `None` = the generic action glyph.
    #[serde(default)]
    pub icon: Option<String>,
    /// The markdown prompt — EMPTY on shape-synced rows (EXP-268: the proxy
    /// excludes it); [`get`] returns the real body.
    #[serde(default)]
    pub body: String,
    /// The typed run-time inputs schema (EXP-257; empty on input-less
    /// actions and on rows from a pre-inputs server).
    #[serde(default)]
    pub inputs: Vec<ActionInput>,
    /// EXP-825: the composer's field hint while this action is picked
    /// (`promptPlaceholder`, ≤[`MAX_PROMPT_PLACEHOLDER_CHARS`]); `None` =
    /// the generic "Additional instructions" hint. Absent on rows from a
    /// pre-EXP-825 server.
    #[serde(default)]
    pub prompt_placeholder: Option<String>,
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
    body: &'a str,
    /// EXP-825: the composer hint; skipped when `None` (the server schema
    /// is `.nullable().optional()`, an omitted field leaves it NULL).
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt_placeholder: Option<&'a str>,
}

/// `actions.create` — mutation, owner-only. The server appends to the end of
/// the sort order.
pub fn create(
    trpc: &TrpcClient,
    team_id: &str,
    name: &str,
    description: Option<&str>,
    body: &str,
    prompt_placeholder: Option<&str>,
) -> Result<Action, ApiError> {
    let response: ActionResponse = trpc.mutation(
        "actions.create",
        &CreateInput {
            team_id,
            name,
            description,
            body,
            prompt_placeholder,
        },
    )?;
    Ok(response.action)
}

/// `actions.update` input. Omitted fields stay unchanged.
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionUpdate {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// EXP-298: the curated registry glyph (`actionIconSchema`, the same set
    /// as `boards.icon`). Set-only — the detail's picker always lands on a
    /// real name, and the web editor can't clear it either, so the server's
    /// `nullable()` half stays unused.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// EXP-282: the typed run-time input DEFINITIONS, as a whole-array
    /// replace (the server patches nothing here — `inputs` is small and
    /// orderful). `None` omits the field and leaves the schema untouched;
    /// `Some(vec![])` deliberately CLEARS it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inputs: Option<Vec<ActionInput>>,
    /// EXP-825: the composer hint. `None` omits the field; `Some("")` clears
    /// it (the server nulls an empty string, the desktop's description
    /// convention).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_placeholder: Option<String>,
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
/// unknown-`type` start block still guards starts).
pub fn from_row(row: &domain::rows::ActionRow) -> Action {
    let inputs = row
        .inputs
        .clone()
        .and_then(|value| serde_json::from_value::<Vec<ActionInput>>(value).ok())
        .unwrap_or_default();
    Action {
        id: row.id.clone(),
        team_id: row.team_id.clone().unwrap_or_default(),
        name: row.name.clone().unwrap_or_default(),
        description: row.description.clone(),
        icon: row.icon.clone(),
        body: String::new(),
        inputs,
        prompt_placeholder: row.prompt_placeholder.clone(),
        sort_order: row.sort_order.unwrap_or_default(),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
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
                 "body":"# Review\n","sortOrder":1,"triggers":[],
                 "createdAt":"2026-07-24T00:00:00.000Z","updatedAt":"2026-07-24T00:00:00.000Z"}]}}}"##,
        );
        let actions = list(&client(&base), "team-1").unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].name, "Code review");
        assert_eq!(actions[0].body, "# Review\n");
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/actions.list?input="));
    }

    #[test]
    fn list_tolerates_null_description() {
        let (base, _captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"actions":[
                {"id":"act-1","teamId":"team-1",
                 "name":"Groom","description":null,"body":"do it","sortOrder":0}]}}}"#,
        );
        let actions = list(&client(&base), "team-1").unwrap();
        assert_eq!(actions[0].description, None);
    }

    #[test]
    fn get_unwraps_the_action_envelope() {
        // EXP-1292: the server's leftover `mcpServerIds`/`triggers` are ignored.
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"action":
                {"id":"act-1","teamId":"team-1","repositoryId":null,"mcpServerIds":[],
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
                "name":"Groom","description":null,
                "body":"do it","sortOrder":1}}}}"#,
        );
        let action = create(&client(&base), "team-1", "Groom", None, "do it", None).unwrap();
        assert_eq!(action.id, "act-1");
        // EXP-825: a hint-less response decodes hint-less.
        assert_eq!(action.prompt_placeholder, None);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/actions.create HTTP/1.1"));
        // Omitted optionals stay off the wire (zod .optional()).
        assert!(
            request.ends_with(r#"{"teamId":"team-1","name":"Groom","body":"do it"}"#),
            "{request}"
        );
    }

    /// EXP-825: the composer hint rides `actions.create` under its camelCase
    /// wire key when given.
    #[test]
    fn create_sends_the_prompt_placeholder_when_given() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"action":{"id":"act-1","teamId":"team-1",
                "name":"Groom","description":null,
                "body":"do it","promptPlaceholder":"Scope: which platforms","sortOrder":1}}}}"#,
        );
        let action = create(
            &client(&base),
            "team-1",
            "Groom",
            Some("Groom the backlog"),
            "do it",
            Some("Scope: which platforms"),
        )
        .unwrap();
        assert_eq!(
            action.prompt_placeholder.as_deref(),
            Some("Scope: which platforms")
        );
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            request.ends_with(
                r#"{"teamId":"team-1","name":"Groom","description":"Groom the backlog","body":"do it","promptPlaceholder":"Scope: which platforms"}"#
            ),
            "{request}"
        );
    }

    /// EXP-825: the wire row round-trips the hint — present (camelCase key)
    /// and absent alike; a literal `null` reads as absent too.
    #[test]
    fn action_deserializes_the_prompt_placeholder_present_or_absent() {
        let present: Action = serde_json::from_str(
            r#"{"id":"act-1","teamId":"team-1","name":"Groom","body":"do it",
                "promptPlaceholder":"Scope: which platforms, which version"}"#,
        )
        .unwrap();
        assert_eq!(
            present.prompt_placeholder.as_deref(),
            Some("Scope: which platforms, which version")
        );
        let absent: Action =
            serde_json::from_str(r#"{"id":"act-1","teamId":"team-1","name":"Groom","body":"do it"}"#)
                .unwrap();
        assert_eq!(absent.prompt_placeholder, None);
        let null: Action = serde_json::from_str(
            r#"{"id":"act-1","teamId":"team-1","name":"Groom","body":"do it","promptPlaceholder":null}"#,
        )
        .unwrap();
        assert_eq!(null.prompt_placeholder, None);
        // The snake_case shape key never matches the camelCase wire struct.
        let snake: Action = serde_json::from_str(
            r#"{"id":"act-1","teamId":"team-1","name":"Groom","body":"do it","prompt_placeholder":"x"}"#,
        )
        .unwrap();
        assert_eq!(snake.prompt_placeholder, None);
    }

    /// EXP-825: `actions.update` carries the hint only when set; an empty
    /// string clears it (the server nulls it), an omitted field leaves it.
    #[test]
    fn update_serializes_the_prompt_placeholder() {
        let mut input = ActionUpdate::new("act-1");
        input.prompt_placeholder = Some("Scope: which platforms".to_string());
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains(r#""promptPlaceholder":"Scope: which platforms""#));

        let mut cleared = ActionUpdate::new("act-1");
        cleared.prompt_placeholder = Some(String::new());
        assert!(serde_json::to_string(&cleared)
            .unwrap()
            .contains(r#""promptPlaceholder":"""#));

        let omitted = ActionUpdate::new("act-1");
        assert_eq!(serde_json::to_string(&omitted).unwrap(), r#"{"id":"act-1"}"#);
    }

    #[test]
    fn update_posts_the_set_fields_only() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"action":{"id":"act-1","teamId":"team-1","name":"Renamed","body":"x"},"txId":3}}}"#,
        );
        let mut input = ActionUpdate::new("act-1");
        input.name = Some("Renamed".to_string());
        input.icon = Some("sparkles".to_string());
        let action = update(&client(&base), &input).unwrap();
        assert_eq!(action.name, "Renamed");
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/actions.update HTTP/1.1"));
        assert!(
            request.ends_with(r#"{"id":"act-1","name":"Renamed","icon":"sparkles"}"#),
            "{request}"
        );
    }

    #[test]
    fn delete_posts_the_id() {
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"ok":true,"id":"act-1","txId":4}}}"#);
        delete(&client(&base), "act-1").unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/actions.delete HTTP/1.1"));
        assert!(request.ends_with(r#"{"id":"act-1"}"#));
    }

    #[test]
    fn update_serializes_the_inputs_whole_array() {
        // EXP-282: the action detail edits the input DEFINITIONS — the array
        // rides `actions.update` verbatim (`type` on the wire, absent
        // placeholder OMITTED so the server's `.optional()` accepts it).
        let mut input = ActionUpdate::new("act-1");
        input.inputs = Some(vec![
            ActionInput {
                key: "issue".to_string(),
                label: "Issue".to_string(),
                input_type: "issue".to_string(),
                required: true,
                placeholder: Some("e.g. EXP-1".to_string()),
            },
            ActionInput {
                key: "repo".to_string(),
                label: "Repository".to_string(),
                input_type: "repo".to_string(),
                required: false,
                placeholder: None,
            },
        ]);
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains(r#""inputs":[{"key":"issue","label":"Issue","type":"issue","required":true,"placeholder":"e.g. EXP-1"}"#));
        assert!(json.contains(r#"{"key":"repo","label":"Repository","type":"repo","required":false}"#));
        for def in input.inputs.as_ref().unwrap() {
            assert!(domain::contract::ACTION_INPUT_TYPE_VALUES.contains(&def.input_type.as_str()));
        }
        // An omitted array leaves the schema untouched; an empty one clears it.
        let untouched = ActionUpdate::new("act-1");
        assert!(!serde_json::to_string(&untouched).unwrap().contains("inputs"));
        let mut cleared = ActionUpdate::new("act-1");
        cleared.inputs = Some(Vec::new());
        assert!(serde_json::to_string(&cleared)
            .unwrap()
            .contains(r#""inputs":[]"#));
    }

    #[test]
    fn list_decodes_inputs() {
        // EXP-257: rows may carry a typed inputs schema (`type` on the wire,
        // `required` absent = optional); an input-less row defaults.
        let (base, _captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"actions":[
                {"id":"act-1","teamId":"team-1",
                 "name":"Groom","description":null,"body":"do it","sortOrder":0,
                 "inputs":[{"key":"board","label":"Board","type":"board","required":true,
                            "placeholder":"e.g. backlog"},
                           {"key":"repo","label":"Repository","type":"repo"}]},
                {"id":"act-2","teamId":"team-1","name":"Plain","body":"x","sortOrder":1}]}}}"#,
        );
        let actions = list(&client(&base), "team-1").unwrap();
        assert_eq!(
            actions[0].inputs,
            vec![
                ActionInput {
                    key: "board".to_string(),
                    label: "Board".to_string(),
                    input_type: "board".to_string(),
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
        assert!(actions[1].inputs.is_empty());
    }

    #[test]
    fn from_row_hydrates_the_synced_list_projection() {
        // EXP-268: a synced actions row (no body on the wire) hydrates to the
        // list projection — body empty, inputs re-parsed from the stored JSON.
        let row: domain::rows::ActionRow = serde_json::from_value(serde_json::json!({
            "id": "act-1",
            "team_id": "team-1",
            "name": "Code review",
            "description": "Review + file issues",
            "icon": "sparkles",
            "inputs": r#"[{"key":"pr","label":"Pull request","type":"pr","required":true}]"#,
            // EXP-825: the synced composer hint (snake_case on the shape).
            "prompt_placeholder": "Scope: which platforms, which version",
            "sort_order": "1.5"
        }))
        .unwrap();
        let action = from_row(&row);
        assert_eq!(action.id, "act-1");
        assert_eq!(action.team_id, "team-1");
        assert_eq!(action.name, "Code review");
        assert_eq!(action.icon.as_deref(), Some("sparkles"));
        assert!(action.body.is_empty());
        assert_eq!(action.sort_order, 1.5);
        assert_eq!(action.inputs.len(), 1);
        assert_eq!(action.inputs[0].key, "pr");
        assert!(action.inputs[0].required);
        assert_eq!(
            action.prompt_placeholder.as_deref(),
            Some("Scope: which platforms, which version")
        );

        // A hint-less row reads as the generic hint.
        let old: domain::rows::ActionRow = serde_json::from_value(serde_json::json!({
            "id": "act-2",
            "team_id": "team-1",
            "name": "Groom",
        }))
        .unwrap();
        assert_eq!(from_row(&old).prompt_placeholder, None);
    }
}
