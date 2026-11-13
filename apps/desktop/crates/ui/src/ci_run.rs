//! #EXP-1617 / #EXP-1804 — our own CI on the desktop, genUI catalog only
//! (SAME LOOK ×4): THE CI RUN PAGE (`fixtures/ci-run.json`, view `ci-run`,
//! [`CiRunView`]; [`crate::navigation::Screen::CiRun`], the in-app target of
//! every check's `details_url` `/t/<team>/ci/<runId>?job=<jobId>`) and a
//! device's CI RUNNER SECTION (`fixtures/runner-settings.json`, view
//! `machine-settings`, [`RunnerSection`] in the device settings dialog).
//! Both are documents of catalog components painted by the SDK's gpui
//! painter; the rules are `domain::ci_run`'s.
//!
//! DATA: the synced `ci_runs` / `ci_jobs` rows and the devices shape; the
//! selected job's log through tRPC `ci.log` (read once from 0, then from
//! `nextOffset` every `liveTailMs` while not `complete`). WRITES: `ci.rerun`
//! (then the new run opens), `ci.cancel`, `ci.setRunner` (the device owner).

use std::rc::Rc;
use std::time::Duration;

use eui_gpui::host::{ActionEvent, HostPlugin};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{
    div, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Task, WeakEntity, Window,
};
use serde_json::{json, Value};
use sync::Store;

use domain::ci_run::{self as model, CiJobListRow, CiLogView, CiTile, CiTraceItem, RunnerJob, RunnerNotice, RunnerSettings};
use domain::pr_checks::CheckTone;
use domain::rows::{CiJobRow, CiRunRow, DeviceRow};

use crate::queries;

// ---------------------------------------------------------------------------
// The document host
// ---------------------------------------------------------------------------

/// The painter's host for these documents: the IDE's icons, every action to
/// `on_action` (deferred: the surface is mid-update when it fires).
struct ActionHost {
    on_action: Rc<dyn Fn(ActionEvent, &mut App)>,
}

impl HostPlugin for ActionHost {
    fn icon(&self, name: &str) -> Option<SharedString> {
        crate::exponential_ui_host::icon_path(name)
    }

    fn on_action(&self, event: &ActionEvent, cx: &mut App) {
        let (on_action, event) = (self.on_action.clone(), event.clone());
        cx.defer(move |cx| on_action(event, cx));
    }
}

/// A document of catalog components in the Exponential theme (dark, the
/// frame's width), its actions routed to `on_action`.
fn mount(surface_id: &str, on_action: Rc<dyn Fn(ActionEvent, &mut App)>, window: &mut Window, cx: &mut App) -> Entity<SurfaceView> {
    let options = SurfaceViewOptions {
        surface_id: surface_id.to_string(),
        theme: eui_core::themes::builtin_theme(crate::exponential_ui_host::THEME_ID),
        mode: eui_core::theme::Mode::Dark,
        host: Rc::new(ActionHost { on_action }),
        width: None,
        settings: Some(eui_core::surface::SurfaceSettings {
            mode: eui_core::theme::ModeSetting::Dark,
            reduced_motion: theme::motion::reduce_motion(),
            ..Default::default()
        }),
        ..Default::default()
    };
    cx.new(|cx| SurfaceView::new(options, window, cx))
}

/// Shows `tree` (a nested catalog node) on `view` when it changed.
fn show(view: &Entity<SurfaceView>, last: &mut Value, tree: Value, cx: &mut App) {
    if *last == tree {
        return;
    }
    let nested: eui_core::NestedNode = match serde_json::from_value(tree.clone()) {
        Ok(nested) => nested,
        Err(error) => {
            log::warn!("[ci] the document does not parse: {error}");
            return;
        }
    };
    *last = tree;
    view.update(cx, |view, cx| {
        let outcome = view.set_nested(nested, cx);
        if !outcome.issues.is_empty() {
            log::debug!("[ci] document issues: {:?}", outcome.issues);
        }
    });
}

fn set_data(view: &Entity<SurfaceView>, path: &str, value: Value, cx: &mut App) {
    view.update(cx, |view, cx| {
        if let Err(error) = view.set_data(path, Some(value), cx) {
            log::warn!("[ci] the document data {path}: {error}");
        }
    });
}

fn press(name: &str, context: Value) -> Value {
    json!({"press": {"event": {"name": name, "context": context}}})
}

/// A job row's glyph: the pr-checks concept, red / green as the tone.
fn glyph(id: &str, row: &CiJobListRow) -> Value {
    let mut props = json!({"name": row.glyph.concept()});
    match row.tone {
        CheckTone::Red => props["tone"] = json!("danger"),
        CheckTone::Green => props["tone"] = json!("success"),
        CheckTone::Muted => {}
    }
    json!({"id": id, "component": "Icon", "props": props})
}

fn job_row(id: &str, row: &CiJobListRow, selected: bool, on: Option<Value>) -> Value {
    let mut props = json!({"title": row.title, "selected": selected});
    if let Some(subtitle) = &row.subtitle {
        props["subtitle"] = json!(subtitle);
    }
    if let Some(duration) = &row.duration {
        props["meta"] = json!(duration);
    }
    let mut node = json!({
        "id": id,
        "component": "Row",
        "props": props,
        "slots": {"leading": glyph(&format!("{id}-glyph"), row)},
    });
    if let Some(on) = on {
        node["props"]["pressable"] = json!(true);
        node["on"] = on;
    }
    node
}

// ---------------------------------------------------------------------------
// The run page
// ---------------------------------------------------------------------------

/// What the run page shows (pure; [`page_tree`] draws it).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PageModel {
    pub number: i64,
    pub name: String,
    pub tiles: Vec<CiTile>,
    pub rerun_failed: bool,
    pub cancel: bool,
    pub error: Option<String>,
    pub jobs: Vec<CiJobListRow>,
    pub selected: Option<String>,
    pub steps: Vec<CiTraceItem>,
    /// The selected job's log as shown; `None` = copy.emptyLog.
    pub log: Option<CiLogView>,
    pub failing: bool,
    pub follow: bool,
}

impl PageModel {
    /// The page over `run` and its `jobs` with the selected job's `log`
    /// text (`None` = not read yet).
    pub(crate) fn new(
        run: &CiRunRow,
        jobs: &[CiJobRow],
        requested: Option<&str>,
        log: Option<&str>,
        runner_label: &dyn Fn(&CiJobRow) -> Option<String>,
        now_ms: i64,
    ) -> Self {
        let selected = model::ci_default_job(jobs, requested);
        let job = selected.as_deref().and_then(|id| jobs.iter().find(|job| job.id == id));
        let actions = model::ci_run_actions(&run.state);
        let running = job.is_some_and(|job| job.status != "completed");
        let log = log.filter(|text| !text.is_empty() || running).map(model::ci_log_view);
        Self {
            number: run.number.unwrap_or_default(),
            name: run.name.clone(),
            tiles: model::ci_run_tiles(run, jobs, runner_label, now_ms),
            rerun_failed: actions.rerun_failed,
            cancel: actions.cancel,
            error: run.error.clone().filter(|error| !error.is_empty()),
            jobs: model::ci_job_rows(jobs, now_ms),
            steps: job.map(|job| model::ci_steps(&model::ci_steps_of(job.steps.as_ref()), now_ms)).unwrap_or_default(),
            log,
            failing: job.is_some_and(|job| domain::pr_checks::is_failing(&job.status, job.conclusion.as_deref())),
            follow: running,
            selected,
        }
    }
}

/// The page as ONE document (ci-run.json LAYOUT, wide): the heading with the
/// run's name muted, the four tiles, the actions, then the jobs Section
/// beside the selected job's steps and log (or the pipeline error).
pub(crate) fn page_tree(page: &PageModel) -> Value {
    let id = model::test_id;
    // The same tree as web's (`apps/web/src/lib/ci-run-view.ts`): the title over its muted name.
    let heading = json!({
        "id": "ci-run-title",
        "component": "Stack",
        "props": {"direction": "vertical", "gap": "xs", "align": "stretch"},
        "children": [
            {"id": "ci-run-heading", "component": "Heading", "props": {"text": model::run_title(page.number), "level": "h2"}},
            {"id": "ci-run-name", "component": "Text", "props": {"text": page.name, "variant": "muted"}},
        ],
    });
    let tiles: Vec<Value> = page
        .tiles
        .iter()
        .enumerate()
        .map(|(index, tile)| {
            json!({
                "id": format!("{}-{index}", id("tiles")),
                "component": "StatTile",
                "props": {"label": tile.label, "value": tile.value, "tone": tile.tone.id()},
            })
        })
        .collect();
    let mut children = vec![
        heading,
        json!({"id": id("tiles"), "component": "Grid", "props": {"columns": 4, "gap": "md"}, "children": tiles}),
    ];
    let mut buttons = Vec::new();
    if page.rerun_failed {
        buttons.push(json!({
            "id": id("rerun"),
            "component": "Button",
            "props": {"label": model::copy("rerunFailed")},
            "on": press("rerun", json!({})),
        }));
    }
    if page.cancel {
        // ci-run.json: destructive-outline; the catalog's `variant` has
        // `destructive` as its one danger form.
        buttons.push(json!({
            "id": id("cancel"),
            "component": "Button",
            "props": {"label": model::copy("cancel"), "variant": "destructive"},
            "on": press("cancel", json!({})),
        }));
    }
    if !buttons.is_empty() {
        children.push(json!({
            "id": "ci-run-actions",
            "component": "Stack",
            "props": {"direction": "horizontal", "gap": "sm"},
            "children": buttons,
        }));
    }
    if let Some(error) = &page.error {
        children.push(json!({
            "id": "ci-run-error",
            "component": "Alert",
            "props": {"type": "error", "title": model::copy("pipelineError"), "message": error},
        }));
    } else {
        let rows: Vec<Value> = page
            .jobs
            .iter()
            .enumerate()
            .map(|(index, row)| {
                job_row(
                    &format!("{}-{index}", id("jobRow")),
                    row,
                    page.selected.as_deref() == Some(row.id.as_str()),
                    Some(press("select", json!({"job": row.id}))),
                )
            })
            .collect();
        let jobs = json!({
            "id": "ci-run-jobs",
            "component": "Section",
            "props": {"title": model::copy("jobs")},
            "style": {"width": 280, "flexShrink": 0},
            "children": rows,
        });
        let mut detail = Vec::new();
        if !page.steps.is_empty() {
            let items: Vec<Value> = page
                .steps
                .iter()
                .enumerate()
                .map(|(index, step)| {
                    let mut props = json!({"title": step.title, "status": step.status.id()});
                    if let Some(description) = &step.description {
                        props["description"] = json!(description);
                    }
                    json!({"id": format!("{}-{index}", id("steps")), "component": "TraceItem", "props": props})
                })
                .collect();
            detail.push(json!({"id": id("steps"), "component": "Trace", "children": items}));
        }
        detail.push(match &page.log {
            Some(log) => json!({
                "id": id("log"),
                "component": "CodeBlock",
                "props": {
                    "code": log.code,
                    "language": "ansi",
                    "title": model::copy("log"),
                    "lineNumbers": true,
                    "wrap": false,
                    "copyable": true,
                    "maxLines": model::log_max_lines(),
                    "variant": if page.failing { "destructive" } else { "default" },
                    "highlight": log.highlight,
                    "follow": page.follow,
                },
            }),
            None => json!({"id": id("log"), "component": "Text", "props": {"text": model::copy("emptyLog"), "variant": "muted"}}),
        });
        children.push(json!({
            "id": "ci-run-body",
            "component": "Stack",
            "props": {"direction": "horizontal", "gap": "lg", "align": "start"},
            "children": [
                jobs,
                {
                    "id": "ci-run-detail",
                    "component": "Stack",
                    "props": {"direction": "vertical", "gap": "md"},
                    "style": {"flexGrow": 1, "flexShrink": 1, "minWidth": 0},
                    "children": detail,
                },
            ],
        }));
    }
    json!({
        "id": id("page"),
        "component": "Stack",
        "props": {"direction": "vertical", "gap": "lg"},
        "children": children,
    })
}

/// Where the page's data comes from.
#[derive(Clone, PartialEq)]
enum Source {
    /// The synced run `run_id`.
    Live(String),
    /// ci-run.json `specimen` (the shot).
    Specimen,
}

/// A job's log as read so far.
#[derive(Clone, Default)]
struct LogState {
    text: String,
    next_offset: u64,
    complete: bool,
}

/// THE run page.
pub(crate) struct CiRunView {
    source: Source,
    /// The job asked for (`?job=`, then a row press).
    requested: Option<String>,
    logs: std::collections::HashMap<String, LogState>,
    /// The tail of the log being read (`None` = none).
    tail: Option<(String, Task<()>)>,
    view: Entity<SurfaceView>,
    last: Value,
    _subscriptions: Vec<Subscription>,
}

impl CiRunView {
    /// The live page of run `run_id`, `job` selected when it is one of its.
    pub(crate) fn new(run_id: String, job: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build(Source::Live(run_id), job, window, cx)
    }

    /// ci-run.json `specimen`, no server.
    pub(crate) fn specimen(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self::build(Source::Specimen, None, window, cx);
        let log = model::CI_RUN["specimen"]["log"].as_str().unwrap_or_default().to_string();
        let selected = model::CI_RUN["specimen"]["selected"].as_str().unwrap_or_default().to_string();
        this.logs.insert(selected, LogState { next_offset: log.len() as u64, text: log, complete: true });
        this
    }

    fn build(source: Source, requested: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let weak = cx.entity().downgrade();
        let on_action: Rc<dyn Fn(ActionEvent, &mut App)> = Rc::new(move |event, cx| {
            let _ = weak.update(cx, |this, cx| this.on_action(event, cx));
        });
        let view = mount(&format!("ci-run-{}", cx.entity_id()), on_action, window, cx);
        let mut subscriptions = Vec::new();
        if source != Source::Specimen {
            if let Some(store) = Store::try_global(cx) {
                let collections = store.collections().clone();
                subscriptions.push(cx.observe(&collections.ci_runs, |_, _, cx| cx.notify()));
                subscriptions.push(cx.observe(&collections.ci_jobs, |_, _, cx| cx.notify()));
                subscriptions.push(cx.observe(&collections.devices, |_, _, cx| cx.notify()));
            }
        }
        Self { source, requested, logs: Default::default(), tail: None, view, last: Value::Null, _subscriptions: subscriptions }
    }

    /// Whether this view shows `run_id` with `job` asked for.
    pub(crate) fn shows(&self, run_id: &str, job: Option<&str>) -> bool {
        self.source == Source::Live(run_id.to_string()) && self.requested.as_deref() == job
    }

    fn on_action(&mut self, event: ActionEvent, cx: &mut Context<Self>) {
        match event.name.as_str() {
            "select" => {
                self.requested = event.context["job"].as_str().map(str::to_string);
                cx.notify();
            }
            "rerun" => self.rerun(cx),
            "cancel" => self.cancel(cx),
            _ => {}
        }
    }

    fn run_id(&self) -> Option<String> {
        match &self.source {
            Source::Live(run_id) => Some(run_id.clone()),
            Source::Specimen => None,
        }
    }

    fn rerun(&mut self, cx: &mut Context<Self>) {
        let (Some(run_id), Some(trpc)) = (self.run_id(), queries::trpc_client(cx)) else { return };
        cx.spawn(async move |_, cx| {
            let result = cx.background_executor().spawn(async move { api::ci::rerun(&trpc, &run_id, true) }).await;
            let _ = cx.update(|cx| match result {
                Ok(run_id) => crate::navigation::on_active_window(cx, move |window, cx| {
                    crate::navigation::navigate(window, cx, crate::navigation::Screen::CiRun { run_id, job: None });
                }),
                Err(err) => crate::action_error::failed_action_err("re-run", "the run", &err, cx),
            });
        })
        .detach();
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        let (Some(run_id), Some(trpc)) = (self.run_id(), queries::trpc_client(cx)) else { return };
        cx.spawn(async move |_, cx| {
            let result = cx.background_executor().spawn(async move { api::ci::cancel(&trpc, &run_id) }).await;
            if let Err(err) = result {
                let _ = cx.update(|cx| crate::action_error::failed_action_err("cancel", "the run", &err, cx));
            }
        })
        .detach();
    }

    /// Reads `job_id`'s log from where it stopped and keeps reading every
    /// `liveTailMs` while it is not complete; one tail at a time (selecting
    /// another job drops this one).
    fn tail_log(&mut self, job_id: &str, cx: &mut Context<Self>) {
        if self.tail.as_ref().is_some_and(|(id, _)| id == job_id) || self.logs.get(job_id).is_some_and(|log| log.complete) {
            return;
        }
        if queries::trpc_client(cx).is_none() {
            return;
        }
        let job_id = job_id.to_string();
        let id = job_id.clone();
        let task = cx.spawn(async move |this: WeakEntity<Self>, cx| loop {
            let Ok((offset, Some(trpc))) =
                this.update(cx, |this, cx| (this.logs.get(&job_id).map(|log| log.next_offset).unwrap_or(0), queries::trpc_client(cx)))
            else {
                return;
            };
            let read_id = job_id.clone();
            let read = cx.background_executor().spawn(async move { api::ci::log(&trpc, &read_id, offset) }).await;
            let done = this.update(cx, |this, cx| {
                let done = match read {
                    Ok(read) => {
                        let log = this.logs.entry(job_id.clone()).or_default();
                        if read.next_offset >= log.next_offset {
                            log.text.push_str(&read.text);
                            log.next_offset = read.next_offset;
                        }
                        log.complete = read.complete;
                        read.complete
                    }
                    Err(err) => {
                        log::warn!("[ci] ci.log {job_id}: {err}");
                        true
                    }
                };
                if done {
                    this.tail = None;
                }
                cx.notify();
                done
            });
            if !matches!(done, Ok(false)) {
                return;
            }
            cx.background_executor().timer(Duration::from_millis(model::live_tail_ms())).await;
        });
        self.tail = Some((id, task));
    }

    fn page(&mut self, cx: &mut Context<Self>) -> Option<PageModel> {
        match self.source.clone() {
            Source::Specimen => {
                let specimen = &model::CI_RUN["specimen"];
                let now = specimen["now"].as_str().and_then(domain::instant::parse_instant).unwrap_or_default();
                let run = model::fixture_run(&specimen["run"]);
                let jobs = model::fixture_jobs(&specimen["jobs"]);
                let selected = model::ci_default_job(&jobs, self.requested.as_deref());
                let log = selected.and_then(|id| self.logs.get(&id)).map(|log| log.text.clone());
                Some(PageModel::new(&run, &jobs, self.requested.as_deref(), log.as_deref(), &|job| job.runner_device_id.clone(), now))
            }
            Source::Live(run_id) => {
                let store = Store::try_global(cx)?;
                let collections = store.collections().clone();
                let run = collections.ci_runs.read(cx).get(&run_id).cloned()?;
                let jobs: Vec<CiJobRow> = collections.ci_jobs.read(cx).iter().filter(|job| job.run_id == run_id).cloned().collect();
                let devices = collections.devices.read(cx);
                let label = |job: &CiJobRow| {
                    let device = devices.get(job.runner_device_id.as_deref()?)?;
                    device.label.clone()
                };
                let selected = model::ci_default_job(&jobs, self.requested.as_deref());
                let log = selected.as_ref().and_then(|id| self.logs.get(id)).map(|log| log.text.clone());
                let page = PageModel::new(&run, &jobs, self.requested.as_deref(), log.as_deref(), &label, crate::session_data::now_ms());
                if let Some(job) = selected.as_ref().and_then(|id| jobs.iter().find(|job| &job.id == id)) {
                    if job.status != "queued" || job.log_key.is_some() {
                        let id = job.id.clone();
                        self.tail_log(&id, cx);
                    }
                }
                Some(page)
            }
        }
    }
}

impl Render for CiRunView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "CiRunView").entered();
        let tree = match self.page(cx) {
            Some(page) => page_tree(&page),
            // Not synced (yet): skeleton rows (feedback-states.json).
            None => json!({
                "id": model::test_id("page"),
                "component": "Skeleton",
                "props": {"rows": 6},
            }),
        };
        show(&self.view, &mut self.last, tree, cx);
        div()
            .id("ci-run-page")
            .size_full()
            .overflow_y_scroll()
            .p_6()
            .child(div().w_full().max_w(gpui::px(1100.)).child(self.view.clone()))
    }
}

// ---------------------------------------------------------------------------
// The runner section
// ---------------------------------------------------------------------------

/// What the runner section shows (pure; [`runner_tree`] draws it).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RunnerModel {
    pub notice: Option<RunnerNotice>,
    pub settings: RunnerSettings,
    pub os_label: Option<&'static str>,
    pub recent: Vec<CiJobListRow>,
    /// Not the owner: every control disabled.
    pub disabled: bool,
    /// The last label typed was refused (copy.labels.invalid).
    pub invalid: bool,
}

/// The section's bound values (`/runner/...`).
fn runner_data(settings: &RunnerSettings) -> Value {
    json!({
        "enabled": settings.enabled,
        "labels": settings.labels,
        "slots": settings.slots,
        "timeout": settings.timeout_min.to_string(),
    })
}

fn bound(path: &str) -> Value {
    json!({"@path": path})
}

fn change(name: &str) -> Value {
    json!({"change": {"event": {"name": name}}})
}

/// The section as ONE document (runner-settings.json LAYOUT).
pub(crate) fn runner_tree(runner: &RunnerModel) -> Value {
    let id = model::runner_test_id;
    let copy = model::runner_copy;
    let mut children = Vec::new();
    if let Some(notice) = runner.notice {
        let key = format!("notice.{}", notice.id());
        children.push(json!({
            "id": "runner-notice",
            "component": "Alert",
            "props": {"type": copy(&format!("{key}.type")), "title": copy(&format!("{key}.title")), "message": copy(&format!("{key}.message"))},
        }));
    }
    children.push(json!({
        "id": "runner-enabled-row",
        "component": "SettingsRow",
        "props": {"label": copy("enabled.label"), "description": copy("enabled.description"), "stackBelow": "never"},
        "children": [{
            "id": id("switch"),
            "component": "Switch",
            "props": {"label": "", "name": "enabled", "checked": bound("/runner/enabled"), "disabled": runner.disabled},
            "on": change("enabled"),
        }],
    }));
    if runner.settings.enabled {
        let (min, max) = model::runner_slot_limits();
        let suggestions: Vec<Value> = runner
            .os_label
            .filter(|label| !runner.settings.labels.iter().any(|have| have == label))
            .map(|label| json!({"label": label, "value": label}))
            .into_iter()
            .collect();
        let options: Vec<Value> =
            model::runner_timeout_options().into_iter().map(|(value, label)| json!({"value": value, "label": label})).collect();
        let mut fields = vec![json!({
            "id": id("labels"),
            "component": "ChipInput",
            "props": {
                "label": copy("labels.label"),
                "name": "labels",
                "placeholder": copy("labels.placeholder"),
                "values": bound("/runner/labels"),
                "suggestions": suggestions,
                "max": model::runner_max_labels(),
                "disabled": runner.disabled,
            },
            "on": change("labels"),
        })];
        if runner.invalid {
            fields.push(json!({
                "id": "runner-labels-invalid",
                "component": "Text",
                "props": {"text": copy("labels.invalid"), "variant": "caption", "tone": "danger"},
            }));
        }
        fields.push(json!({
            "id": id("slots"),
            "component": "NumberField",
            "props": {"label": copy("slots"), "name": "slots", "value": bound("/runner/slots"), "min": min, "max": max, "step": 1, "disabled": runner.disabled},
            "on": change("slots"),
        }));
        fields.push(json!({
            "id": id("timeout"),
            "component": "Select",
            "props": {"label": copy("timeout"), "name": "timeout", "options": options, "value": bound("/runner/timeout"), "disabled": runner.disabled},
            "on": change("timeout"),
        }));
        children.push(json!({
            "id": "runner-fields",
            "component": "Stack",
            "props": {"direction": "vertical", "gap": "md"},
            "style": {"paddingTop": 8, "paddingBottom": 8},
            "children": fields,
        }));
    }
    children.push(json!({"id": "runner-recent-heading", "component": "Heading", "props": {"text": copy("recent"), "level": "h4"}}));
    if runner.recent.is_empty() {
        children.push(json!({"id": id("recent"), "component": "Text", "props": {"text": copy("noJobs"), "variant": "muted"}}));
    } else {
        let rows: Vec<Value> = runner
            .recent
            .iter()
            .enumerate()
            .map(|(index, row)| job_row(&format!("{}-{index}", id("recent")), row, false, None))
            .collect();
        children.push(json!({"id": id("recent"), "component": "Stack", "props": {"direction": "vertical", "gap": "none"}, "children": rows}));
    }
    json!({
        "id": id("section"),
        "component": "Section",
        "props": {"title": copy("section")},
        "children": children,
    })
}

/// The section's input: the device and who looks.
#[derive(Clone, PartialEq)]
pub(crate) struct RunnerInput {
    pub device: DeviceRow,
    pub owner: bool,
    /// The recent jobs' rows and their clock.
    pub jobs: Vec<RunnerJob>,
    pub now_ms: i64,
    /// ci-run.json-free specimen: writes stay local.
    pub specimen: bool,
}

/// The CI runner section of a device's settings.
pub(crate) struct RunnerSection {
    input: Option<RunnerInput>,
    /// The settings as last written here, until the row echoes them.
    pending: Option<RunnerSettings>,
    invalid: bool,
    view: Entity<SurfaceView>,
    last: Value,
    last_data: Value,
}

impl RunnerSection {
    pub(crate) fn new(surface_id: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let weak = cx.entity().downgrade();
        let on_action: Rc<dyn Fn(ActionEvent, &mut App)> = Rc::new(move |event, cx| {
            let _ = weak.update(cx, |this, cx| this.on_action(event, cx));
        });
        let view = mount(surface_id, on_action, window, cx);
        Self { input: None, pending: None, invalid: false, view, last: Value::Null, last_data: Value::Null }
    }

    /// The device to show (every render of the host).
    pub(crate) fn set_input(&mut self, input: RunnerInput, cx: &mut Context<Self>) {
        if self.input.as_ref() == Some(&input) {
            return;
        }
        let synced = model::runner_settings(input.device.runner.as_ref());
        if self.pending.as_ref() == Some(&synced) {
            self.pending = None;
        }
        self.input = Some(input);
        cx.notify();
    }

    fn settings(&self) -> RunnerSettings {
        self.pending
            .clone()
            .or_else(|| self.input.as_ref().map(|input| model::runner_settings(input.device.runner.as_ref())))
            .unwrap_or_else(|| model::runner_settings(None))
    }

    fn on_action(&mut self, event: ActionEvent, cx: &mut Context<Self>) {
        let Some(input) = self.input.clone() else { return };
        if !input.owner {
            return;
        }
        let mut next = self.settings();
        let context = &event.context;
        match event.name.as_str() {
            "enabled" => next.enabled = context["checked"].as_bool().unwrap_or(!next.enabled),
            "labels" => {
                let typed: Vec<String> = context["values"]
                    .as_array()
                    .map(|values| values.iter().filter_map(|value| value.as_str().map(str::to_string)).collect())
                    .unwrap_or_default();
                let mut labels: Vec<String> = Vec::new();
                self.invalid = false;
                for label in typed {
                    match model::normalize_runner_label(&label) {
                        Some(label) if !labels.contains(&label) => labels.push(label),
                        Some(_) => {}
                        None => self.invalid = true,
                    }
                }
                labels.truncate(model::runner_max_labels());
                next.labels = labels;
                // The refused entry leaves the field.
                set_data(&self.view, "/runner/labels", json!(next.labels), cx);
            }
            "slots" => {
                let Some(slots) = context["value"].as_f64() else { return };
                let (min, max) = model::runner_slot_limits();
                next.slots = (slots.round() as i64).clamp(min as i64, max as i64) as u32;
            }
            "timeout" => {
                let Some(timeout) = context["value"].as_str().and_then(|value| value.parse().ok()) else { return };
                next.timeout_min = timeout;
            }
            _ => return,
        }
        if next == self.settings() {
            cx.notify();
            return;
        }
        self.pending = Some(next.clone());
        cx.notify();
        if input.specimen {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else { return };
        let write = api::ci::RunnerSettingsInput {
            device_id: input.device.device_id.clone().unwrap_or_default(),
            enabled: next.enabled,
            labels: next.labels.clone(),
            slots: next.slots,
            timeout_min: next.timeout_min,
        };
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let result = cx.background_executor().spawn(async move { api::ci::set_runner(&trpc, &write) }).await;
            if let Err(err) = result {
                let _ = this.update(cx, |this, cx| {
                    this.pending = None;
                    crate::action_error::failed_action_err("save", "the CI runner", &err, cx);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn model(&self) -> Option<RunnerModel> {
        let input = self.input.as_ref()?;
        let caps = input.device.cap_ids();
        Some(RunnerModel {
            notice: model::runner_notice("desktop", &caps),
            settings: self.settings(),
            os_label: model::runner_os_label(input.device.platform.as_deref()),
            recent: model::runner_recent_jobs(&input.device.id, &input.jobs, input.now_ms),
            disabled: !input.owner,
            invalid: self.invalid,
        })
    }
}

impl Render for RunnerSection {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "RunnerSection").entered();
        if let Some(runner) = self.model() {
            let data = runner_data(&runner.settings);
            show(&self.view, &mut self.last, runner_tree(&runner), cx);
            if data != self.last_data {
                self.last_data = data.clone();
                set_data(&self.view, "/runner", data, cx);
            }
        }
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// The recent-jobs input of `device_row_id` off the synced CI rows (run
/// numbers off `ci_runs`, repository names off the window's resolver).
pub(crate) fn runner_jobs(device_row_id: &str, window: &mut Window, cx: &mut App) -> Vec<RunnerJob> {
    let Some(store) = Store::try_global(cx) else { return Vec::new() };
    let collections = store.collections().clone();
    let jobs: Vec<CiJobRow> = collections
        .ci_jobs
        .read(cx)
        .iter()
        .filter(|job| job.runner_device_id.as_deref() == Some(device_row_id))
        .cloned()
        .collect();
    if jobs.is_empty() {
        return Vec::new();
    }
    let resolver = crate::repo_resolver::repo_resolver_for_window(window, cx);
    resolver.update(cx, |resolver, cx| resolver.ensure_loaded(cx));
    let repos: std::collections::HashMap<String, String> = resolver
        .read(cx)
        .all_repos()
        .map(|repos| repos.iter().map(|repo| (repo.repository_id.clone(), repo.full_name.clone())).collect())
        .unwrap_or_default();
    let runs = collections.ci_runs.read(cx);
    jobs.into_iter()
        .map(|job| {
            let run = runs.get(&job.run_id);
            RunnerJob {
                id: job.id,
                runner_device_id: job.runner_device_id,
                check: job.check,
                run_number: run.and_then(|run| run.number).unwrap_or_default(),
                repo: run.and_then(|run| run.repository_id.as_ref()).and_then(|id| repos.get(id).cloned()).unwrap_or_default(),
                status: job.status,
                conclusion: job.conclusion,
                started_at: job.started_at,
                completed_at: job.completed_at,
            }
        })
        .collect()
}

/// runner-settings.json `specimen` (+ `recentJobs[0]`'s jobs at its `now`).
pub(crate) fn runner_specimen_input() -> RunnerInput {
    let fixture = &*model::RUNNER_SETTINGS;
    let specimen = &fixture["specimen"];
    let device = &specimen["device"];
    let case = &fixture["recentJobs"][0];
    let row: DeviceRow = serde_json::from_value(json!({
        "id": device["id"],
        "device_id": device["id"],
        "label": device["label"],
        "platform": device["platform"],
        "caps": device["caps"],
        "runner": device["runner"],
    }))
    .expect("the specimen device hydrates");
    RunnerInput {
        device: row,
        owner: specimen["owner"].as_bool().unwrap_or(true),
        jobs: model::fixture_runner_jobs(&case["jobs"]),
        now_ms: case["now"].as_str().and_then(domain::instant::parse_instant).unwrap_or_default(),
        specimen: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specimen_page() -> PageModel {
        let specimen = &model::CI_RUN["specimen"];
        let now = specimen["now"].as_str().and_then(domain::instant::parse_instant).unwrap();
        PageModel::new(
            &model::fixture_run(&specimen["run"]),
            &model::fixture_jobs(&specimen["jobs"]),
            None,
            specimen["log"].as_str(),
            &|job| job.runner_device_id.clone(),
            now,
        )
    }

    fn parses(tree: &Value) {
        let nested: eui_core::NestedNode = serde_json::from_value(tree.clone()).expect("a nested node");
        let mut surface = eui_core::surface::Surface::new("ci", Default::default());
        let outcome = surface.set_nested(nested);
        let errors: Vec<_> = outcome.issues.iter().filter(|issue| format!("{issue:?}").contains("nknown")).collect();
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn the_specimen_page_selects_the_failing_job() {
        let page = specimen_page();
        assert_eq!(page.selected.as_deref(), Some("j2"));
        assert_eq!(page.tiles[0].value, "Failed");
        assert_eq!(page.tiles[2].value, "mint");
        assert!(page.rerun_failed && !page.cancel);
        assert!(page.failing && !page.follow);
        assert_eq!(page.steps.len(), 4);
        assert_eq!(page.log.as_ref().unwrap().highlight.len(), 1);
        let tree = page_tree(&page);
        assert_eq!(tree["id"], "ci-run");
        parses(&tree);
    }

    #[test]
    fn a_pipeline_error_replaces_the_jobs() {
        let mut page = specimen_page();
        page.error = Some("/jobs/Bad: invalid_name".into());
        let tree = page_tree(&page).to_string();
        assert!(tree.contains("The pipeline file did not plan"));
        assert!(!tree.contains("ci-run-jobs"));
    }

    #[test]
    fn the_runner_specimen_draws_its_section() {
        let input = runner_specimen_input();
        let runner = RunnerModel {
            notice: model::runner_notice("desktop", &input.device.cap_ids()),
            settings: model::runner_settings(input.device.runner.as_ref()),
            os_label: model::runner_os_label(input.device.platform.as_deref()),
            recent: model::runner_recent_jobs(&input.device.id, &input.jobs, input.now_ms),
            disabled: !input.owner,
            invalid: false,
        };
        assert_eq!(runner.notice, None);
        assert_eq!(runner.recent.len(), 5);
        assert_eq!(runner.os_label, Some("linux"));
        let tree = runner_tree(&runner);
        assert_eq!(tree["id"], "runner-settings");
        parses(&tree);
        let off = runner_tree(&RunnerModel { settings: model::runner_settings(None), ..runner });
        assert!(!off.to_string().contains("runner-labels"));
    }
}
