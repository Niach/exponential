//! #EXP-1611 — CI on the chat's PR, the UI ×4 (`packages/domain-contract/
//! fixtures/pr-checks.json` CHECKS LIST, TS reference
//! `packages/domain-contract/src/pr-checks.ts`): the `…` menu's Checks
//! submenu ([`checks_summary`] for its entry, [`checks_list`] for its rows;
//! the header pill = merge-train.json `statusPill`). Pure: the IDE feeds it
//! the synced `pull_request_checks` rows.

use crate::contract;
use crate::rows::PullRequestCheckRow;

/// `copy.checks`: the `…` menu entry's label.
pub const CHECKS_LABEL: &str = "Checks";
/// `copy.summary.*`: the entry's value per aggregate.
pub const SUMMARY_FAILING: &str = "{count} failing";
pub const SUMMARY_RUNNING: &str = "{count} running";
pub const SUMMARY_PASSED: &str = "{count} passed";
/// `testIds`.
pub const TEST_ID_CHECKS_MENU: &str = "pr-checks-menu";
pub const TEST_ID_CHECK_ROW: &str = "pr-check-row";

/// One check row's glyph (icons.json concepts).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CheckGlyph {
    // Declaration order IS the list order (failed, running, queued, passed,
    // skipped).
    Failed,
    Running,
    Queued,
    Passed,
    Skipped,
}

impl CheckGlyph {
    /// The icons.json concept id.
    pub fn concept(self) -> &'static str {
        match self {
            Self::Failed => "ci-failed",
            Self::Running => "ci-running",
            Self::Queued => "ci-queued",
            Self::Passed => "ci-passed",
            Self::Skipped => "ci-skipped",
        }
    }
}

/// A glyph's tint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckTone {
    Muted,
    Green,
    Red,
}

impl CheckTone {
    pub fn id(self) -> &'static str {
        match self {
            Self::Muted => "muted",
            Self::Green => "green",
            Self::Red => "red",
        }
    }
}

/// A completed check whose conclusion is red (contract
/// `prChecks.failingConclusions`).
pub fn is_failing(status: &str, conclusion: Option<&str>) -> bool {
    status == contract::PR_CHECK_STATUS_COMPLETED
        && conclusion.is_some_and(|c| contract::PR_CHECKS_FAILING_CONCLUSIONS.contains(&c))
}

fn row_failing(row: &PullRequestCheckRow) -> bool {
    is_failing(&row.status, row.conclusion.as_deref())
}

/// The aggregate (contract `prChecksState`): none · failure (wins) ·
/// pending · success.
pub fn aggregate_state(rows: &[PullRequestCheckRow]) -> &'static str {
    if rows.is_empty() {
        contract::PR_CHECKS_STATE_NONE
    } else if rows.iter().any(row_failing) {
        contract::PR_CHECKS_STATE_FAILURE
    } else if rows.iter().any(|row| row.status != contract::PR_CHECK_STATUS_COMPLETED) {
        contract::PR_CHECKS_STATE_PENDING
    } else {
        contract::PR_CHECKS_STATE_SUCCESS
    }
}

/// `formatDuration`: `42s` · `3m 5s` · `1h 2m` (floored; negative = 0s).
pub fn format_check_duration(ms: i64) -> String {
    let seconds = ms.max(0) / 1000;
    if seconds < 60 {
        return format!("{seconds}s");
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m {}s", seconds % 60);
    }
    format!("{}h {}m", minutes / 60, minutes % 60)
}

fn ms_of(value: Option<&str>) -> Option<i64> {
    value.filter(|v| !v.is_empty()).and_then(crate::instant::parse_instant)
}

/// `checkDuration`: completed = completed - started, in_progress = now -
/// started, else none (queued, or no start). `now_ms` = UTC milliseconds.
pub fn check_duration(row: &PullRequestCheckRow, now_ms: i64) -> Option<String> {
    check_duration_of(&row.status, row.started_at.as_deref(), row.completed_at.as_deref(), now_ms)
}

/// [`check_duration`] over the bare fields (#EXP-1804: a CI job's row).
pub fn check_duration_of(status: &str, started_at: Option<&str>, completed_at: Option<&str>, now_ms: i64) -> Option<String> {
    let started = ms_of(started_at)?;
    match status {
        contract::PR_CHECK_STATUS_COMPLETED => ms_of(completed_at).map(|completed| format_check_duration(completed - started)),
        contract::PR_CHECK_STATUS_IN_PROGRESS => Some(format_check_duration(now_ms - started)),
        _ => None,
    }
}

/// `checkGlyph`.
pub fn check_glyph(status: &str, conclusion: Option<&str>) -> (CheckGlyph, CheckTone) {
    if status == contract::PR_CHECK_STATUS_QUEUED {
        (CheckGlyph::Queued, CheckTone::Muted)
    } else if status != contract::PR_CHECK_STATUS_COMPLETED {
        (CheckGlyph::Running, CheckTone::Muted)
    } else if is_failing(status, conclusion) {
        (CheckGlyph::Failed, CheckTone::Red)
    } else if conclusion == Some(contract::PR_CHECK_CONCLUSION_SUCCESS) {
        (CheckGlyph::Passed, CheckTone::Green)
    } else {
        (CheckGlyph::Skipped, CheckTone::Muted)
    }
}

/// One row of the Checks submenu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckListRow {
    pub name: String,
    pub glyph: CheckGlyph,
    pub tone: CheckTone,
    pub duration: Option<String>,
    /// `details_url` (the logs); none = the row is disabled.
    pub url: Option<String>,
}

/// `checksList`: failed, running, queued, passed, skipped; ties by name
/// (code-point order, Rust's byte order on UTF-8).
pub fn checks_list(rows: &[PullRequestCheckRow], now_ms: i64) -> Vec<CheckListRow> {
    let mut list: Vec<CheckListRow> = rows
        .iter()
        .map(|row| {
            let (glyph, tone) = check_glyph(&row.status, row.conclusion.as_deref());
            CheckListRow {
                name: row.name.clone(),
                glyph,
                tone,
                duration: check_duration(row, now_ms),
                url: row.details_url.clone().filter(|url| !url.is_empty()),
            }
        })
        .collect();
    list.sort_by(|a, b| a.glyph.cmp(&b.glyph).then_with(|| a.name.cmp(&b.name)));
    list
}

/// The Checks entry's glyph, tone and value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChecksSummary {
    pub glyph: CheckGlyph,
    pub tone: CheckTone,
    pub value: String,
}

/// `checksSummary`: none = no rows (no entry).
pub fn checks_summary(rows: &[PullRequestCheckRow]) -> Option<ChecksSummary> {
    let fill = |template: &str, count: usize| template.replace("{count}", &count.to_string());
    match aggregate_state(rows) {
        contract::PR_CHECKS_STATE_FAILURE => Some(ChecksSummary {
            glyph: CheckGlyph::Failed,
            tone: CheckTone::Red,
            value: fill(SUMMARY_FAILING, rows.iter().filter(|row| row_failing(row)).count()),
        }),
        contract::PR_CHECKS_STATE_PENDING => Some(ChecksSummary {
            glyph: CheckGlyph::Running,
            tone: CheckTone::Muted,
            value: fill(
                SUMMARY_RUNNING,
                rows.iter().filter(|row| row.status != contract::PR_CHECK_STATUS_COMPLETED).count(),
            ),
        }),
        contract::PR_CHECKS_STATE_SUCCESS => Some(ChecksSummary {
            glyph: CheckGlyph::Passed,
            tone: CheckTone::Green,
            value: fill(SUMMARY_PASSED, rows.len()),
        }),
        _ => None,
    }
}

/// The fixture's camelCase checks (`specimen.checks`, the cases' `checks`)
/// as synced rows: the styleguide specimens ×4 draw from them.
pub fn fixture_checks(checks: &serde_json::Value) -> Vec<PullRequestCheckRow> {
    checks
        .as_array()
        .map(|checks| {
            checks
                .iter()
                .map(|check| {
                    let text = |key: &str| check[key].as_str().map(str::to_string);
                    let name = text("name").unwrap_or_default();
                    PullRequestCheckRow {
                        id: name.clone(),
                        team_id: None,
                        pull_request_id: String::new(),
                        head_sha: None,
                        name,
                        status: text("status").unwrap_or_default(),
                        conclusion: text("conclusion"),
                        required: check["required"].as_bool().unwrap_or(false),
                        started_at: text("startedAt"),
                        completed_at: text("completedAt"),
                        details_url: text("detailsUrl"),
                        created_at: None,
                        updated_at: None,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/pr-checks.json");

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE).expect("pr-checks.json parses")
    }

    fn rows(checks: &Value) -> Vec<PullRequestCheckRow> {
        fixture_checks(checks)
    }

    fn now(case: &Value) -> i64 {
        crate::instant::parse_instant(case["now"].as_str().unwrap()).unwrap()
    }

    #[test]
    fn the_copy_and_test_ids_are_the_fixtures() {
        let fixture = fixture();
        assert_eq!(fixture["copy"]["checks"], CHECKS_LABEL);
        assert_eq!(fixture["copy"]["summary"]["failing"], SUMMARY_FAILING);
        assert_eq!(fixture["copy"]["summary"]["running"], SUMMARY_RUNNING);
        assert_eq!(fixture["copy"]["summary"]["passed"], SUMMARY_PASSED);
        assert_eq!(fixture["testIds"]["checksMenu"], TEST_ID_CHECKS_MENU);
        assert_eq!(fixture["testIds"]["checkRow"], TEST_ID_CHECK_ROW);
    }

    #[test]
    fn format_duration_matches_the_fixture() {
        for case in fixture()["formatDuration"].as_array().unwrap() {
            let ms = case["ms"].as_i64().unwrap();
            assert_eq!(format_check_duration(ms), case["expected"].as_str().unwrap(), "{ms}");
        }
    }

    #[test]
    fn checks_list_matches_the_fixture() {
        let cases = fixture()["checksList"].as_array().unwrap().clone();
        assert!(!cases.is_empty());
        for case in cases {
            let list = checks_list(&rows(&case["checks"]), now(&case));
            let got: Vec<Value> = list
                .iter()
                .map(|row| {
                    serde_json::json!({
                        "name": row.name,
                        "glyph": row.glyph.concept(),
                        "tone": row.tone.id(),
                        "duration": row.duration,
                        "url": row.url,
                    })
                })
                .collect();
            assert_eq!(Value::Array(got), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn the_specimen_is_the_first_checks_list_case() {
        let fixture = fixture();
        assert_eq!(fixture["specimen"]["checks"], fixture["checksList"][0]["checks"]);
        assert_eq!(fixture["specimen"]["now"], fixture["checksList"][0]["now"]);
    }

    #[test]
    fn checks_summary_matches_the_fixture() {
        for case in fixture()["checksSummary"].as_array().unwrap() {
            let got = checks_summary(&rows(&case["checks"])).map(|summary| {
                serde_json::json!({
                    "glyph": summary.glyph.concept(),
                    "tone": summary.tone.id(),
                    "value": summary.value,
                })
            });
            assert_eq!(got.unwrap_or(Value::Null), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn the_aggregate_matches_the_fixture() {
        let fixture = fixture();
        let Some(cases) = fixture["aggregate"].as_array() else {
            return;
        };
        for case in cases {
            let Some(checks) = case.get("checks") else {
                continue;
            };
            assert_eq!(
                aggregate_state(&rows(checks)),
                case["expected"].as_str().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
}
