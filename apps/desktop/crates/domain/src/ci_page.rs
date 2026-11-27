//! #EXP-2172 — THE CI PAGE, the pure half ×4
//! (`packages/domain-contract/fixtures/ci-page.json`; TS reference
//! `packages/domain-contract/src/ci-page.ts`): one root row after Devices
//! across every member team: our runs and GitHub Actions runs in one list,
//! the merge train's current batches, the runner slots, and the row's badge.
//! The statuses are ci-run.json `runStatus`, the copy is read off the
//! fixtures (one source).

use std::collections::{BTreeMap, HashSet};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ci_run::{ci_run_status_of, runner_os_label, CiRunStatus};
use crate::merge_train::{ordinal, MERGE_TRAIN};
use crate::nav_badges::{CountBadgeTone, CountBadgeValue};
use crate::pr_checks::format_check_duration;

const CI_PAGE_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/ci-page.json");

/// ci-page.json as parsed once.
pub static CI_PAGE: LazyLock<Value> = LazyLock::new(|| serde_json::from_str(CI_PAGE_JSON).expect("ci-page.json parses"));

/// ci-page.json `copy.<path>`.
pub fn copy(path: &str) -> &'static str {
    path.split('.').fold(&CI_PAGE["copy"], |value, key| &value[key]).as_str().unwrap_or_default()
}

/// ci-page.json `testIds.<key>`.
pub fn test_id(key: &str) -> &'static str {
    CI_PAGE["testIds"][key].as_str().unwrap_or_default()
}

fn limit(key: &str) -> i64 {
    CI_PAGE["limits"][key].as_i64().unwrap_or_default()
}

// ── the input ────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiPageRun {
    pub id: String,
    pub repository_id: String,
    pub number: i64,
    pub name: String,
    #[serde(default)]
    pub sha: String,
    #[serde(default)]
    pub r#ref: Option<String>,
    #[serde(default)]
    pub trigger: String,
    pub state: String,
    #[serde(default)]
    pub pull_request_id: Option<String>,
    #[serde(default)]
    pub created_by_user_id: Option<String>,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub created_at: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiPageJob {
    pub run_id: String,
    pub status: String,
    #[serde(default)]
    pub conclusion: Option<String>,
    #[serde(default)]
    pub runs_on: Vec<String>,
    #[serde(default)]
    pub runner_device_id: Option<String>,
    #[serde(default)]
    pub provider: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiPageRepo {
    pub id: String,
    pub full_name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiPagePr {
    pub id: String,
    pub number: i64,
    pub title: String,
    pub repository_id: String,
    #[serde(default)]
    pub root_session_id: Option<String>,
    #[serde(default)]
    pub base: String,
    #[serde(default)]
    pub merge_state: Option<String>,
    #[serde(default)]
    pub merge_position: Option<i64>,
    #[serde(default)]
    pub merge_train_id: Option<String>,
}

/// A session row: its starter.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiPageSession {
    pub id: String,
    pub user_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiPageRunner {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub slots: i64,
    #[serde(default)]
    pub last_seen_at: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiPageInput {
    #[serde(default)]
    pub runs: Vec<CiPageRun>,
    #[serde(default)]
    pub jobs: Vec<CiPageJob>,
    #[serde(default)]
    pub repos: Vec<CiPageRepo>,
    #[serde(default)]
    pub pull_requests: Vec<CiPagePr>,
    #[serde(default)]
    pub sessions: Vec<CiPageSession>,
    #[serde(default)]
    pub runners: Vec<CiPageRunner>,
    #[serde(default)]
    pub viewer_id: String,
    #[serde(default)]
    pub now: String,
}

fn ms_of(value: Option<&str>) -> Option<i64> {
    value.filter(|v| !v.is_empty()).and_then(crate::instant::parse_instant)
}

/// A run's source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CiSource {
    Exponential,
    Github,
}

impl CiSource {
    pub fn id(self) -> &'static str {
        match self {
            Self::Exponential => "exponential",
            Self::Github => "github",
        }
    }
}

/// `ciSource`: anything but `github` is ours.
pub fn ci_source(provider: &str) -> CiSource {
    if provider == "github" {
        CiSource::Github
    } else {
        CiSource::Exponential
    }
}

/// The status filter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CiFilter {
    #[default]
    All,
    Running,
    Failed,
    Mine,
}

impl CiFilter {
    pub fn id(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Running => "running",
            Self::Failed => "failed",
            Self::Mine => "mine",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "running" => Self::Running,
            "failed" => Self::Failed,
            "mine" => Self::Mine,
            _ => Self::All,
        }
    }
}

/// The page's tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CiTab {
    #[default]
    Runs,
    Train,
    Runners,
}

impl CiTab {
    pub fn id(self) -> &'static str {
        match self {
            Self::Runs => "runs",
            Self::Train => "train",
            Self::Runners => "runners",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "runs" => Some(Self::Runs),
            "train" => Some(Self::Train),
            "runners" => Some(Self::Runners),
            _ => None,
        }
    }
}

/// The Runs tab's filter (`None` = All).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiRunFilter {
    pub status: CiFilter,
    #[serde(default)]
    pub repository_id: Option<String>,
    #[serde(default)]
    pub source: Option<CiSource>,
}

impl CiRunFilter {
    /// Anything but All, All repositories, Every source.
    pub fn filtered(&self) -> bool {
        self.status != CiFilter::All || self.repository_id.is_some() || self.source.is_some()
    }
}

fn run_at(run: &CiPageRun) -> i64 {
    ms_of(run.started_at.as_deref()).or_else(|| ms_of(Some(&run.created_at))).unwrap_or(0)
}

/// Newest first; ties by id.
fn newest_first(runs: &[CiPageRun]) -> Vec<&CiPageRun> {
    let mut sorted: Vec<&CiPageRun> = runs.iter().collect();
    sorted.sort_by(|a, b| run_at(b).cmp(&run_at(a)).then_with(|| a.id.cmp(&b.id)));
    sorted
}

fn status_of(run: &CiPageRun, jobs: &[CiPageJob]) -> CiRunStatus {
    let jobs: Vec<(&str, Option<&str>)> =
        jobs.iter().filter(|job| job.run_id == run.id).map(|job| (job.status.as_str(), job.conclusion.as_deref())).collect();
    ci_run_status_of(&run.state, &jobs)
}

/// The glyph and tone of a status (repo-ci.json).
fn glyph_of(status: CiRunStatus) -> (&'static str, &'static str) {
    match status {
        CiRunStatus::Queued => ("ci-queued", "muted"),
        CiRunStatus::Running => ("ci-running", "muted"),
        CiRunStatus::Passed => ("ci-passed", "green"),
        CiRunStatus::Failed => ("ci-failed", "red"),
        CiRunStatus::Cancelled => ("ci-skipped", "muted"),
    }
}

/// `mine`: the viewer started the run, or its pull request's chat is theirs.
pub fn ci_is_mine(run: &CiPageRun, viewer_id: &str, pull_requests: &[CiPagePr], sessions: &[CiPageSession]) -> bool {
    if run.created_by_user_id.as_deref() == Some(viewer_id) {
        return true;
    }
    let Some(pr_id) = run.pull_request_id.as_deref() else { return false };
    let Some(root) = pull_requests.iter().find(|pr| pr.id == pr_id).and_then(|pr| pr.root_session_id.as_deref()) else { return false };
    sessions.iter().any(|session| session.id == root && session.user_id == viewer_id)
}

/// One row of the Runs tab.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CiRunListRow {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub status: &'static str,
    pub status_label: String,
    pub glyph: &'static str,
    pub tone: &'static str,
    pub repository_id: String,
    pub repository: String,
    pub source: &'static str,
    pub source_label: String,
    pub r#ref: String,
    pub trigger: String,
    pub duration: Option<String>,
    pub started_at: Option<String>,
    pub subtitle: String,
}

fn trigger_text(run: &CiPageRun, pull_requests: &[CiPagePr]) -> String {
    if run.trigger == "pull_request" {
        if let Some(pr) = run.pull_request_id.as_deref().and_then(|id| pull_requests.iter().find(|pr| pr.id == id)) {
            return copy("pullRequest").replace("{number}", &pr.number.to_string());
        }
    }
    match CI_PAGE["copy"]["triggers"][&run.trigger].as_str() {
        Some(text) => text.to_string(),
        None => run.trigger.clone(),
    }
}

/// A repository's full name (copy.unknownRepository while not synced).
pub fn repo_name(repos: &[CiPageRepo], id: &str) -> String {
    repos.iter().find(|repo| repo.id == id).map(|repo| repo.full_name.clone()).unwrap_or_else(|| copy("unknownRepository").to_string())
}

/// `ciRunList`: every run of the filter, newest first, at most `limits.runs`.
pub fn ci_run_list(input: &CiPageInput, filter: &CiRunFilter) -> Vec<CiRunListRow> {
    let at = ms_of(Some(&input.now)).unwrap_or(0);
    let max = limit("runs").max(0) as usize;
    let mut rows = Vec::new();
    for run in newest_first(&input.runs) {
        let status = status_of(run, &input.jobs);
        let source = ci_source(&run.provider);
        if filter.repository_id.as_deref().is_some_and(|id| id != run.repository_id) {
            continue;
        }
        if filter.source.is_some_and(|wanted| wanted != source) {
            continue;
        }
        let keep = match filter.status {
            CiFilter::All => true,
            CiFilter::Running => matches!(status, CiRunStatus::Running | CiRunStatus::Queued),
            CiFilter::Failed => status == CiRunStatus::Failed,
            CiFilter::Mine => ci_is_mine(run, &input.viewer_id, &input.pull_requests, &input.sessions),
        };
        if !keep {
            continue;
        }
        let started = ms_of(run.started_at.as_deref());
        let repository = repo_name(&input.repos, &run.repository_id);
        let r#ref = run.r#ref.clone().unwrap_or_else(|| crate::repo_ci::copy("runs.noRef").to_string());
        let source_label = copy(&format!("sources.{}", source.id())).to_string();
        let (glyph, tone) = glyph_of(status);
        rows.push(CiRunListRow {
            id: run.id.clone(),
            identifier: format!("#{}", run.number),
            title: format!("#{} {}", run.number, run.name),
            status: status.id(),
            status_label: crate::repo_ci::copy(&format!("status.{}", status.id())).to_string(),
            glyph,
            tone,
            repository_id: run.repository_id.clone(),
            subtitle: format!("{repository} · {} · {source_label}", r#ref),
            repository,
            source: source.id(),
            source_label,
            r#ref,
            trigger: trigger_text(run, &input.pull_requests),
            duration: started.map(|started| format_check_duration(ms_of(run.completed_at.as_deref()).unwrap_or(at) - started)),
            started_at: run.started_at.clone(),
        });
        if rows.len() >= max {
            break;
        }
    }
    rows
}

/// A pick's option.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CiOption {
    pub label: String,
    pub value: String,
}

/// `ciRepoOptions`: All repositories, then every repository with a run, by name.
pub fn ci_repo_options(input: &CiPageInput) -> Vec<CiOption> {
    let mut ids: Vec<&str> = Vec::new();
    for run in &input.runs {
        if !ids.contains(&run.repository_id.as_str()) {
            ids.push(&run.repository_id);
        }
    }
    let mut named: Vec<CiOption> = ids.into_iter().map(|id| CiOption { label: repo_name(&input.repos, id), value: id.to_string() }).collect();
    named.sort_by(|a, b| a.label.cmp(&b.label).then_with(|| a.value.cmp(&b.value)));
    let mut out = vec![CiOption { label: copy("repositories.all").to_string(), value: "all".into() }];
    out.extend(named);
    out
}

/// One tile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CiPageTile {
    pub label: String,
    pub value: String,
    pub tone: &'static str,
    pub delta: Option<String>,
}

fn online(runner: &CiPageRunner, now: i64) -> bool {
    ms_of(runner.last_seen_at.as_deref()).is_some_and(|seen| now - seen < limit("runnerOnlineMs"))
}

/// `ciPageTiles`: Running, Queued, Failed today, Runners.
pub fn ci_page_tiles(input: &CiPageInput) -> Vec<CiPageTile> {
    let at = ms_of(Some(&input.now)).unwrap_or(0);
    let (mut running, mut queued) = (0, 0);
    let mut failed: Vec<&CiPageRun> = Vec::new();
    for run in &input.runs {
        match status_of(run, &input.jobs) {
            CiRunStatus::Running => running += 1,
            CiRunStatus::Queued => queued += 1,
            CiRunStatus::Failed => {
                if ms_of(run.completed_at.as_deref()).is_some_and(|done| at - done < 24 * 3_600_000) {
                    failed.push(run);
                }
            }
            _ => {}
        }
    }
    failed.sort_by_key(|run| std::cmp::Reverse(ms_of(run.completed_at.as_deref()).unwrap_or(0)));
    let delta = failed.first().map(|run| {
        let repo = repo_name(&input.repos, &run.repository_id);
        repo.rsplit('/').next().unwrap_or(&repo).to_string()
    });
    let online_count = input.runners.iter().filter(|runner| online(runner, at)).count();
    vec![
        CiPageTile { label: copy("tiles.running").into(), value: running.to_string(), tone: "neutral", delta: None },
        CiPageTile { label: copy("tiles.queued").into(), value: queued.to_string(), tone: "neutral", delta: None },
        CiPageTile {
            label: copy("tiles.failedToday").into(),
            value: failed.len().to_string(),
            tone: if failed.is_empty() { "neutral" } else { "danger" },
            delta,
        },
        CiPageTile {
            label: copy("tiles.runners").into(),
            value: copy("tiles.runnersValue").replace("{online}", &online_count.to_string()).replace("{total}", &input.runners.len().to_string()),
            tone: "neutral",
            delta: None,
        },
    ]
}

/// `ciBadge` (nav-badges.json destination `ci`): the viewer's newest run per
/// (repository, pipeline, ref): failed ones = attention, else the live ones
/// = muted.
pub fn ci_badge(input: &CiPageInput) -> CountBadgeValue {
    let mut seen: HashSet<(String, String, String)> = HashSet::new();
    let (mut failed, mut live) = (0, 0);
    for run in newest_first(&input.runs) {
        if !ci_is_mine(run, &input.viewer_id, &input.pull_requests, &input.sessions) {
            continue;
        }
        if !seen.insert((run.repository_id.clone(), run.name.clone(), run.r#ref.clone().unwrap_or_default())) {
            continue;
        }
        match status_of(run, &input.jobs) {
            CiRunStatus::Failed => failed += 1,
            CiRunStatus::Running | CiRunStatus::Queued => live += 1,
            _ => {}
        }
    }
    if failed > 0 {
        CountBadgeValue { count: failed, tone: CountBadgeTone::Attention }
    } else {
        CountBadgeValue { count: live, tone: CountBadgeTone::Muted }
    }
}

/// One merge-train row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CiTrainRow {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub glyph: &'static str,
    pub badge: String,
    /// The PR a press opens (a batch: its first member).
    pub pull_request_id: String,
}

/// One repository's train.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CiTrainGroup {
    pub repository_id: String,
    pub title: String,
    pub caption: String,
    pub rows: Vec<CiTrainRow>,
}

const TRAIN_WAITING: &[&str] = &["queued", "checks", "held", "conflict", "failing"];

/// `ciTrainGroups`: per repository its testing batches, then (`queue`) the
/// queued PRs by position and the waiting ones by number.
pub fn ci_train_groups(input: &CiPageInput, queue: bool) -> Vec<CiTrainGroup> {
    let pill = &MERGE_TRAIN.copy.pill;
    // Insertion order of first appearance (a JS Map).
    let mut order: Vec<String> = Vec::new();
    let mut by_repo: BTreeMap<String, Vec<&CiPagePr>> = BTreeMap::new();
    for pr in &input.pull_requests {
        let state = pr.merge_state.as_deref();
        if state != Some("testing") && !(queue && state.is_some_and(|s| TRAIN_WAITING.contains(&s))) {
            continue;
        }
        if !by_repo.contains_key(&pr.repository_id) {
            order.push(pr.repository_id.clone());
        }
        by_repo.entry(pr.repository_id.clone()).or_default().push(pr);
    }
    let mut groups = Vec::new();
    for repository_id in order {
        let prs = &by_repo[&repository_id];
        let mut rows = Vec::new();
        let mut testing: Vec<&CiPagePr> = prs.iter().copied().filter(|pr| pr.merge_state.as_deref() == Some("testing")).collect();
        testing.sort_by_key(|pr| pr.number);
        let mut batch_keys: Vec<String> = Vec::new();
        let mut batches: BTreeMap<String, Vec<&CiPagePr>> = BTreeMap::new();
        for pr in testing {
            let key = pr.merge_train_id.clone().unwrap_or_else(|| pr.id.clone());
            if !batches.contains_key(&key) {
                batch_keys.push(key.clone());
            }
            batches.entry(key).or_default().push(pr);
        }
        batch_keys.sort_by_key(|key| batches[key][0].number);
        for key in batch_keys {
            let members = &batches[&key];
            let alone = members.len() == 1;
            rows.push(CiTrainRow {
                id: format!("batch.{key}"),
                identifier: format!("#{}", members[0].number),
                title: if alone { members[0].title.clone() } else { copy("train.batch").replace("{count}", &members.len().to_string()) },
                subtitle: (!alone).then(|| members.iter().map(|m| format!("#{} {}", m.number, m.title)).collect::<Vec<_>>().join(" · ")),
                glyph: "ci-running",
                badge: pill.testing.clone(),
                pull_request_id: members[0].id.clone(),
            });
        }
        if queue {
            let mut queued: Vec<&CiPagePr> = prs.iter().copied().filter(|pr| pr.merge_state.as_deref() == Some("queued")).collect();
            queued.sort_by(|a, b| a.merge_position.unwrap_or(i64::MAX).cmp(&b.merge_position.unwrap_or(i64::MAX)).then(a.number.cmp(&b.number)));
            for pr in queued {
                rows.push(CiTrainRow {
                    id: pr.id.clone(),
                    identifier: format!("#{}", pr.number),
                    title: pr.title.clone(),
                    subtitle: None,
                    glyph: "ci-queued",
                    badge: match pr.merge_position.filter(|p| *p != 0) {
                        Some(position) => pill.queued_at.replace("{position}", &ordinal(position)),
                        None => pill.queued.clone(),
                    },
                    pull_request_id: pr.id.clone(),
                });
            }
            let mut waiting: Vec<&CiPagePr> =
                prs.iter().copied().filter(|pr| !matches!(pr.merge_state.as_deref(), Some("queued") | Some("testing"))).collect();
            waiting.sort_by_key(|pr| pr.number);
            for pr in waiting {
                let state = pr.merge_state.as_deref().unwrap_or_default();
                let (glyph, badge) = match state {
                    "checks" => ("ci-queued", pill.checks_running.clone()),
                    "held" => ("ci-skipped", pill.on_hold.clone()),
                    "conflict" => ("ci-failed", pill.conflict.clone()),
                    _ => ("ci-failed", pill.failing.clone()),
                };
                rows.push(CiTrainRow {
                    id: pr.id.clone(),
                    identifier: format!("#{}", pr.number),
                    title: pr.title.clone(),
                    subtitle: None,
                    glyph,
                    badge,
                    pull_request_id: pr.id.clone(),
                });
            }
        }
        if rows.is_empty() {
            continue;
        }
        groups.push(CiTrainGroup { title: repo_name(&input.repos, &repository_id), caption: prs[0].base.clone(), repository_id, rows });
    }
    groups.sort_by(|a, b| a.title.cmp(&b.title).then_with(|| a.repository_id.cmp(&b.repository_id)));
    groups
}

/// One runner row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CiRunnerRow {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub online: bool,
    pub busy: Option<String>,
    pub queue: Option<String>,
    pub offline_since: Option<String>,
}

/// A runner's labels: its OS, then its own lower-cased.
pub fn ci_runner_labels(runner: &CiPageRunner) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for label in runner_os_label(runner.platform.as_deref()).map(str::to_string).into_iter().chain(runner.labels.iter().map(|l| l.to_lowercase())) {
        if !out.contains(&label) {
            out.push(label);
        }
    }
    out
}

/// `ciRunnerRows`: online first, then by name.
pub fn ci_runner_rows(input: &CiPageInput) -> Vec<CiRunnerRow> {
    let at = ms_of(Some(&input.now)).unwrap_or(0);
    let mut rows: Vec<CiRunnerRow> = input
        .runners
        .iter()
        .map(|runner| {
            let labels = ci_runner_labels(runner);
            let is_online = online(runner, at);
            let busy = input.jobs.iter().filter(|job| job.status == "in_progress" && job.runner_device_id.as_deref() == Some(runner.id.as_str())).count();
            let queued = input
                .jobs
                .iter()
                .filter(|job| {
                    ci_source(&job.provider) == CiSource::Exponential
                        && job.status == "queued"
                        && job.runner_device_id.is_none()
                        && job.runs_on.iter().all(|label| labels.contains(&label.to_lowercase()))
                })
                .count();
            CiRunnerRow {
                id: runner.id.clone(),
                title: runner.label.clone(),
                subtitle: if labels.is_empty() { copy("runners.noLabels").to_string() } else { labels.join(", ") },
                online: is_online,
                busy: is_online.then(|| copy("runners.busy").replace("{busy}", &busy.to_string()).replace("{slots}", &runner.slots.to_string())),
                queue: (queued > 0).then(|| copy("runners.queue").replace("{count}", &queued.to_string())),
                offline_since: if is_online { None } else { runner.last_seen_at.clone() },
            }
        })
        .collect();
    rows.sort_by(|a, b| b.online.cmp(&a.online).then_with(|| a.title.cmp(&b.title)).then_with(|| a.id.cmp(&b.id)));
    rows
}

/// `ciTabCounts`: the train's rows, the runners.
pub fn ci_tab_counts(input: &CiPageInput) -> (usize, usize) {
    (ci_train_groups(input, true).iter().map(|group| group.rows.len()).sum(), input.runners.len())
}

/// ci-page.json `scenario` as the page's input (the specimen ×4).
pub fn scenario() -> CiPageInput {
    serde_json::from_value(CI_PAGE["scenario"].clone()).expect("the scenario hydrates")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> &'static Value {
        &CI_PAGE
    }

    #[test]
    fn run_lists_match_the_fixture() {
        let input = scenario();
        let cases = fixture()["runListCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let filter: CiRunFilter = serde_json::from_value(case["filter"].clone()).unwrap();
            assert_eq!(json!(ci_run_list(&input, &filter)), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn repo_options_match_the_fixture() {
        assert_eq!(json!(ci_repo_options(&scenario())), fixture()["repoOptions"]);
    }

    #[test]
    fn tiles_match_the_fixture() {
        assert_eq!(json!(ci_page_tiles(&scenario())), fixture()["tiles"]);
    }

    #[test]
    fn tab_counts_match_the_fixture() {
        let (train, runners) = ci_tab_counts(&scenario());
        assert_eq!(json!({"train": train, "runners": runners}), fixture()["tabCounts"]);
    }

    #[test]
    fn train_groups_match_the_fixture() {
        let input = scenario();
        for case in fixture()["trainCases"].as_array().unwrap() {
            assert_eq!(json!(ci_train_groups(&input, case["queue"].as_bool().unwrap())), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn runner_rows_match_the_fixture() {
        assert_eq!(json!(ci_runner_rows(&scenario())), fixture()["runners"]);
    }

    #[test]
    fn badges_match_the_fixture() {
        let cases = fixture()["badgeCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input: CiPageInput = serde_json::from_value(case["input"].clone()).unwrap();
            assert_eq!(json!(ci_badge(&input)), case["expected"], "{}", case["name"]);
        }
    }
}
