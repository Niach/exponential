//! `widgets.submissionForIssue` — the reporter/page/env metadata behind a
//! widget- or agent-filed issue (EXP-496; web's `widget-submission-card.tsx`
//! mirror). Server-only data (never an Electric shape): the row carries
//! reporter PII that is deliberately kept out of issue descriptions, so it is
//! fetched on demand, member-gated, and renders nothing on `null`/error.

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
