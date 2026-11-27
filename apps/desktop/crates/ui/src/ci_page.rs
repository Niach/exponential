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
    App, AppContext as _, Context, Entity, Global, IntoElement, ParentElement as _, Render, Styled as _, Subscription, Window,
};
use serde_json::{json, Value};
use sync::Store;

use domain::ci_page::{
    self as model, CiFilter, CiPageInput, CiPageJob, CiPagePr, CiPageRepo, CiPageRun, CiPageRunner, CiPageSession, CiRunFilter, CiRunListRow,
    CiSource, CiTab, CiTrainGroup, CiPageUser,
};
use domain::nav_badges::CountBadgeValue;

use crate::queries;

// ---------------------------------------------------------------------------
// The tree (ci-page-view.json)
// ---------------------------------------------------------------------------

/// The page's events (web `CI_PAGE_EVENTS`).
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

pub(crate) const EVENT_OPEN_RUNNERS: &str = "ci-open-runners";

/// ci-page-view.json `TRAIN_TONE`: a train glyph's tone (`statusTones`).
fn train_tone(glyph: &str) -> &'static str {
    match glyph {
        "ci-running" => model::status_tone(domain::ci_run::CiRunStatus::Running).1,
        "ci-failed" => "danger",
        "ci-passed" => "success",
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
                    "leading": {"id": format!("{row_id}.{}.glyph", row.id), "component": "Icon", "props": {"name": row.glyph, "tone": train_tone(row.glyph), "size": "sm"}},
                    "trailing": {"id": format!("{row_id}.{}.badge", row.id), "component": "Badge", "props": {"text": row.badge, "variant": "secondary"}},
                },
                "on": {"press": {"event": {"name": EVENT_OPEN_PR, "context": {"pr": row.pull_request_id}}}},
            })
        })
        .collect();
    json!({
        "id": id,
        "component": "Section",
        "props": {"title": group.title, "caption": group.caption, "icon": "pr-batch"},
        "children": rows,
    })
}

/// The filter bar (ci-page-view.json, #EXP-2169 "one look"): ONE Toolbar of
/// three `Select{button}` pickers (status, repository, source), the app's
/// ordinary picker buttons; wide adds the runner Chip at its end.
fn filters(input: &CiPageInput, state: &PageState) -> Value {
    let id = model::test_id;
    let picker = |id: &str, label: &str, value: &str, options: Value, event: &str| {
        json!({
            "id": id,
            "component": "Select",
            "props": {"name": id, "label": label, "triggerVariant": "button", "value": value, "options": options},
            "on": change(event),
        })
    };
    let statuses: Vec<Value> = [CiFilter::All, CiFilter::Running, CiFilter::Failed, CiFilter::Mine]
        .iter()
        .map(|value| json!({"label": model::copy(&format!("filters.{}", value.id())), "value": value.id()}))
        .collect();
    let status = picker(id("statusFilter"), model::copy("filterLabels.status"), state.filter.status.id(), json!(statuses), EVENT_STATUS);
    let repository = picker(
        id("repoFilter"),
        model::copy("filterLabels.repository"),
        state.filter.repository_id.as_deref().unwrap_or("all"),
        json!(model::ci_repo_options(input)),
        EVENT_REPOSITORY,
    );
    let source = picker(
        id("sourceFilter"),
        model::copy("filterLabels.source"),
        state.filter.source.map(CiSource::id).unwrap_or("all"),
        json!([
            {"label": model::copy("sources.all"), "value": "all"},
            {"label": model::copy("sources.exponential"), "value": "exponential"},
            {"label": model::copy("sources.github"), "value": "github"},
        ]),
        EVENT_SOURCE,
    );
    let mut node = json!({"id": id("filters"), "component": "Toolbar", "children": [status, repository, source]});
    if let Some((label, online)) = model::ci_runner_chip(input).filter(|_| !state.phone) {
        node["slots"] = json!({"trailing": {
            "id": id("runnerChip"),
            "component": "Chip",
            "props": {"label": label, "dot": if online { "$color.success" } else { "$color.mutedForeground" }, "pressable": true},
            "on": {"press": {"event": {"name": EVENT_OPEN_RUNNERS}}},
        }});
    }
    node
}

/// One run's row (web `runRow`).
fn run_row(row: &CiRunListRow, phone: bool) -> Value {
    let id = format!("{}.{}", model::test_id("runRow"), row.id);
    let leading = json!({"id": format!("{id}.glyph"), "component": "Icon", "props": {"name": row.glyph, "tone": row.tone, "size": "sm"}});
    let press = json!({"press": {"event": {"name": EVENT_OPEN_RUN, "context": {"run": row.id}}}});
    if phone {
        return json!({
            "id": id,
            "component": "Row",
            "props": {"title": row.title, "subtitle": row.phone_subtitle, "meta": row.phone_meta, "pressable": true},
            "slots": {"leading": leading},
            "on": press,
        });
    }
    let age = json!({"@call": "formatRelativeTime", "args": {"value": row.at}});
    let meta = match &row.duration {
        Some(duration) => json!({"@call": "fill", "args": {"template": "{duration} · {age}", "params": {"duration": duration, "age": age}}}),
        None => json!({"@call": "fill", "args": {"template": "{queued} · {age}", "params": {"queued": model::copy("waitingForRunner"), "age": age}}}),
    };
    let mut trailing = vec![
        json!({"id": format!("{id}.meta"), "component": "Text", "props": {"text": meta, "variant": "muted"}}),
        json!({
            "id": format!("{id}.source"),
            "component": "Tooltip",
            "props": {"content": row.source_label},
            "children": [{"id": format!("{id}.source.icon"), "component": "Icon", "props": {"name": row.source_icon, "size": "sm", "tone": "neutral", "label": row.source_label}}],
        }),
    ];
    if let Some(author) = &row.author {
        let mut props = json!({"name": author.name, "seed": author.id, "size": "sm"});
        if let Some(image) = &author.image {
            props["src"] = json!(image);
        }
        trailing.push(json!({"id": format!("{id}.author"), "component": "Avatar", "props": props}));
    }
    json!({
        "id": id,
        "component": "Row",
        "props": {"title": row.title, "subtitle": row.subtitle, "pressable": true},
        "slots": {
            "leading": leading,
            "trailing": {"id": format!("{id}.trailing"), "component": "Stack", "props": {"direction": "horizontal", "gap": "sm", "align": "center"}, "children": trailing},
        },
        "on": press,
    })
}

/// The runs as day Sections (or the EmptyState): the Runs tab's body, and
/// the repo page's Runs tab ([`repo_runs_tree`]).
fn run_sections(input: &CiPageInput, filter: &CiRunFilter, phone: bool, filtered: bool) -> Value {
    let id = model::test_id;
    let groups = model::ci_run_groups(input, filter);
    if groups.is_empty() {
        let title = if filtered { model::copy("empty.filtered") } else { model::copy("empty.runs") };
        return json!({"id": id("runs"), "component": "EmptyState", "props": {"icon": "nav-ci", "title": title, "size": "list"}});
    }
    let sections: Vec<Value> = groups
        .iter()
        .map(|group| {
            let mut props = json!({"title": group.title});
            if let Some(count) = group.count {
                props["count"] = json!(count);
            }
            json!({
                "id": format!("{}.{}", id("runs"), group.id),
                "component": "Section",
                "props": props,
                "children": group.rows.iter().map(|row| run_row(row, phone)).collect::<Vec<_>>(),
            })
        })
        .collect();
    vstack(id("runs"), "md", sections)
}

fn runs_panel(input: &CiPageInput, state: &PageState) -> Value {
    let id = model::test_id;
    let sections = run_sections(input, &state.filter, state.phone, state.filter.filtered());
    vstack(&format!("{}.runs", id("page")), "md", vec![filters(input, state), sections])
}

/// THE REPO PAGE'S RUNS TAB (#EXP-2169 "one look", web `ciRepoRunsTree`):
/// the CI page's runs of ONE repository, the same day Sections and rows, no
/// filter bar. Events: the page's `ci-open-run {run}`.
pub(crate) fn repo_runs_tree(input: &CiPageInput, repository_id: &str, phone: bool) -> Value {
    let filter = CiRunFilter { status: CiFilter::All, repository_id: Some(repository_id.to_string()), source: None };
    run_sections(input, &filter, phone, false)
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

/// The CI page's body (web's `ciPageTree`): the CURRENT tab's panel. The tab
/// switcher is the app's segmented capsule ([`crate::controls::page_tabs`],
/// #EXP-2169 "one look"), drawn by the view above this tree.
pub(crate) fn page_tree(input: &CiPageInput, state: &PageState) -> Value {
    match state.tab {
        CiTab::Runs => runs_panel(input, state),
        CiTab::Train => train_panel(input),
        CiTab::Runners => runners_panel(input),
    }
}

/// The tab switcher's entries: copy.tabs, Merge train and Runners counted
/// (`ciTabCounts`; Runs none, zero = none).
fn page_tabs(input: Option<&CiPageInput>) -> Vec<crate::controls::PageTab> {
    let (train, runners) = input.map(model::ci_tab_counts).unwrap_or_default();
    [(CiTab::Runs, 0), (CiTab::Train, train), (CiTab::Runners, runners)]
        .into_iter()
        .map(|(tab, count)| crate::controls::PageTab {
            value: tab.id().to_string(),
            label: model::copy(&format!("tabs.{}", tab.id())).to_string(),
            count: (count > 0).then_some(count),
        })
        .collect()
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
    let Some(store) = Store::try_global(cx) else { return entity };
    let teams = store.collections().teams_sorted(cx);
    for team in teams {
        if entity.read(cx).loaded.contains(&team.id) {
            continue;
        }
        let Some(trpc) = queries::trpc_client(cx) else { return entity };
        entity.update(cx, |names, _| names.loaded.insert(team.id.clone()));
        let (team_id, weak) = (team.id.clone(), entity.downgrade());
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

/// A synced timestamp (Postgres' `2026-11-26 12:03:05.335+00`) as the ISO
/// instant the catalog's `formatRelativeTime` reads (web hands it a `Date`).
pub(crate) fn iso_instant(text: &str) -> String {
    domain::instant::parse_instant(text)
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|at| at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_else(|| text.to_string())
}

/// A synced `ci_runs` row as the page's run.
pub(crate) fn page_run(run: &domain::rows::CiRunRow) -> CiPageRun {
    CiPageRun {
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
        started_at: run.started_at.as_deref().map(iso_instant),
        completed_at: run.completed_at.as_deref().map(iso_instant),
        created_at: run.created_at.as_deref().map(iso_instant).unwrap_or_default(),
        title: run.title.clone().filter(|title| !title.is_empty()),
    }
}

/// A synced `ci_jobs` row as the page's job.
pub(crate) fn page_job(job: &domain::rows::CiJobRow) -> CiPageJob {
    CiPageJob {
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
    }
}

/// Every synced row the page reads, as [`CiPageInput`] (`repos` = what
/// [`repo_names`] holds; `None` = no store).
pub(crate) fn live_input(with_repos: bool, cx: &mut App) -> Option<CiPageInput> {
    let repos: Vec<CiPageRepo> = if with_repos {
        repo_names(cx).read(cx).names.iter().map(|(id, name)| CiPageRepo { id: id.clone(), full_name: name.clone() }).collect()
    } else {
        Vec::new()
    };
    synced_input(repos, cx)
}

/// [`live_input`] over the given `repos` (the repo page's Runs tab: that
/// repository alone).
pub(crate) fn synced_input(repos: Vec<CiPageRepo>, cx: &App) -> Option<CiPageInput> {
    let store = Store::try_global(cx)?;
    let collections = store.collections().clone();
    let viewer_id = crate::session_data::signed_in_user(cx).unwrap_or_default();
    let runs: Vec<CiPageRun> = collections.ci_runs.read(cx).iter().map(page_run).collect();
    let jobs: Vec<CiPageJob> = collections.ci_jobs.read(cx).iter().map(page_job).collect();
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
    let users: Vec<CiPageUser> = collections
        .users
        .read(cx)
        .iter()
        .map(|user| CiPageUser {
            id: user.id.clone(),
            name: user.name.clone().filter(|name| !name.is_empty()).unwrap_or_else(|| domain::rows::member_fallback_label(&user.id)),
            image: user.image.clone().filter(|image| !image.is_empty()),
        })
        .collect();
    let now = crate::exponential_ui_host::now_iso();
    let utc_offset_minutes = i64::from(chrono::Local::now().offset().local_minus_utc() / 60);
    Some(CiPageInput { runs, jobs, repos, pull_requests, sessions, runners, users, viewer_id, now, utc_offset_minutes })
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
    scroll: gpui::ScrollHandle,
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
        Self { source, state: PageState::default(), view, last: Value::Null, scroll: gpui::ScrollHandle::new(), _subscriptions: subscriptions }
    }

    fn on_action(&mut self, event: ActionEvent, cx: &mut Context<Self>) {
        let value = event.payload.as_ref().and_then(|payload| payload["value"].as_str()).map(str::to_string);
        match event.name.as_str() {
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
            EVENT_OPEN_RUNNERS => self.state.tab = CiTab::Runners,
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

    fn input(&self, cx: &mut Context<Self>) -> Option<CiPageInput> {
        match self.source {
            Source::Specimen => Some(model::scenario()),
            Source::Live => live_input(true, cx),
        }
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
        let input = self.input(cx);
        let tree = input
            .as_ref()
            .map(|input| page_tree(input, &self.state))
            .unwrap_or_else(|| json!({"id": model::test_id("page"), "component": "Skeleton", "props": {"rows": 6}}));
        crate::ci_run::show(&self.view, &mut self.last, tree, cx);
        let weak = cx.entity().downgrade();
        let on_pick: Rc<dyn Fn(String, &mut Window, &mut App)> = Rc::new(move |value, _, cx| {
            let _ = weak.update(cx, |this, cx| {
                if let Some(tab) = CiTab::parse(&value) {
                    this.state.tab = tab;
                    cx.notify();
                }
            });
        });
        let tabs = crate::controls::page_tabs(model::test_id("tabs"), page_tabs(input.as_ref()), self.state.tab.id(), on_pick, cx);
        // #EXP-2169 ("one look"): the device page's column and entity heading
        // (`nav-ci` + "CI").
        let heading = crate::scroll_pane::page_heading(gpui_component::Icon::new(crate::icons::registry::NAV_CI), model::copy("title").into(), cx);
        // Web: the heading, then (space-y-4) the switcher over the panel.
        let body = gpui_component::v_flex().gap_4().child(tabs).child(self.view.clone());
        let column = gpui_component::v_flex().gap_6().child(crate::controls::web_rem(heading)).child(body);
        crate::scroll_pane::page_scaffold("ci-page", &self.scroll, column, window, cx)
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
        assert!(cases.iter().any(|case| case["repositoryId"].is_string()), "the repo page's Runs tab cases");
        for case in cases {
            let world = world(case["world"].as_str().unwrap());
            let phone = case["phone"].as_bool() == Some(true);
            // #EXP-2169: a `repositoryId` case = the repo page's Runs tab.
            if let Some(repository_id) = case["repositoryId"].as_str() {
                let got = normalized(&repo_runs_tree(&world, repository_id, phone));
                if let Some(drift) = first_drift("", &got, &normalized(&case["tree"])) {
                    panic!("{}: {drift}", case["name"]);
                }
                continue;
            }
            let state = PageState {
                tab: CiTab::parse(case["tab"].as_str().unwrap()).unwrap(),
                filter: serde_json::from_value(case["filter"].clone()).unwrap(),
                phone,
            };
            let got = normalized(&page_tree(&world, &state));
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
