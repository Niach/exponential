//! #EXP-1617 / #EXP-1804 — our own CI, the pure half ×4: THE CI RUN PAGE
//! (`packages/domain-contract/fixtures/ci-run.json`) and a device's CI
//! RUNNER SECTION (`fixtures/runner-settings.json`); TS reference
//! `packages/domain-contract/src/ci-run.ts`. The glyphs, tones and
//! durations are pr-checks' ([`crate::pr_checks`]). The copy is read off the
//! fixtures themselves (one source, never a Rust mirror to drift).

use std::sync::LazyLock;

use serde_json::Value;

use crate::pr_checks::{check_duration_of, check_glyph, format_check_duration, is_failing, CheckGlyph, CheckTone};
use crate::rows::{CiJobRow, CiRunRow};

const CI_RUN_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/ci-run.json");
const RUNNER_SETTINGS_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/runner-settings.json");

/// ci-run.json as parsed once.
pub static CI_RUN: LazyLock<Value> = LazyLock::new(|| serde_json::from_str(CI_RUN_JSON).expect("ci-run.json parses"));
/// runner-settings.json as parsed once.
pub static RUNNER_SETTINGS: LazyLock<Value> =
    LazyLock::new(|| serde_json::from_str(RUNNER_SETTINGS_JSON).expect("runner-settings.json parses"));

fn at<'a>(root: &'a Value, path: &str) -> &'a Value {
    path.split('.').fold(root, |value, key| &value[key])
}

fn text_at(root: &'static Value, path: &str) -> &'static str {
    at(root, path).as_str().unwrap_or_default()
}

/// ci-run.json `copy.<path>` (`tiles.status`, `reasons.needs`…).
pub fn copy(path: &str) -> &'static str {
    text_at(&CI_RUN["copy"], path)
}

/// ci-run.json `testIds.<key>`.
pub fn test_id(key: &str) -> &'static str {
    text_at(&CI_RUN["testIds"], key)
}

/// runner-settings.json `copy.<path>`.
pub fn runner_copy(path: &str) -> &'static str {
    text_at(&RUNNER_SETTINGS["copy"], path)
}

/// runner-settings.json `testIds.<key>`.
pub fn runner_test_id(key: &str) -> &'static str {
    text_at(&RUNNER_SETTINGS["testIds"], key)
}

/// `liveTailMs`: how often a running job's log is read again.
pub fn live_tail_ms() -> u64 {
    CI_RUN["liveTailMs"].as_u64().unwrap_or(2000)
}

/// `logMaxLines.wide` (the desktop is wide).
pub fn log_max_lines() -> u64 {
    CI_RUN["logMaxLines"]["wide"].as_u64().unwrap_or(32)
}

/// `copy.title` with the run's number.
pub fn run_title(number: i64) -> String {
    copy("title").replace("{number}", &number.to_string())
}

// ── the run page (ci-run.json) ───────────────────────────────────────────

/// The run's status for its first tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CiRunStatus {
    Queued,
    Running,
    Passed,
    Failed,
    Cancelled,
}

impl CiRunStatus {
    pub fn id(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn tone(self) -> CiTone {
        match self {
            Self::Queued | Self::Cancelled => CiTone::Neutral,
            Self::Running => CiTone::Info,
            Self::Passed => CiTone::Success,
            Self::Failed => CiTone::Danger,
        }
    }
}

/// A tile's tone (genUI `tone`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CiTone {
    Neutral,
    /// #EXP-2172: running, the one blue mark (ci-page.json `statusTones`).
    Info,
    Success,
    Danger,
}

impl CiTone {
    pub fn id(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Info => "info",
            Self::Success => "success",
            Self::Danger => "danger",
        }
    }
}

fn job_failing(job: &CiJobRow) -> bool {
    is_failing(&job.status, job.conclusion.as_deref())
}

/// `runStatus`: queued = pending with nothing in progress, running = pending
/// with a job in progress, passed = success, cancelled = a failure whose
/// failing jobs are ALL cancelled, failed = any other failure.
pub fn ci_run_status(state: &str, jobs: &[CiJobRow]) -> CiRunStatus {
    let jobs: Vec<(&str, Option<&str>)> = jobs.iter().map(|job| (job.status.as_str(), job.conclusion.as_deref())).collect();
    ci_run_status_of(state, &jobs)
}

/// [`ci_run_status`] over `(status, conclusion)` pairs.
pub fn ci_run_status_of(state: &str, jobs: &[(&str, Option<&str>)]) -> CiRunStatus {
    match state {
        "pending" if jobs.iter().any(|(status, _)| *status == "in_progress") => CiRunStatus::Running,
        "pending" => CiRunStatus::Queued,
        "success" => CiRunStatus::Passed,
        _ => {
            let failing: Vec<&(&str, Option<&str>)> = jobs.iter().filter(|(status, conclusion)| is_failing(status, *conclusion)).collect();
            if !failing.is_empty() && failing.iter().all(|(_, conclusion)| *conclusion == Some("cancelled")) {
                CiRunStatus::Cancelled
            } else {
                CiRunStatus::Failed
            }
        }
    }
}

fn ms_of(value: Option<&str>) -> Option<i64> {
    value.filter(|v| !v.is_empty()).and_then(crate::instant::parse_instant)
}

fn by_position(jobs: &[CiJobRow]) -> Vec<&CiJobRow> {
    let mut ordered: Vec<&CiJobRow> = jobs.iter().collect();
    ordered.sort_by_key(|job| job.position.unwrap_or(0));
    ordered
}

/// One job's `Row` (`jobRows`, and a runner's `recentJobs`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CiJobListRow {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub glyph: CheckGlyph,
    pub tone: CheckTone,
    pub duration: Option<String>,
    /// #EXP-2172: the run page's status and its tone (`statusTones`).
    pub status: CiRunStatus,
    pub status_tone: CiTone,
}

/// `ciJobStatus`: a job's status in the run page's terms.
pub fn ci_job_status(status: &str, conclusion: Option<&str>) -> CiRunStatus {
    if status == "queued" {
        CiRunStatus::Queued
    } else if status != "completed" {
        CiRunStatus::Running
    } else if is_failing(status, conclusion) && conclusion != Some("cancelled") {
        CiRunStatus::Failed
    } else if conclusion == Some("success") {
        CiRunStatus::Passed
    } else {
        CiRunStatus::Cancelled
    }
}

fn reason_text(reason: Option<&str>) -> Option<String> {
    let reason = reason?;
    CI_RUN["copy"]["reasons"][reason].as_str().map(str::to_string)
}

/// `jobRows`: the jobs in planner order.
pub fn ci_job_rows(jobs: &[CiJobRow], now_ms: i64) -> Vec<CiJobListRow> {
    by_position(jobs)
        .into_iter()
        .map(|job| {
            let (glyph, tone) = check_glyph(&job.status, job.conclusion.as_deref());
            CiJobListRow {
                id: job.id.clone(),
                title: job.check.clone(),
                subtitle: if job.status == "completed" { reason_text(job.reason.as_deref()) } else { None },
                glyph,
                tone,
                duration: check_duration_of(&job.status, job.started_at.as_deref(), job.completed_at.as_deref(), now_ms),
                status: ci_job_status(&job.status, job.conclusion.as_deref()),
                status_tone: ci_job_status(&job.status, job.conclusion.as_deref()).tone(),
            }
        })
        .collect()
}

/// `jobsCaption`: `1 failed · 3 passed · 1 skipped` (the non-zero counts,
/// failed, running, queued, passed, cancelled).
pub fn ci_jobs_caption(jobs: &[(&str, Option<&str>)]) -> String {
    let order = [CiRunStatus::Failed, CiRunStatus::Running, CiRunStatus::Queued, CiRunStatus::Passed, CiRunStatus::Cancelled];
    order
        .iter()
        .filter_map(|status| {
            let count = jobs.iter().filter(|(s, c)| ci_job_status(s, *c) == *status).count();
            (count > 0).then(|| copy(&format!("jobsCaption.{}", status.id())).replace("{count}", &count.to_string()))
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// `jobSummary`: the job Card's line, `Failed after 8m 25s on mint`.
pub fn ci_job_summary(job: &CiJobRow, runner_label: Option<&str>, now_ms: i64) -> String {
    let status = ci_job_status(&job.status, job.conclusion.as_deref());
    let duration = check_duration_of(&job.status, job.started_at.as_deref(), job.completed_at.as_deref(), now_ms);
    match status {
        CiRunStatus::Queued => copy("jobSummary.queued").to_string(),
        CiRunStatus::Cancelled => reason_text(job.reason.as_deref()).unwrap_or_else(|| copy("jobSummary.skipped").to_string()),
        _ => {
            let head = copy(&format!("jobSummary.{}", status.id())).replace("{duration}", duration.as_deref().unwrap_or(copy("none")));
            match runner_label.filter(|label| !label.is_empty()) {
                Some(runner) => format!("{head}{}", copy("jobSummary.on").replace("{runner}", runner)),
                None => head,
            }
        }
    }
}

/// One chip of the run page's strip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CiRunChip {
    pub id: &'static str,
    pub label: String,
    pub icon: Option<&'static str>,
    pub dot: Option<&'static str>,
}

fn dot_of(tone: CiTone) -> &'static str {
    match tone {
        CiTone::Success => "$color.success",
        CiTone::Danger => "$color.destructive",
        CiTone::Info => "$color.info",
        CiTone::Neutral => "$color.mutedForeground",
    }
}

/// `chips`: status (dot) · ref → base · sha · #PR · author · duration ·
/// runners (distinct labels in planner order); a chip without data is out.
pub fn ci_run_chips(
    run: &CiRunRow,
    jobs: &[CiJobRow],
    runner_label: &dyn Fn(&CiJobRow) -> Option<String>,
    pull_request: Option<(i64, &str)>,
    author: Option<&str>,
    now_ms: i64,
) -> Vec<CiRunChip> {
    let status = ci_run_status(&run.state, jobs);
    let mut chips = vec![CiRunChip { id: "status", label: copy(&format!("status.{}", status.id())).into(), icon: None, dot: Some(dot_of(status.tone())) }];
    if let Some(r#ref) = run.r#ref.as_deref() {
        let label = match pull_request {
            Some((_, base)) => format!("{} → {}", r#ref, base),
            None => r#ref.to_string(),
        };
        chips.push(CiRunChip { id: "ref", label, icon: Some("git-branch"), dot: None });
    }
    chips.push(CiRunChip { id: "sha", label: run.sha.chars().take(7).collect(), icon: Some("hash"), dot: None });
    if let Some((number, _)) = pull_request {
        chips.push(CiRunChip { id: "pr", label: format!("#{number}"), icon: Some("git-pull-request"), dot: None });
    }
    if let Some(author) = author {
        chips.push(CiRunChip { id: "author", label: author.to_string(), icon: Some("user"), dot: None });
    }
    if let Some(started) = ms_of(run.started_at.as_deref()) {
        let end = ms_of(run.completed_at.as_deref()).unwrap_or(now_ms);
        chips.push(CiRunChip { id: "duration", label: format_check_duration(end - started), icon: Some("clock"), dot: None });
    }
    let mut runners: Vec<String> = Vec::new();
    for job in jobs {
        if let Some(label) = runner_label(job).filter(|label| !label.is_empty()) {
            if !runners.contains(&label) {
                runners.push(label);
            }
        }
    }
    if !runners.is_empty() {
        chips.push(CiRunChip { id: "runner", label: runners.join(", "), icon: Some("server"), dot: None });
    }
    chips
}

/// `failureSummary`: `Failed in {job}` and the first error line of its log
/// as read; None until such a line is read.
pub fn ci_failure_summary(check: Option<&str>, log: Option<&str>) -> Option<(String, String)> {
    let (check, log) = (check?, log?);
    let view = ci_log_view(log);
    let first = *view.highlight.first()?;
    let line = view.code.split('\n').nth(first - 1).unwrap_or_default();
    Some((copy("failure.title").replace("{job}", check), line.strip_prefix("Error: ").unwrap_or(line).to_string()))
}

/// `defaultJob`: the requested job when it is one of the run's, else the
/// first failing, else the first running, else the first.
pub fn ci_default_job(jobs: &[CiJobRow], requested: Option<&str>) -> Option<String> {
    let ordered = by_position(jobs);
    if let Some(requested) = requested.filter(|id| ordered.iter().any(|job| job.id == *id)) {
        return Some(requested.to_string());
    }
    ordered
        .iter()
        .find(|job| job_failing(job))
        .or_else(|| ordered.iter().find(|job| job.status == "in_progress"))
        .or_else(|| ordered.first())
        .map(|job| job.id.clone())
}

/// One step as the runner reports it (`ci_jobs.steps[]`, epoch ms).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CiStep {
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
}

/// A job's `steps` jsonb (tolerant: a malformed entry is skipped).
pub fn ci_steps_of(steps: Option<&Value>) -> Vec<CiStep> {
    steps
        .and_then(Value::as_array)
        .map(|steps| {
            steps
                .iter()
                .filter_map(|step| {
                    Some(CiStep {
                        name: step["name"].as_str()?.to_string(),
                        status: step["status"].as_str().unwrap_or("queued").to_string(),
                        conclusion: step["conclusion"].as_str().map(str::to_string),
                        started_at: step["startedAt"].as_i64(),
                        completed_at: step["completedAt"].as_i64(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A `TraceItem`'s status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CiTraceStatus {
    Pending,
    Running,
    Done,
    Skipped,
    Error,
}

impl CiTraceStatus {
    pub fn id(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Done => "done",
            Self::Skipped => "skipped",
            Self::Error => "error",
        }
    }
}

/// One step as a `TraceItem`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CiTraceItem {
    pub title: String,
    pub status: CiTraceStatus,
    pub description: Option<String>,
}

fn trace_status(step: &CiStep) -> CiTraceStatus {
    match (step.status.as_str(), step.conclusion.as_deref()) {
        ("queued", _) => CiTraceStatus::Pending,
        ("completed", Some("success")) => CiTraceStatus::Done,
        ("completed", Some("skipped" | "neutral")) => CiTraceStatus::Skipped,
        ("completed", _) => CiTraceStatus::Error,
        _ => CiTraceStatus::Running,
    }
}

/// `steps`: a job's steps as trace items, the description = the duration.
pub fn ci_steps(steps: &[CiStep], now_ms: i64) -> Vec<CiTraceItem> {
    steps
        .iter()
        .map(|step| {
            let end = match step.status.as_str() {
                "completed" => step.completed_at,
                "in_progress" => Some(now_ms),
                _ => None,
            };
            CiTraceItem {
                title: step.name.clone(),
                status: trace_status(step),
                description: match (step.started_at, end) {
                    (Some(started), Some(end)) => Some(format_check_duration(end - started)),
                    _ => None,
                },
            }
        })
        .collect()
}

/// The log as shown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CiLogView {
    pub code: String,
    /// 1-based DISPLAYED lines of the runner's errors.
    pub highlight: Vec<usize>,
}

/// `logView`: the runner's markers become plain lines (`##[group]X` → `▸ X`,
/// `##[endgroup]` dropped, `##[error]X` → `Error: X` highlighted,
/// `##[warning]X` → `Warning: X`); a trailing newline adds no line.
pub fn ci_log_view(text: &str) -> CiLogView {
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut highlight = Vec::new();
    for line in lines {
        if line.starts_with("##[endgroup]") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("##[group]") {
            out.push(format!("▸ {rest}"));
        } else if let Some(rest) = line.strip_prefix("##[error]") {
            out.push(format!("Error: {rest}"));
            highlight.push(out.len());
        } else if let Some(rest) = line.strip_prefix("##[warning]") {
            out.push(format!("Warning: {rest}"));
        } else {
            out.push(line.to_string());
        }
    }
    CiLogView { code: out.join("\n"), highlight }
}

/// THE one default action of the run page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CiPrimary {
    RerunFailed,
    RerunAll,
    Cancel,
}

impl CiPrimary {
    pub fn id(self) -> &'static str {
        match self {
            Self::RerunFailed => "rerunFailed",
            Self::RerunAll => "rerunAll",
            Self::Cancel => "cancel",
        }
    }
}

/// What the run offers (#EXP-2172 redesign).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CiRunActions {
    /// Cancel while pending, Re-run failed after a failure, else Re-run all;
    /// None when the run is not ours to change (GitHub without write).
    pub primary: Option<CiPrimary>,
    pub fix: bool,
    /// The ⋯ menu: `rerunAll` (when not the primary), `openExternal`.
    pub menu: Vec<&'static str>,
}

/// `actions`.
pub fn ci_run_actions(run: &CiRunRow) -> CiRunActions {
    let github = run.provider.as_deref() == Some("github");
    let writable = !github || run.external_access.as_deref() == Some("write");
    let primary = writable.then(|| match run.state.as_str() {
        "pending" => CiPrimary::Cancel,
        "failure" => CiPrimary::RerunFailed,
        _ => CiPrimary::RerunAll,
    });
    let mut menu = Vec::new();
    if writable && primary != Some(CiPrimary::RerunAll) {
        menu.push("rerunAll");
    }
    if github && run.external_url.as_deref().is_some_and(|url| !url.is_empty()) {
        menu.push("openExternal");
    }
    CiRunActions { primary, fix: run.state == "failure", menu }
}

/// `fixLogLines`: how much of the log's end Fix with an agent quotes.
pub fn fix_log_lines() -> usize {
    CI_RUN["fixLogLines"].as_u64().unwrap_or(40) as usize
}

/// `fixPrompt`: the first message of Fix with an agent (the composer's
/// text): the run, the failing job, how to read its log, its log's end.
pub fn ci_fix_prompt(run: &CiRunRow, repository: &str, job: Option<&str>, log: Option<&str>) -> String {
    let none = copy("none");
    let r#ref = run.r#ref.as_deref().unwrap_or(none);
    let sha: String = run.sha.chars().take(7).collect();
    let fill = |text: &str| {
        text.replace("{number}", &run.number.unwrap_or_default().to_string())
            .replace("{name}", &run.name)
            .replace("{repository}", repository)
            .replace("{ref}", r#ref)
            .replace("{sha}", &sha)
            .replace("{runId}", &run.id)
            .replace("{job}", job.unwrap_or_default())
    };
    let mut lines = vec![fill(copy("fixPrompt.head"))];
    if job.is_some() {
        lines.push(fill(copy("fixPrompt.job")));
    }
    lines.push(fill(copy("fixPrompt.read")));
    let code = log.map(|log| ci_log_view(log).code).unwrap_or_default();
    if !code.is_empty() {
        let all: Vec<&str> = code.split('\n').collect();
        let tail = &all[all.len().saturating_sub(fix_log_lines())..];
        lines.push(String::new());
        lines.push(copy("fixPrompt.tail").to_string());
        lines.push("```text".into());
        lines.extend(tail.iter().map(|line| line.to_string()));
        lines.push("```".into());
    }
    lines.join("\n")
}

/// The run page's link (`details_url`): `/t/<team>/ci/<runId>[?job=<jobId>]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CiRunLink {
    pub team_slug: String,
    pub run_id: String,
    pub job_id: Option<String>,
}

/// Parses an instance PATH (query and hash kept, as `AppLink::App` holds
/// it) into the run page's link; `None` = another page.
pub fn ci_run_link(path: &str) -> Option<CiRunLink> {
    let path = path.split('#').next().unwrap_or_default();
    let (path, query) = match path.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (path, None),
    };
    let segments: Vec<&str> = path.trim_end_matches('/').split('/').collect();
    let ["", "t", team_slug, "ci", run_id] = segments.as_slice() else {
        return None;
    };
    if team_slug.is_empty() || run_id.is_empty() {
        return None;
    }
    let job_id = query.and_then(|query| {
        query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(key, value)| *key == "job" && !value.is_empty())
            .map(|(_, value)| value.to_string())
    });
    Some(CiRunLink { team_slug: team_slug.to_string(), run_id: run_id.to_string(), job_id })
}

// ── the runner section (runner-settings.json) ────────────────────────────

/// `devices.runner` (`defaults` fill a NULL one).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunnerSettings {
    pub enabled: bool,
    pub labels: Vec<String>,
    pub slots: u32,
    pub timeout_min: u32,
}

/// The row's `runner` jsonb, the fixture's `defaults` for NULL or a missing
/// key.
pub fn runner_settings(runner: Option<&Value>) -> RunnerSettings {
    let defaults = &RUNNER_SETTINGS["defaults"];
    let pick = |key: &str| runner.and_then(|runner| runner.get(key)).filter(|value| !value.is_null()).unwrap_or(&defaults[key]);
    RunnerSettings {
        enabled: pick("enabled").as_bool().unwrap_or(false),
        labels: pick("labels")
            .as_array()
            .map(|labels| labels.iter().filter_map(|label| label.as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
        slots: pick("slots").as_u64().unwrap_or(1) as u32,
        timeout_min: pick("timeoutMin").as_u64().unwrap_or(60) as u32,
    }
}

/// `limits.labels`.
pub fn runner_max_labels() -> usize {
    RUNNER_SETTINGS["limits"]["labels"].as_u64().unwrap_or(16) as usize
}

/// `limits.slots` (min, max).
pub fn runner_slot_limits() -> (u32, u32) {
    let slots = &RUNNER_SETTINGS["limits"]["slots"];
    (slots["min"].as_u64().unwrap_or(1) as u32, slots["max"].as_u64().unwrap_or(16) as u32)
}

fn runner_recent_limit() -> usize {
    RUNNER_SETTINGS["limits"]["recent"].as_u64().unwrap_or(5) as usize
}

/// `timeoutOptions` as (value, label).
pub fn runner_timeout_options() -> Vec<(String, String)> {
    RUNNER_SETTINGS["timeoutOptions"]
        .as_array()
        .map(|options| {
            options
                .iter()
                .map(|option| {
                    (
                        option["value"].as_str().unwrap_or_default().to_string(),
                        option["label"].as_str().unwrap_or_default().to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `normalizeLabel`: trimmed, lowercased, then eCode's label rule
/// `^[a-z0-9][a-z0-9._-]{0,31}$`; `None` = refused.
pub fn normalize_runner_label(input: &str) -> Option<String> {
    let label = input.trim().to_lowercase();
    let mut chars = label.chars();
    let first = chars.next()?;
    let ok = (first.is_ascii_lowercase() || first.is_ascii_digit())
        && label.chars().count() <= 32
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));
    ok.then_some(label)
}

/// The section's notice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunnerNotice {
    Phone,
    Update,
}

impl RunnerNotice {
    pub fn id(self) -> &'static str {
        match self {
            Self::Phone => "phone",
            Self::Update => "update",
        }
    }
}

/// `notice`: a phone client never runs jobs; a device without the
/// `ci-runner` cap is too old to.
pub fn runner_notice(client: &str, caps: &[String]) -> Option<RunnerNotice> {
    if matches!(client, "ios" | "android" | "web-mobile") {
        Some(RunnerNotice::Phone)
    } else if !caps.iter().any(|cap| cap == "ci-runner") {
        Some(RunnerNotice::Update)
    } else {
        None
    }
}

/// `osLabel`: the OS label a runner always has.
pub fn runner_os_label(platform: Option<&str>) -> Option<&'static str> {
    match platform? {
        "linux" => Some("linux"),
        "macos" | "darwin" => Some("mac"),
        "windows" => Some("windows"),
        _ => None,
    }
}

/// A job with its run's number and repository, for `recentJobs`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunnerJob {
    pub id: String,
    pub runner_device_id: Option<String>,
    pub check: String,
    pub run_number: i64,
    pub repo: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

/// `recentJobs`: this runner's jobs, newest start first (never started
/// last), at most `limits.recent`.
pub fn runner_recent_jobs(device_id: &str, jobs: &[RunnerJob], now_ms: i64) -> Vec<CiJobListRow> {
    let mut mine: Vec<&RunnerJob> = jobs.iter().filter(|job| job.runner_device_id.as_deref() == Some(device_id)).collect();
    mine.sort_by_key(|job| std::cmp::Reverse(ms_of(job.started_at.as_deref())));
    mine.into_iter()
        .take(runner_recent_limit())
        .map(|job| {
            let (glyph, tone) = check_glyph(&job.status, job.conclusion.as_deref());
            CiJobListRow {
                id: job.id.clone(),
                title: job.check.clone(),
                subtitle: Some(
                    runner_copy("runSubtitle").replace("{number}", &job.run_number.to_string()).replace("{repo}", &job.repo),
                ),
                glyph,
                tone,
                duration: check_duration_of(&job.status, job.started_at.as_deref(), job.completed_at.as_deref(), now_ms),
                status: ci_job_status(&job.status, job.conclusion.as_deref()),
                status_tone: ci_job_status(&job.status, job.conclusion.as_deref()).tone(),
            }
        })
        .collect()
}

// ── the fixtures as rows (the specimens ×4 draw from them) ───────────────

/// The fixture's camelCase jobs as synced rows (`runnerLabel` rides in
/// `runner_device_id`: the specimens name runners by label; a missing
/// `position` = the index).
pub fn fixture_jobs(jobs: &Value) -> Vec<CiJobRow> {
    jobs.as_array()
        .map(|jobs| {
            jobs.iter()
                .enumerate()
                .map(|(index, job)| {
                    let text = |key: &str| job[key].as_str().map(str::to_string);
                    CiJobRow {
                        id: text("id").unwrap_or_else(|| format!("job-{index}")),
                        team_id: None,
                        run_id: String::new(),
                        key: None,
                        check: text("check").unwrap_or_default(),
                        position: Some(job["position"].as_i64().unwrap_or(index as i64)),
                        runs_on: None,
                        needs: None,
                        matrix: None,
                        timeout_min: None,
                        status: text("status").unwrap_or_default(),
                        conclusion: text("conclusion"),
                        reason: text("reason"),
                        runner_device_id: text("runnerLabel"),
                        steps: job.get("steps").cloned(),
                        log_key: None,
                        log_bytes: None,
                        queued_at: None,
                        started_at: text("startedAt"),
                        completed_at: text("completedAt"),
                        provider: text("provider"),
                        external_id: None,
                        external_url: None,
                        created_at: None,
                        updated_at: None,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The fixture's camelCase run as a synced row.
pub fn fixture_run(run: &Value) -> CiRunRow {
    let text = |key: &str| run[key].as_str().map(str::to_string);
    CiRunRow {
        id: text("id").unwrap_or_default(),
        team_id: None,
        repository_id: text("repositoryId"),
        number: run["number"].as_i64(),
        name: text("name").unwrap_or_default(),
        sha: text("sha").unwrap_or_default(),
        r#ref: text("ref"),
        trigger: text("trigger"),
        pull_request_id: text("pullRequestId"),
        state: text("state").unwrap_or_default(),
        error: text("error"),
        rerun_of: None,
        created_by_user_id: text("createdByUserId"),
        started_at: text("startedAt"),
        completed_at: text("completedAt"),
        provider: text("provider"),
        title: text("title"),
        external_id: None,
        external_url: text("externalUrl"),
        external_attempt: None,
        external_access: text("externalAccess"),
        created_at: text("createdAt"),
        updated_at: None,
    }
}

/// runner-settings.json `recentJobs[]` jobs as [`RunnerJob`]s.
pub fn fixture_runner_jobs(jobs: &Value) -> Vec<RunnerJob> {
    jobs.as_array()
        .map(|jobs| {
            jobs.iter()
                .map(|job| {
                    let text = |key: &str| job[key].as_str().map(str::to_string);
                    RunnerJob {
                        id: text("id").unwrap_or_default(),
                        runner_device_id: text("runnerDeviceId"),
                        check: text("check").unwrap_or_default(),
                        run_number: job["runNumber"].as_i64().unwrap_or_default(),
                        repo: text("repo").unwrap_or_default(),
                        status: text("status").unwrap_or_default(),
                        conclusion: text("conclusion"),
                        started_at: text("startedAt"),
                        completed_at: text("completedAt"),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/ci-run.json");
    const RUNNER: &str = include_str!("../../../../../packages/domain-contract/fixtures/runner-settings.json");

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE).expect("ci-run.json parses")
    }

    fn runner() -> Value {
        serde_json::from_str(RUNNER).expect("runner-settings.json parses")
    }

    fn now(case: &Value) -> i64 {
        crate::instant::parse_instant(case["now"].as_str().unwrap()).unwrap()
    }

    fn row_json(row: &CiJobListRow) -> Value {
        json!({
            "id": row.id,
            "title": row.title,
            "subtitle": row.subtitle,
            "glyph": row.glyph.concept(),
            "tone": row.tone.id(),
            "duration": row.duration,
        })
    }

    #[test]
    fn run_status_matches_the_fixture() {
        for case in fixture()["runStatus"].as_array().unwrap() {
            let status = ci_run_status(case["state"].as_str().unwrap(), &fixture_jobs(&case["jobs"]));
            assert_eq!(json!({"status": status.id(), "tone": status.tone().id()}), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn job_rows_match_the_fixture() {
        for case in fixture()["jobRows"].as_array().unwrap() {
            let got: Vec<Value> = ci_job_rows(&fixture_jobs(&case["jobs"]), now(case))
                .iter()
                .map(|row| {
                    let mut value = row_json(row);
                    value["status"] = json!(row.status.id());
                    value["statusTone"] = json!(row.status_tone.id());
                    value
                })
                .collect();
            assert_eq!(Value::Array(got), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn default_job_matches_the_fixture() {
        for case in fixture()["defaultJob"].as_array().unwrap() {
            let got = ci_default_job(&fixture_jobs(&case["jobs"]), case["requested"].as_str());
            assert_eq!(got.map(Value::String).unwrap_or(Value::Null), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn steps_match_the_fixture() {
        for case in fixture()["steps"].as_array().unwrap() {
            let got: Vec<Value> = ci_steps(&ci_steps_of(Some(&case["steps"])), now(case))
                .iter()
                .map(|item| json!({"title": item.title, "status": item.status.id(), "description": item.description}))
                .collect();
            assert_eq!(Value::Array(got), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn log_view_matches_the_fixture() {
        for case in fixture()["logView"].as_array().unwrap() {
            let view = ci_log_view(case["text"].as_str().unwrap());
            assert_eq!(json!({"code": view.code, "highlight": view.highlight}), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn actions_match_the_fixture() {
        for case in fixture()["actions"].as_array().unwrap() {
            let actions = ci_run_actions(&fixture_run(case));
            let got = json!({"primary": actions.primary.map(CiPrimary::id), "fix": actions.fix, "menu": actions.menu});
            assert_eq!(got, case["expected"], "{}", case["name"]);
        }
    }

    fn pairs(jobs: &[CiJobRow]) -> Vec<(&str, Option<&str>)> {
        jobs.iter().map(|job| (job.status.as_str(), job.conclusion.as_deref())).collect()
    }

    #[test]
    fn jobs_caption_matches_the_fixture() {
        for case in fixture()["jobsCaption"].as_array().unwrap() {
            let jobs = fixture_jobs(&case["jobs"]);
            assert_eq!(ci_jobs_caption(&pairs(&jobs)), case["expected"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn job_summary_matches_the_fixture() {
        for case in fixture()["jobSummary"].as_array().unwrap() {
            let job = &fixture_jobs(&json!([case["job"]]))[0];
            let got = ci_job_summary(job, job.runner_device_id.as_deref(), now(case));
            assert_eq!(got, case["expected"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn chips_match_the_fixture() {
        for case in fixture()["chips"].as_array().unwrap() {
            let pr = case["pullRequest"].as_object().map(|pr| (pr["number"].as_i64().unwrap(), pr["base"].as_str().unwrap()));
            let chips = ci_run_chips(&fixture_run(&case["run"]), &fixture_jobs(&case["jobs"]), &|job| job.runner_device_id.clone(), pr, case["author"].as_str(), now(case));
            let got: Vec<Value> = chips.iter().map(|chip| json!({"id": chip.id, "label": chip.label, "icon": chip.icon, "dot": chip.dot})).collect();
            assert_eq!(Value::Array(got), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn failure_summary_matches_the_fixture() {
        for case in fixture()["failureSummary"].as_array().unwrap() {
            let got = ci_failure_summary(case["job"]["check"].as_str(), case["log"].as_str());
            let got = got.map(|(title, message)| json!({"title": title, "message": message})).unwrap_or(Value::Null);
            assert_eq!(got, case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn fix_prompt_matches_the_fixture() {
        let fixture = fixture();
        let cases = fixture["fixPrompt"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let got = ci_fix_prompt(&fixture_run(&input["run"]), input["repository"].as_str().unwrap(), input["job"].as_str(), input["log"].as_str());
            assert_eq!(got, case["expected"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn the_specimen_selects_its_failing_job() {
        let fixture = fixture();
        let specimen = &fixture["specimen"];
        assert_eq!(ci_default_job(&fixture_jobs(&specimen["jobs"]), None).as_deref(), specimen["selected"].as_str());
        assert_eq!(ci_log_view(specimen["log"].as_str().unwrap()).highlight.len(), 1);
    }

    #[test]
    fn the_copy_reads_off_the_fixture() {
        assert_eq!(run_title(12), "Run #12");
        assert_eq!(test_id("log"), "ci-run-log");
        assert_eq!(live_tail_ms(), 2000);
        assert_eq!(log_max_lines(), 32);
        assert_eq!(runner_copy("section"), "CI runner");
        assert_eq!(runner_test_id("switch"), "runner-enabled");
        assert_eq!(runner_timeout_options()[2], ("60".to_string(), "1 h".to_string()));
        assert_eq!(runner_slot_limits(), (1, 16));
        assert_eq!(runner_max_labels(), 16);
    }

    #[test]
    fn the_run_link_parses() {
        assert_eq!(
            ci_run_link("/t/acme/ci/r12?job=j2"),
            Some(CiRunLink { team_slug: "acme".into(), run_id: "r12".into(), job_id: Some("j2".into()) })
        );
        assert_eq!(ci_run_link("/t/acme/ci/r12/").map(|link| link.job_id), Some(None));
        assert_eq!(ci_run_link("/t/acme/boards/x"), None);
        assert_eq!(ci_run_link("/t/acme/ci"), None);
    }

    #[test]
    fn normalize_label_matches_the_fixture() {
        for case in runner()["normalizeLabel"].as_array().unwrap() {
            let got = normalize_runner_label(case["input"].as_str().unwrap());
            assert_eq!(got.map(Value::String).unwrap_or(Value::Null), case["expected"], "{}", case["input"]);
        }
    }

    #[test]
    fn notice_matches_the_fixture() {
        for case in runner()["notice"].as_array().unwrap() {
            let caps: Vec<String> = case["caps"].as_array().unwrap().iter().map(|cap| cap.as_str().unwrap().to_string()).collect();
            let got = runner_notice(case["client"].as_str().unwrap(), &caps);
            assert_eq!(got.map(|n| Value::String(n.id().into())).unwrap_or(Value::Null), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn os_label_matches_the_fixture() {
        for case in runner()["osLabel"].as_array().unwrap() {
            let got = runner_os_label(case["platform"].as_str());
            assert_eq!(got.map(|label| Value::String(label.into())).unwrap_or(Value::Null), case["expected"], "{}", case["platform"]);
        }
    }

    #[test]
    fn recent_jobs_match_the_fixture() {
        for case in runner()["recentJobs"].as_array().unwrap() {
            let got: Vec<Value> = runner_recent_jobs(case["deviceId"].as_str().unwrap(), &fixture_runner_jobs(&case["jobs"]), now(case))
                .iter()
                .map(row_json)
                .collect();
            assert_eq!(Value::Array(got), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn runner_settings_fill_the_defaults() {
        assert_eq!(runner_settings(None), RunnerSettings { enabled: false, labels: vec![], slots: 1, timeout_min: 60 });
        let runner = runner();
        let specimen = runner_settings(Some(&runner["specimen"]["device"]["runner"]));
        assert_eq!(specimen, RunnerSettings { enabled: true, labels: vec!["gpu".into()], slots: 2, timeout_min: 60 });
        assert_eq!(runner_settings(Some(&json!({"enabled": true}))).timeout_min, 60);
    }
}
