//! eCode's pipeline file `.exponential/ci.json` (`fixtures/pipeline.json` is
//! the spec): validation with JSON-pointer errors, the glob rule, and the
//! planner that turns a pipeline plus a trigger into jobs (matrix expanded,
//! ordered by `needs`, skipped by `paths` or for a fork).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::{
    check_name, is_valid_label, js, Skip, Trigger, MATRIX_ENV_PREFIX, PIPELINE_DEFAULT_TIMEOUT_MIN, PIPELINE_MAX_JOBS,
    PIPELINE_MAX_MATRIX, PIPELINE_MAX_STEPS, PIPELINE_MAX_TIMEOUT_MIN, PIPELINE_NAME_MAX, PIPELINE_VERSION,
};

// ── globs ───────────────────────────────────────────────────────────────

/// `**` = any run of path segments (`**/` may match none), `*` = any run of
/// characters but `/`, `?` = one character but `/`, the rest literal.
pub fn glob_match(pattern: &str, path: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = path.chars().collect();
    glob_from(&p, &s, 0, 0)
}

fn glob_from(p: &[char], s: &[char], i: usize, j: usize) -> bool {
    if i == p.len() {
        return j == s.len();
    }
    if p[i] == '*' && p.get(i + 1) == Some(&'*') {
        let mut k = i;
        while p.get(k) == Some(&'*') {
            k += 1;
        }
        let slash = p.get(k) == Some(&'/');
        if slash {
            k += 1;
        }
        return (j..=s.len()).any(|t| !(slash && t != j && s[t - 1] != '/') && glob_from(p, s, k, t));
    }
    if p[i] == '*' {
        for t in j..=s.len() {
            if t > j && s[t - 1] == '/' {
                break;
            }
            if glob_from(p, s, i + 1, t) {
                return true;
            }
        }
        return false;
    }
    if j == s.len() {
        return false;
    }
    if p[i] == '?' {
        return s[j] != '/' && glob_from(p, s, i + 1, j + 1);
    }
    p[i] == s[j] && glob_from(p, s, i + 1, j + 1)
}

/// A glob is non-empty and relative.
pub fn is_valid_glob(glob: &str) -> bool {
    !glob.is_empty() && !glob.starts_with('/')
}

// ── validation ──────────────────────────────────────────────────────────

/// One validation error: a JSON pointer and a code.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PipelineError {
    pub path: String,
    pub code: String,
}

fn ptr(key: &str) -> String {
    format!("/{}", key.replace('~', "~0").replace('/', "~1"))
}

fn obj(value: Option<&Value>) -> Option<&Map<String, Value>> {
    value.and_then(Value::as_object)
}

/// `^[a-z][a-z0-9_-]{0,63}$`
fn is_job_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && key.len() <= 64
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// `^[A-Z_][A-Z0-9_]*$`
fn is_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase() || c == '_')
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// `^[a-z][a-z0-9_]*$`
fn is_matrix_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

const TOP_KEYS: &[&str] = &["$schema", "version", "name", "on", "env", "secrets", "jobs", "required"];
const JOB_KEYS: &[&str] = &["name", "runsOn", "needs", "timeoutMin", "env", "secrets", "paths", "matrix", "steps"];
const STEP_KEYS: &[&str] = &["name", "run", "env", "timeoutMin", "continueOnError", "workingDirectory"];

fn trigger_keys(trigger: Trigger) -> &'static [&'static str] {
    match trigger {
        Trigger::Push | Trigger::PullRequest => &["branches"],
        Trigger::Tag => &["tags"],
        Trigger::MergeTrain | Trigger::Manual => &[],
    }
}

/// The matrix combinations in file order (first key outermost); one empty
/// combination = one plain job.
pub fn matrix_combos(matrix: Option<&Value>) -> Vec<Vec<(String, String)>> {
    let mut combos: Vec<Vec<(String, String)>> = vec![vec![]];
    let Some(matrix) = obj(matrix) else { return combos };
    for (key, values) in js::entries(matrix) {
        let Some(values) = values.as_array() else { continue };
        let strings: Vec<&str> = values.iter().filter_map(Value::as_str).collect();
        if strings.is_empty() {
            continue;
        }
        combos = combos
            .iter()
            .flat_map(|combo| {
                strings.iter().map(move |value| {
                    let mut next = combo.clone();
                    next.push((key.clone(), (*value).to_string()));
                    next
                })
            })
            .collect();
    }
    combos
}

fn job_display_name<'a>(key: &'a str, job: &'a Map<String, Value>) -> &'a str {
    job.get("name").and_then(Value::as_str).unwrap_or(key)
}

fn combo_check(name: &str, combo: &[(String, String)]) -> String {
    let values: Vec<&str> = combo.iter().map(|(_, v)| v.as_str()).collect();
    check_name(name, &values)
}

/// Every check name a job expands to.
pub fn job_check_names(key: &str, job: Option<&Value>) -> Vec<String> {
    let Some(job) = obj(job) else { return vec![key.to_string()] };
    let name = job_display_name(key, job);
    matrix_combos(job.get("matrix")).iter().map(|combo| combo_check(name, combo)).collect()
}

struct Validator {
    errors: Vec<PipelineError>,
}

impl Validator {
    fn err(&mut self, path: impl Into<String>, code: &str) {
        self.errors.push(PipelineError { path: path.into(), code: code.to_string() });
    }

    fn unknown_keys(&mut self, obj: &Map<String, Value>, allowed: &[&str], at: &str) {
        for (key, _) in js::entries(obj) {
            if !allowed.contains(&key.as_str()) {
                self.err(format!("{at}{}", ptr(key)), "unknown_field");
            }
        }
    }

    fn text(&mut self, obj: &Map<String, Value>, key: &str, at: &str, required: bool, max: Option<usize>) {
        let path = format!("{at}/{key}");
        match obj.get(key) {
            None if required => self.err(path, "missing"),
            None => {}
            Some(Value::String(value)) => {
                if value.is_empty() || max.is_some_and(|max| value.chars().count() > max) {
                    self.err(path, "invalid_value");
                }
            }
            Some(_) => self.err(path, "type"),
        }
    }

    fn env(&mut self, value: Option<&Value>, at: &str) {
        let Some(value) = value else { return };
        let Some(env) = value.as_object() else { return self.err(at, "type") };
        for (name, v) in js::entries(env) {
            if !is_env_name(name) {
                self.err(format!("{at}{}", ptr(name)), "invalid_name");
            } else if !v.is_string() {
                self.err(format!("{at}{}", ptr(name)), "type");
            }
        }
    }

    fn names(&mut self, value: Option<&Value>, at: &str) {
        let Some(value) = value else { return };
        let Some(items) = value.as_array() else { return self.err(at, "type") };
        for (i, v) in items.iter().enumerate() {
            match v.as_str() {
                None => self.err(format!("{at}/{i}"), "type"),
                Some(name) if !is_env_name(name) => self.err(format!("{at}/{i}"), "invalid_name"),
                Some(_) => {}
            }
        }
    }

    fn globs(&mut self, value: Option<&Value>, at: &str, non_empty: bool) {
        let Some(value) = value else { return };
        let Some(items) = value.as_array() else { return self.err(at, "type") };
        if non_empty && items.is_empty() {
            return self.err(at, "invalid_value");
        }
        for (i, v) in items.iter().enumerate() {
            match v.as_str() {
                None => self.err(format!("{at}/{i}"), "type"),
                Some(glob) if !is_valid_glob(glob) => self.err(format!("{at}/{i}"), "invalid_glob"),
                Some(_) => {}
            }
        }
    }

    fn timeout(&mut self, value: Option<&Value>, at: &str) {
        let Some(value) = value else { return };
        if !js::is_int(Some(value)) {
            return self.err(at, "type");
        }
        let minutes = value.as_f64().unwrap_or_default();
        if minutes < 1.0 || minutes > f64::from(PIPELINE_MAX_TIMEOUT_MIN) {
            self.err(at, "invalid_value");
        }
    }

    fn on(&mut self, on: Option<&Value>) {
        let Some(on) = on else { return self.err("/on", "missing") };
        let Some(on) = on.as_object() else { return self.err("/on", "type") };
        if on.is_empty() {
            return self.err("/on", "invalid_value");
        }
        for (kind, filter) in js::entries(on) {
            let at = format!("/on{}", ptr(kind));
            let Some(trigger) = Trigger::parse(kind) else {
                self.err(at, "unknown_field");
                continue;
            };
            let Some(filter) = filter.as_object() else {
                self.err(at, "type");
                continue;
            };
            let allowed = trigger_keys(trigger);
            self.unknown_keys(filter, allowed, &at);
            for key in allowed {
                self.globs(filter.get(*key), &format!("{at}/{key}"), false);
            }
        }
    }

    /// One job; `false` = its `needs` are unsound (no cycle check then).
    fn job(&mut self, jobs: &Map<String, Value>, key: &str, job: &Value, expanded: &mut usize) -> bool {
        let at = format!("/jobs{}", ptr(key));
        let mut sound = true;
        if !is_job_key(key) {
            self.err(at.clone(), "invalid_name");
        }
        let Some(job) = job.as_object() else {
            self.err(at, "type");
            *expanded += 1;
            return false;
        };
        self.unknown_keys(job, JOB_KEYS, &at);
        self.text(job, "name", &at, false, Some(PIPELINE_NAME_MAX));

        match job.get("runsOn") {
            None => self.err(format!("{at}/runsOn"), "missing"),
            Some(Value::Array(labels)) if labels.is_empty() => self.err(format!("{at}/runsOn"), "invalid_value"),
            Some(Value::Array(labels)) => {
                for (i, label) in labels.iter().enumerate() {
                    match label.as_str() {
                        None => self.err(format!("{at}/runsOn/{i}"), "type"),
                        Some(label) if !is_valid_label(label) => self.err(format!("{at}/runsOn/{i}"), "invalid_label"),
                        Some(_) => {}
                    }
                }
            }
            Some(_) => self.err(format!("{at}/runsOn"), "type"),
        }

        match job.get("needs") {
            None => {}
            Some(Value::Array(needs)) => {
                for (i, need) in needs.iter().enumerate() {
                    match need.as_str() {
                        None => {
                            self.err(format!("{at}/needs/{i}"), "type");
                            sound = false;
                        }
                        Some(need) if !jobs.contains_key(need) => {
                            self.err(format!("{at}/needs/{i}"), "unknown_need");
                            sound = false;
                        }
                        Some(_) => {}
                    }
                }
            }
            Some(_) => {
                self.err(format!("{at}/needs"), "type");
                sound = false;
            }
        }

        self.timeout(job.get("timeoutMin"), &format!("{at}/timeoutMin"));
        self.env(job.get("env"), &format!("{at}/env"));
        self.names(job.get("secrets"), &format!("{at}/secrets"));
        self.globs(job.get("paths"), &format!("{at}/paths"), true);

        let matrix = job.get("matrix");
        if let Some(matrix) = matrix {
            let mat_at = format!("{at}/matrix");
            match matrix.as_object() {
                None => self.err(mat_at, "type"),
                Some(m) if m.is_empty() => self.err(mat_at, "invalid_value"),
                Some(m) => {
                    for (mkey, values) in js::entries(m) {
                        let mat = format!("{mat_at}{}", ptr(mkey));
                        if !is_matrix_key(mkey) {
                            self.err(mat.clone(), "invalid_name");
                        }
                        match values.as_array() {
                            None => self.err(mat, "type"),
                            Some(values) if values.is_empty() => self.err(mat, "invalid_value"),
                            Some(values) => {
                                for (i, v) in values.iter().enumerate() {
                                    if !v.is_string() {
                                        self.err(format!("{mat}/{i}"), "type");
                                    }
                                }
                            }
                        }
                    }
                    if matrix_combos(Some(matrix)).len() > PIPELINE_MAX_MATRIX {
                        self.err(mat_at, "limit");
                    }
                }
            }
        }
        *expanded += matrix_combos(matrix).len();

        match job.get("steps") {
            None => self.err(format!("{at}/steps"), "missing"),
            Some(Value::Array(steps)) if steps.is_empty() => self.err(format!("{at}/steps"), "invalid_value"),
            Some(Value::Array(steps)) => {
                if steps.len() > PIPELINE_MAX_STEPS {
                    self.err(format!("{at}/steps"), "limit");
                }
                for (i, step) in steps.iter().enumerate() {
                    self.step(step, &format!("{at}/steps/{i}"));
                }
            }
            Some(_) => self.err(format!("{at}/steps"), "type"),
        }
        sound
    }

    fn step(&mut self, step: &Value, sat: &str) {
        let Some(step) = step.as_object() else { return self.err(sat, "type") };
        self.unknown_keys(step, STEP_KEYS, sat);
        self.text(step, "name", sat, true, Some(PIPELINE_NAME_MAX));
        self.text(step, "run", sat, true, None);
        self.env(step.get("env"), &format!("{sat}/env"));
        self.timeout(step.get("timeoutMin"), &format!("{sat}/timeoutMin"));
        if step.get("continueOnError").is_some_and(|v| !v.is_boolean()) {
            self.err(format!("{sat}/continueOnError"), "type");
        }
        match step.get("workingDirectory") {
            None => {}
            Some(Value::String(dir)) => {
                if dir.is_empty() || dir.starts_with('/') || dir.split('/').any(|s| s == "..") {
                    self.err(format!("{sat}/workingDirectory"), "invalid_value");
                }
            }
            Some(_) => self.err(format!("{sat}/workingDirectory"), "type"),
        }
    }
}

/// Every error of a pipeline file, sorted by path, then code (empty = valid).
pub fn validate_pipeline(file: &Value) -> Vec<PipelineError> {
    let Some(file) = file.as_object() else {
        return vec![PipelineError { path: String::new(), code: "type".into() }];
    };
    let mut v = Validator { errors: vec![] };
    v.unknown_keys(file, TOP_KEYS, "");

    match file.get("version") {
        None => v.err("/version", "missing"),
        Some(version) if !js::is_int(Some(version)) => v.err("/version", "type"),
        Some(version) if version.as_f64() != Some(f64::from(PIPELINE_VERSION)) => v.err("/version", "invalid_value"),
        Some(_) => {}
    }
    v.text(file, "name", "", false, Some(PIPELINE_NAME_MAX));
    v.on(file.get("on"));
    v.env(file.get("env"), "/env");
    v.names(file.get("secrets"), "/secrets");

    let jobs = file.get("jobs");
    match jobs {
        None => v.err("/jobs", "missing"),
        Some(Value::Object(jobs)) if jobs.is_empty() => v.err("/jobs", "invalid_value"),
        Some(Value::Object(jobs)) => {
            let mut expanded = 0;
            let mut needs_sound = true;
            for (key, job) in js::entries(jobs) {
                needs_sound &= v.job(jobs, key, job, &mut expanded);
            }
            if expanded > PIPELINE_MAX_JOBS {
                v.err("/jobs", "limit");
            }
            if needs_sound && has_cycle(jobs) {
                v.err("/jobs", "cycle");
            }
        }
        Some(_) => v.err("/jobs", "type"),
    }

    if let Some(required) = file.get("required") {
        match required.as_array() {
            None => v.err("/required", "type"),
            Some(required) => {
                let known: Vec<String> = obj(jobs)
                    .map(|jobs| js::entries(jobs).into_iter().flat_map(|(k, j)| job_check_names(k, Some(j))).collect())
                    .unwrap_or_default();
                for (i, name) in required.iter().enumerate() {
                    match name.as_str() {
                        None => v.err(format!("/required/{i}"), "type"),
                        Some(name) if !known.iter().any(|k| k == name) => {
                            v.err(format!("/required/{i}"), "unknown_required");
                        }
                        Some(_) => {}
                    }
                }
            }
        }
    }

    let mut errors = v.errors;
    errors.sort();
    errors
}

fn needs_of(job: Option<&Value>) -> Vec<&str> {
    obj(job).map(needs_of_job).unwrap_or_default()
}

fn needs_of_job(job: &Map<String, Value>) -> Vec<&str> {
    job.get("needs")
        .and_then(Value::as_array)
        .map(|needs| needs.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

fn has_cycle(jobs: &Map<String, Value>) -> bool {
    #[derive(PartialEq)]
    enum Mark {
        Visiting,
        Done,
    }
    fn visit<'a>(jobs: &'a Map<String, Value>, key: &'a str, marks: &mut BTreeMap<&'a str, Mark>) -> bool {
        match marks.get(key) {
            Some(Mark::Done) => return false,
            Some(Mark::Visiting) => return true,
            None => {}
        }
        marks.insert(key, Mark::Visiting);
        for need in needs_of(jobs.get(key)) {
            if visit(jobs, need, marks) {
                return true;
            }
        }
        marks.insert(key, Mark::Done);
        false
    }
    let mut marks = BTreeMap::new();
    js::entries(jobs).into_iter().any(|(key, _)| visit(jobs, key, &mut marks))
}

// ── the planner ─────────────────────────────────────────────────────────

/// What fired: the trigger and what the filters and skips look at.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerInput {
    pub kind: Trigger,
    /// push: the pushed branch.
    #[serde(default)]
    pub branch: Option<String>,
    /// pull_request: the PR's base branch.
    #[serde(default)]
    pub base: Option<String>,
    /// tag: the tag name.
    #[serde(default)]
    pub tag: Option<String>,
    /// Changed paths; `None` = unknown (every job runs).
    #[serde(default)]
    pub changed_files: Option<Vec<String>>,
    /// pull_request from outside the team.
    #[serde(default)]
    pub fork: bool,
}

impl TriggerInput {
    /// A trigger with nothing else known.
    pub fn new(kind: Trigger) -> Self {
        Self { kind, branch: None, base: None, tag: None, changed_files: None, fork: false }
    }
}

/// One step of a planned job, its env merged.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedStep {
    pub name: String,
    pub run: String,
    pub env: BTreeMap<String, String>,
    pub timeout_min: Option<i64>,
    pub continue_on_error: bool,
    pub working_directory: Option<String>,
}

/// One check to run: a job, expanded for one matrix combination.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedJob {
    pub check: String,
    /// The job's key in the file.
    pub job: String,
    /// The combination, in matrix key order.
    #[serde(serialize_with = "pairs")]
    pub matrix: Vec<(String, String)>,
    pub runs_on: Vec<String>,
    /// Check names.
    pub needs: Vec<String>,
    pub timeout_min: i64,
    pub env: BTreeMap<String, String>,
    pub secrets: Vec<String>,
    pub skip: Option<Skip>,
    pub steps: Vec<PlannedStep>,
}

fn pairs<S: Serializer>(pairs: &[(String, String)], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_map(pairs.iter().map(|(k, v)| (k, v)))
}

/// What a pipeline runs for one trigger.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Plan {
    pub name: String,
    pub jobs: Vec<PlannedJob>,
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

fn string_map(value: Option<&Value>) -> BTreeMap<String, String> {
    obj(value)
        .map(|m| js::entries(m).into_iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect())
        .unwrap_or_default()
}

fn filter_matches(filter: &Map<String, Value>, key: &str, value: Option<&str>) -> bool {
    let Some(globs) = filter.get(key) else { return true };
    let Some(value) = value else { return false };
    strings(Some(globs)).iter().any(|glob| glob_match(glob, value))
}

fn int(value: Option<&Value>) -> Option<i64> {
    js::is_int(value).then(|| value.and_then(Value::as_f64).unwrap_or_default() as i64)
}

/// The jobs a VALID pipeline runs for a trigger, `None` = not triggered.
pub fn plan_pipeline(file: &Value, trigger: &TriggerInput) -> Option<Plan> {
    let file = file.as_object()?;
    let on = obj(file.get("on"))?;
    let jobs = obj(file.get("jobs"))?;
    let filter = obj(on.get(trigger.kind.as_str()))?;
    let fires = match trigger.kind {
        Trigger::Push => filter_matches(filter, "branches", trigger.branch.as_deref()),
        Trigger::PullRequest => filter_matches(filter, "branches", trigger.base.as_deref()),
        Trigger::Tag => filter_matches(filter, "tags", trigger.tag.as_deref()),
        Trigger::MergeTrain | Trigger::Manual => true,
    };
    if !fires {
        return None;
    }

    let keys: Vec<&String> = js::entries(jobs).into_iter().map(|(k, _)| k).collect();
    let mut order: Vec<&String> = vec![];
    while order.len() < keys.len() {
        let next = keys.iter().copied().find(|key| {
            !order.contains(key)
                && needs_of(jobs.get(key.as_str())).iter().all(|need| order.iter().any(|o| o.as_str() == *need))
        })?;
        order.push(next);
    }

    let top_env = string_map(file.get("env"));
    let top_secrets = strings(file.get("secrets"));
    let empty = Map::new();
    let mut planned = vec![];
    for key in order {
        let job = obj(jobs.get(key.as_str())).unwrap_or(&empty);
        let skip = if trigger.kind == Trigger::PullRequest && trigger.fork {
            Some(Skip::Fork)
        } else {
            match (&trigger.changed_files, job.get("paths")) {
                (Some(changed), Some(paths)) => {
                    let globs = strings(Some(paths));
                    let hit = changed.iter().any(|path| globs.iter().any(|glob| glob_match(glob, path)));
                    (!hit).then_some(Skip::Paths)
                }
                _ => None,
            }
        };
        let needs: Vec<String> =
            needs_of_job(job).iter().flat_map(|need| job_check_names(need, jobs.get(*need))).collect();
        let secrets: Vec<String> =
            top_secrets.iter().chain(strings(job.get("secrets")).iter()).cloned().collect::<BTreeSet<_>>().into_iter().collect();
        let steps: Vec<&Map<String, Value>> =
            job.get("steps").and_then(Value::as_array).map(|s| s.iter().filter_map(Value::as_object).collect()).unwrap_or_default();
        for combo in matrix_combos(job.get("matrix")) {
            let mut env = top_env.clone();
            env.extend(string_map(job.get("env")));
            for (k, v) in &combo {
                env.insert(format!("{MATRIX_ENV_PREFIX}{}", k.to_uppercase()), v.clone());
            }
            let planned_steps = steps
                .iter()
                .map(|step| {
                    let mut step_env = env.clone();
                    step_env.extend(string_map(step.get("env")));
                    PlannedStep {
                        name: js::string(step.get("name")),
                        run: js::string(step.get("run")),
                        env: step_env,
                        timeout_min: int(step.get("timeoutMin")),
                        continue_on_error: step.get("continueOnError") == Some(&Value::Bool(true)),
                        working_directory: step.get("workingDirectory").and_then(Value::as_str).map(str::to_string),
                    }
                })
                .collect();
            planned.push(PlannedJob {
                check: combo_check(job_display_name(key, job), &combo),
                job: key.clone(),
                matrix: combo,
                runs_on: strings(job.get("runsOn")),
                needs: needs.clone(),
                timeout_min: int(job.get("timeoutMin")).unwrap_or(i64::from(PIPELINE_DEFAULT_TIMEOUT_MIN)),
                env,
                secrets: secrets.clone(),
                skip,
                steps: planned_steps,
            });
        }
    }
    let name = file.get("name").and_then(Value::as_str).unwrap_or("ci").to_string();
    Some(Plan { name, jobs: planned })
}
