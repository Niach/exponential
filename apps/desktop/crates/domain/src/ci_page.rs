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
    /// #EXP-2172 redesign: what the run is about (`ci_runs.title`).
    #[serde(default)]
    pub title: Option<String>,
}

/// A person a run names (its author), from the users shape.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiPageUser {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub image: Option<String>,
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
    pub users: Vec<CiPageUser>,
    #[serde(default)]
    pub viewer_id: String,
    #[serde(default)]
    pub now: String,
    /// The viewer's clock offset from UTC in minutes (Today / Yesterday).
    #[serde(default)]
    pub utc_offset_minutes: i64,
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

/// ci-page.json `statusTones`: THE status table (glyph, tone); running is
/// the one blue (`info`) mark.
pub fn status_tone(status: CiRunStatus) -> (&'static str, &'static str) {
    let entry = &CI_PAGE["statusTones"][status.id()];
    (entry["glyph"].as_str().unwrap_or_default(), entry["tone"].as_str().unwrap_or_default())
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

/// One row of the Runs tab (both form factors' strings).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CiRunListRow {
    pub id: String,
    pub status: &'static str,
    pub status_label: String,
    pub glyph: &'static str,
    pub tone: &'static str,
    /// The commit / PR line, else the merge train batch, else the pipeline.
    pub title: String,
    /// `name #n`.
    pub workflow: String,
    pub repository_id: String,
    pub repository: String,
    /// The repository's own name (after the owner).
    pub repo: String,
    pub r#ref: String,
    pub sha: String,
    pub trigger: String,
    pub source: &'static str,
    pub source_label: String,
    pub source_icon: &'static str,
    pub duration: Option<String>,
    /// When it started, else when it was queued.
    pub at: String,
    pub author: Option<CiPageUser>,
    /// Wide: `workflow · repo · ref · sha · trigger`.
    pub subtitle: String,
    /// Phones: `repo · ref · workflow`.
    pub phone_subtitle: String,
    /// Phones' meta: the duration, else copy.queued.
    pub phone_meta: String,
}

/// A day's (or In progress') section of the Runs tab.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CiRunGroup {
    /// `in-progress`, else the local day `yyyy-mm-dd`.
    pub id: String,
    pub title: String,
    pub count: Option<usize>,
    pub rows: Vec<CiRunListRow>,
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

/// A running merge train run is its batch: `Testing #a, #b together`.
fn batch_title(run: &CiPageRun, pull_requests: &[CiPagePr]) -> Option<String> {
    if run.trigger != "merge_train" || run.state != "pending" {
        return None;
    }
    let mut members: Vec<i64> = pull_requests
        .iter()
        .filter(|pr| pr.repository_id == run.repository_id && pr.merge_state.as_deref() == Some("testing"))
        .map(|pr| pr.number)
        .collect();
    members.sort();
    if members.is_empty() {
        return None;
    }
    Some(copy("batchTitle").replace("{numbers}", &members.iter().map(|n| format!("#{n}")).collect::<Vec<_>>().join(", ")))
}

/// `ciRunRow`: one row of the list.
pub fn ci_run_row(run: &CiPageRun, input: &CiPageInput) -> CiRunListRow {
    let at = ms_of(Some(&input.now)).unwrap_or(0);
    let status = status_of(run, &input.jobs);
    let source = ci_source(&run.provider);
    let started = ms_of(run.started_at.as_deref());
    let repository = repo_name(&input.repos, &run.repository_id);
    let repo = repository.rsplit('/').next().unwrap_or(&repository).to_string();
    let r#ref = run.r#ref.clone().unwrap_or_else(|| crate::repo_ci::copy("runs.noRef").to_string());
    let sha: String = run.sha.chars().take(7).collect();
    let workflow = format!("{} #{}", run.name, run.number);
    let trigger = trigger_text(run, &input.pull_requests);
    let pr = run.pull_request_id.as_deref().and_then(|id| input.pull_requests.iter().find(|pr| pr.id == id));
    let duration = started.map(|started| format_check_duration(ms_of(run.completed_at.as_deref()).unwrap_or(at) - started));
    let (glyph, tone) = status_tone(status);
    let title = run
        .title
        .clone()
        .or_else(|| batch_title(run, &input.pull_requests))
        .or_else(|| pr.map(|pr| pr.title.clone()))
        .unwrap_or_else(|| run.name.clone());
    CiRunListRow {
        id: run.id.clone(),
        status: status.id(),
        status_label: crate::repo_ci::copy(&format!("status.{}", status.id())).to_string(),
        glyph,
        tone,
        title,
        subtitle: [workflow.as_str(), repo.as_str(), r#ref.as_str(), sha.as_str(), trigger.as_str()].join(" · "),
        phone_subtitle: [repo.as_str(), r#ref.as_str(), workflow.as_str()].join(" · "),
        phone_meta: duration.clone().unwrap_or_else(|| copy("queued").to_string()),
        workflow,
        repository_id: run.repository_id.clone(),
        repository,
        repo,
        r#ref,
        sha,
        trigger,
        source: source.id(),
        source_label: copy(&format!("sources.{}", source.id())).to_string(),
        source_icon: if source == CiSource::Github { "github" } else { "brand-exponential" },
        duration,
        at: run.started_at.clone().unwrap_or_else(|| run.created_at.clone()),
        author: run.created_by_user_id.as_deref().and_then(|id| input.users.iter().find(|user| user.id == id).cloned()),
    }
}

/// `ciRunList`: every run of the filter, newest first, at most `limits.runs`.
pub fn ci_run_list(input: &CiPageInput, filter: &CiRunFilter) -> Vec<CiRunListRow> {
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
        rows.push(ci_run_row(run, input));
        if rows.len() >= max {
            break;
        }
    }
    rows
}

/// The local day of an instant, `yyyy-mm-dd`, at the viewer's offset.
fn local_day(ms: i64, offset_minutes: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms + offset_minutes * 60_000)
        .map(|at| at.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// `ciDayTitle`: Today, Yesterday, else `27 Nov` (+ the year when not this year's).
pub fn ci_day_title(day: &str, now: &str, offset_minutes: i64) -> String {
    let now_ms = ms_of(Some(now)).unwrap_or(0);
    let today = local_day(now_ms, offset_minutes);
    if day == today {
        return copy("today").to_string();
    }
    if day == local_day(now_ms - 86_400_000, offset_minutes) {
        return copy("yesterday").to_string();
    }
    let mut parts = day.split('-');
    let (y, m, d) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
    let month = m.parse::<usize>().ok().and_then(|m| CI_PAGE["copy"]["months"][m.saturating_sub(1)].as_str()).unwrap_or_default();
    let label = format!("{} {month}", d.parse::<i64>().unwrap_or_default());
    if today.starts_with(y) { label } else { format!("{label} {y}") }
}

/// `ciRunGroups`: In progress (running + queued, with a count), then one
/// section per local day, newest first.
pub fn ci_run_groups(input: &CiPageInput, filter: &CiRunFilter) -> Vec<CiRunGroup> {
    let rows = ci_run_list(input, filter);
    let live = |row: &CiRunListRow| row.status == "running" || row.status == "queued";
    let mut groups = Vec::new();
    let in_progress: Vec<CiRunListRow> = rows.iter().filter(|row| live(row)).cloned().collect();
    if !in_progress.is_empty() {
        groups.push(CiRunGroup { id: "in-progress".into(), title: copy("inProgress").into(), count: Some(in_progress.len()), rows: in_progress });
    }
    for row in rows {
        if live(&row) {
            continue;
        }
        let day = local_day(ms_of(Some(&row.at)).unwrap_or(0), input.utc_offset_minutes);
        match groups.last_mut() {
            Some(last) if last.id == day => last.rows.push(row),
            _ => groups.push(CiRunGroup { title: ci_day_title(&day, &input.now, input.utc_offset_minutes), id: day, count: None, rows: vec![row] }),
        }
    }
    groups
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

fn online(runner: &CiPageRunner, now: i64) -> bool {
    ms_of(runner.last_seen_at.as_deref()).is_some_and(|seen| now - seen < limit("runnerOnlineMs"))
}

/// `ciRunnerChip`: the toolbar's `{online} of {total} runners` (it opens
/// the Runners tab), online = a green dot. None without runners.
pub fn ci_runner_chip(input: &CiPageInput) -> Option<(String, bool)> {
    if input.runners.is_empty() {
        return None;
    }
    let at = ms_of(Some(&input.now)).unwrap_or(0);
    let count = input.runners.iter().filter(|runner| online(runner, at)).count();
    Some((copy("runnerChip").replace("{online}", &count.to_string()).replace("{total}", &input.runners.len().to_string()), count > 0))
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
    fn run_groups_match_the_fixture() {
        let input = scenario();
        let cases = fixture()["runGroupsCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let filter: CiRunFilter = serde_json::from_value(case["filter"].clone()).unwrap();
            assert_eq!(json!(ci_run_groups(&input, &filter)), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn day_titles_match_the_fixture() {
        for case in fixture()["dayTitleCases"].as_array().unwrap() {
            let got = ci_day_title(case["day"].as_str().unwrap(), case["now"].as_str().unwrap(), case["utcOffsetMinutes"].as_i64().unwrap());
            assert_eq!(got, case["expected"].as_str().unwrap(), "{}", case["day"]);
        }
    }

    #[test]
    fn runner_chip_matches_the_fixture() {
        let (label, online) = ci_runner_chip(&scenario()).unwrap();
        assert_eq!(json!({"label": label, "online": online}), fixture()["runnerChip"]);
    }

    #[test]
    fn repo_options_match_the_fixture() {
        assert_eq!(json!(ci_repo_options(&scenario())), fixture()["repoOptions"]);
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
