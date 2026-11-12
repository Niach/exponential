//! #EXP-1797: the team's issue trash, mirrored from
//! `packages/domain-contract/fixtures/issue-lifecycle.json` `trash` (TS
//! reference `issueTrashCaption` / `issueTrashActions` / `issueTrashRowText`
//! / `isIssuePurgeDue` in `src/issue-lifecycle.ts`) and locked against it by
//! the tests below. A delete moves an issue here; members restore it, owners
//! delete it forever, the watchdog's sweep purges it after the retention.

use crate::instant::parse_instant;

/// Days a trashed issue stays restorable.
pub const RETENTION_DAYS: i64 = 30;
const HOUR_MS: i64 = 60 * 60 * 1000;
const RETENTION_MS: i64 = RETENTION_DAYS * 24 * HOUR_MS;

/// The band's title on the Boards settings page.
pub const BAND: &str = "Deleted issues";
/// The row text template.
pub const ROW: &str = "{identifier} {title}";
/// The pill every member sees.
pub const RESTORE: &str = "Restore";
/// The owners' pill (asks `prompts::purge_issue` first).
pub const PURGE: &str = "Delete forever";

/// The role whose rows also carry Delete forever (`access.purge`).
pub const PURGE_ROLE: &str = "owner";

/// A trash row's pill, display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrashAction {
    Restore,
    Purge,
}

impl TrashAction {
    /// The fixture's key.
    pub fn key(self) -> &'static str {
        match self {
            TrashAction::Restore => "restore",
            TrashAction::Purge => "purge",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            TrashAction::Restore => RESTORE,
            TrashAction::Purge => PURGE,
        }
    }
}

/// The row caption: the purge countdown, in hours inside the last two days,
/// else in whole days (both rounded up); `Purging soon` once due, `Purges
/// soon` without (a readable) purge time. `now_ms` = UTC milliseconds.
pub fn trash_caption(purge_at: Option<&str>, now_ms: i64) -> String {
    let Some(at) = purge_at.and_then(parse_instant) else {
        return "Purges soon".into();
    };
    let ms = at - now_ms;
    if ms <= 0 {
        return "Purging soon".into();
    }
    if ms <= 48 * HOUR_MS {
        return format!("Purges in ~{}h", ceil_div(ms, HOUR_MS));
    }
    format!("Purges in ~{}d", ceil_div(ms, 24 * HOUR_MS))
}

/// The row's pills for a member of `role` (`owner` | `member`).
pub fn trash_actions(role: &str) -> &'static [TrashAction] {
    if role == PURGE_ROLE {
        &[TrashAction::Restore, TrashAction::Purge]
    } else {
        &[TrashAction::Restore]
    }
}

/// The row text: `{identifier} {title}`.
pub fn trash_row_text(identifier: &str, title: &str) -> String {
    crate::prompts::fill(ROW, &[("identifier", identifier), ("title", title)])
}

/// The sweep's rule: due once `deleted_at` + the retention has passed.
pub fn is_purge_due(deleted_at: Option<&str>, now_ms: i64) -> bool {
    deleted_at.and_then(parse_instant).is_some_and(|at| at + RETENTION_MS <= now_ms)
}

fn ceil_div(a: i64, b: i64) -> i64 {
    (a + b - 1) / b
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/issue-lifecycle.json");

    fn trash() -> Value {
        serde_json::from_str::<Value>(FIXTURE).unwrap()["trash"].clone()
    }

    fn now(case: &Value) -> i64 {
        parse_instant(case["input"]["now"].as_str().unwrap()).unwrap()
    }

    #[test]
    fn the_copy_and_retention_are_the_contracts() {
        let trash = trash();
        assert_eq!(trash["retentionDays"].as_i64(), Some(RETENTION_DAYS));
        assert_eq!(trash["copy"]["band"].as_str(), Some(BAND));
        assert_eq!(trash["copy"]["row"].as_str(), Some(ROW));
        assert_eq!(trash["copy"]["restore"].as_str(), Some(RESTORE));
        assert_eq!(trash["copy"]["purge"].as_str(), Some(PURGE));
        assert_eq!(trash["access"]["purge"].as_str(), Some(PURGE_ROLE));
    }

    #[test]
    fn caption_cases() {
        for case in trash()["captionCases"].as_array().unwrap() {
            let got = trash_caption(case["input"]["purgeAt"].as_str(), now(case));
            assert_eq!(got, case["expected"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn action_cases() {
        for case in trash()["actionCases"].as_array().unwrap() {
            let got: Vec<&str> =
                trash_actions(case["input"]["role"].as_str().unwrap()).iter().map(|a| a.key()).collect();
            let expected: Vec<&str> =
                case["expected"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
            assert_eq!(got, expected, "{}", case["name"]);
        }
    }

    #[test]
    fn purge_due_cases() {
        for case in trash()["purgeDueCases"].as_array().unwrap() {
            let got = is_purge_due(case["input"]["deletedAt"].as_str(), now(case));
            assert_eq!(got, case["expected"].as_bool().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn row_text_fills_both_params() {
        assert_eq!(trash_row_text("EXP-12", "Fix the {title} bug"), "EXP-12 Fix the {title} bug");
    }
}
