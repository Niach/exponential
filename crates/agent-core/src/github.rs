//! Thin GitHub REST wrapper — a port of the bits of `github-api.ts` the loop
//! uses: `get_repo` (default branch), `create_pull_request`, `get_pull_request`
//! (for pr_poll), and `parse_pr_url`. Blocking `ureq`; OAuth user token bearer.

use serde_json::{json, Value};
use std::time::Duration;

const BASE: &str = "https://api.github.com";

fn gh(token: &str, method: &str, path: &str, body: Option<&Value>, timeout_s: u64) -> Result<Value, String> {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(timeout_s)).build();
    let url = format!("{BASE}{path}");
    let mut req = agent
        .request(method, &url)
        .set("accept", "application/vnd.github+json")
        .set("authorization", &format!("Bearer {token}"))
        .set("x-github-api-version", "2022-11-28")
        .set("user-agent", "exponential-agent-core");
    let result = match body {
        Some(b) => {
            req = req.set("content-type", "application/json");
            req.send_json(b)
        }
        None => req.call(),
    };
    let resp = match result {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            let text = r.into_string().unwrap_or_default();
            return Err(format!("GitHub {method} {path} failed: {code} {}", &text[..text.len().min(300)]));
        }
        Err(e) => return Err(format!("GitHub request failed: {e}")),
    };
    resp.into_json().map_err(|e| format!("GitHub parse: {e}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoMinimal {
    pub full_name: String,
    pub default_branch: String,
    pub private: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullFile {
    pub filename: String,
    pub status: String,
    pub additions: i64,
    pub deletions: i64,
    pub patch: Option<String>,
}

/// Changed files for a PR (for a local/desktop diff view). The web diff uses the
/// server-side `issues.prFiles` endpoint; this exists for completeness.
pub fn list_pull_files(token: &str, owner: &str, repo: &str, number: i64, timeout_s: u64) -> Result<Vec<PullFile>, String> {
    let raw = gh(token, "GET", &format!("/repos/{owner}/{repo}/pulls/{number}/files?per_page=100"), None, timeout_s)?;
    let arr = raw.as_array().ok_or("GitHub: expected an array of files")?;
    Ok(arr
        .iter()
        .map(|f| PullFile {
            filename: f.get("filename").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            status: f.get("status").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            additions: f.get("additions").and_then(|v| v.as_i64()).unwrap_or(0),
            deletions: f.get("deletions").and_then(|v| v.as_i64()).unwrap_or(0),
            patch: f.get("patch").and_then(|v| v.as_str()).map(|s| s.to_string()),
        })
        .collect())
}

pub fn get_repo(token: &str, owner_repo: &str, timeout_s: u64) -> Result<RepoMinimal, String> {
    let raw = gh(token, "GET", &format!("/repos/{owner_repo}"), None, timeout_s)?;
    Ok(RepoMinimal {
        full_name: raw.get("full_name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        default_branch: raw.get("default_branch").and_then(|v| v.as_str()).unwrap_or("main").to_string(),
        private: raw.get("private").and_then(|v| v.as_bool()).unwrap_or(false),
    })
}

/// Returns (html_url, number).
pub fn create_pull_request(
    token: &str,
    owner: &str,
    repo: &str,
    head: &str,
    base: &str,
    title: &str,
    body: &str,
    timeout_s: u64,
) -> Result<(String, i64), String> {
    let payload = json!({ "title": title, "head": head, "base": base, "body": body, "draft": false });
    let raw = gh(token, "POST", &format!("/repos/{owner}/{repo}/pulls"), Some(&payload), timeout_s)?;
    let url = raw.get("html_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let number = raw.get("number").and_then(|v| v.as_i64()).unwrap_or(0);
    Ok((url, number))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    pub number: i64,
    pub state: String, // "open" | "closed"
    pub merged: bool,
    pub merged_at: Option<String>,
    pub closed_at: Option<String>,
}

pub fn get_pull_request(token: &str, owner: &str, repo: &str, number: i64, timeout_s: u64) -> Result<PullRequest, String> {
    let raw = gh(token, "GET", &format!("/repos/{owner}/{repo}/pulls/{number}"), None, timeout_s)?;
    Ok(PullRequest {
        number: raw.get("number").and_then(|v| v.as_i64()).unwrap_or(number),
        state: raw.get("state").and_then(|v| v.as_str()).unwrap_or("open").to_string(),
        merged: raw.get("merged").and_then(|v| v.as_bool()).unwrap_or(false),
        merged_at: raw.get("merged_at").and_then(|v| v.as_str()).map(|s| s.to_string()),
        closed_at: raw.get("closed_at").and_then(|v| v.as_str()).map(|s| s.to_string()),
    })
}

/// Parse `https://github.com/owner/repo/pull/123` (or `/pulls/123`).
pub fn parse_pr_url(url: &str) -> Option<(String, String, i64)> {
    let rest = url.strip_prefix("https://github.com/")?;
    let parts: Vec<&str> = rest.split('/').collect();
    // owner / repo / "pull"|"pulls" / number[/...]
    if parts.len() < 4 {
        return None;
    }
    let (owner, repo, kind, num) = (parts[0], parts[1], parts[2], parts[3]);
    if owner.is_empty() || repo.is_empty() || (kind != "pull" && kind != "pulls") {
        return None;
    }
    let number: i64 = num.parse().ok()?;
    Some((owner.to_string(), repo.to_string(), number))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pr_url_variants() {
        assert_eq!(parse_pr_url("https://github.com/Niach/exponential/pull/42"), Some(("Niach".into(), "exponential".into(), 42)));
        assert_eq!(parse_pr_url("https://github.com/o/r/pulls/7"), Some(("o".into(), "r".into(), 7)));
        assert_eq!(parse_pr_url("https://github.com/o/r/pull/9/files"), Some(("o".into(), "r".into(), 9)));
        assert!(parse_pr_url("https://github.com/o/r/issues/3").is_none());
        assert!(parse_pr_url("https://gitlab.com/o/r/pull/1").is_none());
        assert!(parse_pr_url("https://github.com/o/r/pull/notanumber").is_none());
    }
}
