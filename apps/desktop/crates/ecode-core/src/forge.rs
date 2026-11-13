//! eCode's forge (`fixtures/forge.json` is the spec): the PR-tree primitives
//! every provider offers, as ONE pure function over a small repository model.
//! Commits carry per-path changes (content or `None` = removed); a branch
//! names a commit; a PR is a review unit whose base is ANY branch.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{js, supports, Capability, Failure, ForgeReason, Mergeability, OpError, Outcome, PrState, Provider};

/// Per-path changes: content, or `None` = removed.
pub type Changes = BTreeMap<String, Option<String>>;
/// A whole tree: path → content.
pub type Tree = BTreeMap<String, String>;

/// A commit: its parent and its changes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    pub parent: Option<String>,
    pub changes: Changes,
}

/// A review unit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub number: i64,
    pub head: String,
    pub base: String,
    pub title: String,
    pub state: PrState,
    /// The head's commit when it merged or closed.
    pub head_sha: Option<String>,
    /// The squash commit on the base.
    pub merged_sha: Option<String>,
}

/// The repository a forge op runs on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeState {
    pub provider: Provider,
    pub default_branch: String,
    pub commits: BTreeMap<String, Commit>,
    pub branches: BTreeMap<String, String>,
    pub prs: Vec<PullRequest>,
    pub next_pr: i64,
    /// The shared counter behind `m<n>` and `r<n>` commit ids.
    pub next_commit: u64,
}

/// A forge op's outcome.
pub type ForgeOutcome = Outcome<ForgeState>;

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Given {
    provider: Option<Provider>,
    default_branch: Option<String>,
    commits: Option<BTreeMap<String, GivenCommit>>,
    branches: Option<BTreeMap<String, String>>,
    prs: Option<Vec<GivenPr>>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct GivenCommit {
    parent: Option<String>,
    changes: Option<Changes>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GivenPr {
    number: i64,
    head: String,
    base: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    state: Option<PrState>,
    #[serde(default)]
    head_sha: Option<String>,
    #[serde(default)]
    merged_sha: Option<String>,
}

/// A fixture's `given` with the defaults filled in (provider `ecode`,
/// default branch `main`, PRs open, the next PR number after the highest).
pub fn hydrate(given: &Value) -> Result<ForgeState, OpError> {
    let given: Given = serde_json::from_value(given.clone()).map_err(|e| OpError::InvalidGiven(e.to_string()))?;
    let prs: Vec<PullRequest> = given
        .prs
        .unwrap_or_default()
        .into_iter()
        .map(|pr| PullRequest {
            number: pr.number,
            head: pr.head,
            base: pr.base,
            title: pr.title.unwrap_or_default(),
            state: pr.state.unwrap_or(PrState::Open),
            head_sha: pr.head_sha,
            merged_sha: pr.merged_sha,
        })
        .collect();
    let commits = given
        .commits
        .unwrap_or_default()
        .into_iter()
        .map(|(id, c)| (id, Commit { parent: c.parent, changes: c.changes.unwrap_or_default() }))
        .collect();
    Ok(ForgeState {
        provider: given.provider.unwrap_or(Provider::Ecode),
        default_branch: given.default_branch.unwrap_or_else(|| "main".into()),
        commits,
        branches: given.branches.unwrap_or_default(),
        next_pr: prs.iter().map(|pr| pr.number).fold(0, i64::max) + 1,
        prs,
        next_commit: 1,
    })
}

// ── the git model ───────────────────────────────────────────────────────

impl ForgeState {
    /// The commit and its ancestors, newest first.
    pub fn ancestry(&self, id: &str) -> Vec<String> {
        let mut chain: Vec<String> = vec![];
        let mut cursor = Some(id.to_string());
        while let Some(current) = cursor {
            if chain.contains(&current) {
                break;
            }
            cursor = self.commits.get(&current).and_then(|c| c.parent.clone());
            chain.push(current);
        }
        chain
    }

    /// The newest common ancestor of two commits, `None` = unrelated histories.
    pub fn merge_base(&self, a: &str, b: &str) -> Option<String> {
        let seen: BTreeSet<String> = self.ancestry(a).into_iter().collect();
        self.ancestry(b).into_iter().find(|id| seen.contains(id))
    }

    /// The whole tree at a commit (`None` = the empty tree).
    pub fn snapshot(&self, id: Option<&str>) -> Tree {
        let mut tree = Tree::new();
        let Some(id) = id else { return tree };
        for commit_id in self.ancestry(id).iter().rev() {
            let Some(commit) = self.commits.get(commit_id) else { continue };
            apply_changes(&mut tree, &commit.changes);
        }
        tree
    }
}

fn apply_changes(tree: &mut Tree, changes: &Changes) {
    for (path, content) in changes {
        match content {
            Some(content) => tree.insert(path.clone(), content.clone()),
            None => tree.remove(path),
        };
    }
}

fn sorted_paths(trees: &[&Tree]) -> BTreeSet<String> {
    trees.iter().flat_map(|t| t.keys().cloned()).collect()
}

/// A three-way merge of `ours` and `theirs` over `origin`, path by path:
/// the merged tree and the conflicting paths (ours wins there).
pub fn three_way(origin: &Tree, ours: &Tree, theirs: &Tree) -> (Tree, Vec<String>) {
    let mut tree = Tree::new();
    let mut conflicts = vec![];
    for path in sorted_paths(&[origin, ours, theirs]) {
        let (o, a, b) = (origin.get(&path), ours.get(&path), theirs.get(&path));
        let merged = if a == o {
            b
        } else if b == o || a == b {
            a
        } else {
            conflicts.push(path.clone());
            a
        };
        if let Some(content) = merged {
            tree.insert(path, content.clone());
        }
    }
    (tree, conflicts)
}

/// The per-path changes that turn `from` into `to`, in path order.
pub fn diff_trees(from: &Tree, to: &Tree) -> Changes {
    let mut changes = Changes::new();
    for path in sorted_paths(&[from, to]) {
        let (before, after) = (from.get(&path), to.get(&path));
        if before != after {
            changes.insert(path, after.cloned());
        }
    }
    changes
}

// ── the ops ─────────────────────────────────────────────────────────────

enum Stop {
    Refused(ForgeReason),
    Op(OpError),
}

type Step<T> = Result<T, Stop>;

fn refuse<T>(reason: ForgeReason) -> Step<T> {
    Err(Stop::Refused(reason))
}

fn tip(state: &ForgeState, branch: &str, reason: ForgeReason) -> Step<String> {
    state.branches.get(branch).cloned().map_or_else(|| refuse(reason), Ok)
}

fn live_pr(state: &ForgeState, number: Option<&Value>) -> Step<usize> {
    let Some(index) = state.prs.iter().position(|pr| js::is_number(number, pr.number)) else {
        return refuse(ForgeReason::Pr);
    };
    if !state.prs[index].state.is_live() {
        return refuse(ForgeReason::NotOpen);
    }
    Ok(index)
}

/// Would basing a PR with head `head` on `base` close a loop of live PRs?
fn closes_loop(state: &ForgeState, head: &str, base: &str) -> bool {
    let mut cursor = base;
    for _ in 0..=state.prs.len() {
        if cursor == head {
            return true;
        }
        match state.prs.iter().find(|pr| pr.state.is_live() && pr.head == cursor) {
            Some(parent) => cursor = &parent.base,
            None => return false,
        }
    }
    true
}

struct Merge {
    mergeability: Mergeability,
    conflicts: Vec<String>,
    tree: Tree,
    base: String,
}

fn mergeability(state: &ForgeState, pr: &PullRequest) -> Step<Merge> {
    let head = tip(state, &pr.head, ForgeReason::Head)?;
    let base = tip(state, &pr.base, ForgeReason::Base)?;
    let origin = state.merge_base(&head, &base);
    let (tree, conflicts) =
        three_way(&state.snapshot(origin.as_deref()), &state.snapshot(Some(&base)), &state.snapshot(Some(&head)));
    let mergeability = if conflicts.is_empty() { Mergeability::Clean } else { Mergeability::Dirty };
    Ok(Merge { mergeability, conflicts, tree, base })
}

fn new_commit_id(state: &mut ForgeState, prefix: &str) -> String {
    let mut id = format!("{prefix}{}", state.next_commit);
    while state.commits.contains_key(&id) {
        state.next_commit += 1;
        id = format!("{prefix}{}", state.next_commit);
    }
    state.next_commit += 1;
    id
}

fn squash_merge(
    state: &mut ForgeState,
    events: &mut Vec<Value>,
    number: Option<&Value>,
    expected_head: Option<&str>,
) -> Step<String> {
    let index = live_pr(state, number)?;
    let pr = state.prs[index].clone();
    if pr.state == PrState::Draft {
        return refuse(ForgeReason::Draft);
    }
    let head = tip(state, &pr.head, ForgeReason::Head)?;
    if expected_head.is_some_and(|expected| expected != head) {
        return refuse(ForgeReason::HeadMoved);
    }
    let merge = mergeability(state, &pr)?;
    if merge.mergeability == Mergeability::Dirty {
        return refuse(ForgeReason::Dirty);
    }
    let id = new_commit_id(state, "m");
    let changes = diff_trees(&state.snapshot(Some(&merge.base)), &merge.tree);
    state.commits.insert(id.clone(), Commit { parent: Some(merge.base), changes });
    state.branches.insert(pr.base.clone(), id.clone());
    let merged = &mut state.prs[index];
    merged.state = PrState::Merged;
    merged.head_sha = Some(head);
    merged.merged_sha = Some(id.clone());
    events.push(json!({ "kind": "pr_merged", "number": pr.number, "sha": id }));
    // The PR tree: every live child moves onto the merged PR's base (EXP-324).
    for child in state.prs.iter_mut() {
        if child.state.is_live() && child.base == pr.head {
            child.base = pr.base.clone();
            events.push(json!({ "kind": "pr_retargeted", "number": child.number, "from": pr.head, "to": pr.base }));
        }
    }
    if pr.head != state.default_branch && !state.prs.iter().any(|o| o.state.is_live() && o.head == pr.head) {
        state.branches.remove(&pr.head);
        events.push(json!({ "kind": "branch_deleted", "branch": pr.head }));
    }
    Ok(id)
}

/// `{...changes}` of a pushed commit.
fn changes_of(value: Option<&Value>) -> Changes {
    let Some(Value::Object(map)) = value else { return Changes::new() };
    map.iter()
        .map(|(path, content)| {
            let content = match content {
                Value::Null => None,
                other => Some(js::string(Some(other))),
            };
            (path.clone(), content)
        })
        .collect()
}

fn arg_string(args: &Value, key: &str) -> String {
    js::string(args.get(key))
}

fn apply(state: &mut ForgeState, op: &str, args: &Value, events: &mut Vec<Value>) -> Step<Value> {
    let force = args.get("force") == Some(&Value::Bool(true));
    match op {
        "push" => {
            let branch = arg_string(args, "branch");
            let current = state.branches.get(&branch).cloned();
            let target = match args.get("commit").filter(|c| js::truthy(c)) {
                Some(commit) => {
                    let id = js::string(commit.get("id"));
                    if state.commits.contains_key(&id) {
                        return refuse(ForgeReason::CommitExists);
                    }
                    let parent = js::nullish(commit.get("parent")).map(|p| js::string(Some(p)));
                    if parent.as_ref().is_some_and(|p| !state.commits.contains_key(p)) {
                        return refuse(ForgeReason::Parent);
                    }
                    let fast_forward = match &current {
                        None => true,
                        Some(current) => parent.as_ref().is_some_and(|p| state.ancestry(p).contains(current)),
                    };
                    if !fast_forward && !force {
                        return refuse(ForgeReason::NonFastForward);
                    }
                    state.commits.insert(id.clone(), Commit { parent, changes: changes_of(commit.get("changes")) });
                    id
                }
                None => {
                    let target = arg_string(args, "sha");
                    if !state.commits.contains_key(&target) {
                        return refuse(ForgeReason::Commit);
                    }
                    if current.is_some_and(|c| !force && !state.ancestry(&target).contains(&c)) {
                        return refuse(ForgeReason::NonFastForward);
                    }
                    target
                }
            };
            state.branches.insert(branch.clone(), target.clone());
            events.push(json!({ "kind": "pushed", "branch": branch, "sha": target }));
            Ok(json!({ "sha": target }))
        }
        "pr.open" => {
            let head = arg_string(args, "head");
            let base = arg_string(args, "base");
            tip(state, &head, ForgeReason::Head)?;
            tip(state, &base, ForgeReason::Base)?;
            if head == base {
                return refuse(ForgeReason::SameBranch);
            }
            if state.prs.iter().any(|pr| pr.state.is_live() && pr.head == head) {
                return refuse(ForgeReason::Exists);
            }
            if closes_loop(state, &head, &base) {
                return refuse(ForgeReason::Cycle);
            }
            let number = state.next_pr;
            state.next_pr += 1;
            let title = js::nullish(args.get("title")).map_or_else(String::new, |t| js::string(Some(t)));
            let draft = args.get("draft") == Some(&Value::Bool(true));
            state.prs.push(PullRequest {
                number,
                head,
                base,
                title,
                state: if draft { PrState::Draft } else { PrState::Open },
                head_sha: None,
                merged_sha: None,
            });
            events.push(json!({ "kind": "pr_opened", "number": number }));
            Ok(json!({ "number": number }))
        }
        "pr.list_by_base" => {
            let base = arg_string(args, "base");
            let mut numbers: Vec<i64> =
                state.prs.iter().filter(|pr| pr.state.is_live() && pr.base == base).map(|pr| pr.number).collect();
            numbers.sort_unstable();
            Ok(json!({ "numbers": numbers }))
        }
        "pr.retarget" => {
            let index = live_pr(state, args.get("number"))?;
            let base = arg_string(args, "base");
            tip(state, &base, ForgeReason::Base)?;
            let pr = &state.prs[index];
            if base == pr.head {
                return refuse(ForgeReason::SameBranch);
            }
            if !supports(state.provider, Capability::Retarget) {
                return refuse(ForgeReason::Unsupported);
            }
            if base == pr.base {
                return Ok(json!({}));
            }
            if closes_loop(state, &pr.head, &base) {
                return refuse(ForgeReason::Cycle);
            }
            events.push(json!({ "kind": "pr_retargeted", "number": pr.number, "from": pr.base, "to": base }));
            state.prs[index].base = base;
            Ok(json!({}))
        }
        "pr.mergeability" => {
            let index = live_pr(state, args.get("number"))?;
            let merge = mergeability(state, &state.prs[index])?;
            Ok(json!({ "mergeability": merge.mergeability, "conflicts": merge.conflicts }))
        }
        "pr.squash_merge" => {
            let expected = args.get("expectedHead").and_then(Value::as_str);
            let sha = squash_merge(state, events, args.get("number"), expected)?;
            Ok(json!({ "sha": sha }))
        }
        "pr.merge_ordered" => {
            let numbers: &[Value] = args.get("numbers").and_then(Value::as_array).map_or(&[], Vec::as_slice);
            for (i, a) in numbers.iter().enumerate() {
                if numbers[..i].iter().any(|b| js::same_value_zero(a, b)) {
                    return refuse(ForgeReason::Duplicate);
                }
            }
            for number in numbers {
                live_pr(state, Some(number))?;
            }
            let mut merged = vec![];
            for number in numbers {
                // Each merge is its own transaction: a refusal leaves the earlier ones landed.
                let before = state.clone();
                let event_count = events.len();
                match squash_merge(state, events, Some(number), None) {
                    Ok(sha) => merged.push(json!({ "number": number, "sha": sha })),
                    Err(Stop::Refused(reason)) => {
                        *state = before;
                        events.truncate(event_count);
                        let failed = json!({ "number": number, "code": reason.code(), "reason": reason });
                        return Ok(json!({ "merged": merged, "failed": failed }));
                    }
                    Err(other) => return Err(other),
                }
            }
            Ok(json!({ "merged": merged, "failed": null }))
        }
        "pr.close" => {
            let index = live_pr(state, args.get("number"))?;
            let head_sha = state.branches.get(&state.prs[index].head).cloned();
            let pr = &mut state.prs[index];
            pr.state = PrState::Closed;
            pr.head_sha = head_sha;
            events.push(json!({ "kind": "pr_closed", "number": pr.number }));
            Ok(json!({}))
        }
        "pr.files" => {
            let index = live_pr(state, args.get("number"))?;
            let pr = &state.prs[index];
            let head = tip(state, &pr.head, ForgeReason::Head)?;
            let base = tip(state, &pr.base, ForgeReason::Base)?;
            let before = state.snapshot(state.merge_base(&head, &base).as_deref());
            let after = state.snapshot(Some(&head));
            let files: Vec<Value> = diff_trees(&before, &after)
                .into_iter()
                .map(|(path, content)| {
                    let change = match content {
                        None => "removed",
                        Some(_) if before.contains_key(&path) => "modified",
                        Some(_) => "added",
                    };
                    json!({ "path": path, "change": change })
                })
                .collect();
            Ok(json!({ "files": files }))
        }
        "branch.rebase_onto" => {
            let branch = arg_string(args, "branch");
            let from = tip(state, &branch, ForgeReason::Head)?;
            let onto = tip(state, &arg_string(args, "onto"), ForgeReason::Base)?;
            let upstream = arg_string(args, "upstream");
            let chain = state.ancestry(&from);
            let Some(stop) = chain.iter().position(|id| *id == upstream) else {
                return refuse(ForgeReason::Upstream);
            };
            let mut tree = state.snapshot(Some(&onto));
            let mut parent = onto;
            for id in chain[..stop].iter().rev() {
                let original = state.commits.get(id).cloned().unwrap_or_default();
                let before = state.snapshot(original.parent.as_deref());
                let mut next = tree.clone();
                for (path, content) in &original.changes {
                    let (was, now) = (before.get(path), tree.get(path));
                    if now != was && now != content.as_ref() {
                        return refuse(ForgeReason::Dirty);
                    }
                    apply_changes(&mut next, &Changes::from([(path.clone(), content.clone())]));
                }
                let rebased = new_commit_id(state, "r");
                state.commits.insert(rebased.clone(), Commit { parent: Some(parent), changes: diff_trees(&tree, &next) });
                tree = next;
                parent = rebased;
            }
            state.branches.insert(branch.clone(), parent.clone());
            events.push(json!({ "kind": "pushed", "branch": branch, "sha": parent, "forced": true }));
            Ok(json!({ "sha": parent }))
        }
        _ => Err(Stop::Op(OpError::UnknownOp(op.to_string()))),
    }
}

/// One op on the forge: the next state, the events it emits and its result,
/// or a refusal (state unchanged, no events). `Err` = an unknown op.
pub fn forge(state: &ForgeState, op: &str, args: &Value) -> Result<ForgeOutcome, OpError> {
    let mut next = state.clone();
    let mut events = vec![];
    match apply(&mut next, op, args, &mut events) {
        Ok(result) => Ok(Outcome { state: next, events, result: Some(result), failure: None }),
        Err(Stop::Refused(reason)) => {
            Ok(Outcome { state: state.clone(), events: vec![], result: None, failure: Some(Failure::new(reason)) })
        }
        Err(Stop::Op(error)) => Err(error),
    }
}
