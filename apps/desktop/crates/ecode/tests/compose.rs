//! #EXP-1617: the train engine against real git repos (no forge, no
//! network): every scenario of `scripts/merge-train/compose.test.ts`, ported
//! case by case, so the TS engine (GitHub's train) and this one (an eCode
//! repository's) are pinned by the same cases.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use ecode::compose::{compose, compose_with, ComposeInput, ComposeResult};
use ecode::core::changelog_shapes::{rust_changelog, web_changelog};
use ecode::core::{
    Against, LockManifests, LockfilePolicy, ResolveRule, ResolveStrategy, TrainChecksMode, TrainCi, TrainMode, TrainPipelinePr,
    TrainPipelineUnit, TrainRepoConfig, TrainUnitOutcome, UnitOutcome,
};
use serde_json::{json, Value};

fn tempdir(name: &str) -> PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("ecode-compose-{}-{n}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn must(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").current_dir(dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

struct Repo {
    dir: PathBuf,
    base: String,
}

type Files<'a> = &'a [(&'a str, Option<&'a str>)];

impl Repo {
    fn new(files: &[(&str, &str)], config: Option<&TrainRepoConfig>) -> Repo {
        let dir = tempdir("repo");
        must(&dir, &["init", "--quiet", "-b", "master"]);
        must(&dir, &["config", "user.email", "train@example.com"]);
        must(&dir, &["config", "user.name", "train"]);
        must(&dir, &["config", "commit.gpgsign", "false"]);
        let mut all: Vec<(String, Option<String>)> = files.iter().map(|(p, b)| (p.to_string(), Some(b.to_string()))).collect();
        if let Some(config) = config {
            all.push((".exponential/merge-train.json".into(), Some(serde_json::to_string(config).unwrap())));
        }
        write(&dir, &all);
        must(&dir, &["add", "-A"]);
        must(&dir, &["commit", "--quiet", "-m", "base"]);
        let base = must(&dir, &["rev-parse", "HEAD"]);
        Repo { dir, base }
    }

    /// A PR branch off `from` (default the base): → its head sha.
    fn branch_from(&self, files: Files<'_>, from: &str) -> String {
        must(&self.dir, &["checkout", "--quiet", "--detach", from]);
        let owned: Vec<(String, Option<String>)> = files.iter().map(|(p, b)| (p.to_string(), b.map(str::to_string))).collect();
        write(&self.dir, &owned);
        must(&self.dir, &["add", "-A"]);
        must(&self.dir, &["commit", "--quiet", "-m", "wip"]);
        let sha = must(&self.dir, &["rev-parse", "HEAD"]);
        must(&self.dir, &["checkout", "--quiet", "--detach", &self.base]);
        sha
    }

    fn branch(&self, files: Files<'_>) -> String {
        let base = self.base.clone();
        self.branch_from(files, &base)
    }

    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.dir.join(path)).unwrap()
    }

    fn has(&self, path: &str) -> bool {
        self.dir.join(path).exists()
    }

    fn input(&self, train: &str, base: &str, mode: TrainMode, units: Vec<TrainPipelineUnit>) -> ComposeInput {
        ComposeInput { train_id: train.into(), base_sha: base.into(), mode, units, config: None, identity: None }
    }

    fn compose(&self, base: &str, mode: TrainMode, units: Vec<TrainPipelineUnit>) -> ComposeResult {
        compose(&self.dir, &self.input("t", base, mode, units)).unwrap()
    }

    fn log(&self, range: &str) -> Vec<String> {
        must(&self.dir, &["log", "--format=%s", range]).lines().map(str::to_string).collect()
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn write(dir: &Path, files: &[(String, Option<String>)]) {
    for (p, body) in files {
        let abs = dir.join(p);
        match body {
            None => {
                let _ = std::fs::remove_file(&abs);
            }
            Some(body) => {
                std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
                std::fs::write(&abs, body).unwrap();
            }
        }
    }
}

fn pr(number: u64, head_sha: &str) -> TrainPipelinePr {
    TrainPipelinePr { number, head_sha: head_sha.into(), title: format!("EXP-{number}: change"), author: None }
}

fn unit(prs: Vec<TrainPipelinePr>) -> TrainPipelineUnit {
    TrainPipelineUnit { prs, tested_sha: None }
}

fn one(number: u64, head: &str) -> TrainPipelineUnit {
    unit(vec![pr(number, head)])
}

fn rule(name: &str, paths: &[&str], strategy: ResolveStrategy, command: Option<&str>, verify: Option<&[&str]>) -> ResolveRule {
    ResolveRule {
        name: name.into(),
        paths: paths.iter().map(|p| p.to_string()).collect(),
        strategy,
        command: command.map(str::to_string),
        verify: verify.map(|v| v.iter().map(|p| p.to_string()).collect()),
    }
}

fn policy() -> TrainRepoConfig {
    TrainRepoConfig {
        setup: None,
        ci: Some(TrainCi { workflows: vec!["ci.yml".into()] }),
        generated: vec![
            rule("changelog", &["changelog.ts", "changelog.rs"], ResolveStrategy::PrependUnion, None, None),
            rule("bindings", &["gen/**"], ResolveStrategy::Regenerate, Some("cp src/enum.txt gen/enum.txt"), None),
            rule("migrations", &["db/*.sql", "db/meta/**"], ResolveStrategy::Renumber, Some("bash gen-migration.sh"), Some(&["db/*.sql"])),
            rule("index", &["shots/index.json"], ResolveStrategy::JsonUnion, None, None),
            rule("shots", &["shots/**/*.webp"], ResolveStrategy::TakeLater, None, None),
        ],
        lockfiles: None,
    }
}

const CHANGELOG: &str = "export const CHANGELOG = [\n  { v: 1 },\n]\n";

fn kinds(units: &[TrainUnitOutcome]) -> Vec<(UnitOutcome, Option<Against>)> {
    units.iter().map(|u| (u.outcome, u.against)).collect()
}

fn as_json<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap()
}

fn skip() -> bool {
    if !ecode::git_available() {
        eprintln!("skipped: no git");
        return true;
    }
    false
}

// ── a sibling in the way is no bounce (#EXP-1711) ──────────────────────────

#[test]
fn a_unit_that_conflicts_only_behind_an_earlier_unit_of_the_batch_reports_against_batch() {
    if skip() { return }
    let p = policy();
    let r = Repo::new(&[("a.txt", "a\n")], Some(&p));
    let first = r.branch(&[("a.txt", Some("first\n"))]);
    let second = r.branch(&[("a.txt", Some("second\n"))]);
    let out = r.compose(&r.base, TrainMode::Test, vec![one(1, &first), one(2, &second)]).outcome;
    assert_eq!(kinds(&out.units), [(UnitOutcome::Composed, None), (UnitOutcome::Conflict, Some(Against::Batch))]);
    assert_eq!(r.read("a.txt"), "first\n");
}

#[test]
fn a_unit_that_conflicts_with_the_base_itself_stays_a_plain_conflict() {
    if skip() { return }
    let p = policy();
    let r = Repo::new(&[("a.txt", "a\n"), ("c.txt", "c\n")], Some(&p));
    let stale = r.branch(&[("a.txt", Some("stale\n"))]);
    let moved = r.branch(&[("a.txt", Some("moved\n"))]);
    let other = r.branch_from(&[("c.txt", Some("c1\n"))], &moved);
    let out = r.compose(&moved, TrainMode::Test, vec![one(3, &other), one(4, &stale)]).outcome;
    assert_eq!(kinds(&out.units), [(UnitOutcome::Composed, None), (UnitOutcome::Conflict, None)]);
}

// ── changelogs prepended at one spot (#LOGS-2) ─────────────────────────────

fn rs(id: &str) -> String {
    rust_changelog(id, &[(233, "a"), (232, "z")])
}

fn changelog_base() -> (String, String) {
    (web_changelog(&["a"]), rust_changelog("a", &[(232, "z")]))
}

fn changelog_files(r: &Repo, id: &str) -> String {
    let (ts, rs_) = (web_changelog(&[id, "a"]), rs(id));
    r.branch(&[("changelog.ts", Some(&ts)), ("changelog.rs", Some(&rs_))])
}

fn changelog_repo() -> Repo {
    let (ts, rs_) = changelog_base();
    Repo::new(&[("changelog.ts", &ts), ("changelog.rs", &rs_)], Some(&policy()))
}

#[test]
fn three_units_three_whole_entries_newest_first_the_mirror_renumbered() {
    if skip() { return }
    let r = changelog_repo();
    let (b, c, d) = (changelog_files(&r, "b"), changelog_files(&r, "c"), changelog_files(&r, "d"));
    let out = r.compose(&r.base, TrainMode::Test, vec![one(1, &b), one(2, &c), one(3, &d)]).outcome;
    let got: Vec<(UnitOutcome, Vec<String>)> = out.units.iter().map(|u| (u.outcome, u.resolved.clone())).collect();
    assert_eq!(
        got,
        [(UnitOutcome::Composed, vec![]), (UnitOutcome::Composed, vec!["changelog".to_string()]), (UnitOutcome::Composed, vec!["changelog".to_string()])]
    );
    assert_eq!(r.read("changelog.ts"), web_changelog(&["d", "c", "b", "a"]));
    assert_eq!(r.read("changelog.rs"), rust_changelog("d", &[(235, "c"), (234, "b"), (233, "a"), (232, "z")]));
    // the old resolver, a line union, fused them into one object
    let dir = tempdir("union");
    for (n, body) in [("base", web_changelog(&["a"])), ("ours", web_changelog(&["b", "a"])), ("theirs", web_changelog(&["c", "a"]))] {
        std::fs::write(dir.join(n), body).unwrap();
    }
    let fused = Command::new("git").current_dir(&dir).args(["merge-file", "-p", "--union", "theirs", "base", "ours"]).output().unwrap();
    assert!(String::from_utf8_lossy(&fused.stdout).contains("    title: `C`,\n    id: `b`,"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_edit_beside_the_prepend_bounces_the_unit_instead_of_a_line_union() {
    if skip() { return }
    let r = changelog_repo();
    let b = changelog_files(&r, "b");
    let edited = web_changelog(&["c", "a"]).replacen("title: `A`", "title: `A2`", 1);
    let c = r.branch(&[("changelog.ts", Some(&edited))]);
    let out = r.compose(&r.base, TrainMode::Test, vec![one(1, &b), one(2, &c)]).outcome;
    assert_eq!((out.units[1].outcome, out.units[1].files.clone()), (UnitOutcome::Conflict, vec!["changelog.ts".to_string()]));
    assert_eq!(r.read("changelog.ts"), web_changelog(&["b", "a"]));
}

#[test]
fn a_merge_now_that_had_to_resolve_a_conflict_runs_ci_a_clean_one_stays_instant() {
    if skip() { return }
    let r = changelog_repo();
    let landed = changelog_files(&r, "b");
    let c = changelog_files(&r, "c");
    let resolved = r.compose(&landed, TrainMode::Untested, vec![one(2, &c)]);
    assert_eq!(resolved.outcome.units[0].resolved, ["changelog"]);
    assert_eq!((resolved.outcome.checks, resolved.dispatch), (TrainChecksMode::Dispatched, vec!["ci.yml".to_string()]));
    let clean = r.compose(&r.base, TrainMode::Untested, vec![one(2, &c)]);
    assert_eq!((clean.outcome.checks, clean.dispatch), (TrainChecksMode::Skipped, vec![]));
}

#[test]
fn a_merge_now_composed_on_a_base_that_moved_past_its_pr_runs_ci_once_rebased_it_stays_instant() {
    if skip() { return }
    // 2026-11-10: #2590 was green on master B; #2597 was admin-merged (B → B');
    // #2590's Merge now composed cleanly on B' and landed untested.
    let r = Repo::new(&[("a.txt", "a\n"), ("test.txt", "old\n")], Some(&policy()));
    let pr2590 = r.branch(&[("a.txt", Some("a1\n"))]);
    let moved = r.branch(&[("test.txt", Some("new test\n"))]);
    let on_moved = r.compose(&moved, TrainMode::Untested, vec![one(2590, &pr2590)]);
    assert_eq!(as_json(&on_moved.outcome.units[0]), json!({ "prs": [2590], "outcome": "composed", "files": [], "resolved": [] }));
    assert_eq!((on_moved.outcome.checks, on_moved.dispatch), (TrainChecksMode::Dispatched, vec!["ci.yml".to_string()]));
    let rebased = r.branch_from(&[("a.txt", Some("a1\n"))], &moved);
    let on_rebased = r.compose(&moved, TrainMode::Untested, vec![one(2590, &rebased)]);
    assert_eq!((on_rebased.outcome.checks, on_rebased.dispatch), (TrainChecksMode::Skipped, vec![]));
}

// ── a Merge now whose lockfile drifted runs CI (#LOGS-2) ───────────────────

fn locks_policy() -> TrainRepoConfig {
    TrainRepoConfig {
        lockfiles: Some(LockfilePolicy {
            paths: vec!["**/Cargo.lock".into()],
            manifests: Some(vec![LockManifests { paths: vec!["**/Cargo.toml".into()], lock: "Cargo.lock".into() }]),
        }),
        ..policy()
    }
}

fn lock(top: &str, bottom: &str) -> String {
    format!("{top}\n{}\n{bottom}\n", ["b", "c", "d", "e", "f", "g"].join("\n"))
}

fn lock_repo() -> Repo {
    let l = lock("smithay 1", "sentry 0");
    Repo::new(&[("ede/Cargo.lock", &l), ("ede/crates/x/Cargo.toml", "[deps]\n")], Some(&locks_policy()))
}

#[test]
fn a_clean_lockfile_merge_onto_a_moved_base_is_dispatched_onto_an_unmoved_base_skipped() {
    if skip() { return }
    let r = lock_repo();
    let moved = r.branch(&[("ede/Cargo.lock", Some(&lock("smithay git", "sentry 0")))]);
    let u = r.branch(&[("ede/Cargo.lock", Some(&lock("smithay 1", "sentry 1")))]);
    let on_moved = r.compose(&moved, TrainMode::Untested, vec![one(2, &u)]);
    let first = &on_moved.outcome.units[0];
    assert_eq!((first.outcome, first.resolved.clone(), first.drifted.clone()), (UnitOutcome::Composed, vec![], Some(vec!["ede/Cargo.lock".to_string()])));
    assert_eq!((on_moved.outcome.checks, on_moved.dispatch), (TrainChecksMode::Dispatched, vec!["ci.yml".to_string()]));
    let on_base = r.compose(&r.base, TrainMode::Untested, vec![one(2, &u)]);
    assert_eq!(as_json(&on_base.outcome.units[0]), json!({ "prs": [2], "outcome": "composed", "files": [], "resolved": [] }));
    assert_eq!((on_base.outcome.checks, on_base.dispatch), (TrainChecksMode::Skipped, vec![]));
}

#[test]
fn a_manifest_changed_while_the_base_moved_its_paired_lock_is_dispatched() {
    if skip() { return }
    let r = lock_repo();
    let moved = r.branch(&[("ede/Cargo.lock", Some(&lock("smithay git", "sentry 0")))]);
    let u = r.branch(&[("ede/crates/x/Cargo.toml", Some("[deps]\nsentry = \"1\"\n"))]);
    let out = r.compose(&moved, TrainMode::Untested, vec![one(2, &u)]).outcome;
    assert_eq!((out.units[0].drifted.clone(), out.checks), (Some(vec!["ede/Cargo.lock".to_string()]), TrainChecksMode::Dispatched));
    assert_eq!(r.compose(&r.base, TrainMode::Untested, vec![one(2, &u)]).outcome.checks, TrainChecksMode::Skipped);
}

// ── compose ────────────────────────────────────────────────────────────────

#[test]
fn squashes_every_pr_in_order_with_its_title_a_stack_bottom_first() {
    if skip() { return }
    let r = Repo::new(&[("a.txt", "a\n"), ("b.txt", "b\n")], Some(&policy()));
    let first = r.branch(&[("a.txt", Some("a1\n"))]);
    let low = r.branch(&[("b.txt", Some("b1\n"))]);
    let high = r.branch_from(&[("c.txt", Some("c\n"))], &low);
    let ComposeResult { outcome, dispatch } =
        compose(&r.dir, &r.input("t1", &r.base, TrainMode::Test, vec![one(1, &first), unit(vec![pr(2, &low), pr(3, &high)])])).unwrap();
    assert_eq!(outcome.units.iter().map(|u| u.outcome).collect::<Vec<_>>(), [UnitOutcome::Composed, UnitOutcome::Composed]);
    assert_eq!(outcome.checks, TrainChecksMode::Dispatched);
    assert_eq!(dispatch, ["ci.yml"]);
    assert_eq!(r.log(&format!("{}..{}", r.base, outcome.head.unwrap())), ["EXP-3: change (#3)", "EXP-2: change (#2)", "EXP-1: change (#1)"]);
    assert_eq!(r.read("c.txt"), "c\n");
}

#[test]
fn each_squash_is_authored_by_its_pr_author() {
    // Not in compose.test.ts: the `author` field compose.ts passes as --author.
    if skip() { return }
    let r = Repo::new(&[("a.txt", "a\n")], Some(&policy()));
    let a = r.branch(&[("a.txt", Some("a1\n"))]);
    let mut p = pr(1, &a);
    p.author = Some("Ada Lovelace <ada@example.com>".into());
    let mut input = r.input("t", &r.base, TrainMode::Test, vec![unit(vec![p])]);
    input.identity = Some(("Exponential".into(), "noreply@exponential.at".into()));
    let head = compose(&r.dir, &input).unwrap().outcome.head.unwrap();
    assert_eq!(must(&r.dir, &["log", "-1", "--format=%an <%ae>|%cn", &head]), "Ada Lovelace <ada@example.com>|Exponential");
}

#[test]
fn a_changelog_both_prepended_keeps_both_entries_the_later_units_first() {
    if skip() { return }
    let r = Repo::new(&[("changelog.ts", CHANGELOG)], Some(&policy()));
    let a = r.branch(&[("changelog.ts", Some(&CHANGELOG.replacen("[\n", "[\n  { v: 2 },\n", 1)))]);
    let b = r.branch(&[("changelog.ts", Some(&CHANGELOG.replacen("[\n", "[\n  { v: 3 },\n", 1)))]);
    let out = r.compose(&r.base, TrainMode::Test, vec![one(1, &a), one(2, &b)]).outcome;
    assert_eq!(as_json(&out.units[1]), json!({ "prs": [2], "outcome": "composed", "files": [], "resolved": ["changelog"] }));
    assert_eq!(r.read("changelog.ts"), "export const CHANGELOG = [\n  { v: 3 },\n  { v: 2 },\n  { v: 1 },\n]\n");
}

#[test]
fn the_shots_index_unions_the_later_webp_wins() {
    if skip() { return }
    let r = Repo::new(&[("shots/index.json", "{\"a\":1}\n"), ("shots/v/web.webp", "0")], Some(&policy()));
    let a = r.branch(&[("shots/index.json", Some("{\"a\":1,\"b\":1}\n")), ("shots/v/web.webp", Some("A"))]);
    let b = r.branch(&[("shots/index.json", Some("{\"a\":1,\"c\":1}\n")), ("shots/v/web.webp", Some("B"))]);
    let out = r.compose(&r.base, TrainMode::Test, vec![one(1, &a), one(2, &b)]).outcome;
    let mut resolved = out.units[1].resolved.clone();
    resolved.sort();
    assert_eq!(resolved, ["index", "shots"]);
    assert_eq!(serde_json::from_str::<Value>(&r.read("shots/index.json")).unwrap(), json!({ "a": 1, "b": 1, "c": 1 }));
    assert_eq!(r.read("shots/v/web.webp"), "B");
}

#[test]
fn generated_bindings_regenerate_from_the_merged_source() {
    if skip() { return }
    let r = Repo::new(&[("src/enum.txt", "a\nm\nz\n"), ("gen/enum.txt", "a\nm\nz\n")], Some(&policy()));
    let a = r.branch(&[("src/enum.txt", Some("a\nb\nm\nz\n")), ("gen/enum.txt", Some("a\nb\nm\nz\n"))]);
    let b = r.branch(&[("src/enum.txt", Some("a\nm\ny\nz\n")), ("gen/enum.txt", Some("a\nm\nY\nz\n"))]);
    let out = r.compose(&r.base, TrainMode::Test, vec![one(1, &a), one(2, &b)]).outcome;
    assert_eq!(out.units[1].outcome, UnitOutcome::Composed);
    assert_eq!(r.read("gen/enum.txt"), "a\nb\nm\ny\nz\n");
}

#[test]
fn a_regenerate_that_keeps_the_bases_blob_for_a_path_the_unit_changed_bounces_it() {
    if skip() { return }
    // #FEED-212: the lockfiles rule claimed both workspaces' locks but
    // regenerated only one: the other came out as master's.
    let locks = |command: &str| TrainRepoConfig {
        generated: vec![rule("lockfiles", &["desktop/Cargo.lock", "ede/Cargo.lock"], ResolveStrategy::Regenerate, Some(command), None)],
        ..TrainRepoConfig::default()
    };
    let r = Repo::new(
        &[("desktop/want", "d0\n"), ("desktop/Cargo.lock", "d0\n"), ("ede/Cargo.lock", "e0\n")],
        Some(&locks("cp desktop/want desktop/Cargo.lock")),
    );
    let moved = r.branch(&[("ede/Cargo.lock", Some("e-master\n"))]);
    let u = r.branch(&[("ede/Cargo.lock", Some("e-unit\n"))]);
    let out = r.compose(&moved, TrainMode::Test, vec![one(1, &u)]).outcome;
    assert_eq!((out.units[0].outcome, out.units[0].files.clone()), (UnitOutcome::Conflict, vec!["ede/Cargo.lock".to_string()]));
    assert_ne!(out.head.as_deref(), Some(moved.as_str()));
    assert_eq!(r.read("ede/Cargo.lock"), "e-master\n");
    // a regenerate that does rewrite it (here: to the unit's side) still composes
    let mut input = r.input("t", &moved, TrainMode::Test, vec![one(1, &u)]);
    input.config = Some(locks(&format!("git checkout {u} -- ede/Cargo.lock")));
    let ok = compose(&r.dir, &input).unwrap().outcome;
    assert_eq!((ok.units[0].outcome, ok.units[0].resolved.clone()), (UnitOutcome::Composed, vec!["lockfiles".to_string()]));
}

#[test]
fn any_other_conflict_drops_that_whole_unit_naming_only_the_unmatched_files_and_the_next_unit_still_rides() {
    if skip() { return }
    let r = Repo::new(&[("src/x.ts", "1\n"), ("gen/enum.txt", "0\n"), ("y.ts", "y\n")], Some(&policy()));
    let a = r.branch(&[("src/x.ts", Some("2\n")), ("gen/enum.txt", Some("2\n"))]);
    let b = r.branch(&[("src/x.ts", Some("3\n")), ("gen/enum.txt", Some("3\n"))]);
    let c = r.branch(&[("y.ts", Some("y2\n"))]);
    let out = r.compose(&r.base, TrainMode::Test, vec![one(1, &a), one(2, &b), one(3, &c)]).outcome;
    let got: Vec<(UnitOutcome, Vec<String>)> = out.units.iter().map(|u| (u.outcome, u.files.clone())).collect();
    assert_eq!(got, [(UnitOutcome::Composed, vec![]), (UnitOutcome::Conflict, vec!["src/x.ts".to_string()]), (UnitOutcome::Composed, vec![])]);
    assert_eq!(r.read("src/x.ts"), "2\n");
    assert_eq!(r.log(&format!("{}..{}", r.base, out.head.unwrap())), ["EXP-3: change (#3)", "EXP-1: change (#1)"]);
}

#[test]
fn a_failing_command_bounces_the_unit_with_the_rules_paths() {
    if skip() { return }
    let r = Repo::new(&[("src/enum.txt", "a\n"), ("gen/enum.txt", "a\n")], Some(&policy()));
    let a = r.branch(&[("gen/enum.txt", Some("b\n"))]);
    let b = r.branch(&[("gen/enum.txt", Some("c\n"))]);
    let out = compose_with(&r.dir, &r.input("t", &r.base, TrainMode::Test, vec![one(1, &a), one(2, &b)]), &|_, _| false).unwrap().outcome;
    assert_eq!(as_json(&out.units[1]), json!({ "prs": [2], "outcome": "conflict", "files": ["gen/enum.txt"], "resolved": [], "against": "batch" }));
}

// A toy generator: the next migration = the schema lines no migration has yet.
const GEN: &str = "n=$(ls db/*.sql | wc -l); n=$(printf '%04d' $((n+1)))\nnew=$(grep -x 't[0-9]*' schema.txt | grep -vxF -f <(cat db/*.sql) || true)\n[ -z \"$new\" ] && exit 0\necho \"$new\" > db/${n}_gen.sql\necho \"$n\" >> db/meta/journal.txt\n";

fn migration_repo() -> Repo {
    Repo::new(
        &[("schema.txt", "t1\n1\n2\n3\nend\n"), ("db/0001_init.sql", "t1\n"), ("db/meta/journal.txt", "0001\n"), ("gen-migration.sh", GEN)],
        Some(&policy()),
    )
}

#[test]
fn migrations_the_later_units_migration_takes_the_next_number() {
    if skip() { return }
    let r = migration_repo();
    let a = r.branch(&[("schema.txt", Some("t1\nt2\n1\n2\n3\nend\n")), ("db/0002_owl.sql", Some("t2\n")), ("db/meta/journal.txt", Some("0001\n0002\n"))]);
    let b = r.branch(&[("schema.txt", Some("t1\n1\n2\n3\nt3\nend\n")), ("db/0002_cat.sql", Some("t3\n")), ("db/meta/journal.txt", Some("0001\n0002\n"))]);
    let out = r.compose(&r.base, TrainMode::Test, vec![one(1, &a), one(2, &b)]).outcome;
    assert_eq!(as_json(&out.units[1]), json!({ "prs": [2], "outcome": "composed", "files": [], "resolved": ["migrations"] }));
    assert!(!r.has("db/0002_cat.sql"));
    assert_eq!(r.read("db/0003_gen.sql"), "t3\n");
    assert_eq!(r.read("db/meta/journal.txt"), "0001\n0002\n0003\n");
}

#[test]
fn migrations_a_hand_edited_migration_the_command_cannot_regenerate_bounces() {
    if skip() { return }
    let r = migration_repo();
    let a = r.branch(&[("schema.txt", Some("t1\nt2\n1\n2\n3\nend\n")), ("db/0002_owl.sql", Some("t2\n")), ("db/meta/journal.txt", Some("0001\n0002\n"))]);
    let b = r.branch(&[
        ("schema.txt", Some("t1\n1\n2\n3\nt3\nend\n")),
        ("db/0002_cat.sql", Some("t3\n-- backfill\n")),
        ("db/meta/journal.txt", Some("0001\n0002\n")),
    ]);
    let out = r.compose(&r.base, TrainMode::Test, vec![one(1, &a), one(2, &b)]).outcome;
    assert_eq!(out.units[1].outcome, UnitOutcome::Conflict);
    assert_eq!(out.units[1].files, ["db/0002_cat.sql", "db/meta/journal.txt"]);
    assert!(r.has("db/0002_owl.sql"));
}

#[test]
fn a_lone_unit_tested_on_this_base_is_proven_merge_now_is_untested() {
    if skip() { return }
    let r = Repo::new(&[("a.txt", "a\n")], Some(&policy()));
    let a = r.branch(&[("a.txt", Some("a1\n"))]);
    let tested = |sha: &str| TrainPipelineUnit { prs: vec![pr(1, &a)], tested_sha: Some(sha.into()) };
    let proven = r.compose(&r.base, TrainMode::Test, vec![tested(&a)]);
    assert_eq!((proven.outcome.checks, proven.dispatch), (TrainChecksMode::Proven, vec![]));
    let stale = r.branch(&[("a.txt", Some("a1\n")), ("z.txt", Some("older base\n"))]);
    assert_eq!(r.compose(&r.base, TrainMode::Test, vec![tested(&stale)]).outcome.checks, TrainChecksMode::Dispatched);
    assert_eq!(r.compose(&r.base, TrainMode::Untested, vec![one(1, &a)]).outcome.checks, TrainChecksMode::Skipped);
}

#[test]
fn nothing_composed_is_one_empty_marker_commit_on_the_base_no_ci() {
    if skip() { return }
    let r = Repo::new(&[("x.ts", "1\n")], None);
    let a = r.branch(&[("x.ts", Some("2\n"))]);
    let b = r.branch(&[("x.ts", Some("3\n"))]);
    let out = compose(&r.dir, &r.input("t9", &a, TrainMode::Test, vec![one(2, &b)])).unwrap().outcome;
    assert_eq!(out.checks, TrainChecksMode::None);
    let head = out.head.unwrap();
    assert_eq!(must(&r.dir, &["rev-parse", &format!("{head}^")]), a);
    assert_eq!(must(&r.dir, &["log", "-1", "--format=%s", &head]), "exp-train t9: nothing composed");
}

#[test]
fn no_repo_config_no_resolvers_no_ci() {
    if skip() { return }
    let r = Repo::new(&[("a.txt", "a\n")], None);
    let a = r.branch(&[("a.txt", Some("a1\n"))]);
    assert_eq!(r.compose(&r.base, TrainMode::Test, vec![one(1, &a)]).outcome.checks, TrainChecksMode::None);
}

#[test]
fn the_bases_policy_rules_never_a_units() {
    if skip() { return }
    let r = Repo::new(&[("changelog.ts", CHANGELOG)], None);
    let p = serde_json::to_string(&policy()).unwrap();
    let a = r.branch(&[("changelog.ts", Some(&CHANGELOG.replacen("[\n", "[\n  { v: 2 },\n", 1))), (".exponential/merge-train.json", Some(&p))]);
    let b = r.branch(&[("changelog.ts", Some(&CHANGELOG.replacen("[\n", "[\n  { v: 3 },\n", 1)))]);
    let out = r.compose(&r.base, TrainMode::Test, vec![one(1, &a), one(2, &b)]).outcome;
    assert_eq!(as_json(&out.units[1]), json!({ "prs": [2], "outcome": "conflict", "files": ["changelog.ts"], "resolved": [], "against": "batch" }));
}

// ── licence inventories regenerate on the composed tree (#LOGS-2) ──────────
// THIS repo's rules (paths, order, commands), each command stubbed by what
// it does: lockfiles = bun.lock from the manifest, collect:npm = the
// inventory from bun.lock, notices = NOTICES.txt from the inventory.

const REPO_CONFIG: &str = include_str!("../../../../../.exponential/merge-train.json");
const INVENTORY: &str = "packages/licenses/inventory/npm-web.json";
const NOTICES: &str = "apps/web/public/NOTICES.txt";

fn repo_config() -> TrainRepoConfig {
    serde_json::from_str(REPO_CONFIG).unwrap()
}

fn repo_rule(name: &str) -> String {
    repo_config().generated.into_iter().find(|r| r.name == name).unwrap().command.unwrap()
}

fn lock_of(deps: &[&str]) -> String {
    deps.iter().map(|d| format!("{d}@1")).collect::<Vec<_>>().join("\n") + "\n"
}

fn inventory_of(lock: &str) -> String {
    let components: Vec<&str> = lock.split('\n').filter(|l| !l.is_empty()).collect();
    format!("{}\n", serde_json::to_string_pretty(&json!({ "components": components })).unwrap())
}

fn notices_of(inventory: &str) -> String {
    let v: Value = serde_json::from_str(inventory).unwrap();
    v["components"].as_array().unwrap().iter().map(|c| format!("== {}\n", c.as_str().unwrap())).collect()
}

fn licence_files(deps: &[&str]) -> BTreeMap<String, String> {
    let lock = lock_of(deps);
    let inventory = inventory_of(&lock);
    BTreeMap::from([
        ("apps/web/package.json".to_string(), format!("{}\n", deps.join("\n"))),
        ("bun.lock".to_string(), lock),
        (INVENTORY.to_string(), inventory.clone()),
        (NOTICES.to_string(), notices_of(&inventory)),
    ])
}

fn as_files(map: &BTreeMap<String, String>) -> Vec<(&str, Option<&str>)> {
    map.iter().map(|(k, v)| (k.as_str(), Some(v.as_str()))).collect()
}

fn stub_runner(ran: &Mutex<Vec<String>>) -> impl Fn(&str, &Path) -> bool + '_ {
    move |command: &str, dir: &Path| {
        ran.lock().unwrap().push(command.to_string());
        let at = |p: &str| dir.join(p);
        let config = repo_config();
        if Some(command) == config.setup.as_deref() {
            return true;
        }
        if command == repo_rule("lockfiles") {
            let manifest = std::fs::read_to_string(at("apps/web/package.json")).unwrap();
            let deps: Vec<&str> = manifest.split('\n').filter(|l| !l.is_empty()).collect();
            std::fs::write(at("bun.lock"), lock_of(&deps)).unwrap();
        } else if command == repo_rule("npm licence inventory") {
            std::fs::write(at(INVENTORY), inventory_of(&std::fs::read_to_string(at("bun.lock")).unwrap())).unwrap();
        } else if command == repo_rule("licence notices") {
            std::fs::write(at(NOTICES), notices_of(&std::fs::read_to_string(at(INVENTORY)).unwrap())).unwrap();
        } else {
            return false;
        }
        true
    }
}

fn licence_repo(extra: &[(&str, &str)]) -> Repo {
    let base = licence_files(&["a", "m", "z"]);
    let mut files: Vec<(&str, &str)> = base.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    files.extend_from_slice(extra);
    Repo::new(&files, Some(&repo_config()))
}

#[test]
fn two_units_that_each_add_an_npm_dependency_compose_the_inventory_and_the_notices_carry_both_ci_runs() {
    if skip() { return }
    let r = licence_repo(&[]);
    let one_ = r.branch(&as_files(&licence_files(&["a", "b", "m", "z"])));
    let two = r.branch(&as_files(&licence_files(&["a", "m", "y", "z"])));
    let ran = Mutex::new(vec![]);
    let runner = stub_runner(&ran);
    let ComposeResult { outcome, dispatch } =
        compose_with(&r.dir, &r.input("t", &r.base, TrainMode::Test, vec![one(1, &one_), one(2, &two)]), &runner).unwrap();
    let got: Vec<Value> = outcome.units.iter().map(|u| json!([u.outcome, u.files, u.resolved])).collect();
    assert_eq!(got, [json!(["composed", [], []]), json!(["composed", [], ["lockfiles", "npm licence inventory", "licence notices"]])]);
    assert_eq!(r.read(INVENTORY), inventory_of(&lock_of(&["a", "b", "m", "y", "z"])));
    assert_eq!(r.read(NOTICES), notices_of(&r.read(INVENTORY)));
    // the composed tree, in policy order: the lock, then its inventory, then the notices
    let config = repo_config();
    assert_eq!(
        *ran.lock().unwrap(),
        [config.setup.unwrap(), repo_rule("lockfiles"), repo_rule("npm licence inventory"), repo_rule("licence notices")]
    );
    assert_eq!((outcome.checks, dispatch), (TrainChecksMode::Dispatched, vec!["test.yml".to_string()]));
}

#[test]
fn a_merge_now_the_rule_fired_for_runs_ci() {
    if skip() { return }
    let r = licence_repo(&[]);
    let landed = r.branch(&as_files(&licence_files(&["a", "b", "m", "z"])));
    let u = r.branch(&as_files(&licence_files(&["a", "m", "y", "z"])));
    let ran = Mutex::new(vec![]);
    let runner = stub_runner(&ran);
    let ComposeResult { outcome, dispatch } = compose_with(&r.dir, &r.input("t", &landed, TrainMode::Untested, vec![one(2, &u)]), &runner).unwrap();
    assert_eq!((outcome.units[0].outcome, outcome.units[0].resolved.clone()), (UnitOutcome::Composed, vec![
        "lockfiles".to_string(),
        "npm licence inventory".to_string(),
        "licence notices".to_string()
    ]));
    assert_eq!(r.read(INVENTORY), inventory_of(&lock_of(&["a", "b", "m", "y", "z"])));
    assert_eq!((outcome.checks, dispatch), (TrainChecksMode::Dispatched, vec!["test.yml".to_string()]));
}

#[test]
fn an_inventory_the_runner_cannot_collect_ios_bounces_instead_of_keeping_ours() {
    if skip() { return }
    const IOS: &str = "packages/licenses/inventory/ios.json";
    let r = licence_repo(&[(IOS, "{\"v\":0}\n")]);
    let landed = r.branch(&[(IOS, Some("{\"v\":1}\n"))]);
    let u = r.branch(&[(IOS, Some("{\"v\":2}\n"))]);
    let ran = Mutex::new(vec![]);
    let runner = stub_runner(&ran);
    let outcome = compose_with(&r.dir, &r.input("t", &landed, TrainMode::Test, vec![one(2, &u)]), &runner).unwrap().outcome;
    assert_eq!((outcome.units[0].outcome, outcome.units[0].files.clone()), (UnitOutcome::Conflict, vec![IOS.to_string()]));
}
