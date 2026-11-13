//! #EXP-1617: the merge train's pure half against the same cases as the TS
//! engine: `fixtures/merge-train.json` (`resolver` under `repoPolicy`,
//! `pipeline.checksModes`) and every case of
//! `scripts/merge-train/strategies.test.ts`, verbatim.

use ecode_core::changelog_shapes::{rust_changelog, web_changelog};
use ecode_core::*;
use serde_json::{json, Value};

const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/merge-train.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).unwrap()
}

#[test]
fn resolver_cases_replay() {
    let f = fixture();
    let policy: TrainRepoConfig = serde_json::from_value(f["repoPolicy"].clone()).unwrap();
    let cases = f["resolver"].as_array().unwrap();
    assert!(!cases.is_empty());
    for case in cases {
        let paths: Vec<String> = serde_json::from_value(case["paths"].clone()).unwrap();
        let verdict = serde_json::to_value(resolve_conflicts(&paths, &policy.generated)).unwrap();
        assert_eq!(verdict, case["expected"], "resolver: {}", case["name"]);
    }
}

#[test]
fn checks_modes_replay() {
    let f = fixture();
    let cases = f["pipeline"]["checksModes"].as_array().unwrap();
    assert!(!cases.is_empty());
    for case in cases {
        let input: ChecksModeInput = serde_json::from_value(case["input"].clone()).unwrap();
        assert_eq!(serde_json::to_value(train_checks_mode(&input)).unwrap(), case["expected"], "pipeline checks: {}", case["name"]);
    }
}

#[test]
fn repo_policy_is_this_repos_config() {
    let ours: Value = serde_json::from_str(include_str!("../../../../../.exponential/merge-train.json")).unwrap();
    let f = fixture();
    assert_eq!(ours, f["repoPolicy"]);
    let config: TrainRepoConfig = serde_json::from_value(ours.clone()).unwrap();
    assert_eq!(serde_json::to_value(&config).unwrap(), ours, "the config round-trips");
}

#[test]
fn globs_as_merge_train_ts_defines_them() {
    assert!(train_glob_match("**/Cargo.lock", "Cargo.lock"));
    assert!(train_glob_match("**/Cargo.lock", "a/b/Cargo.lock"));
    assert!(!train_glob_match("**/Cargo.lock", "a/xCargo.lock"));
    assert!(train_glob_match("shots/**/*.webp", "shots/v/web.webp"));
    assert!(train_glob_match("shots/**/*.webp", "shots/web.webp"));
    assert!(train_glob_match("db/meta/**", "db/meta/a/b.json"));
    assert!(train_glob_match("**", "a/b"));
    assert!(!train_glob_match("db/*.sql", "db/x/1.sql"));
    assert!(train_glob_match("a?c", "abc"));
    assert!(!train_glob_match("a?c", "a/c"));
    assert!(train_glob_match("lucide-*/**", "lucide-x/y"));
    assert!(train_glob_match("a.b", "a.b") && !train_glob_match("a.b", "axb"));
    assert!(train_glob_match("**.json", "x/y.json"));
}

// ── strategies.test.ts ──────────────────────────────────────────────────────

fn mj(base: Value, ours: Value, theirs: Value) -> Value {
    merge_json(Some(&base), Some(&ours), Some(&theirs)).unwrap()
}

#[test]
fn json_union_unions_keys_the_later_unit_wins_a_key_both_changed() {
    assert_eq!(mj(json!({"a": 1, "b": 1}), json!({"a": 2, "b": 1, "c": 1}), json!({"a": 3, "b": 1, "d": 1})), json!({"a": 3, "b": 1, "c": 1, "d": 1}));
}

#[test]
fn json_union_a_key_only_one_side_changed_keeps_that_change_deletions_included() {
    assert_eq!(mj(json!({"a": 1, "b": 1}), json!({"a": 2}), json!({"a": 1, "b": 1, "c": 1})), json!({"a": 2, "c": 1}));
}

#[test]
fn json_union_recurses_into_objects_and_unions_arrays() {
    assert_eq!(
        mj(json!({"v": {"x": [1]}}), json!({"v": {"x": [1, 2], "o": 1}}), json!({"v": {"x": [1, 3], "t": 1}})),
        json!({"v": {"x": [1, 2, 3], "o": 1, "t": 1}})
    );
}

#[test]
fn json_union_keeps_the_tips_indentation_and_final_newline() {
    assert_eq!(json_union(Some("{}"), "{\n    \"a\": 1\n}\n", "{\"b\": 2}").unwrap(), "{\n    \"a\": 1,\n    \"b\": 2\n}\n");
}

#[test]
fn json_union_an_add_add_without_a_base() {
    assert_eq!(json_union(None, "{\"a\":[1]}", "{\"a\":[2]}").unwrap(), "{\n  \"a\": [\n    1,\n    2\n  ]\n}");
}

#[test]
fn renumber_reads_a_files_leading_number() {
    assert_eq!(file_number("db/out/0042_brave_owl.sql"), Some(42));
    assert_eq!(file_number("db/out/meta/_journal.json"), None);
}

#[test]
fn renumber_compares_contents_not_names() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert!(same_contents(&s(&["a\n", "b"]), &s(&["b\n", "a"])));
    assert!(!same_contents(&s(&["a"]), &s(&["a2"])));
}

fn entries_have_one_id(text: &str) -> bool {
    // text.split(/\n\s*\}[,;]?\n/)
    let mut chunks = vec![];
    let mut rest = text;
    'outer: loop {
        for (i, _) in rest.match_indices('\n') {
            let after = &rest[i + 1..];
            let trimmed = after.trim_start_matches(|c: char| c.is_whitespace());
            if let Some(tail) = trimmed.strip_prefix('}') {
                let tail = tail.strip_prefix([',', ';']).unwrap_or(tail);
                if let Some(next) = tail.strip_prefix('\n') {
                    chunks.push(&rest[..i]);
                    rest = next;
                    continue 'outer;
                }
            }
        }
        chunks.push(rest);
        break;
    }
    chunks.iter().all(|chunk| chunk.matches("id: ").count() <= 1)
}

#[test]
fn prepend_union_three_web_entries_prepended_at_one_spot_stay_three_entries_the_later_units_first() {
    let base = web_changelog(&["a"]);
    let two = prepend_union(&base, &web_changelog(&["b", "a"]), &web_changelog(&["c", "a"]));
    assert_eq!(two.as_deref(), Some(web_changelog(&["c", "b", "a"]).as_str()));
    let three = prepend_union(&base, &two.unwrap(), &web_changelog(&["d", "a"])).unwrap();
    assert_eq!(three, web_changelog(&["d", "c", "b", "a"]));
    assert!(entries_have_one_id(&three));
}

#[test]
fn prepend_union_the_rust_mirror_both_sides_previous_233_the_later_one_renumbered() {
    let base = rust_changelog("a", &[(232, "z")]);
    let side = |id: &str| rust_changelog(id, &[(233, "a"), (232, "z")]);
    let two = prepend_union(&base, &side("b"), &side("c")).unwrap();
    assert_eq!(two, rust_changelog("c", &[(234, "b"), (233, "a"), (232, "z")]));
    let three = prepend_union(&base, &two, &side("d")).unwrap();
    assert_eq!(three, rust_changelog("d", &[(235, "c"), (234, "b"), (233, "a"), (232, "z")]));
    assert!(entries_have_one_id(&three));
}

#[test]
fn prepend_union_anything_but_two_pure_insertions_at_one_point_is_a_conflict() {
    let base = web_changelog(&["a", "z"]);
    // an edit of a base line
    assert_eq!(prepend_union(&base, &web_changelog(&["b", "a", "z"]), &web_changelog(&["c", "a", "z"]).replacen("title: `A`", "title: `A2`", 1)), None);
    // two insertion points
    assert_eq!(prepend_union(&base, &web_changelog(&["b", "a", "z"]), &web_changelog(&["a", "c", "z"])), None);
    // a deletion
    assert_eq!(prepend_union(&base, &web_changelog(&["b", "a", "z"]), &web_changelog(&["z"])), None);
}

#[test]
fn prepend_union_a_duplicate_const_that_cannot_be_renumbered_safely_is_a_conflict() {
    let base = rust_changelog("a", &[(232, "z")]);
    let ours = rust_changelog("b", &[(233, "a"), (232, "z")]);
    // theirs names its PREVIOUS_233 twice: no rename
    let twice = rust_changelog("c", &[(233, "a"), (232, "z")]).replacen("title: \"C\",", "title: \"C\", // see PREVIOUS_233", 1);
    assert_eq!(prepend_union(&base, &ours, &twice), None);
    // an unnumbered name both declare
    let plain = |x: &str| format!("const A = 1\n{x}const END = 2\n");
    assert_eq!(prepend_union(&plain(""), &plain("const NEW = 1\n"), &plain("const NEW = 2\n")), None);
}

#[test]
fn prepend_union_one_side_unchanged_takes_the_other() {
    assert_eq!(prepend_union("a\n", "a\n", "b\na\n").as_deref(), Some("b\na\n"));
    assert_eq!(prepend_union("a\n", "b\na\n", "a\n").as_deref(), Some("b\na\n"));
}
