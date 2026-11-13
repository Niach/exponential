//! `packages/ecode/fixtures/forge.json` replayed against a REAL git repo:
//! each case's commits are materialised into a temp bare repository, its
//! PRs imported into a file Store, and every step runs through
//! `LocalForge` (gix + git argv). Events are read by polling.
//!
//! The fixture names commits (`c1`, `m1`, `r2`); git names them by hash. The
//! harness keeps one bijection: given commits are bound when written, a new
//! sha is bound to the name the fixture expects at the same place in the
//! step, and at the end every created commit's parent and per-path changes
//! must equal the fixture's. A name bound twice fails the comparison.

use std::collections::BTreeMap;
use std::path::PathBuf;

use ecode::core::{Failure, PrState, PullRequest};
use ecode::{Error, Expected, FileStore, Forge, Identity, LocalForge, LocalRepo, OpenPr, PushSource};
use serde_json::{json, Map, Value};

const FORGE: &str = include_str!("../../../../../packages/ecode/fixtures/forge.json");

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ecode-replay-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// name ↔ sha.
#[derive(Default)]
struct Names {
    by_sha: BTreeMap<String, String>,
    by_name: BTreeMap<String, String>,
}

fn is_sha(s: &str) -> bool {
    s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit())
}

impl Names {
    fn bind(&mut self, name: &str, sha: &str) {
        self.by_sha.insert(sha.into(), name.into());
        self.by_name.insert(name.into(), sha.into());
    }

    /// A fixture name → its sha; anything else passes through ("nope").
    fn sha(&self, name: &str) -> String {
        self.by_name.get(name).cloned().unwrap_or_else(|| name.to_string())
    }

    /// `actual` with shas renamed, binding new ones to `expected`'s names.
    fn unify(&mut self, actual: &Value, expected: Option<&Value>) -> Value {
        match actual {
            Value::String(s) if is_sha(s) => {
                if let Some(name) = self.by_sha.get(s) {
                    return json!(name);
                }
                match expected.and_then(Value::as_str) {
                    Some(name) if !self.by_name.contains_key(name) => {
                        self.bind(name, s);
                        json!(name)
                    }
                    _ => actual.clone(),
                }
            }
            Value::Array(items) => Value::Array(
                items.iter().enumerate().map(|(i, v)| self.unify(v, expected.and_then(|e| e.get(i)))).collect(),
            ),
            Value::Object(map) => Value::Object(
                map.iter().map(|(k, v)| (k.clone(), self.unify(v, expected.and_then(|e| e.get(k))))).collect(),
            ),
            other => other.clone(),
        }
    }
}

fn str_arg(args: &Value, key: &str) -> String {
    args.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn int_arg(args: &Value, key: &str) -> i64 {
    args.get(key).and_then(Value::as_i64).unwrap_or(-1)
}

fn changes(value: Option<&Value>) -> BTreeMap<String, Option<Vec<u8>>> {
    value
        .and_then(Value::as_object)
        .map(|m| m.iter().map(|(p, c)| (p.clone(), c.as_str().map(|s| s.as_bytes().to_vec()))).collect())
        .unwrap_or_default()
}

fn failure(error: Error) -> Failure {
    match error {
        Error::Refused(failure) => failure,
        other => panic!("not a refusal: {other}"),
    }
}

/// One step through the forge: `Ok(result)` or the refusal.
fn run(forge: &LocalForge<FileStore>, names: &Names, op: &str, args: &Value) -> Result<Value, Failure> {
    let out: Result<Value, Error> = match op {
        "push" => {
            let force = args.get("force") == Some(&Value::Bool(true));
            let branch = str_arg(args, "branch");
            let source = match args.get("commit") {
                Some(commit) => PushSource::Commit {
                    parent: commit.get("parent").and_then(Value::as_str).map(|p| names.sha(p)),
                    changes: changes(commit.get("changes")),
                    message: str_arg(commit, "id"),
                },
                None => PushSource::Sha(names.sha(&str_arg(args, "sha"))),
            };
            forge.push(&branch, source, force).map(|sha| json!({ "sha": sha }))
        }
        "pr.open" => forge
            .open(OpenPr {
                head: str_arg(args, "head"),
                base: str_arg(args, "base"),
                title: str_arg(args, "title"),
                draft: args.get("draft") == Some(&Value::Bool(true)),
            })
            .map(|number| json!({ "number": number })),
        "pr.list_by_base" => forge.list_by_base(&str_arg(args, "base")).map(|numbers| json!({ "numbers": numbers })),
        "pr.retarget" => forge.retarget(int_arg(args, "number"), &str_arg(args, "base")).map(|()| json!({})),
        "pr.mergeability" => forge.mergeability(int_arg(args, "number")).map(|check| json!(check)),
        "pr.squash_merge" => {
            let expected = args.get("expectedHead").and_then(Value::as_str).map(|h| names.sha(h));
            forge.squash_merge(int_arg(args, "number"), expected.as_deref()).map(|sha| json!({ "sha": sha }))
        }
        "pr.merge_ordered" => {
            let numbers: Vec<i64> = args["numbers"].as_array().unwrap().iter().map(|n| n.as_i64().unwrap()).collect();
            forge.merge_ordered(&numbers).map(|out| {
                json!({
                    "merged": out.merged,
                    "failed": out.failed.map(|f| json!({ "number": f.number, "code": f.failure.code, "reason": f.failure.reason })),
                })
            })
        }
        "pr.close" => forge.close(int_arg(args, "number")).map(|()| json!({})),
        "pr.files" => forge.files(int_arg(args, "number")).map(|files| json!({ "files": files })),
        "branch.rebase_onto" => forge
            .rebase_onto(&str_arg(args, "branch"), &str_arg(args, "onto"), &names.sha(&str_arg(args, "upstream")))
            .map(|sha| json!({ "sha": sha })),
        other => panic!("unknown op {other}"),
    };
    out.map_err(failure)
}

fn replay(index: usize, case: &Value) {
    let name = format!("forge.json: {}", case["name"].as_str().unwrap());
    let dir = TempDir::new(&index.to_string());
    let given = &case["given"];
    let repo = LocalRepo::init_bare(dir.0.join("repo.git"))
        .unwrap()
        .with_identity(Identity { name: "Fixture".into(), email: "fixture@ecode.test".into() });
    let mut names = Names::default();

    // Materialise the given commits, parents first.
    let commits = given.get("commits").and_then(Value::as_object).cloned().unwrap_or_default();
    let mut pending: Vec<&String> = commits.keys().collect();
    while !pending.is_empty() {
        let before = pending.len();
        pending.retain(|id| {
            let commit = &commits[id.as_str()];
            let parent = commit.get("parent").and_then(Value::as_str);
            if parent.is_some_and(|p| !names.by_name.contains_key(p)) {
                return true;
            }
            let sha = repo.commit(parent.map(|p| names.sha(p)).as_deref(), &changes(commit.get("changes")), id).unwrap();
            names.bind(id, &sha);
            false
        });
        assert!(pending.len() < before, "{name}: a given commit's parent is missing");
    }
    for (branch, id) in given.get("branches").and_then(Value::as_object).cloned().unwrap_or_default() {
        repo.set_branch(&branch, &names.sha(id.as_str().unwrap()), Expected::Any).unwrap();
    }
    let given_shas: Vec<String> = names.by_sha.keys().cloned().collect();

    let store = FileStore::open(dir.0.join("store")).unwrap();
    let default_branch = given.get("defaultBranch").and_then(Value::as_str).unwrap_or("main");
    let forge = LocalForge::new(repo, store, default_branch);
    let prs: Vec<PullRequest> = given
        .get("prs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|pr| PullRequest {
            number: pr["number"].as_i64().unwrap(),
            head: str_arg(&pr, "head"),
            base: str_arg(&pr, "base"),
            title: str_arg(&pr, "title"),
            state: pr.get("state").map_or(PrState::Open, |s| serde_json::from_value(s.clone()).unwrap()),
            head_sha: pr.get("headSha").and_then(Value::as_str).map(|s| names.sha(s)),
            merged_sha: pr.get("mergedSha").and_then(Value::as_str).map(|s| names.sha(s)),
        })
        .collect();
    if !prs.is_empty() {
        forge.import(prs).unwrap();
    }
    let mut cursor = forge.events(None).unwrap().cursor;

    for (n, step) in case["steps"].as_array().unwrap().iter().enumerate() {
        let op = step["op"].as_str().unwrap();
        let args = step.get("args").cloned().unwrap_or_else(|| json!({}));
        let label = format!("{name} step {} ({op})", n + 1);
        let mut out = Map::new();
        out.insert("op".into(), step["op"].clone());
        out.insert("args".into(), args.clone());
        // Git ids are hashes: re-pushing a commit under an id the fixture
        // already used cannot reach a real forge (the content would be a
        // different commit). The model's `commit_exists` is the harness's.
        let reused = args.pointer("/commit/id").and_then(Value::as_str).filter(|id| names.by_name.contains_key(*id));
        let outcome = match reused {
            Some(_) => Err(Failure::new(ecode::core::ForgeReason::CommitExists)),
            None => run(&forge, &names, op, &args),
        };
        match outcome {
            Ok(result) => {
                let unified = names.unify(&result, step.get("result"));
                out.insert("result".into(), unified);
            }
            Err(failure) => {
                out.insert("failure".into(), json!(failure));
            }
        }
        let polled = forge.events(Some(&cursor)).unwrap();
        cursor = polled.cursor;
        let events = names.unify(&json!(polled.events), step.get("events"));
        out.insert("events".into(), events);
        assert_eq!(Value::Object(out), *step, "{label}");
    }

    // The end state: branches, PRs and the commits the case created.
    let expect = &case["expect"];
    let branches = forge.repo().branches().unwrap();
    let branches = names.unify(&json!(branches), expect.get("branches"));
    assert_eq!(branches, expect["branches"], "{name}: branches");
    let prs = names.unify(&json!(forge.prs().unwrap()), expect.get("prs"));
    assert_eq!(prs, expect["prs"], "{name}: prs");
    let mut created = Map::new();
    for (sha, commit_name) in names.by_sha.clone() {
        if given_shas.contains(&sha) {
            continue;
        }
        let repo = forge.repo();
        let entry = repo.log(&sha, 1).unwrap().pop().unwrap();
        let parent = entry.parents.first().cloned();
        let before = repo.tree(parent.as_deref()).unwrap();
        let diff = ecode::core::diff_trees(&before, &repo.tree(Some(&sha)).unwrap());
        let changes: Map<String, Value> = diff
            .keys()
            .map(|path| {
                let content = repo.read_file(&sha, path).unwrap().map(|b| String::from_utf8(b).unwrap());
                (path.clone(), json!(content))
            })
            .collect();
        let parent = parent.map(|p| names.by_sha.get(&p).cloned().unwrap_or(p));
        created.insert(commit_name, json!({ "parent": parent, "changes": changes }));
    }
    assert_eq!(Value::Object(created), expect["commits"], "{name}: created commits");
}

#[test]
fn forge_fixture_replays_on_a_real_repo() {
    if !ecode::git_available() {
        panic!("the replay needs git on PATH");
    }
    let fixture: Value = serde_json::from_str(FORGE).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        replay(index, case);
    }
}
