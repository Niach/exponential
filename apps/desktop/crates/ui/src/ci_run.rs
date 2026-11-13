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

use domain::ci_run::{self as model, CiJobListRow, RunnerJob, RunnerSettings};
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

/// ci-run-view.json `ICON_TONE`: a check tone as the Icon's tone.
fn icon_tone(tone: CheckTone) -> &'static str {
    match tone {
        CheckTone::Muted => "neutral",
        CheckTone::Green => "success",
        CheckTone::Red => "danger",
    }
}

/// A job row's leading glyph, tinted by its tone.
fn glyph(id: &str, row: &CiJobListRow) -> Value {
    json!({"id": id, "component": "Icon", "props": {"name": row.glyph.concept(), "tone": icon_tone(row.tone), "size": "sm"}})
}

/// The `props` of a job's Row: title, then subtitle and meta when present.
fn row_props(row: &CiJobListRow) -> serde_json::Map<String, Value> {
    let mut props = serde_json::Map::new();
    props.insert("title".into(), json!(row.title));
    if let Some(subtitle) = row.subtitle.as_ref().filter(|s| !s.is_empty()) {
        props.insert("subtitle".into(), json!(subtitle));
    }
    if let Some(duration) = row.duration.as_ref().filter(|d| !d.is_empty()) {
        props.insert("meta".into(), json!(duration));
    }
    props
}

// ---------------------------------------------------------------------------
// The run page
// ---------------------------------------------------------------------------

/// The page's events (web `CI_RUN_EVENTS`).
pub(crate) const EVENT_SELECT: &str = "ci-select-job";
pub(crate) const EVENT_RERUN: &str = "ci-rerun";
pub(crate) const EVENT_CANCEL: &str = "ci-cancel";

/// The run page's input (ci-run-view.json `ciRun`).
pub(crate) struct PageInput<'a> {
    pub run: &'a CiRunRow,
    pub jobs: &'a [CiJobRow],
    /// The SELECTED job (`defaultJob` already applied).
    pub selected: Option<&'a str>,
    /// The selected job's log as read so far (`None` = not read).
    pub log: Option<&'a str>,
    pub now_ms: i64,
    pub phone: bool,
    /// The button in flight: `rerun` | `cancel`.
    pub busy: Option<&'a str>,
    pub runner_label: &'a dyn Fn(&CiJobRow) -> Option<String>,
}

/// One job's detail: its steps as a Trace, then its log.
fn job_detail(input: &PageInput, job: &CiJobRow, log: Option<&str>) -> Value {
    let id = model::test_id;
    let steps = model::ci_steps(&model::ci_steps_of(job.steps.as_ref()), input.now_ms);
    let view = model::ci_log_view(log.unwrap_or_default());
    let mut children = Vec::new();
    if !steps.is_empty() {
        let items: Vec<Value> = steps
            .iter()
            .enumerate()
            .map(|(index, step)| {
                let mut props = json!({"title": step.title, "status": step.status.id()});
                if let Some(description) = step.description.as_ref().filter(|d| !d.is_empty()) {
                    props["description"] = json!(description);
                }
                json!({"id": format!("{}.{}.{index}", id("steps"), job.id), "component": "TraceItem", "props": props})
            })
            .collect();
        children.push(json!({"id": format!("{}.{}", id("steps"), job.id), "component": "Trace", "children": items}));
    }
    let log_id = format!("{}.{}", id("log"), job.id);
    if view.code.is_empty() && job.status != "in_progress" {
        children.push(json!({"id": log_id, "component": "Text", "props": {"text": model::copy("emptyLog"), "variant": "muted"}}));
    } else {
        let max_lines = model::CI_RUN["logMaxLines"][if input.phone { "phone" } else { "wide" }].clone();
        let failing = domain::pr_checks::is_failing(&job.status, job.conclusion.as_deref());
        children.push(json!({
            "id": log_id,
            "component": "CodeBlock",
            "props": {
                "code": view.code,
                "title": model::copy("log"),
                "language": "ansi",
                "lineNumbers": true,
                "wrap": false,
                "copyable": true,
                "maxLines": max_lines,
                "variant": if failing { "destructive" } else { "default" },
                "highlight": view.highlight,
                "follow": job.status != "completed",
            },
        }));
    }
    json!({
        "id": format!("ci-run-detail.{}", job.id),
        "component": "Stack",
        "props": {"direction": "vertical", "gap": "md", "align": "stretch"},
        "children": children,
    })
}

/// The run page as ONE document (ci-run.json LAYOUT; node for node web's
/// `ciRunTree`, ci-run-view.json).
pub(crate) fn page_tree(input: &PageInput) -> Value {
    let id = model::test_id;
    let run = input.run;
    let tiles = model::ci_run_tiles(run, input.jobs, input.runner_label, input.now_ms);
    let actions = model::ci_run_actions(&run.state);
    let tiles: Vec<Value> = tiles
        .iter()
        .enumerate()
        .map(|(index, tile)| {
            json!({
                "id": format!("{}.{index}", id("tiles")),
                "component": "StatTile",
                "props": {"label": tile.label, "value": tile.value, "tone": tile.tone.id()},
            })
        })
        .collect();
    let mut children = vec![
        json!({
            "id": "ci-run-title",
            "component": "Stack",
            "props": {"direction": "vertical", "gap": "xs", "align": "stretch"},
            "children": [
                {"id": "ci-run-heading", "component": "Heading", "props": {"text": model::run_title(run.number.unwrap_or_default()), "level": "h2"}},
                {"id": "ci-run-name", "component": "Text", "props": {"text": run.name, "variant": "muted"}},
            ],
        }),
        json!({"id": id("tiles"), "component": "Grid", "props": {"columns": if input.phone { 2 } else { 4 }, "gap": "md"}, "children": tiles}),
    ];
    if actions.rerun_failed || actions.cancel {
        let mut buttons = Vec::new();
        let busy = input.busy.is_some();
        if actions.rerun_failed {
            buttons.push(json!({
                "id": id("rerun"),
                "component": "Button",
                "props": {"label": model::copy("rerunFailed"), "loading": input.busy == Some("rerun"), "disabled": busy},
                "on": {"press": {"event": {"name": EVENT_RERUN}}},
            }));
        }
        if actions.cancel {
            // ci-run.json: destructive-outline; the catalog's `variant` has
            // `destructive` as its one danger form.
            buttons.push(json!({
                "id": id("cancel"),
                "component": "Button",
                "props": {"label": model::copy("cancel"), "variant": "destructive", "loading": input.busy == Some("cancel"), "disabled": busy},
                "on": {"press": {"event": {"name": EVENT_CANCEL}}},
            }));
        }
        children.push(json!({
            "id": "ci-run-actions",
            "component": "Stack",
            "props": {"direction": "horizontal", "gap": "sm", "align": "center"},
            "children": buttons,
        }));
    }
    if let Some(error) = run.error.as_ref().filter(|error| !error.is_empty()) {
        children.push(json!({
            "id": "ci-run-error",
            "component": "Alert",
            "props": {"type": "error", "title": model::copy("pipelineError"), "message": error},
        }));
    } else if !input.jobs.is_empty() {
        let rows = model::ci_job_rows(input.jobs, input.now_ms);
        let job_of = |row_id: &str| input.jobs.iter().find(|job| job.id == row_id);
        let select = |job: &str| json!({"event": {"name": EVENT_SELECT, "context": {"job": job}}});
        if input.phone {
            let folds: Vec<Value> = rows
                .iter()
                .filter_map(|row| {
                    let job = job_of(&row.id)?;
                    let selected = input.selected == Some(row.id.as_str());
                    Some(json!({
                        "id": format!("{}.{}", id("jobRow"), row.id),
                        "component": "Collapsible",
                        "props": {"title": row.title, "open": selected},
                        "on": {"change": select(&row.id)},
                        "children": [job_detail(input, job, if selected { input.log } else { None })],
                    }))
                })
                .collect();
            children.push(json!({
                "id": "ci-run-jobs",
                "component": "Stack",
                "props": {"direction": "vertical", "gap": "sm", "align": "stretch"},
                "children": folds,
            }));
        } else {
            let job_rows: Vec<Value> = rows
                .iter()
                .map(|row| {
                    let row_id = format!("{}.{}", id("jobRow"), row.id);
                    let mut props = row_props(row);
                    props.insert("selected".into(), json!(input.selected == Some(row.id.as_str())));
                    props.insert("pressable".into(), json!(true));
                    json!({
                        "id": row_id,
                        "component": "Row",
                        "props": props,
                        "slots": {"leading": glyph(&format!("{row_id}.glyph"), row)},
                        "on": {"press": select(&row.id)},
                    })
                })
                .collect();
            let mut grid = vec![json!({
                "id": "ci-run-jobs-list",
                "component": "Box",
                "style": {"flexDirection": "column"},
                "children": [{
                    "id": "ci-run-jobs-section",
                    "component": "Section",
                    "props": {"title": model::copy("jobs")},
                    "children": job_rows,
                }],
            })];
            if let Some(job) = input.selected.and_then(job_of) {
                grid.push(json!({
                    "id": "ci-run-detail",
                    "component": "Box",
                    "style": {"minWidth": 0, "flexDirection": "column"},
                    "children": [job_detail(input, job, input.log)],
                }));
            }
            children.push(json!({
                "id": "ci-run-jobs",
                "component": "Box",
                "style": {"display": "grid", "gridTemplateColumns": "300px minmax(0, 1fr)", "gap": "$spacing.lg", "alignItems": "start"},
                "children": grid,
            }));
        }
    }
    json!({
        "id": id("page"),
        "component": "Stack",
        "props": {"direction": "vertical", "gap": "lg", "align": "stretch"},
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
    /// The button in flight (`rerun` | `cancel`).
    busy: Option<&'static str>,
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
        Self { source, requested, logs: Default::default(), tail: None, busy: None, view, last: Value::Null, _subscriptions: subscriptions }
    }

    /// Whether this view shows `run_id` with `job` asked for.
    pub(crate) fn shows(&self, run_id: &str, job: Option<&str>) -> bool {
        self.source == Source::Live(run_id.to_string()) && self.requested.as_deref() == job
    }

    fn on_action(&mut self, event: ActionEvent, cx: &mut Context<Self>) {
        match event.name.as_str() {
            EVENT_SELECT => {
                // A phone fold closing selects nothing new.
                if event.context["open"].as_bool() != Some(false) {
                    self.requested = event.context["job"].as_str().map(str::to_string);
                    cx.notify();
                }
            }
            EVENT_RERUN if self.busy.is_none() => self.rerun(cx),
            EVENT_CANCEL if self.busy.is_none() => self.cancel(cx),
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
        self.busy = Some("rerun");
        cx.notify();
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let result = cx.background_executor().spawn(async move { api::ci::rerun(&trpc, &run_id, true) }).await;
            let _ = this.update(cx, |this, cx| {
                this.busy = None;
                cx.notify();
            });
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
        self.busy = Some("cancel");
        cx.notify();
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let result = cx.background_executor().spawn(async move { api::ci::cancel(&trpc, &run_id) }).await;
            let _ = this.update(cx, |this, cx| {
                this.busy = None;
                cx.notify();
            });
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

    fn page(&mut self, cx: &mut Context<Self>) -> Option<Value> {
        match self.source.clone() {
            Source::Specimen => {
                let specimen = &model::CI_RUN["specimen"];
                let now = specimen["now"].as_str().and_then(domain::instant::parse_instant).unwrap_or_default();
                let run = model::fixture_run(&specimen["run"]);
                let jobs = model::fixture_jobs(&specimen["jobs"]);
                let selected = model::ci_default_job(&jobs, self.requested.as_deref());
                let log = selected.as_ref().and_then(|id| self.logs.get(id)).map(|log| log.text.clone());
                Some(page_tree(&PageInput {
                    run: &run,
                    jobs: &jobs,
                    selected: selected.as_deref(),
                    log: log.as_deref(),
                    now_ms: now,
                    phone: false,
                    busy: self.busy,
                    runner_label: &|job| job.runner_device_id.clone(),
                }))
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
                let page = page_tree(&PageInput {
                    run: &run,
                    jobs: &jobs,
                    selected: selected.as_deref(),
                    log: log.as_deref(),
                    now_ms: crate::session_data::now_ms(),
                    phone: false,
                    busy: self.busy,
                    runner_label: &label,
                });
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
            Some(tree) => tree,
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
    /// `web` | `web-mobile` | `desktop` | `ios` | `android`.
    pub client: String,
    pub caps: Vec<String>,
    pub platform: Option<String>,
    pub settings: RunnerSettings,
    pub owner: bool,
    /// The last labels entry was refused (copy.labels.invalid).
    pub refused: bool,
    pub recent: Vec<CiJobListRow>,
}

/// The section's events (web `RUNNER_EVENTS`).
pub(crate) const EVENT_ENABLED: &str = "runner-enabled";
pub(crate) const EVENT_LABELS: &str = "runner-labels";
pub(crate) const EVENT_SLOTS: &str = "runner-slots";
pub(crate) const EVENT_TIMEOUT: &str = "runner-timeout";

/// The pointer the ChipInput's `values` is bound to (the host writes the
/// normalized list back, so a refused entry leaves the field).
pub(crate) const LABELS_PATH: &str = "/labels";

fn change(name: &str) -> Value {
    json!({"change": {"event": {"name": name}}})
}

/// The section as ONE document (runner-settings.json LAYOUT; node for node
/// web's `runnerSectionTree`, ci-run-view.json).
pub(crate) fn runner_tree(runner: &RunnerModel) -> Value {
    let id = model::runner_test_id;
    let copy = model::runner_copy;
    let disabled = !runner.owner;
    let mut children = Vec::new();
    if let Some(notice) = model::runner_notice(&runner.client, &runner.caps) {
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
        "props": {"label": copy("enabled.label"), "description": copy("enabled.description")},
        "children": [{
            "id": id("switch"),
            "component": "Switch",
            "props": {"label": "", "name": id("switch"), "checked": runner.settings.enabled, "disabled": disabled},
            "on": change(EVENT_ENABLED),
        }],
    }));
    if runner.settings.enabled {
        let limits = &model::RUNNER_SETTINGS["limits"];
        let suggestions: Vec<Value> = model::runner_os_label(runner.platform.as_deref())
            .map(|os| json!({"label": os, "value": os}))
            .into_iter()
            .collect();
        children.push(json!({
            "id": id("labels"),
            "component": "ChipInput",
            "props": {
                "label": copy("labels.label"),
                "name": id("labels"),
                "values": {"@path": LABELS_PATH},
                "placeholder": copy("labels.placeholder"),
                "max": limits["labels"],
                "suggestions": suggestions,
                "disabled": disabled,
            },
            "on": change(EVENT_LABELS),
        }));
        if runner.refused {
            children.push(json!({
                "id": "runner-labels-invalid",
                "component": "Text",
                "props": {"text": copy("labels.invalid"), "variant": "caption", "tone": "danger"},
            }));
        }
        children.push(json!({
            "id": id("slots"),
            "component": "NumberField",
            "props": {
                "label": copy("slots"),
                "name": id("slots"),
                "value": runner.settings.slots,
                "min": limits["slots"]["min"],
                "max": limits["slots"]["max"],
                "step": 1,
                "disabled": disabled,
            },
            "on": change(EVENT_SLOTS),
        }));
        children.push(json!({
            "id": id("timeout"),
            "component": "Select",
            "props": {
                "label": copy("timeout"),
                "name": id("timeout"),
                "options": model::RUNNER_SETTINGS["timeoutOptions"],
                "value": runner.settings.timeout_min.to_string(),
                "disabled": disabled,
            },
            "on": change(EVENT_TIMEOUT),
        }));
    }
    children.push(json!({"id": "runner-recent-heading", "component": "Heading", "props": {"text": copy("recent"), "level": "h4"}}));
    if runner.recent.is_empty() {
        children.push(json!({"id": id("recent"), "component": "Text", "props": {"text": copy("noJobs"), "variant": "muted"}}));
    } else {
        let rows: Vec<Value> = runner
            .recent
            .iter()
            .map(|row| {
                let row_id = format!("{}.{}", id("recent"), row.id);
                json!({
                    "id": row_id,
                    "component": "Row",
                    "props": row_props(row),
                    "slots": {"leading": glyph(&format!("{row_id}.glyph"), row)},
                })
            })
            .collect();
        children.push(json!({
            "id": id("recent"),
            "component": "Stack",
            "props": {"direction": "vertical", "gap": "none", "align": "stretch"},
            "children": rows,
        }));
    }
    json!({
        "id": id("section"),
        "component": "Section",
        "props": {"title": copy("section")},
        "children": children,
    })
}

/// A ChipInput's entries as stored: each through `normalizeLabel`,
/// duplicates dropped, at most `limits.labels`; `true` = an entry was
/// refused (web `runnerLabelsFrom`).
pub(crate) fn runner_labels_from(entries: &[String]) -> (Vec<String>, bool) {
    let mut labels: Vec<String> = Vec::new();
    let mut refused = false;
    for entry in entries {
        match model::normalize_runner_label(entry) {
            None => refused = true,
            Some(label) if !labels.contains(&label) && labels.len() < model::runner_max_labels() => labels.push(label),
            Some(_) => {}
        }
    }
    (labels, refused)
}

/// The settings a control event writes (`None` = nothing to write) and
/// whether a labels entry was refused (web `runnerSettingsAfter`).
pub(crate) fn runner_settings_after(current: &RunnerSettings, event: &str, payload: &Value) -> (Option<RunnerSettings>, bool) {
    match event {
        EVENT_ENABLED => match payload["checked"].as_bool() {
            Some(checked) => ((checked != current.enabled).then(|| RunnerSettings { enabled: checked, ..current.clone() }), false),
            None => (None, false),
        },
        EVENT_LABELS => match payload["values"].as_array() {
            Some(values) => {
                let entries: Vec<String> =
                    values.iter().map(|value| value.as_str().map(str::to_string).unwrap_or_else(|| value.to_string())).collect();
                let (labels, refused) = runner_labels_from(&entries);
                ((labels != current.labels).then(|| RunnerSettings { labels, ..current.clone() }), refused)
            }
            None => (None, false),
        },
        EVENT_SLOTS => match payload["value"].as_f64().filter(|value| value.is_finite()) {
            Some(value) => {
                let (min, max) = model::runner_slot_limits();
                let slots = (value.round() as i64).clamp(min as i64, max as i64) as u32;
                ((slots != current.slots).then(|| RunnerSettings { slots, ..current.clone() }), false)
            }
            None => (None, false),
        },
        EVENT_TIMEOUT => match payload["value"].as_str() {
            Some(value) => {
                let known = model::runner_timeout_options().iter().any(|(option, _)| option == value);
                let timeout = value.parse::<u32>().ok().filter(|timeout| known && *timeout != current.timeout_min);
                (timeout.map(|timeout_min| RunnerSettings { timeout_min, ..current.clone() }), false)
            }
            None => (None, false),
        },
        _ => (None, false),
    }
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
    refused: bool,
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
        Self { input: None, pending: None, refused: false, view, last: Value::Null, last_data: Value::Null }
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
        let current = self.settings();
        let (next, refused) = runner_settings_after(&current, &event.name, &event.context);
        if event.name == EVENT_LABELS {
            self.refused = refused;
            // The stored list goes back into the field: a refused entry leaves it.
            let labels = next.as_ref().map(|next| next.labels.clone()).unwrap_or(current.labels);
            set_data(&self.view, LABELS_PATH, json!(labels), cx);
        }
        let Some(next) = next else {
            cx.notify();
            return;
        };
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
        Some(RunnerModel {
            client: "desktop".into(),
            caps: input.device.cap_ids(),
            platform: input.device.platform.clone(),
            settings: self.settings(),
            owner: input.owner,
            refused: self.refused,
            recent: model::runner_recent_jobs(&input.device.id, &input.jobs, input.now_ms),
        })
    }
}

impl Render for RunnerSection {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "RunnerSection").entered();
        if let Some(runner) = self.model() {
            let data = json!(runner.settings.labels);
            show(&self.view, &mut self.last, runner_tree(&runner), cx);
            if data != self.last_data {
                self.last_data = data.clone();
                set_data(&self.view, LABELS_PATH, data, cx);
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

    const VIEW_FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/ci-run-view.json");

    fn view_fixture() -> Value {
        serde_json::from_str(VIEW_FIXTURE).expect("ci-run-view.json parses")
    }

    /// Numbers as f64 (`2` == `2.0`), so a tree compares node for node.
    fn normalized(value: &Value) -> Value {
        match value {
            Value::Number(number) => json!(number.as_f64().unwrap_or_default()),
            Value::Array(items) => Value::Array(items.iter().map(normalized).collect()),
            Value::Object(map) => Value::Object(map.iter().map(|(key, value)| (key.clone(), normalized(value))).collect()),
            other => other.clone(),
        }
    }

    /// The first path where `got` and `want` part (for a readable failure).
    fn first_drift(path: &str, got: &Value, want: &Value) -> Option<String> {
        match (got, want) {
            (Value::Object(a), Value::Object(b)) => {
                let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
                keys.sort();
                keys.dedup();
                keys.into_iter().find_map(|key| {
                    first_drift(&format!("{path}.{key}"), a.get(key).unwrap_or(&Value::Null), b.get(key).unwrap_or(&Value::Null))
                })
            }
            (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
                a.iter().zip(b).enumerate().find_map(|(index, (a, b))| first_drift(&format!("{path}[{index}]"), a, b))
            }
            (a, b) if a == b => None,
            (a, b) => Some(format!("{path}: desktop {a} ≠ web {b}")),
        }
    }

    fn assert_same(name: &str, got: &Value, want: &Value) {
        let (got, want) = (normalized(got), normalized(want));
        if let Some(drift) = first_drift("", &got, &want) {
            panic!("{name}: {drift}");
        }
    }

    fn page_of(case: &Value) -> Value {
        let model_json = &case["model"];
        let run = model::fixture_run(&model_json["run"]);
        let jobs = model::fixture_jobs(&model_json["jobs"]);
        let now = case["now"].as_str().and_then(domain::instant::parse_instant).unwrap();
        page_tree(&PageInput {
            run: &run,
            jobs: &jobs,
            selected: case["selected"].as_str(),
            log: case["log"].as_str(),
            now_ms: now,
            phone: case["phone"].as_bool() == Some(true),
            busy: case["busy"].as_str(),
            runner_label: &|job| job.runner_device_id.clone(),
        })
    }

    fn settings_of(value: &Value) -> RunnerSettings {
        model::runner_settings(Some(value))
    }

    fn settings_json(settings: &RunnerSettings) -> Value {
        json!({"enabled": settings.enabled, "labels": settings.labels, "slots": settings.slots, "timeoutMin": settings.timeout_min})
    }

    /// #EXP-1804: ci-run-view.json `ciRun` — the run page node for node as
    /// web builds it.
    #[test]
    fn the_run_page_replays_ci_run_view() {
        let fixture = view_fixture();
        let cases = fixture["ciRun"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            assert_same(case["name"].as_str().unwrap_or("ciRun"), &page_of(case), &case["tree"]);
        }
    }

    /// ci-run-view.json `runner` (`recent: true` = runner-settings.json
    /// `recentJobs[0]`'s rows).
    #[test]
    fn the_runner_section_replays_ci_run_view() {
        let fixture = view_fixture();
        let sample = &model::RUNNER_SETTINGS["recentJobs"][0];
        let sample_now = sample["now"].as_str().and_then(domain::instant::parse_instant).unwrap();
        for case in fixture["runner"].as_array().unwrap() {
            let recent = if case["recent"].as_bool() == Some(true) {
                model::runner_recent_jobs(sample["deviceId"].as_str().unwrap(), &model::fixture_runner_jobs(&sample["jobs"]), sample_now)
            } else {
                Vec::new()
            };
            let runner = RunnerModel {
                client: case["client"].as_str().unwrap().to_string(),
                caps: case["caps"].as_array().unwrap().iter().filter_map(|cap| cap.as_str().map(str::to_string)).collect(),
                platform: case["platform"].as_str().map(str::to_string),
                settings: settings_of(&case["settings"]),
                owner: case["owner"].as_bool() == Some(true),
                refused: case["refused"].as_bool() == Some(true),
                recent,
            };
            assert_same(case["name"].as_str().unwrap_or("runner"), &runner_tree(&runner), &case["tree"]);
        }
    }

    /// ci-run-view.json `after`: a control event → the settings to write.
    #[test]
    fn the_settings_change_rule_replays_ci_run_view() {
        let fixture = view_fixture();
        let cases = fixture["after"].as_array().unwrap();
        assert!(!cases.is_empty());
        for (index, case) in cases.iter().enumerate() {
            let (settings, refused) =
                runner_settings_after(&settings_of(&case["current"]), case["event"].as_str().unwrap(), &case["payload"]);
            let got = json!({"settings": settings.as_ref().map(settings_json), "refused": refused});
            let want = json!({"settings": case["settings"], "refused": case["refused"]});
            assert_same(&format!("after[{index}] {}", case["event"]), &got, &want);
        }
    }

    fn parses(tree: &Value) {
        let nested: eui_core::NestedNode = serde_json::from_value(tree.clone()).expect("a nested node");
        let mut surface = eui_core::surface::Surface::new("ci", Default::default());
        let outcome = surface.set_nested(nested);
        let errors: Vec<_> = outcome.issues.iter().filter(|issue| format!("{issue:?}").contains("nknown")).collect();
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn every_fixture_tree_mounts() {
        let fixture = view_fixture();
        for case in fixture["ciRun"].as_array().unwrap().iter().chain(fixture["runner"].as_array().unwrap()) {
            parses(&case["tree"]);
        }
    }

    #[test]
    fn the_runner_specimen_is_the_owner_on_mint() {
        let input = runner_specimen_input();
        assert!(input.owner);
        assert_eq!(model::runner_recent_jobs(&input.device.id, &input.jobs, input.now_ms).len(), 5);
        assert_eq!(model::runner_settings(input.device.runner.as_ref()).slots, 2);
    }
}
