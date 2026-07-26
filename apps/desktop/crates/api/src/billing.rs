//! Typed `billing.teamPlan` tRPC helper (EXP-288 — the desktop's READ-ONLY
//! plan/usage summary in Settings → General; checkout/portal/seat changes
//! stay a browser hand-off to the web settings).
//!
//! Verified against `apps/web/src/lib/trpc/billing.ts` `teamPlan`:
//! `{plan, limits{seats,storageMb,widgetConfigs}, usage{members,storageMb,
//! widgetConfigs}, subscription|null}`. The responses are plain JSON (no
//! superjson), so the web's `Infinity` limits serialize as `null` —
//! modeled as `Option<f64>` (`None` = unlimited). `subscription` is
//! deliberately not modeled (read-only surface).

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanLimits {
    /// `None` = unlimited (`Infinity` → JSON `null`).
    #[serde(default)]
    pub seats: Option<f64>,
    #[serde(default)]
    pub storage_mb: Option<f64>,
    #[serde(default)]
    pub widget_configs: Option<f64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanUsage {
    #[serde(default)]
    pub members: f64,
    /// Fractional — the server rounds to 0.1 MB.
    #[serde(default)]
    pub storage_mb: f64,
    #[serde(default)]
    pub widget_configs: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamPlanOut {
    /// `free` / `pro` / `business` / `unlimited` (self-hosted — the UI hides
    /// the whole billing section then, like the web).
    pub plan: String,
    pub limits: PlanLimits,
    pub usage: PlanUsage,
}

/// `billing.teamPlan` — query (member-readable; the server checks access).
pub fn billing_team_plan(trpc: &TrpcClient, team_id: &str) -> Result<TeamPlanOut, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
    }
    trpc.query_with_input("billing.teamPlan", &Input { team_id })
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
    fn team_plan_gets_input_and_decodes_null_limits() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"plan":"business","limits":{"seats":25,"storageMb":10240,"widgetConfigs":null},"usage":{"members":3,"storageMb":120.4,"widgetConfigs":1},"subscription":{"productId":"prod_x","seats":25,"periodEnd":null,"cancelAtPeriodEnd":false}}}}"#,
        );
        let out = billing_team_plan(&client(&base), "w-1").unwrap();
        assert_eq!(out.plan, "business");
        assert_eq!(out.limits.seats, Some(25.));
        assert_eq!(out.limits.widget_configs, None, "Infinity → null → None");
        assert_eq!(out.usage.members, 3.);
        assert_eq!(out.usage.storage_mb, 120.4);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            request.contains("GET /api/trpc/billing.teamPlan?input="),
            "query, not mutation: {request}"
        );
        assert!(request.contains("%22teamId%22"), "encoded input: {request}");
    }

    #[test]
    fn team_plan_decodes_self_hosted_unlimited() {
        let (base, _captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"plan":"unlimited","limits":{"seats":null,"storageMb":null,"widgetConfigs":null},"usage":{"members":0,"storageMb":0,"widgetConfigs":0},"subscription":null}}}"#,
        );
        let out = billing_team_plan(&client(&base), "w-1").unwrap();
        assert_eq!(out.plan, "unlimited");
        assert_eq!(out.limits.seats, None);
    }
}
