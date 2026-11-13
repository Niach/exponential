//! #EXP-1616 THE REPO PAGE ×4 (`packages/domain-contract/src/repo-page.ts`'s
//! twin, iOS ExpCore `RepoPage`, Android `domain/RepoPage.kt`): the PR tree,
//! the branch list, a repo row's checks rollup on Settings › Repositories and
//! the tab a route opens. Byte-locked by `fixtures/repo-page.json` (its
//! `$comment` is the spec); pure.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::rows::PullRequestRow;

const REPO_PAGE_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/repo-page.json");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RepoTabSpec {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ProviderCopy {
    pub github: String,
    pub ecode: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCopy {
    pub title: String,
    pub github: String,
    pub ecode: String,
    pub name: String,
    pub default_branch: String,
    pub mirror: String,
    pub mirror_url: String,
    pub cancel: String,
    pub create: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EmptyCopy {
    pub empty: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BranchesCopy {
    pub empty: String,
    pub default: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoSettingsCopy {
    pub provider: String,
    pub default_branch: String,
    pub clone_url: String,
    pub backups: String,
    pub no_backups: String,
    pub restore: String,
    pub restore_title: String,
    pub restore_confirm: String,
    pub mirror: String,
    pub mirror_url: String,
}

/// One checks state's chip: its label and dot tone (green | amber | red).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CheckChipCopy {
    pub label: String,
    pub tone: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChecksCopy {
    pub success: CheckChipCopy,
    pub pending: CheckChipCopy,
    pub failure: CheckChipCopy,
}

impl ChecksCopy {
    /// The chip of a rollup / PR checks state (`None` = no chip).
    pub fn of(&self, state: &str) -> Option<&CheckChipCopy> {
        match state {
            "success" => Some(&self.success),
            "pending" => Some(&self.pending),
            "failure" => Some(&self.failure),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RepoPageCopy {
    pub provider: ProviderCopy,
    pub add: AddCopy,
    pub pulls: EmptyCopy,
    pub runs: EmptyCopy,
    pub branches: BranchesCopy,
    pub settings: RepoSettingsCopy,
    pub checks: ChecksCopy,
}

impl ProviderCopy {
    /// The provider Badge's words (`github` | `ecode`; unknown = GitHub).
    pub fn label(&self, provider: &str) -> &str {
        if provider == "ecode" { &self.ecode } else { &self.github }
    }
}

/// `fixtures/repo-page.json` minus its cases.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RepoPageSpec {
    pub tabs: Vec<RepoTabSpec>,
    pub copy: RepoPageCopy,
}

pub static REPO_PAGE: LazyLock<RepoPageSpec> =
    LazyLock::new(|| serde_json::from_str(REPO_PAGE_JSON).expect("repo-page.json parses"));

/// One PR as the folds read it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoPr {
    pub id: String,
    pub number: i64,
    pub title: Option<String>,
    pub head: String,
    pub base: String,
    pub state: String,
    pub checks_state: String,
}

impl RepoPr {
    /// A synced `pull_requests` row (absent fields = empty / `none`).
    pub fn from_row(row: &PullRequestRow) -> Self {
        Self {
            id: row.id.clone(),
            number: row.number.unwrap_or(0),
            title: row.title.clone(),
            head: row.head.clone().unwrap_or_default(),
            base: row.base.clone().unwrap_or_default(),
            state: row.state.clone().unwrap_or_default(),
            checks_state: row.checks_state.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| "none".into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoPrRow {
    pub id: String,
    pub number: i64,
    /// `#<number>`.
    pub identifier: String,
    pub title: String,
    pub depth: usize,
    /// `<head> → <base>`.
    pub subtitle: String,
    pub draft: bool,
    /// The rollup chip; `None` = no checks (no chip).
    pub checks: Option<String>,
}

fn live(pr: &RepoPr) -> bool {
    pr.state == "open" || pr.state == "draft"
}

/// The live PRs as a tree: a PR's parent is the live PR whose head is its
/// base (lowest number when two share a head). Roots newest first; children
/// oldest first, depth-first; a loop starts at its newest member.
pub fn repo_pr_tree(prs: &[RepoPr]) -> Vec<RepoPrRow> {
    let live: Vec<&RepoPr> = prs.iter().filter(|pr| live(pr)).collect();
    let mut ascending = live.clone();
    ascending.sort_by_key(|pr| pr.number);
    let mut by_head: HashMap<&str, &RepoPr> = HashMap::new();
    for pr in &ascending {
        by_head.entry(pr.head.as_str()).or_insert(pr);
    }
    let mut children: HashMap<&str, Vec<&RepoPr>> = HashMap::new();
    let mut roots: Vec<&RepoPr> = Vec::new();
    for pr in &live {
        match by_head.get(pr.base.as_str()).filter(|parent| parent.id != pr.id) {
            Some(parent) => children.entry(parent.id.as_str()).or_default().push(pr),
            None => roots.push(pr),
        }
    }
    for list in children.values_mut() {
        list.sort_by_key(|pr| pr.number);
    }
    let mut rows = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    fn visit<'a>(
        pr: &'a RepoPr,
        depth: usize,
        children: &HashMap<&str, Vec<&'a RepoPr>>,
        seen: &mut HashSet<&'a str>,
        rows: &mut Vec<RepoPrRow>,
    ) {
        if !seen.insert(pr.id.as_str()) {
            return;
        }
        rows.push(RepoPrRow {
            id: pr.id.clone(),
            number: pr.number,
            identifier: format!("#{}", pr.number),
            title: pr.title.clone().unwrap_or_default(),
            depth,
            subtitle: format!("{} → {}", pr.head, pr.base),
            draft: pr.state == "draft",
            checks: (pr.checks_state != "none").then(|| pr.checks_state.clone()),
        });
        if let Some(kids) = children.get(pr.id.as_str()) {
            for child in kids {
                visit(child, depth + 1, children, seen, rows);
            }
        }
    }
    roots.sort_by_key(|pr| std::cmp::Reverse(pr.number));
    for root in roots {
        visit(root, 0, &children, &mut seen, &mut rows);
    }
    let mut descending = live;
    descending.sort_by_key(|pr| std::cmp::Reverse(pr.number));
    for pr in descending {
        visit(pr, 0, &children, &mut seen, &mut rows);
    }
    rows
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RepoBranch {
    pub name: String,
    pub sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoBranchRow {
    pub name: String,
    /// The first 7 characters of the sha.
    pub sha: String,
    pub is_default: bool,
    /// The live PR whose head this branch is.
    pub pr_number: Option<i64>,
}

/// The default branch first, then by name (code points = `str` order).
pub fn repo_branches(branches: &[RepoBranch], default_branch: &str, prs: &[RepoPr]) -> Vec<RepoBranchRow> {
    let mut sorted: Vec<&RepoBranch> = branches.iter().collect();
    sorted.sort_by(|a, b| {
        let (ad, bd) = (a.name == default_branch, b.name == default_branch);
        bd.cmp(&ad).then_with(|| a.name.cmp(&b.name))
    });
    sorted
        .into_iter()
        .map(|branch| RepoBranchRow {
            name: branch.name.clone(),
            sha: branch.sha.chars().take(7).collect(),
            is_default: branch.name == default_branch,
            pr_number: prs.iter().filter(|pr| live(pr) && pr.head == branch.name).map(|pr| pr.number).min(),
        })
        .collect()
}

/// A repo row's checks chip over its live PRs: failure (any) · pending (any)
/// · success (all with checks) · `None` = none has checks.
pub fn repo_checks_rollup(prs: &[RepoPr]) -> Option<&'static str> {
    let states: Vec<&str> =
        prs.iter().filter(|pr| live(pr) && pr.checks_state != "none").map(|pr| pr.checks_state.as_str()).collect();
    if states.is_empty() {
        None
    } else if states.contains(&"failure") {
        Some("failure")
    } else if states.contains(&"pending") {
        Some("pending")
    } else {
        Some("success")
    }
}

/// The tab a route's `tab` param opens (unknown = `pulls`).
pub fn repo_tab(param: Option<&str>) -> &'static str {
    match param {
        Some("runs") => "runs",
        Some("branches") => "branches",
        Some("settings") => "settings",
        _ => "pulls",
    }
}

/// The merge-train pill label of a live PR row (a draft reads the draft pill).
pub fn merge_pill_label(row: &PullRequestRow) -> String {
    let state = crate::merge_train::pr_merge_state(row);
    crate::merge_train::status_pill(crate::merge_train::StatusPillInput {
        state,
        wait: row.merge_wait.as_deref(),
        position: row.merge_position,
        batch_size: None,
        hold_reason: row.merge_hold.as_deref(),
        stack: false,
        checks: 0,
    })
    .label
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture() -> Value {
        serde_json::from_str(REPO_PAGE_JSON).unwrap()
    }

    fn prs(value: &Value) -> Vec<RepoPr> {
        serde_json::from_value(value.clone()).unwrap()
    }

    #[test]
    fn the_spec_loads() {
        let ids: Vec<&str> = REPO_PAGE.tabs.iter().map(|tab| tab.id.as_str()).collect();
        assert_eq!(ids, ["pulls", "runs", "branches", "settings"]);
        assert_eq!(REPO_PAGE.copy.provider.label("ecode"), "Exponential");
        assert_eq!(REPO_PAGE.copy.checks.of("failure").map(|chip| chip.tone.as_str()), Some("red"));
        assert!(REPO_PAGE.copy.checks.of("none").is_none());
    }

    #[test]
    fn repo_page_tree_cases() {
        let fixture = fixture();
        let cases = fixture["treeCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let got = serde_json::to_value(repo_pr_tree(&prs(&case["prs"]))).unwrap();
            assert_eq!(got, case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn repo_page_branch_cases() {
        let fixture = fixture();
        let cases = fixture["branchCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let branches: Vec<RepoBranch> = serde_json::from_value(case["branches"].clone()).unwrap();
            let rows = repo_branches(&branches, case["defaultBranch"].as_str().unwrap(), &prs(&case["prs"]));
            assert_eq!(serde_json::to_value(rows).unwrap(), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn repo_page_rollup_cases() {
        let fixture = fixture();
        let cases = fixture["rollupCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let got = serde_json::to_value(repo_checks_rollup(&prs(&case["prs"]))).unwrap();
            assert_eq!(got, case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn repo_page_tab_cases() {
        let fixture = fixture();
        let cases = fixture["tabCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            assert_eq!(repo_tab(case["param"].as_str()), case["expected"].as_str().unwrap(), "{}", case["param"]);
        }
    }
}
