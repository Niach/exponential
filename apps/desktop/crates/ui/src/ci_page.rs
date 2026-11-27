//! #EXP-2172 — THE CI PAGE on the desktop, genUI catalog only (SAME LOOK
//! ×4): `fixtures/ci-page.json` (what), `fixtures/ci-page-view.json` (the
//! tree, node for node web's `apps/web/src/lib/ci-page-view.ts`), view `ci`,
//! [`crate::navigation::Screen::Ci`]: one root row after Devices across every
//! member team. The rules are `domain::ci_page`'s; this file lays the page
//! out ([`page_tree`]) and maps its events.
//!
//! DATA, all synced: `ci_runs` + `ci_jobs` (ours and GitHub Actions),
//! `pull_requests`, the sessions (a root's starter), the devices (the
//! runners). The repositories' full names come from `repositories.list` per
//! member team ([`repo_names`], what Settings › Repositories reads) until the
//! repositories shape lands.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use eui_gpui::host::ActionEvent;
use eui_gpui::view::SurfaceView;
use gpui::{
    div, App, AppContext as _, Context, Entity, Global, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window,
};
use serde_json::{json, Value};
use sync::Store;

use domain::ci_page::{
    self as model, CiFilter, CiPageInput, CiPageJob, CiPagePr, CiPageRepo, CiPageRun, CiPageRunner, CiPageSession, CiRunFilter, CiSource,
    CiTab, CiTrainGroup,
};
use domain::nav_badges::CountBadgeValue;

use crate::queries;

// ---------------------------------------------------------------------------
// The tree (ci-page-view.json)
// ---------------------------------------------------------------------------

/// The page's events (web `CI_PAGE_EVENTS`).
pub(crate) const EVENT_TAB: &str = "ci-tab";
pub(crate) const EVENT_STATUS: &str = "ci-filter-status";
pub(crate) const EVENT_REPOSITORY: &str = "ci-filter-repo";
pub(crate) const EVENT_SOURCE: &str = "ci-filter-source";
pub(crate) const EVENT_OPEN_RUN: &str = "ci-open-run";
pub(crate) const EVENT_OPEN_PR: &str = "ci-open-pr";
pub(crate) const EVENT_OPEN_RUNNER: &str = "ci-open-runner";

/// The page's own state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PageState {
    pub tab: CiTab,
    pub filter: CiRunFilter,
    pub phone: bool,
}

/// ci-page-view.json `ICON_TONE`.
fn icon_tone(tone: &str) -> &'static str {
    match tone {
        "green" => "success",
        "red" => "danger",
        _ => "neutral",
    }
}

fn change(name: &str) -> Value {
    json!({"change": {"event": {"name": name}}})
}

fn vstack(id: &str, gap: &str, children: Vec<Value>) -> Value {
    json!({"id": id, "component": "Stack", "props": {"direction": "vertical", "gap": gap, "align": "stretch"}, "children": children})
}

fn train_section(group: &CiTrainGroup, id: &str) -> Value {
    let row_id = model::test_id("trainRow");
    let rows: Vec<Value> = group
        .rows
        .iter()
        .map(|row| {
            let mut props = json!({"identifier": row.identifier, "title": row.title, "pressable": true});
            if let Some(subtitle) = &row.subtitle {
                props["subtitle"] = json!(subtitle);
            }
            json!({
                "id": format!("{row_id}.{}", row.id),
                "component": "Row",
                "props": props,
                "slots": {
                    "leading": {"id": format!("{row_id}.{}.glyph", row.id), "component": "Icon", "props": {"name": row.glyph, "tone": "neutral", "size": "sm"}},
                    "trailing": {"id": format!("{row_id}.{}.badge", row.id), "component": "Badge", "props": {"text": row.badge, "variant": "secondary"}},
                },
                "on": {"press": {"event": {"name": EVENT_OPEN_PR, "context": {"pr": row.pull_request_id}}}},
            })
        })
        .collect();
    json!({
        "id": id,
        "component": "Section",
        "props": {"title": model::copy("trainSection"), "caption": format!("{} · {}", group.title, group.caption), "icon": "pr-batch"},
        "children": rows,
    })
}

fn filters(input: &CiPageInput, state: &PageState) -> Value {
    let id = model::test_id;
    let items: Vec<Value> = [CiFilter::All, CiFilter::Running, CiFilter::Failed, CiFilter::Mine]
        .iter()
        .map(|value| json!({"label": model::copy(&format!("filters.{}", value.id())), "value": value.id()}))
        .collect();
    let mut status_props = json!({"items": items, "value": state.filter.status.id(), "size": "sm"});
    if state.phone {
        status_props["fill"] = json!(true);
    }
    let status = json!({"id": id("statusFilter"), "component": "Segmented", "props": status_props, "on": change(EVENT_STATUS)});
    let repository = json!({
        "id": id("repoFilter"),
        "component": "Select",
        "props": {
            "name": id("repoFilter"),
            "triggerVariant": "pill",
            "value": state.filter.repository_id.clone().unwrap_or_else(|| "all".into()),
            "options": model::ci_repo_options(input),
        },
        "on": change(EVENT_REPOSITORY),
    });
    let source = json!({
        "id": id("sourceFilter"),
        "component": "Select",
        "props": {
            "name": id("sourceFilter"),
            "triggerVariant": "pill",
            "value": state.filter.source.map(CiSource::id).unwrap_or("all"),
            "options": [
                {"label": model::copy("sources.all"), "value": "all"},
                {"label": model::copy("sources.exponential"), "value": "exponential"},
                {"label": model::copy("sources.github"), "value": "github"},
            ],
        },
        "on": change(EVENT_SOURCE),
    });
    if state.phone {
        return json!({
            "id": id("filters"),
            "component": "Stack",
            "props": {"direction": "vertical", "gap": "sm", "align": "stretch"},
            "children": [status, {"id": format!("{}.picks", id("filters")), "component": "Toolbar", "children": [repository, source]}],
        });
    }
    json!({"id": id("filters"), "component": "Toolbar", "children": [status, repository, source]})
}

fn runs_panel(input: &CiPageInput, state: &PageState) -> Value {
    let id = model::test_id;
    let rows = model::ci_run_list(input, &state.filter);
    let mut children = vec![filters(input, state)];
    for group in model::ci_train_groups(input, false) {
        children.push(train_section(&group, &format!("{}.{}", id("train"), group.repository_id)));
    }
    if rows.is_empty() {
        let title = if state.filter.filtered() { model::copy("empty.filtered") } else { model::copy("empty.runs") };
        children.push(json!({"id": id("runs"), "component": "EmptyState", "props": {"icon": "nav-ci", "title": title, "size": "list"}}));
    } else if state.phone {
        let row_id = id("runRow");
        let items: Vec<Value> = rows
            .iter()
            .map(|row| {
                let mut props = json!({"title": row.title, "subtitle": row.subtitle, "pressable": true});
                if let Some(duration) = &row.duration {
                    props["meta"] = json!(duration);
                }
                json!({
                    "id": format!("{row_id}.{}", row.id),
                    "component": "Row",
                    "props": props,
                    "slots": {"leading": {"id": format!("{row_id}.{}.glyph", row.id), "component": "Icon", "props": {"name": row.glyph, "tone": icon_tone(row.tone), "size": "sm"}}},
                    "on": {"press": {"event": {"name": EVENT_OPEN_RUN, "context": {"run": row.id}}}},
                })
            })
            .collect();
        children.push(json!({"id": id("runs"), "component": "Section", "props": {"title": model::copy("runsSection")}, "children": items}));
    } else {
        let c = |key: &str| model::copy(&format!("columns.{key}"));
        let table_rows: Vec<Value> = rows
            .iter()
            .map(|row| {
                json!({
                    "id": row.id,
                    "status": row.status_label,
                    "glyph": row.glyph,
                    "tone": icon_tone(row.tone),
                    "run": row.title,
                    "repository": row.repository,
                    "source": row.source_label,
                    "ref": row.r#ref,
                    "trigger": row.trigger,
                    "duration": row.duration.clone().unwrap_or_default(),
                    "started": row.started_at.clone().unwrap_or_default(),
                })
            })
            .collect();
        let runs = id("runs");
        children.push(json!({
            "id": runs,
            "component": "Table",
            "props": {
                "columns": [
                    {"key": "status", "label": c("status"), "type": "slot", "slot": "status", "width": 120},
                    {"key": "run", "label": c("run")},
                    {"key": "repository", "label": c("repository")},
                    {"key": "source", "label": c("source"), "type": "badge"},
                    {"key": "ref", "label": c("ref"), "truncate": true},
                    {"key": "trigger", "label": c("trigger")},
                    {"key": "duration", "label": c("duration"), "align": "end", "width": 96},
                    {"key": "started", "label": c("started"), "type": "relativeTime", "width": 140},
                ],
                "rows": table_rows,
                "rowKey": "id",
            },
            "slots": {
                "status": {
                    "id": format!("{runs}.status"),
                    "component": "Stack",
                    "props": {"direction": "horizontal", "gap": "xs", "align": "center"},
                    "children": [
                        {"id": format!("{runs}.status.glyph"), "component": "Icon", "props": {"name": {"@path": "glyph"}, "tone": {"@path": "tone"}, "size": "sm"}},
                        {"id": format!("{runs}.status.label"), "component": "Text", "props": {"text": {"@path": "status"}}},
                    ],
                },
            },
            "on": {"rowPress": {"event": {"name": EVENT_OPEN_RUN, "context": {"run": {"@path": "id"}}}}},
        }));
    }
    vstack(&format!("{}.runs", id("page")), "md", children)
}

fn train_panel(input: &CiPageInput) -> Value {
    let id = model::test_id;
    let groups = model::ci_train_groups(input, true);
    let children: Vec<Value> = if groups.is_empty() {
        vec![json!({"id": id("train"), "component": "EmptyState", "props": {"icon": "pr-batch", "title": model::copy("empty.train"), "size": "list"}})]
    } else {
        groups.iter().map(|group| train_section(group, &format!("{}.{}", id("train"), group.repository_id))).collect()
    };
    vstack(&format!("{}.train", id("page")), "md", children)
}

fn runners_panel(input: &CiPageInput) -> Value {
    let id = model::test_id;
    let rows = model::ci_runner_rows(input);
    if rows.is_empty() {
        return json!({"id": id("runners"), "component": "EmptyState", "props": {"icon": "server", "title": model::copy("empty.runners"), "size": "list"}});
    }
    let row_id = id("runnerRow");
    let children: Vec<Value> = rows
        .iter()
        .map(|row| {
            let meta = match (&row.busy, &row.offline_since) {
                (Some(busy), _) => Some(json!(busy)),
                (None, Some(since)) => Some(json!({
                    "@call": "fill",
                    "args": {"template": model::copy("runners.offlineSince"), "params": {"time": {"@call": "formatRelativeTime", "args": {"value": since}}}},
                })),
                _ => None,
            };
            let mut props = json!({"icon": "server", "title": row.title, "subtitle": row.subtitle, "pressable": true});
            if let Some(meta) = meta {
                props["meta"] = meta;
            }
            let mut node = json!({
                "id": format!("{row_id}.{}", row.id),
                "component": "Row",
                "props": props,
                "on": {"press": {"event": {"name": EVENT_OPEN_RUNNER, "context": {"device": row.id}}}},
            });
            if let Some(queue) = &row.queue {
                node["slots"] = json!({"trailing": {"id": format!("{row_id}.{}.queue", row.id), "component": "Badge", "props": {"text": queue, "variant": "outline"}}});
            }
            node
        })
        .collect();
    json!({"id": id("runners"), "component": "Section", "props": {}, "children": children})
}

/// The CI page as ONE document (web's `ciPageTree`): the tiles, then the
/// tabs; only the current tab's panel is built.
pub(crate) fn page_tree(input: &CiPageInput, state: &PageState) -> Value {
    let id = model::test_id;
    let tiles: Vec<Value> = model::ci_page_tiles(input)
        .iter()
        .enumerate()
        .map(|(index, tile)| {
            let mut props = json!({"label": tile.label, "value": tile.value, "tone": tile.tone});
            if let Some(delta) = &tile.delta {
                props["delta"] = json!(delta);
            }
            json!({"id": format!("{}.{index}", id("tiles")), "component": "StatTile", "props": props})
        })
        .collect();
    let (train, runners) = model::ci_tab_counts(input);
    let tab = |value: CiTab, count: usize| {
        let mut tab = json!({"label": model::copy(&format!("tabs.{}", value.id())), "value": value.id()});
        if count > 0 {
            tab["count"] = json!(count);
        }
        tab
    };
    let mut tabs_props = json!({"tabs": [tab(CiTab::Runs, 0), tab(CiTab::Train, train), tab(CiTab::Runners, runners)], "value": state.tab.id()});
    if state.phone {
        tabs_props["fill"] = json!(true);
    }
    let empty = |tab: CiTab| vstack(&format!("{}.{}", id("page"), tab.id()), "md", Vec::new());
    let runs = if state.tab == CiTab::Runs { runs_panel(input, state) } else { empty(CiTab::Runs) };
    let train_tab = if state.tab == CiTab::Train { train_panel(input) } else { empty(CiTab::Train) };
    let runners_tab = if state.tab == CiTab::Runners { runners_panel(input) } else { empty(CiTab::Runners) };
    let columns = if state.phone { 2 } else { 4 };
    let gap = if state.phone { "sm" } else { "md" };
    json!({
        "id": id("page"),
        "component": "Stack",
        "props": {"direction": "vertical", "gap": "lg", "align": "stretch"},
        "children": [
            {
                "id": id("tiles"),
                "component": "Grid",
                "props": {"columns": columns, "gap": gap},
                "children": tiles,
            },
            {
                "id": id("tabs"),
                "component": "Tabs",
                "props": tabs_props,
                "on": change(EVENT_TAB),
                "children": [runs, train_tab, runners_tab],
            },
        ],
    })
}

// ---------------------------------------------------------------------------
// The data
// ---------------------------------------------------------------------------

/// Repository full names by id, `repositories.list` once per member team.
#[derive(Default)]
pub(crate) struct RepoNames {
    pub names: HashMap<String, String>,
    loaded: HashSet<String>,
}

struct RepoNamesGlobal(Entity<RepoNames>);
impl Global for RepoNamesGlobal {}

/// The names entity (observe it), fetching every member team not read yet.
pub(crate) fn repo_names(cx: &mut App) -> Entity<RepoNames> {
    let entity = match cx.try_global::<RepoNamesGlobal>() {
        Some(global) => global.0.clone(),
        None => {
            let entity = cx.new(|_| RepoNames::default());
            cx.set_global(RepoNamesGlobal(entity.clone()));
            entity
        }
    };
    let (Some(store), Some(trpc)) = (Store::try_global(cx), queries::trpc_client(cx)) else { return entity };
    let teams = store.collections().teams_sorted(cx);
    for team in teams {
        if entity.read(cx).loaded.contains(&team.id) {
            continue;
        }
        entity.update(cx, |names, _| names.loaded.insert(team.id.clone()));
        let (trpc, team_id, weak) = (trpc.clone(), team.id.clone(), entity.downgrade());
        cx.spawn(async move |cx| {
            let rows = cx.background_executor().spawn(async move { crate::action_run::fetch_repositories(&trpc, &team_id) }).await;
            match rows {
                Ok(rows) => {
                    let _ = weak.update(cx, |names, cx| {
                        names.names.extend(rows.into_iter().map(|row| (row.id, row.full_name)));
                        cx.notify();
                    });
                }
                Err(err) => log::debug!("[ci] repositories.list: {err}"),
            }
        })
        .detach();
    }
    entity
}

/// A repository's full name when known (synced names first).
pub(crate) fn repo_full_name(repository_id: &str, cx: &mut App) -> Option<String> {
    repo_names(cx).read(cx).names.get(repository_id).cloned()
}

/// Every synced row the page reads, as [`CiPageInput`] (`repos` = what
/// [`repo_names`] holds; `None` = no store).
pub(crate) fn live_input(with_repos: bool, cx: &mut App) -> Option<CiPageInput> {
    let store = Store::try_global(cx)?;
    let collections = store.collections().clone();
    let viewer_id = crate::session_data::signed_in_user(cx).unwrap_or_default();
    let repos: Vec<CiPageRepo> = if with_repos {
        repo_names(cx).read(cx).names.iter().map(|(id, name)| CiPageRepo { id: id.clone(), full_name: name.clone() }).collect()
    } else {
        Vec::new()
    };
    let runs: Vec<CiPageRun> = collections
        .ci_runs
        .read(cx)
        .iter()
        .map(|run| CiPageRun {
            id: run.id.clone(),
            repository_id: run.repository_id.clone().unwrap_or_default(),
            number: run.number.unwrap_or_default(),
            name: run.name.clone(),
            sha: run.sha.clone(),
            r#ref: run.r#ref.clone(),
            trigger: run.trigger.clone().unwrap_or_default(),
            state: run.state.clone(),
            pull_request_id: run.pull_request_id.clone(),
            created_by_user_id: run.created_by_user_id.clone(),
            provider: run.provider.clone().unwrap_or_else(|| "exponential".into()),
            started_at: run.started_at.clone(),
            completed_at: run.completed_at.clone(),
            created_at: run.created_at.clone().unwrap_or_default(),
        })
        .collect();
    let jobs: Vec<CiPageJob> = collections
        .ci_jobs
        .read(cx)
        .iter()
        .map(|job| CiPageJob {
            run_id: job.run_id.clone(),
            status: job.status.clone(),
            conclusion: job.conclusion.clone(),
            runs_on: job
                .runs_on
                .as_ref()
                .and_then(Value::as_array)
                .map(|labels| labels.iter().filter_map(|label| label.as_str().map(str::to_string)).collect())
                .unwrap_or_default(),
            runner_device_id: job.runner_device_id.clone(),
            provider: job.provider.clone().unwrap_or_else(|| "exponential".into()),
        })
        .collect();
    let pull_requests: Vec<CiPagePr> = collections
        .pull_requests
        .read(cx)
        .iter()
        .map(|pr| CiPagePr {
            id: pr.id.clone(),
            number: pr.number.unwrap_or_default(),
            title: pr.title.clone().unwrap_or_default(),
            repository_id: pr.repository_id.clone().unwrap_or_default(),
            root_session_id: pr.root_session_id.clone(),
            base: pr.base.clone().unwrap_or_default(),
            merge_state: pr.merge_state.clone(),
            merge_position: pr.merge_position,
            merge_train_id: pr.merge_train_id.clone(),
        })
        .collect();
    let sessions: Vec<CiPageSession> = collections
        .sessions
        .read(cx)
        .iter()
        .filter(|session| session.parent_id.is_none())
        .filter_map(|session| Some(CiPageSession { id: session.id.clone(), user_id: session.user_id.clone()? }))
        .collect();
    let runners: Vec<CiPageRunner> = collections
        .devices
        .read(cx)
        .iter()
        .filter(|device| device.runner.as_ref().is_some_and(|runner| runner["enabled"].as_bool() == Some(true)))
        .map(|device| {
            let settings = domain::ci_run::runner_settings(device.runner.as_ref());
            CiPageRunner {
                id: device.id.clone(),
                label: device.label.clone().unwrap_or_default(),
                platform: device.platform.clone(),
                labels: settings.labels,
                slots: settings.slots as i64,
                last_seen_at: device.last_seen_at.clone(),
            }
        })
        .collect();
    let now = crate::exponential_ui_host::now_iso();
    Some(CiPageInput { runs, jobs, repos, pull_requests, sessions, runners, viewer_id, now })
}

/// The sidebar row's badge (nav-badges.json destination `ci`).
pub(crate) fn ci_badge(cx: &mut App) -> CountBadgeValue {
    live_input(false, cx).map(|input| model::ci_badge(&input)).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// The view
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Live,
    /// ci-page.json `scenario` (the shot).
    Specimen,
}

/// THE CI page.
pub(crate) struct CiPageView {
    source: Source,
    state: PageState,
    view: Entity<SurfaceView>,
    last: Value,
    _subscriptions: Vec<Subscription>,
}

impl CiPageView {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build(Source::Live, window, cx)
    }

    /// ci-page.json `scenario`, no server.
    pub(crate) fn specimen(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build(Source::Specimen, window, cx)
    }

    fn build(source: Source, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let weak = cx.entity().downgrade();
        let on_action: Rc<dyn Fn(ActionEvent, &mut App)> = Rc::new(move |event, cx| {
            let _ = weak.update(cx, |this, cx| this.on_action(event, cx));
        });
        let view = crate::ci_run::mount(&format!("ci-page-{}", cx.entity_id()), on_action, window, cx);
        let mut subscriptions = Vec::new();
        if source == Source::Live {
            if let Some(store) = Store::try_global(cx) {
                let collections = store.collections().clone();
                subscriptions.push(cx.observe(&collections.ci_runs, |_, _, cx| cx.notify()));
                subscriptions.push(cx.observe(&collections.ci_jobs, |_, _, cx| cx.notify()));
                subscriptions.push(cx.observe(&collections.pull_requests, |_, _, cx| cx.notify()));
                subscriptions.push(cx.observe(&collections.devices, |_, _, cx| cx.notify()));
            }
            let names = repo_names(cx);
            subscriptions.push(cx.observe(&names, |_, _, cx| cx.notify()));
        }
        Self { source, state: PageState::default(), view, last: Value::Null, _subscriptions: subscriptions }
    }

    fn on_action(&mut self, event: ActionEvent, cx: &mut Context<Self>) {
        let value = event.payload.as_ref().and_then(|payload| payload["value"].as_str()).map(str::to_string);
        match event.name.as_str() {
            EVENT_TAB => {
                if let Some(tab) = value.as_deref().and_then(CiTab::parse) {
                    self.state.tab = tab;
                }
            }
            // eUI Select fires `change {open}` on open/close: only a value moves it.
            EVENT_STATUS => {
                if let Some(value) = value {
                    self.state.filter.status = CiFilter::parse(&value);
                }
            }
            EVENT_REPOSITORY => {
                if let Some(value) = value {
                    self.state.filter.repository_id = (value != "all").then_some(value);
                }
            }
            EVENT_SOURCE => {
                if let Some(value) = value {
                    self.state.filter.source = match value.as_str() {
                        "github" => Some(CiSource::Github),
                        "exponential" => Some(CiSource::Exponential),
                        _ => None,
                    };
                }
            }
            EVENT_OPEN_RUN if self.source == Source::Live => {
                let run = event.context["run"]
                    .as_str()
                    .or_else(|| event.payload.as_ref().and_then(|payload| payload["id"].as_str()))
                    .map(str::to_string);
                if let Some(run_id) = run {
                    crate::navigation::on_active_window(cx, move |window, cx| {
                        crate::navigation::navigate(window, cx, crate::navigation::Screen::CiRun { run_id, job: None });
                    });
                }
            }
            EVENT_OPEN_PR if self.source == Source::Live => {
                if let Some(pr_id) = event.context["pr"].as_str() {
                    open_pull_request(pr_id, cx);
                }
            }
            EVENT_OPEN_RUNNER if self.source == Source::Live => {
                if let Some(device_id) = event.context["device"].as_str().map(str::to_string) {
                    crate::navigation::on_active_window(cx, move |window, cx| {
                        crate::navigation::navigate(window, cx, crate::navigation::Screen::Device { device_id });
                    });
                }
            }
            _ => return,
        }
        cx.notify();
    }

    fn tree(&self, cx: &mut Context<Self>) -> Option<Value> {
        let input = match self.source {
            Source::Specimen => model::scenario(),
            Source::Live => live_input(true, cx)?,
        };
        Some(page_tree(&input, &self.state))
    }
}

/// A train row's press: the PR's chat (its root session), else its page on
/// the forge.
pub(crate) fn open_pull_request(pr_id: &str, cx: &mut App) {
    let Some(store) = Store::try_global(cx) else { return };
    let Some(pr) = store.collections().pull_requests.read(cx).get(pr_id).cloned() else { return };
    match pr.root_session_id.clone().or(pr.session_id.clone()) {
        Some(session_id) => crate::navigation::on_active_window(cx, move |window, cx| {
            crate::navigation::navigate(window, cx, crate::navigation::Screen::Chat { session_id });
        }),
        None => {
            if let Some(url) = pr.url.as_deref() {
                cx.open_url(url);
            }
        }
    }
}

impl Render for CiPageView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "CiPageView").entered();
        let tree = self.tree(cx).unwrap_or_else(|| json!({"id": model::test_id("page"), "component": "Skeleton", "props": {"rows": 6}}));
        crate::ci_run::show(&self.view, &mut self.last, tree, cx);
        div()
            .id("ci-page")
            .size_full()
            .overflow_y_scroll()
            .p_6()
            .child(div().w_full().max_w(gpui::px(1100.)).child(self.view.clone()))
            .child(crate::assistant_fab::page_end(window, cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW_FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/ci-page-view.json");

    fn normalized(value: &Value) -> Value {
        match value {
            Value::Number(number) => json!(number.as_f64().unwrap_or_default()),
            Value::Array(items) => Value::Array(items.iter().map(normalized).collect()),
            Value::Object(map) => Value::Object(map.iter().map(|(key, value)| (key.clone(), normalized(value))).collect()),
            other => other.clone(),
        }
    }

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

    fn world(name: &str) -> CiPageInput {
        let mut input = model::scenario();
        if name == "empty" {
            input.runs.clear();
            input.jobs.clear();
            input.pull_requests.clear();
            input.runners.clear();
        }
        input
    }

    /// #EXP-2172: ci-page-view.json — the CI page node for node as web builds it.
    #[test]
    fn the_ci_page_replays_ci_page_view() {
        let fixture: Value = serde_json::from_str(VIEW_FIXTURE).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let state = PageState {
                tab: CiTab::parse(case["tab"].as_str().unwrap()).unwrap(),
                filter: serde_json::from_value(case["filter"].clone()).unwrap(),
                phone: case["phone"].as_bool() == Some(true),
            };
            let got = normalized(&page_tree(&world(case["world"].as_str().unwrap()), &state));
            if let Some(drift) = first_drift("", &got, &normalized(&case["tree"])) {
                panic!("{}: {drift}", case["name"]);
            }
        }
    }

    #[test]
    fn every_fixture_tree_mounts() {
        let fixture: Value = serde_json::from_str(VIEW_FIXTURE).unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let nested: eui_core::NestedNode = serde_json::from_value(case["tree"].clone()).expect("a nested node");
            let mut surface = eui_core::surface::Surface::new("ci", Default::default());
            let outcome = surface.set_nested(nested);
            let errors: Vec<_> = outcome.issues.iter().filter(|issue| format!("{issue:?}").contains("nknown")).collect();
            assert!(errors.is_empty(), "{errors:?}");
        }
    }
}
