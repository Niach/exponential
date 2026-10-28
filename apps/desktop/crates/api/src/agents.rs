//! NEW-2: the typed `agents.*` client (web `apps/web/src/lib/trpc/agents.ts`).
//!
//! A team agent is a DEFINITION row, never a session: name, icon, color,
//! server-only `instructions`, `triggers` (cron schedules and named events
//! the SERVER fires), and the runner it uses (device, CLI, login, model).
//! It runs as conversation sessions, one per member, messaged with
//! `sessions.send({agentId, text})` ([`crate::sessions::SendInput`]).
//! Members read the synced `agents` shape; owners write here. `get` is the
//! only path to the instructions.
//!
//! Plain blocking calls: run them on a background executor only.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// One agent definition as the wire carries it (`get` adds the
/// instructions; `list` and the synced shape never do).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    pub id: String,
    #[serde(default)]
    pub team_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    /// Server-only; empty off `list`.
    #[serde(default)]
    pub instructions: String,
    /// `AgentTrigger[]` as is (`{id, enabled, kind: schedule, cron, tz?}` |
    /// `{id, enabled, kind: event, name}`).
    #[serde(default)]
    pub triggers: Vec<serde_json::Value>,
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub cli: Option<String>,
    #[serde(default)]
    pub account: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub owner_id: Option<String>,
    #[serde(default)]
    pub sort_order: f64,
    /// S-3: contract `agentRole` (`assistant` | `agent`); absent = `agent`.
    #[serde(default)]
    pub role: Option<String>,
}

/// `{txid}` of a plain mutation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxOutput {
    #[serde(default, alias = "txid")]
    pub tx_id: Option<i64>,
}

#[derive(Deserialize)]
struct ListResponse {
    agents: Vec<Agent>,
}

#[derive(Deserialize)]
struct AgentResponse {
    agent: Agent,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TeamIdInput<'a> {
    team_id: &'a str,
}

#[derive(Serialize)]
struct IdInput<'a> {
    id: &'a str,
}

/// `agents.list`: the team's agents (member read).
pub fn list(trpc: &TrpcClient, team_id: &str) -> Result<Vec<Agent>, ApiError> {
    let response: ListResponse = trpc.query_with_input("agents.list", &TeamIdInput { team_id })?;
    Ok(response.agents)
}

#[derive(Deserialize)]
struct MaybeAgentResponse {
    #[serde(default)]
    agent: Option<Agent>,
}

/// S-3 `agents.ensureAssistant`: the caller's Assistant in `team_id`,
/// created on first use on their newest own desktop; `None` while they have
/// no device (agent-settings.json `assistant.copy.noDevice`).
pub fn ensure_assistant(trpc: &TrpcClient, team_id: &str) -> Result<Option<Agent>, ApiError> {
    let response: MaybeAgentResponse = trpc.mutation("agents.ensureAssistant", &TeamIdInput { team_id })?;
    Ok(response.agent)
}

/// `agents.get`: the row with its server-only instructions (member read).
pub fn get(trpc: &TrpcClient, id: &str) -> Result<Agent, ApiError> {
    let response: AgentResponse = trpc.query_with_input("agents.get", &IdInput { id })?;
    Ok(response.agent)
}

/// `agents.create` input (owner only). `None` keys are omitted.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateInput {
    pub team_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub instructions: String,
    pub triggers: Vec<serde_json::Value>,
    pub device_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// `agents.create`: the new definition.
pub fn create(trpc: &TrpcClient, input: &CreateInput) -> Result<Agent, ApiError> {
    let response: AgentResponse = trpc.mutation("agents.create", input)?;
    Ok(response.agent)
}

/// `agents.update` input (owner only): unset keys are omitted;
/// `icon`/`color`/`account`/`model` take `Some(None)` to send `null`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInput {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggers: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<f64>,
}

impl UpdateInput {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into(), ..Self::default() }
    }
}

/// `agents.update`: the updated definition.
pub fn update(trpc: &TrpcClient, input: &UpdateInput) -> Result<Agent, ApiError> {
    let response: AgentResponse = trpc.mutation("agents.update", input)?;
    Ok(response.agent)
}

/// `agents.delete`: the definition and, by cascade, every conversation.
pub fn delete(trpc: &TrpcClient, id: &str) -> Result<TxOutput, ApiError> {
    trpc.mutation("agents.delete", &IdInput { id })
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

    fn request(captured: &flume::Receiver<String>) -> String {
        captured.recv_timeout(Duration::from_secs(5)).unwrap()
    }

    #[test]
    fn list_and_get_decode_the_definitions() {
        let (base, captured) = one_shot_server(
            200,
            r##"{"result":{"data":{"agents":[{"id":"ag-1","teamId":"t-1","name":"Triage","icon":null,
                "color":"#22c55e","triggers":[{"id":"tr-1","enabled":true,"kind":"event","name":"issue.created"}],
                "deviceId":"dev-1","cli":"claude","account":null,"model":null,"ownerId":"u-1","sortOrder":1}]}}}"##,
        );
        let agents = list(&client(&base), "t-1").unwrap();
        assert_eq!(agents[0].name, "Triage");
        assert_eq!(agents[0].triggers[0]["name"], "issue.created");
        assert!(agents[0].instructions.is_empty());
        assert!(request(&captured).starts_with("GET /api/trpc/agents.list?input="));

        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"agent":{"id":"ag-1","teamId":"t-1","name":"Triage","instructions":"Keep it tidy."}}}}"#,
        );
        assert_eq!(get(&client(&base), "ag-1").unwrap().instructions, "Keep it tidy.");
        assert!(request(&captured)
            .starts_with("GET /api/trpc/agents.get?input=%7B%22id%22%3A%22ag-1%22%7D HTTP/1.1"));
    }

    #[test]
    fn create_update_and_delete_post_their_inputs() {
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"agent":{"id":"ag-1","teamId":"t-1","name":"Triage"},"txid":1}}}"#);
        let input = CreateInput {
            team_id: "t-1".into(),
            name: "Triage".into(),
            icon: Some("bot".into()),
            triggers: vec![serde_json::json!({"kind": "schedule", "cron": "0 9 * * 1-5"})],
            device_id: "dev-1".into(),
            ..CreateInput::default()
        };
        assert_eq!(create(&client(&base), &input).unwrap().id, "ag-1");
        let text = request(&captured);
        assert!(text.starts_with("POST /api/trpc/agents.create HTTP/1.1"), "{text}");
        let body: serde_json::Value = serde_json::from_str(&text[text.find("\r\n\r\n").unwrap() + 4..]).unwrap();
        assert_eq!(
            body,
            serde_json::json!({"teamId": "t-1", "name": "Triage", "icon": "bot", "instructions": "",
                "triggers": [{"kind": "schedule", "cron": "0 9 * * 1-5"}], "deviceId": "dev-1"})
        );

        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"agent":{"id":"ag-1","teamId":"t-1","name":"Docs"},"txid":2}}}"#);
        let input = UpdateInput {
            name: Some("Docs".into()),
            account: Some(None),
            model: Some(Some("opus".into())),
            ..UpdateInput::new("ag-1")
        };
        update(&client(&base), &input).unwrap();
        let text = request(&captured);
        assert!(text.starts_with("POST /api/trpc/agents.update HTTP/1.1"), "{text}");
        assert!(text.ends_with(r#"{"id":"ag-1","name":"Docs","account":null,"model":"opus"}"#), "{text}");

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":3}}}"#);
        assert_eq!(delete(&client(&base), "ag-1").unwrap().tx_id, Some(3));
        let text = request(&captured);
        assert!(text.starts_with("POST /api/trpc/agents.delete HTTP/1.1"), "{text}");
        assert!(text.ends_with(r#"{"id":"ag-1"}"#), "{text}");
    }
}
