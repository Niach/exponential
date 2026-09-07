//! `widgets.submissionForIssue` — the reporter/page/env metadata behind a
//! widget- or agent-filed issue (EXP-496; web's `widget-submission-card.tsx`
//! mirror). Server-only data (never an Electric shape): the row carries
//! reporter PII that is deliberately kept out of issue descriptions, so it is
//! fetched on demand, member-gated, and renders nothing on `null`/error.
//!
//! Plus `widgets.list` (EXP-548) — the owner-only config list: an existence
//! signal for the getting-started checklist, and since EXP-771 the rows the
//! read-only Settings → Feedback widget pane draws.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// The `widget_submissions` row for an issue, or `None` for non-widget issues.
/// Every field is optional — the wire row is the full drizzle row and this
/// decodes only what the metadata card renders; unknown/new columns must never
/// fail the decode.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WidgetSubmission {
    pub reporter_email: Option<String>,
    pub reporter_name: Option<String>,
    pub page_url: Option<String>,
    pub user_agent: Option<String>,
    pub viewport_width: Option<i64>,
    pub viewport_height: Option<i64>,
    pub screen_width: Option<i64>,
    pub screen_height: Option<i64>,
    pub device_pixel_ratio: Option<f64>,
    /// Free-form blob (`identify()` custom data; `{"via":"mcp"}` for agent
    /// bug reports). Rendered pretty-printed.
    pub custom_data: Option<serde_json::Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SubmissionForIssueInput<'a> {
    issue_id: &'a str,
}

/// Fetch the widget submission metadata for an issue
/// (`widgets.submissionForIssue`). `Ok(None)` when the issue was not filed
/// through the widget/MCP intake. Blocking — call from a background executor,
/// never the foreground (§3.5).
pub fn submission_for_issue(
    client: &TrpcClient,
    issue_id: &str,
) -> Result<Option<WidgetSubmission>, ApiError> {
    client.query_with_input("widgets.submissionForIssue", &SubmissionForIssueInput { issue_id })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SubmissionForThreadInput<'a> {
    thread_id: &'a str,
}

/// Fetch the widget submission metadata behind a SUPPORT thread
/// (`widgets.submissionForThread`, EXP-525 — the web details rail's Context
/// section). `Ok(None)` for threads without a widget submission. Blocking —
/// background executor only (§3.5).
pub fn submission_for_thread(
    client: &TrpcClient,
    thread_id: &str,
) -> Result<Option<WidgetSubmission>, ApiError> {
    client.query_with_input(
        "widgets.submissionForThread",
        &SubmissionForThreadInput { thread_id },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trpc::tests::one_shot_server;
    use crate::StaticToken;
    use std::sync::Arc;
    use std::time::Duration;

    fn client(base: &str) -> TrpcClient {
        TrpcClient::new(base, Arc::new(StaticToken("tok-1".to_string())))
    }

    #[test]
    fn decodes_submission_and_sends_camel_case_input() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{
                "id":"sub-1","widgetConfigId":"cfg-1","issueId":"iss-1",
                "reporterEmail":"dev@example.com","reporterName":"Dev",
                "pageUrl":"https://app.example.com/t/acme","userAgent":"Mozilla/5.0",
                "viewportWidth":1512,"viewportHeight":982,
                "screenWidth":1512,"screenHeight":982,"devicePixelRatio":2,
                "customData":{"via":"mcp"},
                "createdAt":"2026-08-14T00:00:00.000Z"
            }}}"#,
        );
        let out = submission_for_issue(&client(&base), "1f7f6f9e-0000-4000-8000-000000000000")
            .unwrap()
            .unwrap();
        assert_eq!(out.reporter_email.as_deref(), Some("dev@example.com"));
        assert_eq!(out.reporter_name.as_deref(), Some("Dev"));
        assert_eq!(out.viewport_width, Some(1512));
        assert_eq!(out.device_pixel_ratio, Some(2.0));
        assert_eq!(
            out.custom_data,
            Some(serde_json::json!({"via":"mcp"}))
        );

        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/widgets.submissionForIssue?input="));
        assert!(request.contains("%22issueId%22"));
        assert!(crate::trpc::tests::has_header(&request, "Authorization: Bearer tok-1"));
    }

    #[test]
    fn decodes_null_as_none() {
        let (base, _captured) = one_shot_server(200, r#"{"result":{"data":null}}"#);
        let out = submission_for_issue(&client(&base), "1f7f6f9e-0000-4000-8000-000000000000")
            .unwrap();
        assert_eq!(out, None);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListInput<'a> {
    team_id: &'a str,
}

/// One `widgets.list` row, decoded down to what its two consumers render: the
/// getting-started checklist only asks "does the team have a widget?" (web
/// parity: `widgets.list` non-empty), and the EXP-771 settings pane draws one
/// READ-ONLY row per config. Everything past the id is optional — the wire row
/// is the full drizzle select, and a new column must never fail the decode.
/// The public key, the domain allowlist and the form config stay OUT on
/// purpose: the desktop pane hands authoring off to the web.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WidgetConfigSummary {
    pub id: String,
    pub name: Option<String>,
    /// `false` draws the "disabled" pill (web `!widget.enabled`). Absent reads
    /// as ENABLED — the pane must never accuse a live widget of being off.
    pub enabled: Option<bool>,
    /// `count(widget_submissions.id)` for this config.
    pub submission_count: Option<i64>,
}

/// `widgets.list` — OWNER-only on the server (it exposes public keys and
/// submission counts); callers must gate on the owner role, a member call is
/// a FORBIDDEN error, never an empty list. Blocking — background executor
/// only.
pub fn list(client: &TrpcClient, team_id: &str) -> Result<Vec<WidgetConfigSummary>, ApiError> {
    client.query_with_input("widgets.list", &ListInput { team_id })
}

#[cfg(test)]
mod list_tests {
    use super::*;
    use crate::trpc::tests::one_shot_server;
    use crate::StaticToken;
    use std::sync::Arc;

    /// EXP-771: the settings pane reads `enabled` + `submissionCount` off the
    /// list, and the columns it does NOT model (publicKey, allowedDomains,
    /// formConfig, boardName) must not break the decode.
    #[test]
    fn decodes_the_pane_fields_and_ignores_the_rest() {
        let (base, _captured) = one_shot_server(
            200,
            r#"{"result":{"data":[
                {"id":"cfg-1","name":"Docs site","publicKey":"expw_abc",
                 "boardId":"b-1","boardName":"Bugs","allowedDomains":["example.com"],
                 "enabled":false,"formConfig":{"modes":["feedback"]},
                 "createdAt":"2026-09-01T00:00:00.000Z","submissionCount":12},
                {"id":"cfg-2"}
            ]}}"#,
        );
        let client = TrpcClient::new(&base, Arc::new(StaticToken("tok-1".to_string())));
        let rows = list(&client, "1f7f6f9e-0000-4000-8000-000000000000").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name.as_deref(), Some("Docs site"));
        assert_eq!(rows[0].enabled, Some(false));
        assert_eq!(rows[0].submission_count, Some(12));
        // A row with nothing but an id decodes, and reads as enabled.
        assert_eq!(rows[1].enabled, None);
        assert_eq!(rows[1].submission_count, None);
    }
}
