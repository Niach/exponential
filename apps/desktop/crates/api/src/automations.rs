//! Typed `automations.*` client (EXP-583 — automations split out of
//! `actions.trigger`).
//!
//! Listing rides the Electric `automations` shape; this client is the WRITE
//! path (owner-only `create`/`update`/`delete`, `enabled` toggles included)
//! plus a member `list` for pre-shape builds and tests. The server owns every
//! rule — same-team custom action, no required inputs while enabled, a device
//! that is mine-or-team-shared AND advertises the `automations` cap, and
//! agent/model/effort validated per agent — so this layer only shapes the
//! wire.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::patch::Patch;
use crate::trpc::TrpcClient;

/// One `automations` row as the wire carries it.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Automation {
    pub id: String,
    pub team_id: String,
    pub action_id: String,
    /// The steer TEXT device id (`devices.device_id`) whose host fires it.
    pub device_id: String,
    #[serde(default)]
    pub enabled: bool,
    /// The WHEN-part, kept as loose JSON so a newer server's trigger kind
    /// never fails decoding — `coding::automations` owns the tolerant parse.
    #[serde(default)]
    pub trigger: Option<serde_json::Value>,
    /// `None` = the bound device's own launch defaults.
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub sort_order: f64,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Deserialize)]
struct ListResponse {
    automations: Vec<Automation>,
}

#[derive(Deserialize)]
struct AutomationResponse {
    automation: Automation,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListInput<'a> {
    team_id: &'a str,
}

/// `automations.list` — query, member-read, `sortOrder` then `createdAt`.
pub fn list(trpc: &TrpcClient, team_id: &str) -> Result<Vec<Automation>, ApiError> {
    let response: ListResponse =
        trpc.query_with_input("automations.list", &ListInput { team_id })?;
    Ok(response.automations)
}

/// `automations.create` input. `agent`/`model`/`effort` are omitted when
/// `None` — the server reads an omitted field as "the device's defaults".
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationCreate {
    pub team_id: String,
    pub action_id: String,
    pub device_id: String,
    pub trigger: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

/// `automations.create` — mutation, owner-only.
pub fn create(trpc: &TrpcClient, input: &AutomationCreate) -> Result<Automation, ApiError> {
    let response: AutomationResponse = trpc.mutation("automations.create", input)?;
    Ok(response.automation)
}

/// `automations.update` input. Omitted fields stay unchanged; the three
/// launch fields are the server's `.nullable().optional()` tri-state
/// ([`Patch`]): `Null` clears the pin back to the device's launch defaults.
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationUpdate {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub agent: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub model: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub effort: Patch<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<f64>,
}

impl AutomationUpdate {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }

    /// The Automations tab's switch: `update({id, enabled})` and nothing else,
    /// so flipping it can never move the trigger's fingerprint.
    pub fn enabled(id: impl Into<String>, enabled: bool) -> Self {
        Self {
            enabled: Some(enabled),
            ..Self::new(id)
        }
    }
}

/// `automations.update` — mutation, owner-only.
pub fn update(trpc: &TrpcClient, input: &AutomationUpdate) -> Result<Automation, ApiError> {
    let response: AutomationResponse = trpc.mutation("automations.update", input)?;
    Ok(response.automation)
}

/// `automations.delete` — mutation, owner-only.
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
    let _: Ok_ = trpc.mutation("automations.delete", &Input { id })?;
    Ok(())
}

/// Hydrate the wire shape from a synced `automations` row. A row with no
/// trigger keeps `None` — every reader treats that as "nothing to fire".
pub fn from_row(row: &domain::rows::AutomationRow) -> Automation {
    Automation {
        id: row.id.clone(),
        team_id: row.team_id.clone().unwrap_or_default(),
        action_id: row.action_id.clone().unwrap_or_default(),
        device_id: row.device_id.clone().unwrap_or_default(),
        enabled: row.is_enabled(),
        trigger: row.trigger.clone(),
        agent: row.agent.clone(),
        model: row.model.clone(),
        effort: row.effort.clone(),
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
    fn list_decodes_rows_and_null_launch_pins() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"automations":[
                {"id":"auto-1","teamId":"team-1","actionId":"act-1","deviceId":"dev-1",
                 "enabled":true,"trigger":{"kind":"schedule","interval":"daily","minuteOfDay":540},
                 "agent":null,"model":null,"effort":null,"sortOrder":1,
                 "createdAt":"2026-08-21T00:00:00.000Z","updatedAt":"2026-08-21T00:00:00.000Z"}]}}}"#,
        );
        let rows = list(&client(&base), "team-1").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].action_id, "act-1");
        assert_eq!(rows[0].device_id, "dev-1");
        assert!(rows[0].enabled);
        assert_eq!(rows[0].trigger.as_ref().unwrap()["interval"], "daily");
        // NULL pins mean "the device's launch defaults", never a fabricated agent.
        assert_eq!(rows[0].agent, None);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/automations.list?input="));
    }

    #[test]
    fn create_omits_absent_launch_pins() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"automation":{"id":"auto-1","teamId":"team-1",
                "actionId":"act-1","deviceId":"dev-1","enabled":true,
                "trigger":{"kind":"event","event":"created"},"sortOrder":1},"txId":"1"}}}"#,
        );
        let automation = create(
            &client(&base),
            &AutomationCreate {
                team_id: "team-1".to_string(),
                action_id: "act-1".to_string(),
                device_id: "dev-1".to_string(),
                trigger: serde_json::json!({"kind": "event", "event": "created"}),
                enabled: None,
                agent: Some("codex".to_string()),
                model: None,
                effort: None,
            },
        )
        .unwrap();
        assert_eq!(automation.id, "auto-1");
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/automations.create HTTP/1.1"));
        assert!(request.contains(r#""actionId":"act-1""#));
        assert!(request.contains(r#""agent":"codex""#));
        // Omitted optionals stay off the wire (zod .optional()).
        assert!(!request.contains(r#""model""#));
        assert!(!request.contains(r#""enabled""#));
    }

    #[test]
    fn update_serializes_the_toggle_and_the_launch_tristate() {
        // The tab's switch sends `enabled` ALONE — anything else would move
        // the trigger's fingerprint and re-seed the host's state.
        let toggle = AutomationUpdate::enabled("auto-1", false);
        let json = serde_json::to_string(&toggle).unwrap();
        assert!(json.contains(r#""id":"auto-1""#));
        assert!(json.contains(r#""enabled":false"#));
        assert!(!json.contains("trigger"));
        assert!(!json.contains("agent"));

        // Null clears a pin back to the device defaults; Omit leaves it.
        let mut cleared = AutomationUpdate::new("auto-1");
        cleared.agent = Patch::Null;
        cleared.model = Patch::Set("opus".to_string());
        let json = serde_json::to_string(&cleared).unwrap();
        assert!(json.contains(r#""agent":null"#));
        assert!(json.contains(r#""model":"opus""#));
        assert!(!json.contains("effort"));
    }

    #[test]
    fn from_row_hydrates_the_synced_projection() {
        let row: domain::rows::AutomationRow = serde_json::from_value(serde_json::json!({
            "id": "auto-1",
            "team_id": "team-1",
            "action_id": "act-1",
            "device_id": "dev-1",
            "enabled": "f",
            "trigger": r#"{"kind":"schedule","interval":"daily","minuteOfDay":420}"#,
            "sort_order": "2",
        }))
        .unwrap();
        let automation = from_row(&row);
        assert_eq!(automation.team_id, "team-1");
        assert!(!automation.enabled);
        assert_eq!(automation.trigger.as_ref().unwrap()["minuteOfDay"], 420);
        assert_eq!(automation.sort_order, 2.0);
        assert_eq!(automation.agent, None);
    }
}
