//! PR reconcile loop — a port of `pr-poll-loop.ts`. Every 60s it polls the
//! GitHub PRs for issues in `in_review`/`pushed`: a merged PR → clean the
//! worktree + mark the issue done; a PR closed-unmerged for >14 days → cancel.
//! Threaded with a cancellable sleep (no tokio).

use crate::github::{self, parse_pr_url};
use crate::run_pipeline::Config;
use crate::state::State;
use crate::{git, mcp};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TICK_SECS: u64 = 60;
const ABANDON_AFTER_SECS: i64 = 14 * 24 * 60 * 60;

pub fn run_loop(config: &Config, state: &Arc<Mutex<State>>, stop: &Arc<AtomicBool>) {
    while !stop.load(Ordering::Acquire) {
        tick(config, state);
        sleep_cancellable(TICK_SECS * 1000, stop);
    }
}

fn tick(config: &Config, state: &Arc<Mutex<State>>) {
    if config.github_token.is_empty() {
        return;
    }
    let pending = {
        let s = state.lock().unwrap();
        s.list_issues(&["in_review", "pushed"]).unwrap_or_default()
    };
    for issue in pending {
        let Some(pr_url) = issue.pr_url.as_deref() else { continue };
        let Some((owner, repo, number)) = parse_pr_url(pr_url) else { continue };
        let pr = match github::get_pull_request(&config.github_token, &owner, &repo, number, config.timeout_s) {
            Ok(pr) => pr,
            Err(_) => continue, // transient; retry next tick
        };
        if pr.merged {
            react_closed(config, state, &issue, true);
        } else if pr.state == "closed" {
            let closed = pr.closed_at.as_deref().and_then(parse_iso_epoch_secs).unwrap_or_else(now_secs);
            if now_secs() - closed > ABANDON_AFTER_SECS {
                react_closed(config, state, &issue, false);
            }
        }
    }
}

fn react_closed(config: &Config, state: &Arc<Mutex<State>>, issue: &crate::state::IssueRow, merged: bool) {
    if let (Some(wt), Some(branch), Some(repo_path)) = (&issue.worktree_path, &issue.branch, &issue.repo_path) {
        git::worktree_cleanup(
            &config.branch_prefix,
            &git::WorktreeClaim {
                worktree_path: wt.clone(),
                branch: branch.clone(),
                repo_path: repo_path.clone(),
                default_branch: String::new(),
            },
        );
    }
    let final_status = if merged { "done" } else { "cancelled" };
    let _ = state.lock().unwrap().set_issue_status(&issue.id, final_status, None);
    let _ = mcp::update_issue_status(&config.base_url, &config.api_key, &issue.id, final_status, config.timeout_s);
}

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Parse an ISO-8601 UTC timestamp to epoch seconds (date + optional time).
fn parse_iso_epoch_secs(s: &str) -> Option<i64> {
    if s.len() < 10 {
        return None;
    }
    let y: i64 = s.get(0..4)?.parse().ok()?;
    let m: i64 = s.get(5..7)?.parse().ok()?;
    let d: i64 = s.get(8..10)?.parse().ok()?;
    let (mut h, mut mi, mut se) = (0i64, 0i64, 0i64);
    if s.len() >= 19 && s.as_bytes()[10] == b'T' {
        h = s.get(11..13)?.parse().unwrap_or(0);
        mi = s.get(14..16)?.parse().unwrap_or(0);
        se = s.get(17..19)?.parse().unwrap_or(0);
    }
    Some(days_from_civil(y, m, d) * 86400 + h * 3600 + mi * 60 + se)
}

/// Days since 1970-01-01 (Howard Hinnant's days_from_civil).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let yy = if m <= 2 { y - 1 } else { y };
    let era = (if yy >= 0 { yy } else { yy - 399 }).div_euclid(400);
    let yoe = yy - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn sleep_cancellable(ms: u64, stop: &Arc<AtomicBool>) {
    let mut left = ms;
    while left > 0 && !stop.load(Ordering::Acquire) {
        let chunk = left.min(200);
        std::thread::sleep(Duration::from_millis(chunk));
        left -= chunk;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_anchors() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(parse_iso_epoch_secs("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso_epoch_secs("1970-01-02"), Some(86400));
    }

    #[test]
    fn parses_time_component() {
        assert_eq!(parse_iso_epoch_secs("2026-06-01T01:02:03Z"), Some(days_from_civil(2026, 6, 1) * 86400 + 3723));
        assert!(parse_iso_epoch_secs("nope").is_none());
    }
}
