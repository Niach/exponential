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
pub(crate) fn mount(surface_id: &str, on_action: Rc<dyn Fn(ActionEvent, &mut App)>, window: &mut Window, cx: &mut App) -> Entity<SurfaceView> {
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
pub(crate) fn show(view: &Entity<SurfaceView>, last: &mut Value, tree: Value, cx: &mut App) {
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
pub(crate) const EVENT_RERUN_ALL: &str = "ci-rerun-all";
pub(crate) const EVENT_FIX: &str = "ci-fix";
pub(crate) const EVENT_OPEN_EXTERNAL: &str = "ci-open-external";
pub(crate) const EVENT_BREADCRUMB: &str = "ci-breadcrumb";
pub(crate) const EVENT_MENU: &str = "ci-run-menu";

/// The run page's input (ci-run-view.json `ciRun`).
pub(crate) struct PageInput<'a> {
    pub run: &'a CiRunRow,
    pub jobs: &'a [CiJobRow],
    /// The SELECTED job (`None` = `defaultJob`).
    pub selected: Option<&'a str>,
    /// The selected job's log as read so far (`None` = not read).
    pub log: Option<&'a str>,
    pub now_ms: i64,
    pub phone: bool,
    /// The button in flight: `rerun` | `rerunAll` | `cancel`.
    pub busy: Option<&'a str>,
    /// #EXP-2172: the repository's full name (`None` while not synced).
    pub repository: Option<&'a str>,
    /// The run's pull request (number, base), when known.
    pub pull_request: Option<(i64, &'a str)>,
    /// The author's name, when known.
    pub author: Option<&'a str>,
    /// Phones: drilled into the selected job (the tree is its Card alone).
    pub job_open: bool,
    pub runner_label: &'a dyn Fn(&CiJobRow) -> Option<String>,
}

/// `ciRunTitle`: the run's title, else the pipeline's name.
pub(crate) fn run_title_of(run: &CiRunRow) -> String {
    run.title.clone().filter(|title| !title.is_empty()).unwrap_or_else(|| run.name.clone())
}

/// One job's Card (web `jobCard`): its summary, the steps with the open
/// step holding the log (or the log alone without steps).
fn job_card(input: &PageInput, job: &CiJobRow, log: Option<&str>) -> Value {
    let id = model::test_id;
    let steps = model::ci_steps(&model::ci_steps_of(job.steps.as_ref()), input.now_ms);
    let view = model::ci_log_view(log.unwrap_or_default());
    let log_id = format!("{}.{}", id("log"), job.id);
    let log_node = if view.code.is_empty() && job.status != "in_progress" {
        json!({"id": log_id, "component": "Text", "props": {"text": model::copy("emptyLog"), "variant": "muted"}})
    } else {
        let max_lines = model::CI_RUN["logMaxLines"][if input.phone { "phone" } else { "wide" }].clone();
        json!({
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
                "highlight": view.highlight,
                "follow": job.status != "completed",
            },
        })
    };
    let children = if steps.is_empty() {
        vec![log_node]
    } else {
        let open_at = steps
            .iter()
            .position(|step| step.status.id() == "error")
            .or_else(|| steps.iter().position(|step| step.status.id() == "running"))
            .unwrap_or(steps.len() - 1);
        let items: Vec<Value> = steps
            .iter()
            .enumerate()
            .map(|(index, step)| {
                let mut props = json!({"title": step.title, "status": step.status.id(), "collapsible": true, "open": index == open_at});
                if let Some(description) = step.description.as_ref().filter(|d| !d.is_empty()) {
                    props["description"] = json!(description);
                }
                let mut node = json!({"id": format!("{}.{}.{index}", id("steps"), job.id), "component": "TraceItem", "props": props});
                if index == open_at {
                    node["children"] = json!([log_node.clone()]);
                }
                node
            })
            .collect();
        vec![json!({"id": format!("{}.{}", id("steps"), job.id), "component": "Trace", "props": {"size": "sm"}, "children": items})]
    };
    let runner = (input.runner_label)(job);
    json!({
        "id": format!("{}.{}", id("jobCard"), job.id),
        "component": "Card",
        "props": {"title": job.check, "description": model::ci_job_summary(job, runner.as_deref(), input.now_ms)},
        "children": children,
    })
}

fn page(children: Vec<Value>) -> Value {
    json!({"id": model::test_id("page"), "component": "Stack", "props": {"direction": "vertical", "gap": "lg", "align": "stretch"}, "children": children})
}

/// The run page as ONE document (ci-run.json LAYOUT; node for node web's
/// `ciRunTree`, ci-run-view.json).
pub(crate) fn page_tree(input: &PageInput) -> Value {
    let id = model::test_id;
    let run = input.run;
    let selected: Option<String> = input.selected.map(str::to_string).or_else(|| model::ci_default_job(input.jobs, None));
    let job_of = |job_id: &str| input.jobs.iter().find(|job| job.id == job_id);
    if input.phone && input.job_open {
        if let Some(job) = selected.as_deref().and_then(job_of) {
            return page(vec![job_card(input, job, input.log)]);
        }
    }
    let title = model::run_title(run.number.unwrap_or_default());
    let number = run.number.unwrap_or_default();
    let mut head = Vec::new();
    if !input.phone {
        let mut items = vec![json!({"label": model::copy("breadcrumbRoot"), "value": "ci"})];
        if let Some(repository) = input.repository {
            items.push(json!({"label": repository, "value": "repository"}));
        }
        items.push(json!({"label": title, "value": "run"}));
        head.push(json!({"id": id("breadcrumb"), "component": "Breadcrumb", "props": {"items": items}, "on": {"select": {"event": {"name": EVENT_BREADCRUMB}}}}));
    }
    head.push(json!({"id": "ci-run-heading", "component": "Heading", "props": {"text": run_title_of(run), "level": if input.phone { "h3" } else { "h2" }}}));
    if !input.phone {
        let at = run.started_at.clone().or_else(|| run.created_at.clone()).unwrap_or_default();
        let age = json!({"@call": "formatRelativeTime", "args": {"value": at}});
        head.push(json!({
            "id": "ci-run-name",
            "component": "Text",
            "props": {
                "text": {"@call": "fill", "args": {
                    "template": "{workflow} · {repository}{age}",
                    "params": {
                        "workflow": format!("{} #{number}", run.name),
                        "repository": input.repository.unwrap_or_default(),
                        "age": {"@call": "fill", "args": {"template": " · {t}", "params": {"t": age}}},
                    },
                }},
                "variant": "muted",
            },
        }));
    }
    let chips: Vec<Value> = model::ci_run_chips(run, input.jobs, input.runner_label, input.pull_request, input.author, input.now_ms)
        .iter()
        .map(|chip| {
            let mut props = json!({"label": chip.label});
            if let Some(icon) = chip.icon {
                props["icon"] = json!(icon);
            }
            if let Some(dot) = chip.dot {
                props["dot"] = json!(dot);
            }
            json!({"id": format!("{}.{}", id("chips"), chip.id), "component": "Chip", "props": props})
        })
        .collect();
    let chips = json!({"id": id("chips"), "component": "Stack", "props": {"direction": "horizontal", "gap": "xs", "align": "center", "wrap": true}, "children": chips});
    let actions = model::ci_run_actions(run);
    let busy = input.busy.is_some();
    let button = |key: &str, label: &str, event: &str, busy_key: Option<&str>, extra: Value| -> Value {
        let mut props = json!({"label": label, "size": "sm"});
        if let Value::Object(extra) = extra {
            props.as_object_mut().unwrap().extend(extra);
        }
        if let Some(busy_key) = busy_key {
            props["loading"] = json!(input.busy == Some(busy_key));
        }
        props["disabled"] = json!(busy);
        json!({"id": id(key), "component": "Button", "props": props, "on": {"press": {"event": {"name": event}}}})
    };
    let mut buttons = Vec::new();
    match actions.primary {
        Some(model::CiPrimary::Cancel) => buttons.push(button("cancel", model::copy("cancel"), EVENT_CANCEL, Some("cancel"), json!({"icon": "circle-stop"}))),
        Some(model::CiPrimary::RerunFailed) => buttons.push(button("rerun", model::copy("rerunFailed"), EVENT_RERUN, Some("rerun"), json!({"icon": "refresh-cw"}))),
        Some(model::CiPrimary::RerunAll) => buttons.push(button("rerunAll", model::copy("rerunAll"), EVENT_RERUN_ALL, Some("rerunAll"), json!({"icon": "refresh-cw"}))),
        None => {}
    }
    if actions.fix {
        buttons.push(button("fix", model::copy("fix"), EVENT_FIX, None, json!({"variant": "outline", "icon": "bot"})));
    }
    if !actions.menu.is_empty() {
        let items: Vec<Value> = actions
            .menu
            .iter()
            .map(|item| json!({"label": if *item == "rerunAll" { model::copy("rerunAll") } else { model::copy("openExternal") }, "value": item}))
            .collect();
        buttons.push(json!({
            "id": id("more"),
            "component": "Menu",
            "props": {"label": model::copy("more"), "align": "end", "items": items},
            "on": {"select": {"event": {"name": EVENT_MENU}}},
            "children": [{"id": format!("{}.button", id("more")), "component": "Button", "props": {"icon": "ellipsis", "variant": "ghost", "size": "icon", "label": model::copy("more"), "disabled": busy}}],
        }));
    }
    let has_buttons = !buttons.is_empty();
    let actions_node = json!({"id": "ci-run-actions", "component": "Stack", "props": {"direction": "horizontal", "gap": "sm", "align": "center"}, "children": buttons});
    let bar = if input.phone {
        let mut children = vec![chips];
        if has_buttons {
            children.push(actions_node);
        }
        json!({"id": "ci-run-bar", "component": "Stack", "props": {"direction": "vertical", "gap": "sm", "align": "stretch"}, "children": children})
    } else {
        let mut node = json!({"id": "ci-run-bar", "component": "Toolbar", "children": [chips]});
        if has_buttons {
            node["slots"] = json!({"trailing": actions_node});
        }
        node
    };
    let mut children = vec![
        json!({"id": "ci-run-title", "component": "Stack", "props": {"direction": "vertical", "gap": "xs", "align": "stretch"}, "children": head}),
        bar,
    ];
    if let Some(error) = run.error.as_ref().filter(|error| !error.is_empty()) {
        children.push(json!({"id": "ci-run-error", "component": "Alert", "props": {"type": "error", "title": model::copy("pipelineError"), "message": error}}));
        return page(children);
    }
    let failed = selected
        .as_deref()
        .and_then(job_of)
        .filter(|job| model::ci_job_status(&job.status, job.conclusion.as_deref()) == model::CiRunStatus::Failed);
    if let Some((title, message)) = model::ci_failure_summary(failed.map(|job| job.check.as_str()), failed.and(input.log)) {
        children.push(json!({"id": id("failure"), "component": "Alert", "props": {"type": "error", "title": title, "message": message}}));
    }
    if input.jobs.is_empty() {
        return page(children);
    }
    let rows = model::ci_job_rows(input.jobs, input.now_ms);
    let pairs: Vec<(&str, Option<&str>)> = input.jobs.iter().map(|job| (job.status.as_str(), job.conclusion.as_deref())).collect();
    let job_rows: Vec<Value> = rows
        .iter()
        .map(|row| {
            let row_id = format!("{}.{}", id("jobRow"), row.id);
            let mut props = json!({"title": row.title, "selected": selected.as_deref() == Some(row.id.as_str()), "pressable": true});
            if let (Some(subtitle), false) = (&row.subtitle, input.phone) {
                props["subtitle"] = json!(subtitle);
            }
            if let Some(duration) = &row.duration {
                props["meta"] = json!(duration);
            } else if let (Some(subtitle), true) = (&row.subtitle, input.phone) {
                props["meta"] = json!(subtitle);
            }
            if input.phone {
                props["chevron"] = json!("right");
            }
            json!({
                "id": row_id,
                "component": "Row",
                "props": props,
                "slots": {"leading": {"id": format!("{row_id}.glyph"), "component": "Icon", "props": {"name": row.glyph.concept(), "tone": row.status_tone.id(), "size": "sm"}}},
                "on": {"press": {"event": {"name": EVENT_SELECT, "context": {"job": row.id}}}},
            })
        })
        .collect();
    let section = json!({
        "id": "ci-run-jobs-section",
        "component": "Section",
        "props": {"title": model::copy("jobs"), "caption": model::ci_jobs_caption(&pairs)},
        "children": job_rows,
    });
    if input.phone {
        children.push(section);
    } else {
        let mut grid = vec![json!({"id": "ci-run-jobs-list", "component": "Box", "style": {"flexDirection": "column"}, "children": [section]})];
        if let Some(job) = selected.as_deref().and_then(job_of) {
            grid.push(json!({"id": "ci-run-detail", "component": "Box", "style": {"minWidth": 0, "flexDirection": "column"}, "children": [job_card(input, job, input.log)]}));
        }
        children.push(json!({
            "id": "ci-run-jobs",
            "component": "Box",
            "style": {"display": "grid", "gridTemplateColumns": "300px minmax(0, 1fr)", "gap": "$spacing.lg", "alignItems": "start"},
            "children": grid,
        }));
    }
    page(children)
}

/// The specimen's data: ci-run.json `specimen`; DEV-ONLY
/// `EXP_DEV_CI_RUN_SOURCE=github` = ci-run-view.json's GitHub run (the
/// grant not accepted: Open on GitHub and Fix), for its picture.
fn specimen_model() -> &'static Value {
    static GITHUB: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| {
        let view: Value = serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/ci-run-view.json")).unwrap_or_default();
        let case = view["ciRun"]
            .as_array()
            .and_then(|cases| cases.iter().find(|case| case["model"]["run"]["provider"] == "github" && case["phone"] != true))
            .cloned()
            .unwrap_or_default();
        let mut model = case["model"].clone();
        for key in ["selected", "log", "now"] {
            model[key] = case[key].clone();
        }
        model
    });
    if std::env::var("EXP_DEV_CI_RUN_SOURCE").as_deref() == Ok("github") && GITHUB.is_object() {
        &GITHUB
    } else {
        &model::CI_RUN["specimen"]
    }
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
        let specimen = specimen_model();
        let log = specimen["log"].as_str().unwrap_or_default().to_string();
        let selected = specimen["selected"].as_str().unwrap_or_default().to_string();
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
            EVENT_RERUN if self.busy.is_none() => self.rerun(true, cx),
            EVENT_RERUN_ALL if self.busy.is_none() => self.rerun(false, cx),
            EVENT_CANCEL if self.busy.is_none() => self.cancel(cx),
            EVENT_FIX => self.fix(cx),
            EVENT_MENU => match event.payload.as_ref().and_then(|payload| payload["value"].as_str()) {
                Some("rerunAll") if self.busy.is_none() => self.rerun(false, cx),
                Some("openExternal") => {
                    if let Some(url) = self.live_run(cx).and_then(|run| run.external_url).filter(|url| !url.is_empty()) {
                        cx.open_url(&url);
                    }
                }
                _ => {}
            },
            EVENT_OPEN_EXTERNAL => {
                if let Some(url) = self.live_run(cx).and_then(|run| run.external_url).filter(|url| !url.is_empty()) {
                    cx.open_url(&url);
                }
            }
            EVENT_BREADCRUMB => {
                let value = event.payload.as_ref().and_then(|payload| payload["value"].as_str()).unwrap_or_default().to_string();
                let Some(run) = self.live_run(cx) else { return };
                match value.as_str() {
                    "ci" => crate::navigation::on_active_window(cx, |window, cx| {
                        crate::navigation::navigate(window, cx, crate::navigation::Screen::Ci);
                    }),
                    "repository" => {
                        let (Some(repository_id), team) = (run.repository_id.clone(), run.team_id.clone()) else { return };
                        crate::navigation::on_active_window(cx, move |window, cx| {
                            crate::navigation::open_settings(window, cx, team);
                            crate::sidebar::select_settings_section(window, cx, crate::settings::SettingsSection::Repository(repository_id));
                        });
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn run_id(&self) -> Option<String> {
        match &self.source {
            Source::Live(run_id) => Some(run_id.clone()),
            Source::Specimen => None,
        }
    }

    /// The synced run this page shows (`None` = the specimen, or not synced).
    fn live_run(&self, cx: &App) -> Option<CiRunRow> {
        let run_id = self.run_id()?;
        Store::try_global(cx)?.collections().ci_runs.read(cx).get(&run_id).cloned()
    }

    /// #EXP-2172 Fix with an agent: the new-chat composer on the run's team
    /// and repository, its text `ciFixPrompt` over the selected failing job
    /// and its log as read so far.
    fn fix(&mut self, cx: &mut Context<Self>) {
        let Some(run) = self.live_run(cx) else { return };
        let Some(store) = Store::try_global(cx) else { return };
        let jobs: Vec<CiJobRow> = store.collections().ci_jobs.read(cx).iter().filter(|job| job.run_id == run.id).cloned().collect();
        let selected = model::ci_default_job(&jobs, self.requested.as_deref());
        let job = selected.as_ref().and_then(|id| jobs.iter().find(|job| &job.id == id));
        let failing = job.filter(|job| domain::pr_checks::is_failing(&job.status, job.conclusion.as_deref()));
        let log = failing.and_then(|job| self.logs.get(&job.id)).map(|log| log.text.clone());
        let repository = run
            .repository_id
            .as_deref()
            .and_then(|id| crate::ci_page::repo_full_name(id, cx))
            .unwrap_or_else(|| domain::ci_page::copy("unknownRepository").to_string());
        let text = model::ci_fix_prompt(&run, &repository, failing.map(|job| job.check.as_str()), log.as_deref());
        let seed = crate::navigation::ChatSeed { team_id: run.team_id.clone(), repo_id: run.repository_id.clone(), text: Some(text), ..Default::default() };
        crate::navigation::on_active_window(cx, move |window, cx| crate::navigation::navigate_to_chat(window, cx, seed));
    }

    fn rerun(&mut self, failed_only: bool, cx: &mut Context<Self>) {
        let (Some(run_id), Some(trpc)) = (self.run_id(), queries::trpc_client(cx)) else { return };
        self.busy = Some(if failed_only { "rerun" } else { "rerunAll" });
        cx.notify();
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let result = cx.background_executor().spawn(async move { api::ci::rerun(&trpc, &run_id, failed_only) }).await;
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
                let specimen = specimen_model();
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
                    repository: specimen["repository"].as_str(),
                    pull_request: specimen["pullRequest"].as_object().and_then(|pr| Some((pr["number"].as_i64()?, pr["base"].as_str()?))),
                    author: specimen["author"].as_str(),
                    job_open: false,
                    runner_label: &|job| job.runner_device_id.clone(),
                }))
            }
            Source::Live(run_id) => {
                let store = Store::try_global(cx)?;
                let collections = store.collections().clone();
                let run = collections.ci_runs.read(cx).get(&run_id).cloned()?;
                let jobs: Vec<CiJobRow> = collections.ci_jobs.read(cx).iter().filter(|job| job.run_id == run_id).cloned().collect();
                let selected = model::ci_default_job(&jobs, self.requested.as_deref());
                let log = selected.as_ref().and_then(|id| self.logs.get(id)).map(|log| log.text.clone());
                let pull_request: Option<(i64, String)> = run
                    .pull_request_id
                    .as_deref()
                    .and_then(|id| collections.pull_requests.read(cx).get(id).and_then(|pr| Some((pr.number?, pr.base.clone().unwrap_or_default()))));
                let author: Option<String> = run
                    .created_by_user_id
                    .as_deref()
                    .and_then(|id| collections.users.read(cx).get(id).and_then(|user| user.name.clone()))
                    .filter(|name| !name.is_empty());
                let repository = run.repository_id.as_deref().and_then(|id| crate::ci_page::repo_full_name(id, cx));
                let devices = collections.devices.read(cx);
                let label = |job: &CiJobRow| {
                    let device = devices.get(job.runner_device_id.as_deref()?)?;
                    device.label.clone()
                };
                let page = page_tree(&PageInput {
                    run: &run,
                    jobs: &jobs,
                    selected: selected.as_deref(),
                    log: log.as_deref(),
                    now_ms: crate::session_data::now_ms(),
                    phone: false,
                    busy: self.busy,
                    repository: repository.as_deref(),
                    pull_request: pull_request.as_ref().map(|(number, base)| (*number, base.as_str())),
                    author: author.as_deref(),
                    job_open: false,
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            // #EXP-1860: the last row scrolls clear of the Assistant FAB.
            .child(crate::assistant_fab::page_end(window, cx))
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
            repository: model_json["repository"].as_str(),
            pull_request: model_json["pullRequest"].as_object().and_then(|pr| Some((pr["number"].as_i64()?, pr["base"].as_str()?))),
            author: model_json["author"].as_str(),
            job_open: case["jobOpen"].as_bool() == Some(true),
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
