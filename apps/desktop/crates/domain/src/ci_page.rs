//! #EXP-2172 — THE CI PAGE, the pure half ×4
//! (`packages/domain-contract/fixtures/ci-page.json`; TS reference
//! `packages/domain-contract/src/ci-page.ts`): one root row after Devices
//! across every member team. #EXP-2214: four tabs, one meaning each: the
//! MERGE QUEUE ([`ci_queue`]), PULL REQUESTS ([`ci_pr_groups`]), RELEASES
//! ([`ci_run_groups`] of `release` runs) and RUNNERS; the run page's context
//! ([`ci_run_context`]) and the row's badge. The statuses are ci-run.json
//! `runStatus`, the copy is read off the fixtures (one source).

use std::collections::{HashMap, HashSet};
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

/// ci-page.json `copy.<path>` when the fixture has it (a map lookup).
fn copy_opt(path: &str) -> Option<&'static str> {
    path.split('.').fold(&CI_PAGE["copy"], |value, key| &value[key]).as_str()
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
    /// #EXP-2214: the merge queue run it tests (`ci_runs.merge_train_id`).
    #[serde(default)]
    pub merge_train_id: Option<String>,
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
    /// #EXP-2214: the Sent back row's Open log names a job.
    #[serde(default)]
    pub id: String,
    pub run_id: String,
    #[serde(default)]
    pub check: String,
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
    /// #EXP-2214: a push to it is a release run (None = `main`).
    #[serde(default)]
    pub default_branch: Option<String>,
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
    /// #EXP-2214: open | draft | merged | closed.
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub merge_state: Option<String>,
    /// What a `checks` PR waits for (merge-train.json `mergeWait`).
    #[serde(default)]
    pub merge_wait: Option<String>,
    /// person | bounces | timeout | no_checks | conflict.
    #[serde(default)]
    pub merge_hold: Option<String>,
    #[serde(default)]
    pub merge_now: bool,
    #[serde(default)]
    pub merge_attempts: i64,
    #[serde(default)]
    pub merge_position: Option<i64>,
    #[serde(default)]
    pub merge_train_id: Option<String>,
}

/// #EXP-2214: one merge queue run (`merge_trains`, the synced columns).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiPageTrain {
    pub id: String,
    pub repository_id: String,
    #[serde(default)]
    pub base: String,
    #[serde(default)]
    pub base_sha: Option<String>,
    /// PR ids per unit (a stack bottom first).
    #[serde(default)]
    pub units: Vec<Vec<String>>,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub attempt: i64,
    #[serde(default)]
    pub bisect_of: Option<String>,
    /// The bisect halves still to test after it.
    #[serde(default)]
    pub pending: Vec<Vec<Vec<String>>>,
    #[serde(default)]
    pub untested: bool,
    #[serde(default)]
    pub tested_sha: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub finished_at: Option<String>,
}

/// #EXP-2214: one row of the queue's history (`merge_train_events`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiPageTrainEvent {
    pub id: String,
    pub repository_id: String,
    pub pull_request_id: String,
    #[serde(default)]
    pub merge_train_id: Option<String>,
    /// landed | failing | conflict | held | merge_now | resumed.
    pub kind: String,
    #[serde(default)]
    pub base: String,
    #[serde(default)]
    pub attempt: i64,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub checks: Vec<String>,
    #[serde(default)]
    pub ci_job_id: Option<String>,
    #[serde(default)]
    pub job_url: Option<String>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub sha: Option<String>,
    #[serde(default)]
    pub by_user_id: Option<String>,
    #[serde(default)]
    pub created_at: String,
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
    /// #EXP-2214: the merge queue's runs and history.
    #[serde(default)]
    pub trains: Vec<CiPageTrain>,
    #[serde(default)]
    pub train_events: Vec<CiPageTrainEvent>,
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

/// The page's tab (#EXP-2214: the merge queue opens first).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CiTab {
    #[default]
    Train,
    Prs,
    Releases,
    Runners,
}

/// `CI_TABS`: the tab order.
pub const CI_TABS: [CiTab; 4] = [CiTab::Train, CiTab::Prs, CiTab::Releases, CiTab::Runners];

impl CiTab {
    pub fn id(self) -> &'static str {
        match self {
            Self::Train => "train",
            Self::Prs => "prs",
            Self::Releases => "releases",
            Self::Runners => "runners",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        CI_TABS.into_iter().find(|tab| tab.id() == value)
    }

    /// copy.tabs, phones copy.phoneTabs.
    pub fn label(self, phone: bool) -> &'static str {
        copy(&format!("{}.{}", if phone { "phoneTabs" } else { "tabs" }, self.id()))
    }
}

/// #EXP-2214: what a run is for (`ciRunKind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiRunKind {
    Train,
    PullRequest,
    Release,
    Branch,
}

impl CiRunKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::Train => "train",
            Self::PullRequest => "pull_request",
            Self::Release => "release",
            Self::Branch => "branch",
        }
    }
}

/// `ciRunKind` (ci-page.json `kindCases`): a merge queue run (its trigger,
/// its train, an `exp-train/` ref); a pull request's (its PR, or the
/// pull_request trigger); a RELEASE run (a tag, a manual or scheduled run, a
/// push to the repository's default branch, `main` when unknown); anything
/// else is a BRANCH run.
pub fn ci_run_kind(run: &CiPageRun, repos: &[CiPageRepo]) -> CiRunKind {
    if run.trigger == "merge_train" || run.merge_train_id.is_some() || run.r#ref.as_deref().unwrap_or_default().starts_with("exp-train/") {
        return CiRunKind::Train;
    }
    if run.pull_request_id.is_some() || run.trigger == "pull_request" {
        return CiRunKind::PullRequest;
    }
    if matches!(run.trigger.as_str(), "tag" | "manual" | "schedule") {
        return CiRunKind::Release;
    }
    let branch = repos.iter().find(|repo| repo.id == run.repository_id).and_then(|repo| repo.default_branch.as_deref()).unwrap_or("main");
    if run.trigger == "push" && run.r#ref.as_deref() == Some(branch) { CiRunKind::Release } else { CiRunKind::Branch }
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

fn numbers_text(numbers: &[i64]) -> String {
    numbers.iter().map(|n| format!("#{n}")).collect::<Vec<_>>().join(", ")
}

/// A merge queue run is its batch: `Testing #a, #b together` (#EXP-2214:
/// its train's units, else, running, the repo's testing PRs).
fn batch_title(run: &CiPageRun, input: &CiPageInput) -> Option<String> {
    let pull_requests = &input.pull_requests;
    if let Some(train) = run.merge_train_id.as_deref().and_then(|id| input.trains.iter().find(|t| t.id == id)) {
        let numbers: Vec<i64> =
            train.units.iter().flatten().filter_map(|id| pull_requests.iter().find(|pr| &pr.id == id).map(|pr| pr.number)).collect();
        if numbers.len() == 1 {
            return Some(copy("batchOne").replace("{number}", &format!("#{}", numbers[0])));
        }
        if numbers.len() > 1 {
            return Some(copy("batchTitle").replace("{numbers}", &numbers_text(&numbers)));
        }
    }
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
    Some(copy("batchTitle").replace("{numbers}", &numbers_text(&members)))
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
    // #EXP-2214: a tag run reads as its tag (`ios-v0.14.150`).
    let title = (run.trigger == "tag").then(|| run.r#ref.clone().filter(|r| !r.is_empty())).flatten()
        .or_else(|| run.title.clone())
        .or_else(|| batch_title(run, input))
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

/// `ciRunList`: every run of the filter (#EXP-2214: of `kinds` when
/// given), newest first, at most `limits.runs`.
pub fn ci_run_list(input: &CiPageInput, filter: &CiRunFilter, kinds: Option<&[CiRunKind]>) -> Vec<CiRunListRow> {
    let max = limit("runs").max(0) as usize;
    let mut rows = Vec::new();
    for run in newest_first(&input.runs) {
        if kinds.is_some_and(|kinds| !kinds.contains(&ci_run_kind(run, &input.repos))) {
            continue;
        }
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
pub fn ci_run_groups(input: &CiPageInput, filter: &CiRunFilter, kinds: Option<&[CiRunKind]>) -> Vec<CiRunGroup> {
    let rows = ci_run_list(input, filter, kinds);
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

// ── #EXP-2214 PULL REQUESTS TAB ─────────────────────────────────────────

/// A pill: its text and tone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CiPill {
    pub text: String,
    pub tone: &'static str,
}

/// One subject of the Pull requests tab: a pull request (its runs), or a
/// branch without one (`Branches`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CiSubjectRow {
    /// The PR's id, else `branch.<repositoryId>.<ref>`.
    pub id: String,
    pub pull_request_id: Option<String>,
    /// `#214`; None for a branch.
    pub identifier: Option<String>,
    pub title: String,
    pub status: &'static str,
    pub glyph: &'static str,
    pub tone: &'static str,
    /// Wide: `repo · ref · name #n` of the run that decides the status.
    pub subtitle: String,
    /// Phones: `repo · name #n`.
    pub phone_subtitle: String,
    /// That run's duration, else copy.queued.
    pub meta: String,
    /// The PR's merge queue state (wide only); None for a branch.
    pub pill: Option<CiPill>,
    /// The run a press opens (the deciding one).
    pub run_id: String,
    /// Its runs, newest first, at most `limits.subjectRuns` (folded).
    pub runs: Vec<CiRunListRow>,
}

/// A section of the Pull requests tab: open | branches | closed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CiSubjectGroup {
    pub id: &'static str,
    pub title: String,
    pub count: usize,
    /// Merged or closed starts folded.
    pub open: bool,
    pub rows: Vec<CiSubjectRow>,
}

const STATUS_RANK: [CiRunStatus; 5] = [CiRunStatus::Failed, CiRunStatus::Running, CiRunStatus::Queued, CiRunStatus::Passed, CiRunStatus::Cancelled];

/// The first status of [`STATUS_RANK`] present (none = queued).
fn rolled_up(statuses: &[CiRunStatus]) -> CiRunStatus {
    STATUS_RANK.into_iter().find(|status| statuses.contains(status)).unwrap_or(CiRunStatus::Queued)
}

/// `ciMergePill`: a PR's merge queue state as a pill (merge-train.json copy.pill).
pub fn ci_merge_pill(pr: &CiPagePr) -> Option<CiPill> {
    let pill = &MERGE_TRAIN.copy.pill;
    let made = |text: &str, tone: &'static str| Some(CiPill { text: text.to_string(), tone });
    match pr.merge_state.as_deref()? {
        "testing" => made(&pill.testing, "info"),
        "queued" => match pr.merge_position.filter(|p| *p != 0) {
            Some(position) => made(&pill.queued_at.replace("{position}", &ordinal(position)), "neutral"),
            None => made(&pill.queued, "neutral"),
        },
        "merged" => made(&pill.merged, "success"),
        "conflict" => made(&pill.conflict, "danger"),
        "failing" => made(&pill.failing, "danger"),
        "held" => made(&pill.on_hold, "neutral"),
        "draft" => made(&pill.draft, "neutral"),
        "checks" => {
            let text = pr.merge_wait.as_deref().and_then(|wait| copy_opt(&format!("queue.waits.{wait}"))).unwrap_or(pill.checks_running.as_str());
            let wait = pr.merge_wait.as_deref();
            let tone = if wait == Some("failing") {
                "danger"
            } else if wait.unwrap_or("checks") == "checks" {
                "info"
            } else {
                "neutral"
            };
            made(text, tone)
        }
        _ => None,
    }
}

/// THE PULL REQUESTS TAB (`ciPrGroups`, ci-page.json `prGroupsCases`; the
/// TS doc comment is the rule): pull request runs grouped by PR, branch runs
/// by (repository, ref); Open, Branches, Merged or closed (folded).
pub fn ci_pr_groups(input: &CiPageInput, filter: &CiRunFilter) -> Vec<CiSubjectGroup> {
    struct Subject<'a> {
        key: String,
        pr: Option<&'a CiPagePr>,
        r#ref: Option<String>,
        runs: Vec<&'a CiPageRun>,
    }
    let mut subjects: Vec<Subject> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for run in newest_first(&input.runs) {
        let kind = ci_run_kind(run, &input.repos);
        if kind != CiRunKind::PullRequest && kind != CiRunKind::Branch {
            continue;
        }
        if filter.repository_id.as_deref().is_some_and(|id| id != run.repository_id) {
            continue;
        }
        let key = run.pull_request_id.clone().unwrap_or_else(|| format!("branch.{}.{}", run.repository_id, run.r#ref.as_deref().unwrap_or_default()));
        let at = *index.entry(key.clone()).or_insert_with(|| {
            let pr = run.pull_request_id.as_deref().and_then(|id| input.pull_requests.iter().find(|pr| pr.id == id));
            subjects.push(Subject { key, pr, r#ref: run.r#ref.clone(), runs: Vec::new() });
            subjects.len() - 1
        });
        subjects[at].runs.push(run);
    }
    let max = limit("subjects").max(0) as usize;
    let max_runs = limit("subjectRuns").max(0) as usize;
    let (mut open, mut branches, mut closed): (Vec<CiSubjectRow>, Vec<CiSubjectRow>, Vec<CiSubjectRow>) = Default::default();
    for subject in &subjects {
        let mut names: Vec<&str> = Vec::new();
        let mut latest: Vec<(&CiPageRun, CiRunStatus)> = Vec::new();
        for run in &subject.runs {
            if !names.contains(&run.name.as_str()) {
                names.push(&run.name);
                latest.push((run, status_of(run, &input.jobs)));
            }
        }
        let status = rolled_up(&latest.iter().map(|(_, s)| *s).collect::<Vec<_>>());
        let deciding = latest.iter().find(|(_, s)| *s == status).map(|(run, _)| *run).expect("a run of the status");
        let keep = match filter.status {
            CiFilter::All => true,
            CiFilter::Running => matches!(status, CiRunStatus::Running | CiRunStatus::Queued),
            CiFilter::Failed => status == CiRunStatus::Failed,
            CiFilter::Mine => subject.runs.iter().any(|run| ci_is_mine(run, &input.viewer_id, &input.pull_requests, &input.sessions)),
        };
        if !keep {
            continue;
        }
        let row = ci_run_row(deciding, input);
        let pr_id = subject.runs[0].pull_request_id.clone();
        let branch = pr_id.is_none();
        let group = if branch {
            &mut branches
        } else if subject.pr.is_some_and(|pr| pr.state == "merged" || pr.state == "closed") {
            &mut closed
        } else {
            &mut open
        };
        if group.len() >= max {
            continue;
        }
        let (glyph, tone) = status_tone(status);
        group.push(CiSubjectRow {
            id: subject.key.clone(),
            pull_request_id: subject.pr.map(|pr| pr.id.clone()).or(pr_id),
            identifier: subject.pr.map(|pr| format!("#{}", pr.number)),
            title: match subject.pr {
                Some(pr) => pr.title.clone(),
                None if branch => subject.r#ref.clone().unwrap_or_else(|| crate::repo_ci::copy("runs.noRef").to_string()),
                None => row.title.clone(),
            },
            status: status.id(),
            glyph,
            tone,
            subtitle: [row.repo.as_str(), row.r#ref.as_str(), row.workflow.as_str()].join(" · "),
            phone_subtitle: [row.repo.as_str(), row.workflow.as_str()].join(" · "),
            meta: row.phone_meta.clone(),
            pill: subject.pr.and_then(ci_merge_pill),
            run_id: deciding.id.clone(),
            runs: subject.runs.iter().take(max_runs).map(|run| ci_run_row(run, input)).collect(),
        });
    }
    [("open", "prs.open", true, open), ("branches", "prs.branches", true, branches), ("closed", "prs.closed", false, closed)]
        .into_iter()
        .filter(|(_, _, _, rows)| !rows.is_empty())
        .map(|(id, title, open, rows)| CiSubjectGroup { id, title: copy(title).to_string(), count: rows.len(), open, rows })
        .collect()
}

// ── #EXP-2214 MERGE QUEUE TAB ────────────────────────────────────────────

const LIVE_TRAIN: [&str; 4] = ["dispatching", "composing", "testing", "green"];

fn live_train(train: &CiPageTrain) -> bool {
    LIVE_TRAIN.contains(&train.state.as_str()) && train.finished_at.is_none()
}

/// One PR of a run testing now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CiQueueMember {
    pub pull_request_id: String,
    pub identifier: String,
    pub title: String,
    pub author: Option<CiPageUser>,
}

/// The run testing now (one Card).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CiQueueNow {
    pub train_id: String,
    pub title: String,
    pub subtitle: String,
    pub members: Vec<CiQueueMember>,
    pub status: &'static str,
    /// The run Open run opens; None before one exists.
    pub run_id: Option<String>,
    pub elapsed: String,
    pub eta: Option<String>,
    pub progress: Option<i64>,
}

/// One row of Up next, Not ready yet, Sent back or Landed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CiQueueRow {
    pub id: String,
    pub pull_request_id: String,
    pub identifier: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub glyph: &'static str,
    pub tone: &'static str,
    pub badge: Option<CiPill>,
    /// History rows: when it happened (drawn relative).
    pub at: Option<String>,
    /// `open-log` | `resume` | None.
    pub action: Option<&'static str>,
    pub run_id: Option<String>,
    pub job_id: Option<String>,
    pub url: Option<String>,
    /// Up next: the ⋯ menu (Merge now, Hold).
    pub menu: bool,
}

impl CiQueueRow {
    fn of(id: &str, pr: &CiPagePr) -> Self {
        CiQueueRow {
            id: id.to_string(),
            pull_request_id: pr.id.clone(),
            identifier: format!("#{}", pr.number),
            title: pr.title.clone(),
            subtitle: None,
            glyph: "ci-queued",
            tone: "neutral",
            badge: None,
            at: None,
            action: None,
            run_id: None,
            job_id: None,
            url: None,
            menu: false,
        }
    }
}

/// A repository's queue stats (wide).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CiQueueStats {
    pub in_queue: usize,
    pub landed_today: usize,
    pub sent_back_today: usize,
    pub typical_run: Option<String>,
}

/// One repository's merge queue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CiQueueRepo {
    pub repository_id: String,
    pub title: String,
    pub caption: String,
    pub stats: CiQueueStats,
    pub testing: Vec<CiQueueNow>,
    pub up_next: Vec<CiQueueRow>,
    pub waiting: Vec<CiQueueRow>,
    pub sent_back: Vec<CiQueueRow>,
    pub landed: Vec<CiQueueRow>,
}

/// The finished green merge queue runs of a repository: per train, the span
/// of its CI runs, newest first, at most `limits.etaSamples * 2`.
fn train_durations(input: &CiPageInput, repository_id: &str) -> Vec<i64> {
    let mut order: Vec<&str> = Vec::new();
    let mut by_train: HashMap<&str, Vec<&CiPageRun>> = HashMap::new();
    for run in &input.runs {
        let Some(train) = run.merge_train_id.as_deref().filter(|_| run.repository_id == repository_id) else { continue };
        if !by_train.contains_key(train) {
            order.push(train);
        }
        by_train.entry(train).or_default().push(run);
    }
    let mut spans: Vec<(i64, i64)> = Vec::new();
    for train in order {
        let runs = &by_train[train];
        if !runs.iter().all(|run| status_of(run, &input.jobs) == CiRunStatus::Passed) {
            continue;
        }
        let starts: Option<Vec<i64>> = runs.iter().map(|run| ms_of(run.started_at.as_deref())).collect();
        let ends: Option<Vec<i64>> = runs.iter().map(|run| ms_of(run.completed_at.as_deref())).collect();
        let (Some(starts), Some(ends)) = (starts, ends) else { continue };
        let start = starts.into_iter().min().unwrap_or(0);
        let end = ends.into_iter().max().unwrap_or(0);
        spans.push((end, end - start));
    }
    spans.sort_by(|a, b| b.0.cmp(&a.0));
    spans.into_iter().take((limit("etaSamples") * 2).max(0) as usize).map(|(_, ms)| ms).collect()
}

/// `ciTypicalTrainMs`: the median of the repository's finished green merge
/// queue runs, None with fewer than `limits.etaSamples`.
pub fn ci_typical_train_ms(input: &CiPageInput, repository_id: &str) -> Option<i64> {
    let mut spans = train_durations(input, repository_id);
    spans.sort();
    if (spans.len() as i64) < limit("etaSamples") || spans.is_empty() {
        return None;
    }
    let mid = spans.len() / 2;
    Some(if spans.len() % 2 == 1 { spans[mid] } else { ((spans[mid - 1] + spans[mid]) as f64 / 2.0).round() as i64 })
}

/// `ciTrainEta`'s result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CiTrainEta {
    pub eta: String,
    pub progress: i64,
}

/// `ciTrainEta` (ci-page.json `etaCases`): what is left of a run going
/// `elapsed_ms` against the typical run.
pub fn ci_train_eta(typical_ms: Option<i64>, elapsed_ms: i64) -> Option<CiTrainEta> {
    let typical = typical_ms.filter(|ms| *ms > 0)?;
    let left = typical - elapsed_ms;
    let eta = if left > 60_000 {
        copy("queue.left").replace("{n}", &((left as f64 / 60_000.0).ceil() as i64).to_string())
    } else {
        copy("queue.almostDone").to_string()
    };
    let progress = ((elapsed_ms as f64 / typical as f64) * 100.0).floor().clamp(0.0, 99.0) as i64;
    Some(CiTrainEta { eta, progress })
}

fn author_of(pr: &CiPagePr, input: &CiPageInput) -> Option<CiPageUser> {
    let starter = pr.root_session_id.as_deref().and_then(|id| input.sessions.iter().find(|s| s.id == id)).map(|s| s.user_id.as_str())?;
    input.users.iter().find(|user| user.id == starter).cloned()
}

fn list_of(items: &[String], max: usize) -> String {
    if items.len() <= max {
        items.join(", ")
    } else {
        format!("{} +{}", items[..max].join(", "), items.len() - max)
    }
}

fn file_names(files: &[String]) -> Vec<String> {
    files.iter().map(|file| file.rsplit('/').next().unwrap_or(file).to_string()).collect()
}

fn tries(n: i64) -> String {
    copy("queue.tries").replace("{n}", &n.to_string()).replace("{max}", &MERGE_TRAIN.limits.bounces.to_string())
}

fn base_caption(train: &CiPageTrain) -> String {
    match train.base_sha.as_deref() {
        Some(sha) => copy("queue.base").replace("{base}", &train.base).replace("{sha}", &sha.chars().take(7).collect::<String>()),
        None => train.base.clone(),
    }
}

fn sha7(sha: &str) -> String {
    sha.chars().take(7).collect()
}

/// THE MERGE QUEUE TAB (`ciQueue`, ci-page.json `queueCases`; the TS doc
/// comment is the rule), per repository by name.
pub fn ci_queue<'a>(input: &'a CiPageInput) -> Vec<CiQueueRepo> {
    let now = ms_of(Some(&input.now)).unwrap_or(0);
    let since = now - limit("historyHours") * 3_600_000;
    let today = local_day(now, input.utc_offset_minutes);
    let pr_of = |id: &str| input.pull_requests.iter().find(|pr| pr.id == id);
    let event_at = |e: &CiPageTrainEvent| ms_of(Some(&e.created_at)).unwrap_or(0);
    // A JS Set: first appearance; the result is sorted by name below.
    let mut ids: Vec<&str> = Vec::new();
    let mut add = |id: &'a str| {
        if !ids.contains(&id) {
            ids.push(id);
        }
    };
    for train in input.trains.iter().filter(|t| live_train(t)) {
        add(&train.repository_id);
    }
    for pr in &input.pull_requests {
        if pr.state == "open" && pr.merge_state.as_deref().is_some_and(|s| ["queued", "testing", "held", "conflict", "failing"].contains(&s)) {
            add(&pr.repository_id);
        }
    }
    for event in &input.train_events {
        if event_at(event) >= since {
            add(&event.repository_id);
        }
    }
    let history = limit("history").max(0) as usize;
    let mut out = Vec::new();
    for repository_id in ids {
        let typical = ci_typical_train_ms(input, repository_id);
        let mut live: Vec<&CiPageTrain> = input.trains.iter().filter(|t| t.repository_id == repository_id && live_train(t)).collect();
        live.sort_by(|a, b| ms_of(Some(&a.created_at)).unwrap_or(0).cmp(&ms_of(Some(&b.created_at)).unwrap_or(0)).then_with(|| a.id.cmp(&b.id)));
        let testing: Vec<CiQueueNow> = live
            .iter()
            .map(|train| {
                let members: Vec<CiQueueMember> = train
                    .units
                    .iter()
                    .flatten()
                    .filter_map(|id| pr_of(id))
                    .map(|pr| CiQueueMember {
                        pull_request_id: pr.id.clone(),
                        identifier: format!("#{}", pr.number),
                        title: pr.title.clone(),
                        author: author_of(pr, input),
                    })
                    .collect();
                let own: Vec<CiPageRun> = input.runs.iter().filter(|run| run.merge_train_id.as_deref() == Some(train.id.as_str())).cloned().collect();
                let runs = newest_first(&own);
                let statuses: Vec<(&CiPageRun, CiRunStatus)> = runs.iter().map(|run| (*run, status_of(run, &input.jobs))).collect();
                let status = rolled_up(&statuses.iter().map(|(_, s)| *s).collect::<Vec<_>>());
                let deciding = statuses.iter().find(|(_, s)| *s == status).map(|(run, _)| run.id.clone());
                let started = runs.iter().filter_map(|run| ms_of(run.started_at.as_deref())).min();
                let eta = started.and_then(|started| ci_train_eta(typical, now - started));
                let mut parts = vec![base_caption(train)];
                if train.attempt > 0 {
                    parts.push(copy("queue.rerun").to_string());
                }
                if train.bisect_of.is_some() && !train.pending.is_empty() {
                    parts.push(copy("queue.split").replace("{count}", &train.pending.len().to_string()));
                }
                if train.untested {
                    parts.push(copy("queue.untested").to_string());
                }
                CiQueueNow {
                    train_id: train.id.clone(),
                    title: if members.len() == 1 {
                        members[0].title.clone()
                    } else {
                        copy("batchTitle").replace("{numbers}", &members.iter().map(|m| m.identifier.clone()).collect::<Vec<_>>().join(", "))
                    },
                    subtitle: parts.join(" · "),
                    members,
                    status: status.id(),
                    run_id: deciding,
                    elapsed: match started {
                        None => copy("waitingForRunner").to_string(),
                        Some(started) => copy("queue.runningFor").replace("{duration}", &format_check_duration(now - started)),
                    },
                    eta: eta.as_ref().map(|e| e.eta.clone()),
                    progress: eta.map(|e| e.progress),
                }
            })
            .collect();
        let open: Vec<&CiPagePr> = input.pull_requests.iter().filter(|pr| pr.repository_id == repository_id && pr.state == "open").collect();
        let mut queued: Vec<&CiPagePr> = open.iter().copied().filter(|pr| pr.merge_state.as_deref() == Some("queued")).collect();
        queued.sort_by(|a, b| a.merge_position.unwrap_or(i64::MAX).cmp(&b.merge_position.unwrap_or(i64::MAX)).then(a.number.cmp(&b.number)));
        let up_next: Vec<CiQueueRow> = queued
            .iter()
            .map(|pr| CiQueueRow {
                subtitle: if pr.merge_now {
                    Some(copy("queue.mergeNow").to_string())
                } else {
                    pr.merge_position.filter(|p| *p != 0).map(|p| copy("queue.position").replace("{ordinal}", &ordinal(p)))
                },
                glyph: if pr.merge_now { "zap" } else { "ci-queued" },
                tone: if pr.merge_now { "warning" } else { "neutral" },
                menu: true,
                ..CiQueueRow::of(&pr.id, pr)
            })
            .collect();
        let mut waiting_prs: Vec<&CiPagePr> = open
            .iter()
            .copied()
            .filter(|pr| matches!(pr.merge_state.as_deref(), Some("checks" | "held" | "conflict" | "failing")))
            .collect();
        waiting_prs.sort_by_key(|pr| pr.number);
        let waiting: Vec<CiQueueRow> = waiting_prs
            .iter()
            .map(|pr| {
                let state = pr.merge_state.as_deref().unwrap_or_default();
                let held = state == "held";
                let text = if held {
                    copy_opt(&format!("queue.holds.{}", pr.merge_hold.as_deref().unwrap_or("person"))).unwrap_or(copy("queue.holds.person"))
                } else if state == "checks" {
                    copy_opt(&format!("queue.waits.{}", pr.merge_wait.as_deref().unwrap_or("checks"))).unwrap_or(copy("queue.waits.checks"))
                } else {
                    copy(&format!("queue.waits.{state}"))
                };
                let tone = if state == "conflict" || state == "failing" || (state == "checks" && pr.merge_wait.as_deref() == Some("failing")) {
                    "danger"
                } else if state == "checks" && pr.merge_wait.as_deref().unwrap_or("checks") == "checks" {
                    "info"
                } else {
                    "neutral"
                };
                CiQueueRow {
                    glyph: if held {
                        "ci-skipped"
                    } else if tone == "danger" {
                        "ci-failed"
                    } else {
                        "ci-queued"
                    },
                    tone: if tone == "info" { "neutral" } else { tone },
                    badge: Some(CiPill { text: text.to_string(), tone }),
                    action: (held && pr.merge_hold.as_deref().is_some_and(|hold| hold != "person")).then_some("resume"),
                    ..CiQueueRow::of(&pr.id, pr)
                }
            })
            .collect();
        let mut events: Vec<&CiPageTrainEvent> =
            input.train_events.iter().filter(|e| e.repository_id == repository_id && event_at(e) >= since).collect();
        events.sort_by(|a, b| event_at(b).cmp(&event_at(a)).then_with(|| a.id.cmp(&b.id)));
        let sent_back: Vec<CiQueueRow> = events
            .iter()
            .filter(|e| e.kind == "failing" || e.kind == "conflict")
            .take(history)
            .filter_map(|e| {
                let pr = pr_of(&e.pull_request_id)?;
                let job = e.ci_job_id.as_deref().and_then(|id| input.jobs.iter().find(|job| job.id == id));
                let failing = e.kind == "failing";
                let subtitle = if failing {
                    format!("{} · {}", copy("queue.failedIn").replace("{checks}", &list_of(&e.checks, 2)), tries(e.attempt))
                } else {
                    format!("{} · {}", copy("queue.conflictIn").replace("{files}", &list_of(&file_names(&e.files), 2)), tries(e.attempt))
                };
                let open_log = failing && (job.is_some() || e.job_url.is_some());
                Some(CiQueueRow {
                    subtitle: Some(subtitle),
                    glyph: if failing { "ci-failed" } else { "pr-conflict" },
                    tone: "danger",
                    at: Some(e.created_at.clone()),
                    action: open_log.then_some("open-log"),
                    run_id: job.filter(|_| open_log).map(|job| job.run_id.clone()),
                    job_id: job.filter(|_| open_log).map(|job| job.id.clone()),
                    url: if open_log && job.is_none() { e.job_url.clone() } else { None },
                    ..CiQueueRow::of(&e.id, pr)
                })
            })
            .collect();
        // A PR sent back in the window is listed there, not again here.
        let sent_back_prs: HashSet<&str> = sent_back.iter().map(|row| row.pull_request_id.as_str()).collect();
        let not_ready: Vec<CiQueueRow> = waiting
            .into_iter()
            .filter(|row| {
                let state = pr_of(&row.pull_request_id).and_then(|pr| pr.merge_state.as_deref());
                !(matches!(state, Some("conflict" | "failing")) && sent_back_prs.contains(row.pull_request_id.as_str()))
            })
            .collect();
        let landed_events: Vec<&CiPageTrainEvent> = events.iter().copied().filter(|e| e.kind == "landed").collect();
        let landed: Vec<CiQueueRow> = landed_events
            .iter()
            .take(history)
            .filter_map(|e| {
                let pr = pr_of(&e.pull_request_id)?;
                let train = e.merge_train_id.as_deref().and_then(|id| input.trains.iter().find(|t| t.id == id));
                let untested = train.is_some_and(|t| t.untested);
                let mut others: Vec<i64> = landed_events
                    .iter()
                    .filter(|o| o.merge_train_id.is_some() && o.merge_train_id == e.merge_train_id && o.pull_request_id != e.pull_request_id)
                    .filter_map(|o| pr_of(&o.pull_request_id).map(|p| p.number))
                    .collect();
                others.sort();
                let mut parts: Vec<String> = if untested {
                    vec![copy("queue.mergedNow").to_string()]
                } else if !others.is_empty() {
                    vec![copy("queue.landedWith").replace("{numbers}", &numbers_text(&others))]
                } else {
                    Vec::new()
                };
                if let Some(sha) = e.sha.as_deref() {
                    parts.push(sha7(sha));
                }
                Some(CiQueueRow {
                    subtitle: (!parts.is_empty()).then(|| parts.join(" · ")),
                    glyph: if untested { "zap" } else { "ci-passed" },
                    tone: if untested { "warning" } else { "success" },
                    at: Some(e.created_at.clone()),
                    ..CiQueueRow::of(&e.id, pr)
                })
            })
            .collect();
        let is_today = |e: &CiPageTrainEvent| local_day(event_at(e), input.utc_offset_minutes) == today;
        let base = match testing.first() {
            Some(now) => input.trains.iter().find(|t| t.id == now.train_id).map(|t| t.base.clone()).unwrap_or_default(),
            None => open
                .iter()
                .find(|pr| pr.merge_state.as_deref() == Some("queued"))
                .map(|pr| pr.base.clone())
                .or_else(|| events.first().map(|e| e.base.clone()))
                .or_else(|| open.first().map(|pr| pr.base.clone()))
                .unwrap_or_else(|| "main".to_string()),
        };
        out.push(CiQueueRepo {
            repository_id: repository_id.to_string(),
            title: repo_name(&input.repos, repository_id),
            caption: copy("queue.into").replace("{base}", &base),
            stats: CiQueueStats {
                in_queue: testing.iter().map(|t| t.members.len()).sum::<usize>() + up_next.len(),
                landed_today: landed_events.iter().filter(|e| is_today(e)).count(),
                sent_back_today: events.iter().filter(|e| (e.kind == "failing" || e.kind == "conflict") && is_today(e)).count(),
                typical_run: typical.map(format_check_duration),
            },
            testing,
            up_next,
            waiting: not_ready,
            sent_back,
            landed,
        });
    }
    out.sort_by(|a, b| a.title.cmp(&b.title).then_with(|| a.repository_id.cmp(&b.repository_id)));
    out
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

/// `ciTabCounts` (after each label, muted; 0 = none): the merge queue's PRs
/// (testing + up next), the Open pull requests, the release runs in
/// progress, the runners.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct CiTabCounts {
    pub train: usize,
    pub prs: usize,
    pub releases: usize,
    pub runners: usize,
}

impl CiTabCounts {
    pub fn of(self, tab: CiTab) -> usize {
        match tab {
            CiTab::Train => self.train,
            CiTab::Prs => self.prs,
            CiTab::Releases => self.releases,
            CiTab::Runners => self.runners,
        }
    }
}

pub fn ci_tab_counts(input: &CiPageInput) -> CiTabCounts {
    let all = CiRunFilter::default();
    CiTabCounts {
        train: ci_queue(input).iter().map(|repo| repo.stats.in_queue).sum(),
        prs: ci_pr_groups(input, &all).iter().find(|group| group.id == "open").map(|group| group.count).unwrap_or(0),
        releases: ci_run_groups(input, &all, Some(&[CiRunKind::Release]))
            .iter()
            .find(|group| group.id == "in-progress")
            .and_then(|group| group.count)
            .unwrap_or(0),
        runners: input.runners.len(),
    }
}

// ── #EXP-2214 THE RUN PAGE'S CONTEXT ─────────────────────────────────────

/// What happened to one PR of a merge queue run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiRunOutcome {
    pub pull_request_id: String,
    pub identifier: String,
    pub title: String,
    pub text: String,
    pub glyph: String,
    pub tone: String,
    /// A failing PR's red job IN THIS RUN (Open log selects it).
    pub job_id: Option<String>,
}

/// `ciRunContext`: why the run exists.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CiRunContext {
    pub kind: CiRunKind,
    /// copy.kinds[kind]; a pull request run `Pull request #n`.
    pub badge: String,
    /// A merge queue run's batch title, else None.
    pub title: Option<String>,
    /// A merge queue run: `main @ 3db841e`.
    pub caption: Option<String>,
    pub outcome: Vec<CiRunOutcome>,
}

/// `ciRunContext` (ci-page.json `runContextCases`; the TS doc comment is the
/// rule). Reads `repos`, `pull_requests`, `trains`, `train_events`, `jobs`.
pub fn ci_run_context(run: &CiPageRun, input: &CiPageInput) -> CiRunContext {
    let kind = ci_run_kind(run, &input.repos);
    let pr = run.pull_request_id.as_deref().and_then(|id| input.pull_requests.iter().find(|pr| pr.id == id));
    let badge = match pr {
        Some(pr) if kind == CiRunKind::PullRequest => copy("pullRequest").replace("{number}", &pr.number.to_string()),
        _ => copy(&format!("kinds.{}", kind.id())).to_string(),
    };
    let train = run.merge_train_id.as_deref().and_then(|id| input.trains.iter().find(|t| t.id == id));
    let Some(train) = train.filter(|_| kind == CiRunKind::Train) else {
        let title = if kind == CiRunKind::Train { batch_title(run, input) } else { None };
        return CiRunContext { kind, badge, title, caption: None, outcome: Vec::new() };
    };
    let outcome = train
        .units
        .iter()
        .flatten()
        .filter_map(|id| {
            let member = input.pull_requests.iter().find(|p| &p.id == id)?;
            let mut events: Vec<&CiPageTrainEvent> = input
                .train_events
                .iter()
                .filter(|e| &e.pull_request_id == id && e.merge_train_id.as_deref() == Some(train.id.as_str()) && matches!(e.kind.as_str(), "landed" | "failing" | "conflict"))
                .collect();
            events.sort_by(|a, b| ms_of(Some(&b.created_at)).unwrap_or(0).cmp(&ms_of(Some(&a.created_at)).unwrap_or(0)));
            let event = events.first();
            let row = |text: String, glyph: &'static str, tone: &'static str, job_id: Option<String>| CiRunOutcome {
                pull_request_id: id.clone(),
                identifier: format!("#{}", member.number),
                title: member.title.clone(),
                text,
                glyph: glyph.to_string(),
                tone: tone.to_string(),
                job_id,
            };
            Some(match event.map(|e| e.kind.as_str()) {
                Some("landed") => {
                    let event = event.unwrap();
                    if train.untested {
                        row(copy("queue.outcome.untested").to_string(), "zap", "warning", None)
                    } else {
                        row(copy("queue.outcome.landed").replace("{sha}", &sha7(event.sha.as_deref().unwrap_or_default())), "ci-passed", "success", None)
                    }
                }
                Some("failing") => {
                    let event = event.unwrap();
                    let job = event.ci_job_id.as_deref().and_then(|jid| input.jobs.iter().find(|j| j.id == jid && j.run_id == run.id));
                    let text = copy("queue.outcome.failing").replace("{checks}", &list_of(&event.checks, 2)).replace("{tries}", &tries(event.attempt));
                    row(text, "ci-failed", "danger", job.map(|j| j.id.clone()))
                }
                Some(_) => {
                    let event = event.unwrap();
                    let text =
                        copy("queue.outcome.conflict").replace("{files}", &list_of(&file_names(&event.files), 2)).replace("{tries}", &tries(event.attempt));
                    row(text, "pr-conflict", "danger", None)
                }
                None if member.merge_state.as_deref() == Some("testing") && member.merge_train_id.as_deref() == Some(train.id.as_str()) => {
                    row(copy("queue.outcome.testing").to_string(), "ci-running", "info", None)
                }
                None => row(copy("queue.outcome.requeued").to_string(), "ci-queued", "neutral", None),
            })
        })
        .collect();
    CiRunContext { kind, badge, title: batch_title(run, input), caption: Some(base_caption(train)), outcome }
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

    fn filter_of(case: &Value) -> CiRunFilter {
        serde_json::from_value(case["filter"].clone()).unwrap()
    }

    #[test]
    fn run_groups_match_the_fixture() {
        let input = scenario();
        let cases = fixture()["runGroupsCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            assert_eq!(json!(ci_run_groups(&input, &filter_of(case), None)), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn release_groups_match_the_fixture() {
        let input = scenario();
        let cases = fixture()["releaseGroupsCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            assert_eq!(json!(ci_run_groups(&input, &filter_of(case), Some(&[CiRunKind::Release]))), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn pr_groups_match_the_fixture() {
        let input = scenario();
        let cases = fixture()["prGroupsCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            assert_eq!(json!(ci_pr_groups(&input, &filter_of(case))), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn queue_matches_the_fixture() {
        let cases = fixture()["queueCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            // The case's `input` overrides the scenario's keys.
            let mut world = fixture()["scenario"].clone();
            if let Some(over) = case["input"].as_object() {
                for (key, value) in over {
                    world[key] = value.clone();
                }
            }
            let input: CiPageInput = serde_json::from_value(world).unwrap();
            assert_eq!(json!(ci_queue(&input)), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn run_kinds_match_the_fixture() {
        let cases = fixture()["kindCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let mut run = json!({"id": "x", "repositoryId": "", "number": 0, "name": "", "state": ""});
            for (key, value) in case["run"].as_object().unwrap() {
                run[key] = value.clone();
            }
            let run: CiPageRun = serde_json::from_value(run).unwrap();
            let repos: Vec<CiPageRepo> = serde_json::from_value(case["repos"].clone()).unwrap();
            assert_eq!(json!(ci_run_kind(&run, &repos)), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn run_contexts_match_the_fixture() {
        let input = scenario();
        let cases = fixture()["runContextCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let run = input.runs.iter().find(|run| Some(run.id.as_str()) == case["run"].as_str()).unwrap();
            assert_eq!(json!(ci_run_context(run, &input)), case["expected"], "{}", case["run"]);
        }
    }

    #[test]
    fn typical_train_matches_the_fixture() {
        let input = scenario();
        let cases = fixture()["typicalTrainMs"].as_object().unwrap();
        assert!(!cases.is_empty());
        for (repo, ms) in cases {
            assert_eq!(json!(ci_typical_train_ms(&input, repo)), *ms, "{repo}");
        }
    }

    #[test]
    fn eta_matches_the_fixture() {
        let cases = fixture()["etaCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let got = ci_train_eta(case["typicalMs"].as_i64(), case["elapsedMs"].as_i64().unwrap());
            assert_eq!(json!(got), case["expected"], "{case}");
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
        assert_eq!(json!(ci_tab_counts(&scenario())), fixture()["tabCounts"]);
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
