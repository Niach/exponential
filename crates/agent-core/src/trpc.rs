//! tRPC-over-HTTP client for the `companion.*` lifecycle calls — a port of
//! `apps/companion/src/exponential-api.ts`. Blocking `ureq` (threaded model, no
//! tokio); plain JSON body, `{result:{data}}` / `{error:{message}}` envelope (no
//! transformer); auth via `Authorization: Bearer <expk_ key>`.

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;

pub fn endpoint(base_url: &str, path: &str) -> String {
    format!("{}/api/trpc/{}", base_url.trim_end_matches('/'), path)
}

/// Pure envelope decode: `result.data`, or the server's `error.message`.
pub fn parse_envelope(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    if let Some(msg) = payload
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
    {
        return Err(msg.to_string());
    }
    payload
        .get("result")
        .and_then(|r| r.get("data"))
        .cloned()
        .ok_or_else(|| "invalid tRPC response".to_string())
}

/// POST a tRPC mutation. `input == None` sends no body (matches the daemon's
/// `body: undefined` for no-input procedures). Returns the `result.data` subtree.
pub fn call(
    base_url: &str,
    path: &str,
    input: Option<&serde_json::Value>,
    token: Option<&str>,
    timeout_s: u64,
) -> Result<serde_json::Value, String> {
    let url = endpoint(base_url, path);
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(timeout_s))
        .build();
    let mut req = agent.post(&url).set("content-type", "application/json");
    if let Some(t) = token {
        req = req.set("authorization", &format!("Bearer {t}"));
    }
    let result = match input {
        Some(v) => req.send_json(v),
        None => req.send_string(""),
    };
    let resp = match result {
        Ok(r) => r,
        // A tRPC error still returns a JSON body (4xx) we want the message from.
        Err(ureq::Error::Status(_code, r)) => r,
        Err(e) => return Err(format!("request failed: {e}")),
    };
    let payload: serde_json::Value = resp.into_json().map_err(|e| format!("parse: {e}"))?;
    parse_envelope(&payload)
}

// --- companion.* lifecycle calls (Bearer = the agent's expk_ key) ---

pub fn heartbeat(base_url: &str, api_key: &str, timeout_s: u64) -> Result<(), String> {
    call(base_url, "companion.heartbeat", None, Some(api_key), timeout_s).map(|_| ())
}

pub fn uninstall_self(base_url: &str, api_key: &str, timeout_s: u64) -> Result<(), String> {
    call(base_url, "companion.uninstallSelf", None, Some(api_key), timeout_s).map(|_| ())
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct PollActivityIssue {
    pub id: String,
    pub identifier: String,
    pub title: String,
    #[serde(rename = "projectId")]
    pub project_id: String,
    #[serde(rename = "assigneeId")]
    pub assignee_id: Option<String>,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

/// The 5s control poll (companion parity / proxy resilience). Returns the next
/// activity cursor + the issues that changed since `cursor`.
pub fn poll_control(
    base_url: &str,
    api_key: &str,
    cursor: Option<&str>,
    timeout_s: u64,
) -> Result<(String, Vec<PollActivityIssue>), String> {
    let input = match cursor {
        Some(c) => json!({ "activityCursor": c }),
        None => json!({}),
    };
    let data = call(base_url, "companion.pollControl", Some(&input), Some(api_key), timeout_s)?;
    parse_control(&data)
}

fn parse_control(data: &serde_json::Value) -> Result<(String, Vec<PollActivityIssue>), String> {
    let activity = data.get("activity").ok_or("pollControl: missing activity")?;
    let cursor = activity
        .get("cursor")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();
    let issues = activity.get("issues").cloned().unwrap_or_else(|| json!([]));
    let issues: Vec<PollActivityIssue> =
        serde_json::from_value(issues).map_err(|e| format!("pollControl issues: {e}"))?;
    Ok((cursor, issues))
}

#[derive(Debug, Clone, Serialize)]
pub struct GithubRepo {
    #[serde(rename = "fullName")]
    pub full_name: String,
    #[serde(rename = "defaultBranch")]
    pub default_branch: String,
    pub private: bool,
}

pub fn report_github_identity(
    base_url: &str,
    api_key: &str,
    login: &str,
    repos: &[GithubRepo],
    timeout_s: u64,
) -> Result<(), String> {
    let input = json!({ "login": login, "repos": repos });
    call(base_url, "companion.reportGithubIdentity", Some(&input), Some(api_key), timeout_s).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_trims_trailing_slash() {
        assert_eq!(endpoint("https://x.at/", "companion.heartbeat"), "https://x.at/api/trpc/companion.heartbeat");
        assert_eq!(endpoint("https://x.at", "a.b"), "https://x.at/api/trpc/a.b");
    }

    #[test]
    fn envelope_extracts_data() {
        let p = json!({ "result": { "data": { "ok": true, "lastSeenAt": "t" } } });
        let d = parse_envelope(&p).unwrap();
        assert_eq!(d.get("ok").unwrap(), &json!(true));
    }

    #[test]
    fn envelope_surfaces_error_message() {
        let p = json!({ "error": { "message": "Invalid setup token" } });
        assert_eq!(parse_envelope(&p).unwrap_err(), "Invalid setup token");
    }

    #[test]
    fn envelope_rejects_garbage() {
        assert!(parse_envelope(&json!({ "nope": 1 })).is_err());
    }

    #[test]
    fn parse_control_extracts_cursor_and_issues() {
        let data = json!({
            "activity": {
                "cursor": "2026-06-01T00:00:00Z",
                "issues": [
                    { "id": "i1", "identifier": "EXP-1", "title": "t", "projectId": "p", "assigneeId": "u1", "updatedAt": "2026-06-01T00:00:00Z" },
                    { "id": "i2", "identifier": "EXP-2", "title": "t2", "projectId": "p", "assigneeId": null, "updatedAt": "2026-06-01T00:00:01Z" }
                ]
            }
        });
        let (cursor, issues) = parse_control(&data).unwrap();
        assert_eq!(cursor, "2026-06-01T00:00:00Z");
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].id, "i1");
        assert_eq!(issues[0].assignee_id.as_deref(), Some("u1"));
        assert!(issues[1].assignee_id.is_none());
    }

    #[test]
    fn github_repo_serializes_camelcase() {
        let r = GithubRepo { full_name: "o/r".into(), default_branch: "main".into(), private: false };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v.get("fullName").unwrap(), &json!("o/r"));
        assert_eq!(v.get("defaultBranch").unwrap(), &json!("main"));
    }
}
