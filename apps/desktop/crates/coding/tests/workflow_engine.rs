//! EXP-982 — the workflow engine's rule order, replayed AS DATA.
//!
//! Every case under `tests/fixtures/workflows/*.json` is
//! `{name, snapshot, expected}`: the pure [`coding::workflows::evaluate`] run
//! over one plain-data snapshot, compared decision-for-decision and in ORDER
//! (the rule cascade IS the contract). A failure names the case.

use coding::workflows::{evaluate, Decision, Snapshot};

#[derive(serde::Deserialize)]
struct Case {
    name: String,
    snapshot: Snapshot,
    expected: Vec<Decision>,
}

/// The fixture files, by name — listed rather than globbed so a file added
/// without a `mod`-level thought does not silently go unread.
const FIXTURES: [(&str, &str); 3] = [
    ("mirror.json", include_str!("fixtures/workflows/mirror.json")),
    ("run.json", include_str!("fixtures/workflows/run.json")),
    // EXP-983 — the speculative half: start modes, bases, propagation,
    // serialization, the topological train and the base cleanup.
    (
        "speculative.json",
        include_str!("fixtures/workflows/speculative.json"),
    ),
];

#[test]
fn the_rule_cascade_matches_every_fixture_case() {
    let mut cases = 0;
    for (file, raw) in FIXTURES {
        let parsed: Vec<Case> =
            serde_json::from_str(raw).unwrap_or_else(|err| panic!("{file} parses: {err}"));
        assert!(!parsed.is_empty(), "{file} has cases");
        for case in parsed {
            let got = evaluate(&case.snapshot);
            assert_eq!(got, case.expected, "{file} — case: {}", case.name);
            cases += 1;
        }
    }
    // A guard against an empty replay silently passing.
    assert!(cases >= 40, "expected the whole rule cascade, replayed {cases}");
}

/// Evaluation is LEVEL-TRIGGERED: the same snapshot decides the same thing
/// however often it is evaluated (the hosts re-evaluate every beat, and a
/// decision the host has not finished yet must not multiply).
#[test]
fn evaluation_is_idempotent() {
    for (_, raw) in FIXTURES {
        let parsed: Vec<Case> = serde_json::from_str(raw).expect("parses");
        for case in parsed {
            let first = evaluate(&case.snapshot);
            let second = evaluate(&case.snapshot);
            assert_eq!(first, second, "case: {}", case.name);
        }
    }
}
