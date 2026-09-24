//! EXP-630 — story points, rendered on the TEAM's scale
//! (`teams.estimation_type`, contract `issueEstimation`). The stored value is
//! always a point number, so a team can switch scales without touching a
//! single issue; `none` hides the chip everywhere. T-shirt sizes are the
//! fibonacci points worn as XS…XL, exactly how Linear stores them.
//!
//! Web `apps/web/src/lib/issue-estimate.ts`'s twin, byte-locked ×4 by
//! `packages/domain-contract/fixtures/issue-estimate.json` — same cases,
//! same test names (iOS `IssueEstimateTests`, Android `IssueEstimateTest`).

use crate::contract::{
    ISSUE_ESTIMATION_EXPONENTIAL, ISSUE_ESTIMATION_FIBONACCI, ISSUE_ESTIMATION_LINEAR,
    ISSUE_ESTIMATION_TSHIRT,
};

/// The ladders per scale (the fixture's `scales`).
pub const EXPONENTIAL_SCALE: &[i64] = &[1, 2, 4, 8, 16];
pub const FIBONACCI_SCALE: &[i64] = &[1, 2, 3, 5, 8];
pub const LINEAR_SCALE: &[i64] = &[1, 2, 3, 4, 5];
/// T-shirt = the fibonacci points, labelled by [`TSHIRT_LABELS`] by index.
pub const TSHIRT_SCALE: &[i64] = FIBONACCI_SCALE;
pub const TSHIRT_LABELS: &[&str] = &["XS", "S", "M", "L", "XL"];

/// The unset label — the picker's first row and the long label of `None`.
pub const NO_ESTIMATE: &str = "No estimate";

/// The ladder of a scale; `none` and an unknown scale offer nothing.
pub fn estimation_scale(scale: &str) -> &'static [i64] {
    match scale {
        s if s == ISSUE_ESTIMATION_EXPONENTIAL => EXPONENTIAL_SCALE,
        s if s == ISSUE_ESTIMATION_FIBONACCI => FIBONACCI_SCALE,
        s if s == ISSUE_ESTIMATION_LINEAR => LINEAR_SCALE,
        s if s == ISSUE_ESTIMATION_TSHIRT => TSHIRT_SCALE,
        _ => &[],
    }
}

fn tshirt_label(value: i64) -> Option<&'static str> {
    TSHIRT_SCALE
        .iter()
        .position(|point| *point == value)
        .and_then(|index| TSHIRT_LABELS.get(index).copied())
}

/// "No estimate", "M", "1 point", "5 points".
pub fn estimate_label(value: Option<i64>, scale: &str) -> String {
    let Some(value) = value else {
        return NO_ESTIMATE.to_string();
    };
    if scale == ISSUE_ESTIMATION_TSHIRT {
        if let Some(size) = tshirt_label(value) {
            return size.to_string();
        }
    }
    if value == 1 {
        "1 point".to_string()
    } else {
        format!("{value} points")
    }
}

/// The chip form: "M" on the t-shirt scale, "5 pt" elsewhere.
pub fn estimate_short_label(value: i64, scale: &str) -> String {
    if scale == ISSUE_ESTIMATION_TSHIRT {
        if let Some(size) = tshirt_label(value) {
            return size.to_string();
        }
    }
    format!("{value} pt")
}

/// The values a picker offers: the scale's ladder plus the current value when
/// it sits off the ladder (an import from another scale, a value set before
/// the scale was switched), ascending — so the trigger always names a listed
/// option. The "No estimate" row is the caller's.
pub fn estimate_picker_values(current: Option<i64>, scale: &str) -> Vec<i64> {
    let mut values: Vec<i64> = estimation_scale(scale).to_vec();
    if let Some(current) = current.filter(|value| *value >= 0) {
        if !values.contains(&current) {
            values.push(current);
        }
    }
    values.sort_unstable();
    values
}

/// The `estimate_changed` timeline phrase, on the team's scale. `to` may
/// arrive as a number or a numeric string; anything else reads as cleared.
pub fn estimate_event_phrase(payload: &serde_json::Value, scale: &str) -> String {
    let to = match payload.get("to") {
        Some(serde_json::Value::Number(number)) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|value| value as i64)),
        Some(serde_json::Value::String(text)) if !text.is_empty() => text
            .trim()
            .parse::<i64>()
            .ok()
            .or_else(|| text.trim().parse::<f64>().ok().map(|value| value as i64)),
        _ => None,
    };
    match to {
        Some(to) => format!("set the estimate to {}", estimate_label(Some(to), scale)),
        None => "removed the estimate".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::collections::BTreeMap;

    /// The ONE contract fixture, replayed byte-for-byte on every client (web
    /// `issue-estimate.test.ts`, iOS `IssueEstimateTests`, Android
    /// `IssueEstimateTest`) — the SAME case names everywhere.
    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/issue-estimate.json");

    #[derive(Deserialize)]
    struct LabelCase {
        name: String,
        scale: String,
        value: Option<i64>,
        label: String,
        short: Option<String>,
    }

    #[derive(Deserialize)]
    struct PickerCase {
        name: String,
        scale: String,
        current: Option<i64>,
        values: Vec<i64>,
    }

    #[derive(Deserialize)]
    struct PhraseCase {
        name: String,
        scale: String,
        payload: serde_json::Value,
        phrase: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        scales: BTreeMap<String, Vec<i64>>,
        tshirt_labels: Vec<String>,
        no_estimate: String,
        labels: Vec<LabelCase>,
        pickers: Vec<PickerCase>,
        phrases: Vec<PhraseCase>,
    }

    fn fixture() -> Fixture {
        serde_json::from_str(FIXTURE).expect("the issue-estimate fixture parses")
    }

    #[test]
    fn ladders_and_tshirt_labels_match_the_fixture() {
        let fixture = fixture();
        let expected: BTreeMap<String, Vec<i64>> = [
            ("exponential", EXPONENTIAL_SCALE),
            ("fibonacci", FIBONACCI_SCALE),
            ("linear", LINEAR_SCALE),
            ("tshirt", TSHIRT_SCALE),
        ]
        .into_iter()
        .map(|(name, ladder)| (name.to_string(), ladder.to_vec()))
        .collect();
        assert_eq!(fixture.scales, expected);
        for (name, ladder) in &fixture.scales {
            assert_eq!(estimation_scale(name), ladder.as_slice(), "{name}");
        }
        assert_eq!(fixture.tshirt_labels, TSHIRT_LABELS);
        assert_eq!(fixture.no_estimate, NO_ESTIMATE);
        assert!(estimation_scale("none").is_empty());
    }

    #[test]
    fn label() {
        let fixture = fixture();
        assert!(!fixture.labels.is_empty());
        for row in &fixture.labels {
            assert_eq!(estimate_label(row.value, &row.scale), row.label, "label: {}", row.name);
            if let Some(value) = row.value {
                assert_eq!(
                    Some(estimate_short_label(value, &row.scale)),
                    row.short,
                    "label: {}",
                    row.name
                );
            }
        }
    }

    #[test]
    fn picker() {
        let fixture = fixture();
        assert!(!fixture.pickers.is_empty());
        for row in &fixture.pickers {
            assert_eq!(
                estimate_picker_values(row.current, &row.scale),
                row.values,
                "picker: {}",
                row.name
            );
        }
    }

    #[test]
    fn phrase() {
        let fixture = fixture();
        assert!(!fixture.phrases.is_empty());
        for row in &fixture.phrases {
            assert_eq!(
                estimate_event_phrase(&row.payload, &row.scale),
                row.phrase,
                "phrase: {}",
                row.name
            );
        }
    }
}
