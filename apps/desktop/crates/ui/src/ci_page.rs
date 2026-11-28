//! #EXP-2172 — THE CI PAGE on the desktop, genUI catalog only (SAME LOOK
//! ×4): `fixtures/ci-page.json` (what), `fixtures/ci-page-view.json` (the
//! tree, node for node web's `apps/web/src/lib/ci-page-view.ts`), view `ci`,
//! [`crate::navigation::Screen::Ci`]: one root row after Devices across every
//! member team. The rules are `domain::ci_page`'s; this file lays the page
//! out ([`page_tree`]) and maps its events.
//!
//! DATA, all synced: `ci_runs` + `ci_jobs` (ours and GitHub Actions),
//! `pull_requests`, the sessions (a root's starter), the devices (the
//! runners), the repositories (name + default branch) and, #EXP-2214, the
//! merge queue's `merge_trains` + `merge_train_events`.

use std::rc::Rc;

use eui_gpui::host::ActionEvent;
use eui_gpui::view::SurfaceView;
use gpui::{
    App, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _, Subscription, Window,
};
use serde_json::{json, Value};
use sync::Store;

use domain::ci_page::{
    self as model, CiFilter, CiPageInput, CiPageJob, CiPagePr, CiPageRepo, CiPageRun, CiPageRunner, CiPageSession, CiPageTrain,
    CiPageTrainEvent, CiPageUser, CiQueueNow, CiQueueRepo, CiQueueRow, CiRunFilter, CiRunKind, CiRunListRow, CiSubjectGroup, CiSubjectRow,
    CiTab,
};
use domain::nav_badges::CountBadgeValue;

// ---------------------------------------------------------------------------
// The tree (ci-page-view.json)
// ---------------------------------------------------------------------------

/// The page's events (web `CI_PAGE_EVENTS`). Context keys: `run`, `job`,
/// `pr`, `device`, `url`; the pickers, tabs and menus carry their `value`.
pub(crate) const EVENT_TAB: &str = "ci-tab";
pub(crate) const EVENT_STATUS: &str = "ci-filter-status";
pub(crate) const EVENT_REPOSITORY: &str = "ci-filter-repo";
pub(crate) const EVENT_OPEN_RUN: &str = "ci-open-run";
pub(crate) const EVENT_OPEN_JOB: &str = "ci-open-job";
pub(crate) const EVENT_OPEN_URL: &str = "ci-open-url";
pub(crate) const EVENT_OPEN_PR: &str = "ci-open-pr";
pub(crate) const EVENT_OPEN_RUNNER: &str = "ci-open-runner";
pub(crate) const EVENT_OPEN_RUNNERS: &str = "ci-open-runners";
/// The Up next ⋯ menu: `value` = merge-now | hold, context `pr`.
pub(crate) const EVENT_QUEUE_MENU: &str = "ci-queue-menu";
pub(crate) const EVENT_RESUME: &str = "ci-resume";

/// The page's own state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PageState {
    pub tab: CiTab,
    pub filter: CiRunFilter,
    pub phone: bool,
}

/// web `DOT`: a pill tone's dot colour.
fn dot(tone: &str) -> &'static str {
    match tone {
        "info" => "$color.info",
        "success" => "$color.success",
        "danger" => "$color.destructive",
        "warning" => "$color.warning",
        _ => "$color.mutedForeground",
    }
}

fn vstack(id: &str, gap: &str, children: Vec<Value>) -> Value {
    json!({"id": id, "component": "Stack", "props": {"direction": "vertical", "gap": gap, "align": "stretch"}, "children": children})
}

fn hstack(id: &str, children: Vec<Value>) -> Value {
    json!({"id": id, "component": "Stack", "props": {"direction": "horizontal", "gap": "sm", "align": "center"}, "children": children})
}

fn relative(value: &str) -> Value {
    json!({"@call": "formatRelativeTime", "args": {"value": value}})
}

fn open_run(run: &str) -> Value {
    json!({"press": {"event": {"name": EVENT_OPEN_RUN, "context": {"run": run}}}})
}

fn open_pr(pr: &str) -> Value {
    json!({"press": {"event": {"name": EVENT_OPEN_PR, "context": {"pr": pr}}}})
}

fn glyph(id: &str, name: &str, tone: &str) -> Value {
    json!({"id": id, "component": "Icon", "props": {"name": name, "tone": tone, "size": "sm"}})
}

fn avatar(id: &str, user: &CiPageUser) -> Value {
    let mut props = json!({"name": user.name, "seed": user.id, "size": "sm"});
    if let Some(image) = &user.image {
        props["src"] = json!(image);
    }
    json!({"id": id, "component": "Avatar", "props": props})
}

/// The filter bar (ci-page-view.json, #EXP-2169 "one look"): ONE Toolbar of
/// `Select{button}` pickers, status and repository (#EXP-2214: the source
/// picker is gone); wide adds the runner Chip at its end.
fn filters(input: &CiPageInput, state: &PageState) -> Value {
    let id = model::test_id;
    let picker = |id: &str, label: &str, value: &str, options: Value, event: &str| {
        json!({
            "id": id,
            "component": "Select",
            "props": {"name": id, "label": label, "triggerVariant": "button", "value": value, "options": options},
            "on": {"change": {"event": {"name": event}}},
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
    let mut node = json!({"id": id("filters"), "component": "Toolbar", "children": [status, repository]});
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
    let leading = glyph(&format!("{id}.glyph"), row.glyph, row.tone);
    if phone {
        return json!({
            "id": id,
            "component": "Row",
            "props": {"title": row.title, "subtitle": row.phone_subtitle, "meta": row.phone_meta, "pressable": true},
            "slots": {"leading": leading},
            "on": open_run(&row.id),
        });
    }
    let age = relative(&row.at);
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
        trailing.push(avatar(&format!("{id}.author"), author));
    }
    json!({
        "id": id,
        "component": "Row",
        "props": {"title": row.title, "subtitle": row.subtitle, "pressable": true},
        "slots": {"leading": leading, "trailing": hstack(&format!("{id}.trailing"), trailing)},
        "on": open_run(&row.id),
    })
}

/// Runs as day Sections (In progress first), or the EmptyState.
fn run_sections(id: &str, input: &CiPageInput, filter: &CiRunFilter, kinds: &[CiRunKind], phone: bool, empty: &str) -> Value {
    let groups = model::ci_run_groups(input, filter, Some(kinds));
    if groups.is_empty() {
        return json!({"id": id, "component": "EmptyState", "props": {"icon": "nav-ci", "title": empty, "size": "list"}});
    }
    let sections: Vec<Value> = groups
        .iter()
        .map(|group| {
            let mut props = json!({"title": group.title});
            if let Some(count) = group.count {
                props["count"] = json!(count);
            }
            json!({
                "id": format!("{id}.{}", group.id),
                "component": "Section",
                "props": props,
                "children": group.rows.iter().map(|row| run_row(row, phone)).collect::<Vec<_>>(),
            })
        })
        .collect();
    vstack(id, "md", sections)
}

fn filtered(filter: &CiRunFilter) -> bool {
    filter.status != CiFilter::All || filter.repository_id.is_some()
}

// ── Pull requests ─────────────────────────────────────────────────────────

/// One PR (or branch): its rolled-up status, the deciding run's line and,
/// wide, the merge queue pill; a press opens that run (#EXP-2214: no folded
/// runs, genUI's foldable Row drops its leading and trailing slots).
fn subject_row(row: &CiSubjectRow, phone: bool) -> Value {
    let id = format!("{}.{}", model::test_id("prRow"), row.id);
    let mut props = json!({"title": row.title, "subtitle": if phone { &row.phone_subtitle } else { &row.subtitle }, "pressable": true});
    if let Some(identifier) = &row.identifier {
        props["identifier"] = json!(identifier);
    }
    if phone {
        props["meta"] = json!(row.meta);
    }
    let mut slots = json!({"leading": glyph(&format!("{id}.glyph"), row.glyph, row.tone)});
    if !phone {
        let mut trailing = vec![json!({"id": format!("{id}.meta"), "component": "Text", "props": {"text": row.meta, "variant": "muted"}})];
        if let Some(pill) = &row.pill {
            trailing.push(json!({"id": format!("{id}.pill"), "component": "Chip", "props": {"label": pill.text, "dot": dot(pill.tone)}}));
        }
        slots["trailing"] = hstack(&format!("{id}.trailing"), trailing);
    }
    json!({"id": id, "component": "Row", "props": props, "slots": slots, "on": open_run(&row.run_id)})
}

fn subject_sections(id: &str, groups: &[CiSubjectGroup], phone: bool, empty: &str) -> Value {
    if groups.is_empty() {
        return json!({"id": id, "component": "EmptyState", "props": {"icon": "git-pull-request", "title": empty, "size": "list"}});
    }
    let sections: Vec<Value> = groups
        .iter()
        .map(|group| {
            let mut props = json!({"title": group.title, "count": group.count});
            if !group.open {
                props["collapsible"] = json!(true);
                props["open"] = json!(false);
            }
            json!({
                "id": format!("{id}.{}", group.id),
                "component": "Section",
                "props": props,
                "children": group.rows.iter().map(|row| subject_row(row, phone)).collect::<Vec<_>>(),
            })
        })
        .collect();
    vstack(id, "md", sections)
}

// ── Merge queue ───────────────────────────────────────────────────────────

fn now_card(now: &CiQueueNow, id: &str) -> Value {
    let copy = |key: &str| model::copy(&format!("queue.{key}"));
    let mut children = vec![match now.progress {
        None => json!({"id": format!("{id}.elapsed"), "component": "Text", "props": {"text": now.elapsed, "variant": "muted"}}),
        Some(progress) => json!({
            "id": format!("{id}.progress"),
            "component": "Progress",
            "props": {"value": progress, "max": 100, "label": now.elapsed, "valueText": now.eta, "tone": "info"},
        }),
    }];
    let members: Vec<Value> = now
        .members
        .iter()
        .map(|member| {
            let row_id = format!("{id}.member.{}", member.pull_request_id);
            let mut node = json!({
                "id": row_id,
                "component": "Row",
                "props": {"identifier": member.identifier, "title": member.title, "pressable": true},
                "on": open_pr(&member.pull_request_id),
            });
            if let Some(author) = &member.author {
                node["slots"] = json!({"trailing": avatar(&format!("{row_id}.author"), author)});
            }
            node
        })
        .collect();
    children.push(json!({
        "id": format!("{id}.members"),
        "component": "Section",
        "props": {"title": copy("batch"), "count": now.members.len()},
        "children": members,
    }));
    if let Some(run_id) = &now.run_id {
        children.push(hstack(
            &format!("{id}.actions"),
            vec![json!({
                "id": format!("{id}.open"),
                "component": "Button",
                "props": {"label": copy("openRun"), "icon": "external-link", "variant": "outline", "size": "sm"},
                "on": open_run(run_id),
            })],
        ));
    }
    json!({
        "id": id,
        "component": "Card",
        "props": {"title": copy("testing"), "description": now.subtitle},
        "children": [vstack(&format!("{id}.body"), "md", children)],
    })
}

fn queue_row(row: &CiQueueRow, id: &str, phone: bool) -> Value {
    let copy = |key: &str| model::copy(&format!("queue.{key}"));
    let mut trailing: Vec<Value> = Vec::new();
    if let Some(at) = row.at.as_deref().filter(|_| !phone) {
        trailing.push(json!({"id": format!("{id}.at"), "component": "Text", "props": {"text": relative(at), "variant": "muted"}}));
    }
    if let Some(badge) = &row.badge {
        trailing.push(json!({"id": format!("{id}.badge"), "component": "Chip", "props": {"label": badge.text, "dot": dot(badge.tone)}}));
    }
    if row.action == Some("open-log") {
        let press = match &row.job_id {
            Some(job) => json!({"event": {"name": EVENT_OPEN_JOB, "context": {"run": row.run_id, "job": job}}}),
            None => json!({"event": {"name": EVENT_OPEN_URL, "context": {"url": row.url}}}),
        };
        trailing.push(json!({
            "id": format!("{id}.log"),
            "component": "Button",
            "props": {"label": copy("openLog"), "icon": "file-text", "variant": "outline", "size": "sm"},
            "on": {"press": press},
        }));
    }
    if row.action == Some("resume") {
        trailing.push(json!({
            "id": format!("{id}.resume"),
            "component": "Button",
            "props": {"label": copy("resume"), "variant": "outline", "size": "sm"},
            "on": {"press": {"event": {"name": EVENT_RESUME, "context": {"pr": row.pull_request_id}}}},
        }));
    }
    if row.menu {
        trailing.push(json!({
            "id": format!("{id}.menu"),
            "component": "Menu",
            "props": {
                "label": copy("more"),
                "items": [
                    {"label": copy("menu.mergeNow"), "value": "merge-now"},
                    {"label": copy("menu.hold"), "value": "hold"},
                ],
            },
            "children": [{"id": format!("{id}.menu.button"), "component": "Button", "props": {"icon": "ellipsis", "variant": "ghost", "size": "icon-xs", "label": copy("more")}}],
            "on": {"select": {"event": {"name": EVENT_QUEUE_MENU, "context": {"pr": row.pull_request_id}}}},
        }));
    }
    let mut props = json!({"identifier": row.identifier, "title": row.title, "pressable": true});
    if let Some(subtitle) = &row.subtitle {
        props["subtitle"] = json!(subtitle);
    }
    if let Some(at) = row.at.as_deref().filter(|_| phone && row.action.is_none()) {
        props["meta"] = relative(at);
    }
    let mut slots = json!({"leading": glyph(&format!("{id}.glyph"), row.glyph, row.tone)});
    match trailing.len() {
        0 => {}
        1 => slots["trailing"] = trailing.remove(0),
        _ => slots["trailing"] = hstack(&format!("{id}.trailing"), trailing),
    }
    json!({"id": id, "component": "Row", "props": props, "slots": slots, "on": open_pr(&row.pull_request_id)})
}

fn queue_section(id: &str, title: &str, rows: &[CiQueueRow], phone: bool, fold: bool) -> Option<Value> {
    if rows.is_empty() {
        return None;
    }
    let mut props = json!({"title": title, "count": rows.len()});
    if fold {
        props["collapsible"] = json!(true);
        props["open"] = json!(rows.len() <= 5);
    }
    let row_id = model::test_id("queueRow");
    Some(json!({
        "id": id,
        "component": "Section",
        "props": props,
        "children": rows.iter().map(|row| queue_row(row, &format!("{row_id}.{}", row.id), phone)).collect::<Vec<_>>(),
    }))
}

fn queue_repo(repo: &CiQueueRepo, phone: bool, header: bool) -> Value {
    let copy = |key: &str| model::copy(&format!("queue.{key}"));
    let id = format!("{}.{}", model::test_id("queueRepo"), repo.repository_id);
    let mut children: Vec<Value> = Vec::new();
    if header {
        children.push(hstack(
            &format!("{id}.head"),
            vec![
                json!({"id": format!("{id}.title"), "component": "Heading", "props": {"text": repo.title, "level": "h3"}}),
                json!({"id": format!("{id}.caption"), "component": "Text", "props": {"text": repo.caption, "variant": "muted"}}),
            ],
        ));
    }
    if !phone {
        let stat = |key: &str, value: String| {
            json!({"id": format!("{id}.stat.{key}"), "component": "StatTile", "props": {"label": copy(&format!("stats.{key}")), "value": value}})
        };
        children.push(json!({
            "id": format!("{}.{}", model::test_id("queueStats"), repo.repository_id),
            "component": "Grid",
            "props": {"columns": {"base": 2, "md": 4}, "gap": "sm"},
            "children": [
                stat("inQueue", repo.stats.in_queue.to_string()),
                stat("landedToday", repo.stats.landed_today.to_string()),
                stat("sentBackToday", repo.stats.sent_back_today.to_string()),
                stat("typicalRun", repo.stats.typical_run.clone().unwrap_or_else(|| "—".to_string())),
            ],
        }));
    }
    let now_id = model::test_id("queueNow");
    for now in &repo.testing {
        children.push(now_card(now, &format!("{now_id}.{}", now.train_id)));
    }
    let up_next = queue_section(&format!("{id}.next"), copy("upNext"), &repo.up_next, phone, false);
    let waiting = queue_section(&format!("{id}.waiting"), copy("notReady"), &repo.waiting, phone, true);
    let sent_back = queue_section(&format!("{id}.sent"), copy("sentBack"), &repo.sent_back, phone, false);
    let landed = queue_section(&format!("{id}.landed"), copy("landed"), &repo.landed, phone, false);
    if phone {
        children.extend([up_next, sent_back, waiting, landed].into_iter().flatten());
    } else {
        let left: Vec<Value> = [up_next, waiting].into_iter().flatten().collect();
        let right: Vec<Value> = [sent_back, landed].into_iter().flatten().collect();
        if !left.is_empty() && !right.is_empty() {
            children.push(json!({
                "id": format!("{id}.columns"),
                "component": "Grid",
                "props": {"columns": {"base": 1, "lg": 2}, "gap": "lg"},
                "children": [vstack(&format!("{id}.left"), "lg", left), vstack(&format!("{id}.right"), "lg", right)],
            }));
        } else {
            children.extend(left);
            children.extend(right);
        }
    }
    vstack(&id, "lg", children)
}

fn queue_panel(input: &CiPageInput, phone: bool, repository_id: Option<&str>) -> Value {
    let id = model::test_id("queue");
    let repos: Vec<CiQueueRepo> =
        model::ci_queue(input).into_iter().filter(|repo| repository_id.is_none_or(|wanted| repo.repository_id == wanted)).collect();
    if repos.is_empty() {
        return json!({"id": id, "component": "EmptyState", "props": {"icon": "pr-batch", "title": model::copy("empty.train"), "size": "list"}});
    }
    vstack(id, "xl", repos.iter().map(|repo| queue_repo(repo, phone, repository_id.is_none())).collect())
}

// ── Runners ───────────────────────────────────────────────────────────────

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
                    "args": {"template": model::copy("runners.offlineSince"), "params": {"time": relative(since)}},
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

// ── The page ──────────────────────────────────────────────────────────────

/// The CI page's body (web's `ciPageTree`): the CURRENT tab's panel. The tab
/// switcher is the app's segmented capsule ([`crate::controls::page_tabs`],
/// #EXP-2169 "one look"), drawn by the view above this tree.
pub(crate) fn page_tree(input: &CiPageInput, state: &PageState) -> Value {
    let id = model::test_id;
    let empty = |key: &str| if filtered(&state.filter) { model::copy("empty.filtered") } else { model::copy(&format!("empty.{key}")) };
    match state.tab {
        CiTab::Train => queue_panel(input, state.phone, None),
        CiTab::Prs => vstack(
            &format!("{}.prs", id("page")),
            "md",
            vec![filters(input, state), subject_sections(id("prs"), &model::ci_pr_groups(input, &state.filter), state.phone, empty("prs"))],
        ),
        CiTab::Releases => vstack(
            &format!("{}.releases", id("page")),
            "md",
            vec![filters(input, state), run_sections(id("releases"), input, &state.filter, &[CiRunKind::Release], state.phone, empty("releases"))],
        ),
        CiTab::Runners => runners_panel(input),
    }
}

/// THE REPO PAGE'S RUNS TAB (#EXP-2214, web `ciRepoRunsTree`): a `Segmented`
/// of Merge queue · Pull requests · Releases (copy.tabs, phones
/// copy.phoneTabs; `ci-tab` carries the value), then that panel for ONE
/// repository: no repository header, no filter bar. `tab` Runners reads as
/// the merge queue.
pub(crate) fn repo_runs_tree(input: &CiPageInput, repository_id: &str, tab: CiTab, phone: bool) -> Value {
    let id = model::test_id;
    let tab = if tab == CiTab::Runners { CiTab::Train } else { tab };
    let filter = CiRunFilter { status: CiFilter::All, repository_id: Some(repository_id.to_string()), source: None };
    let panel = match tab {
        CiTab::Prs => subject_sections(id("prs"), &model::ci_pr_groups(input, &filter), phone, model::copy("empty.prs")),
        CiTab::Releases => run_sections(id("releases"), input, &filter, &[CiRunKind::Release], phone, model::copy("empty.releases")),
        _ => queue_panel(input, phone, Some(repository_id)),
    };
    let items: Vec<Value> = [CiTab::Train, CiTab::Prs, CiTab::Releases].iter().map(|t| json!({"label": t.label(phone), "value": t.id()})).collect();
    vstack(
        id("runs"),
        "md",
        vec![
            json!({
                "id": id("tabs"),
                "component": "Segmented",
                "props": {"value": tab.id(), "size": "sm", "items": items},
                "on": {"change": {"event": {"name": EVENT_TAB}}},
            }),
            panel,
        ],
    )
}

/// The tab switcher's entries: copy.tabs, each counted (`ciTabCounts`,
/// zero = none).
fn page_tabs(input: Option<&CiPageInput>) -> Vec<crate::controls::PageTab> {
    let counts = input.map(model::ci_tab_counts).unwrap_or_default();
    model::CI_TABS
        .into_iter()
        .map(|tab| crate::controls::PageTab {
            value: tab.id().to_string(),
            label: tab.label(false).to_string(),
            count: Some(counts.of(tab)).filter(|count| *count > 0),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The data
// ---------------------------------------------------------------------------

/// #EXP-2214: the synced repositories as the page reads them (the name and
/// the default branch: a push to it is a release run).
pub(crate) fn page_repos(cx: &App) -> Vec<CiPageRepo> {
    let Some(store) = Store::try_global(cx) else { return Vec::new() };
    store
        .collections()
        .repositories
        .read(cx)
        .iter()
        .map(|repo| CiPageRepo {
            id: repo.id.clone(),
            full_name: repo.full_name.clone(),
            default_branch: repo.default_branch_override.clone().or_else(|| repo.default_branch.clone()),
        })
        .collect()
}

/// A repository's full name when synced.
pub(crate) fn repo_full_name(repository_id: &str, cx: &mut App) -> Option<String> {
    Store::try_global(cx)?.collections().repository(repository_id, cx).map(|repo| repo.full_name)
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
        merge_train_id: run.merge_train_id.clone(),
    }
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    value.and_then(Value::as_array).map(|items| items.iter().filter_map(|item| item.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

/// A synced `pull_requests` row as the page's PR.
pub(crate) fn page_pr(pr: &domain::rows::PullRequestRow) -> CiPagePr {
    CiPagePr {
        id: pr.id.clone(),
        number: pr.number.unwrap_or_default(),
        title: pr.title.clone().unwrap_or_default(),
        repository_id: pr.repository_id.clone().unwrap_or_default(),
        root_session_id: pr.root_session_id.clone(),
        base: pr.base.clone().unwrap_or_default(),
        state: pr.state.clone().unwrap_or_default(),
        merge_state: pr.merge_state.clone(),
        merge_wait: pr.merge_wait.clone(),
        merge_hold: pr.merge_hold.clone(),
        merge_now: pr.merge_now.unwrap_or(false),
        merge_attempts: pr.merge_attempts.unwrap_or_default(),
        merge_position: pr.merge_position,
        merge_train_id: pr.merge_train_id.clone(),
    }
}

/// A synced `merge_trains` row as the page's train.
pub(crate) fn page_train(train: &domain::rows::MergeTrainRow) -> CiPageTrain {
    let nested = |value: Option<&Value>| -> Vec<Vec<String>> {
        value.and_then(Value::as_array).map(|units| units.iter().map(|unit| string_list(Some(unit))).collect()).unwrap_or_default()
    };
    CiPageTrain {
        id: train.id.clone(),
        repository_id: train.repository_id.clone(),
        base: train.base.clone(),
        base_sha: train.base_sha.clone(),
        units: nested(train.units.as_ref()),
        state: train.state.clone(),
        attempt: train.attempt.unwrap_or_default(),
        bisect_of: train.bisect_of.clone(),
        pending: train
            .pending
            .as_ref()
            .and_then(Value::as_array)
            .map(|halves| halves.iter().map(|half| nested(Some(half))).collect())
            .unwrap_or_default(),
        untested: train.untested.unwrap_or(false),
        tested_sha: train.tested_sha.clone(),
        created_at: train.created_at.as_deref().map(iso_instant).unwrap_or_default(),
        finished_at: train.finished_at.as_deref().map(iso_instant),
    }
}

/// A synced `merge_train_events` row as the page's history row.
pub(crate) fn page_train_event(event: &domain::rows::MergeTrainEventRow) -> CiPageTrainEvent {
    CiPageTrainEvent {
        id: event.id.clone(),
        repository_id: event.repository_id.clone(),
        pull_request_id: event.pull_request_id.clone(),
        merge_train_id: event.merge_train_id.clone(),
        kind: event.kind.clone(),
        base: event.base.clone(),
        attempt: event.attempt.unwrap_or_default(),
        reason: event.reason.clone(),
        checks: string_list(event.checks.as_ref()),
        ci_job_id: event.ci_job_id.clone(),
        job_url: event.job_url.clone(),
        files: string_list(event.files.as_ref()),
        sha: event.sha.clone(),
        by_user_id: event.by_user_id.clone(),
        created_at: event.created_at.as_deref().map(iso_instant).unwrap_or_default(),
    }
}

/// A synced `ci_jobs` row as the page's job.
pub(crate) fn page_job(job: &domain::rows::CiJobRow) -> CiPageJob {
    CiPageJob {
        id: job.id.clone(),
        run_id: job.run_id.clone(),
        check: job.check.clone(),
        status: job.status.clone(),
        conclusion: job.conclusion.clone(),
        runs_on: string_list(job.runs_on.as_ref()),
        runner_device_id: job.runner_device_id.clone(),
        provider: job.provider.clone().unwrap_or_else(|| "exponential".into()),
    }
}

/// Every synced row the page reads, as [`CiPageInput`] (`None` = no store).
pub(crate) fn live_input(cx: &App) -> Option<CiPageInput> {
    synced_input(page_repos(cx), cx)
}

/// [`live_input`] over the given `repos`.
pub(crate) fn synced_input(repos: Vec<CiPageRepo>, cx: &App) -> Option<CiPageInput> {
    let store = Store::try_global(cx)?;
    let collections = store.collections().clone();
    let viewer_id = crate::session_data::signed_in_user(cx).unwrap_or_default();
    let runs: Vec<CiPageRun> = collections.ci_runs.read(cx).iter().map(page_run).collect();
    let jobs: Vec<CiPageJob> = collections.ci_jobs.read(cx).iter().map(page_job).collect();
    let pull_requests: Vec<CiPagePr> = collections.pull_requests.read(cx).iter().map(page_pr).collect();
    let trains: Vec<CiPageTrain> = collections.merge_trains.read(cx).iter().map(page_train).collect();
    let train_events: Vec<CiPageTrainEvent> = collections.merge_train_events.read(cx).iter().map(page_train_event).collect();
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
    Some(CiPageInput { runs, jobs, repos, pull_requests, sessions, runners, users, trains, train_events, viewer_id, now, utc_offset_minutes })
}

/// The sidebar row's badge (nav-badges.json destination `ci`).
pub(crate) fn ci_badge(cx: &mut App) -> CountBadgeValue {
    synced_input(Vec::new(), cx).map(|input| model::ci_badge(&input)).unwrap_or_default()
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

    /// ci-page.json `scenario`, no server; on the Merge queue, DEV-ONLY
    /// `EXP_DEV_CI_TAB=prs|releases|runners` on that tab (their pictures).
    pub(crate) fn specimen(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self::build(Source::Specimen, window, cx);
        if let Some(tab) = std::env::var("EXP_DEV_CI_TAB").ok().as_deref().and_then(CiTab::parse) {
            this.state.tab = tab;
        }
        this
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
                subscriptions.push(cx.observe(&collections.repositories, |_, _, cx| cx.notify()));
                subscriptions.push(cx.observe(&collections.merge_trains, |_, _, cx| cx.notify()));
                subscriptions.push(cx.observe(&collections.merge_train_events, |_, _, cx| cx.notify()));
            }
        }
        Self { source, state: PageState::default(), view, last: Value::Null, scroll: gpui::ScrollHandle::new(), _subscriptions: subscriptions }
    }

    fn on_action(&mut self, event: ActionEvent, cx: &mut Context<Self>) {
        match event.name.as_str() {
            // eUI Select fires `change {open}` on open/close: only a value moves it.
            EVENT_STATUS => {
                if let Some(value) = event_value(&event) {
                    self.state.filter.status = CiFilter::parse(&value);
                }
            }
            EVENT_REPOSITORY => {
                if let Some(value) = event_value(&event) {
                    self.state.filter.repository_id = (value != "all").then_some(value);
                }
            }
            EVENT_OPEN_RUNNERS => self.state.tab = CiTab::Runners,
            EVENT_OPEN_RUNNER if self.source == Source::Live => {
                if let Some(device_id) = event.context["device"].as_str().map(str::to_string) {
                    crate::navigation::on_active_window(cx, move |window, cx| {
                        crate::navigation::navigate(window, cx, crate::navigation::Screen::Device { device_id });
                    });
                }
            }
            _ if self.source == Source::Live => {
                on_shared_action(&event, cx);
                return;
            }
            _ => return,
        }
        cx.notify();
    }

    /// The shots' tab (`ci_bench`).
    #[cfg(test)]
    pub(crate) fn set_tab(&mut self, tab: CiTab) {
        self.state.tab = tab;
    }

    fn input(&self, cx: &mut Context<Self>) -> Option<CiPageInput> {
        match self.source {
            Source::Specimen => Some(model::scenario()),
            Source::Live => live_input(cx),
        }
    }
}

/// A change's `value` (Select, Segmented, Menu).
pub(crate) fn event_value(event: &ActionEvent) -> Option<String> {
    event.payload.as_ref().and_then(|payload| payload["value"].as_str()).map(str::to_string)
}

/// The CI trees' shared events (the CI page and the repo page's Runs tab):
/// open a run (`?job=` our job), a URL, a pull request; the queue's Merge
/// now / Hold / Resume (`pullRequests.mergeNow` / `.hold`, a failure =
/// the toast). False = not one of them.
pub(crate) fn on_shared_action(event: &ActionEvent, cx: &mut App) -> bool {
    let context = |key: &str| event.context[key].as_str().map(str::to_string);
    match event.name.as_str() {
        EVENT_OPEN_RUN => {
            let run = context("run").or_else(|| event.payload.as_ref().and_then(|payload| payload["id"].as_str()).map(str::to_string));
            if let Some(run_id) = run {
                crate::navigation::on_active_window(cx, move |window, cx| {
                    crate::navigation::navigate(window, cx, crate::navigation::Screen::CiRun { run_id, job: None });
                });
            }
        }
        EVENT_OPEN_JOB => {
            if let (Some(run_id), Some(job)) = (context("run"), context("job")) {
                crate::navigation::on_active_window(cx, move |window, cx| {
                    crate::navigation::navigate(window, cx, crate::navigation::Screen::CiRun { run_id, job: Some(job) });
                });
            }
        }
        EVENT_OPEN_URL => {
            if let Some(url) = context("url") {
                cx.open_url(&url);
            }
        }
        EVENT_OPEN_PR => {
            if let Some(pr_id) = context("pr") {
                open_pull_request(&pr_id, cx);
            }
        }
        EVENT_QUEUE_MENU => {
            let Some(pr) = context("pr") else { return true };
            match event_value(event).as_deref() {
                Some("merge-now") => crate::pr_merge::run_train_op(pr, crate::pr_merge::TrainOp::MergeNow, cx),
                Some("hold") => crate::pr_merge::run_train_op(pr, crate::pr_merge::TrainOp::Hold(true), cx),
                _ => {}
            }
        }
        EVENT_RESUME => {
            if let Some(pr) = context("pr") {
                crate::pr_merge::run_train_op(pr, crate::pr_merge::TrainOp::Hold(false), cx);
            }
        }
        _ => return false,
    }
    true
}

/// A queue row's press: the PR's chat (its root session), else its page on
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
            input.trains.clear();
            input.train_events.clear();
        }
        input
    }

    /// #EXP-2172 / #EXP-2214: ci-page-view.json — the CI page node for node
    /// as web builds it (the four tabs, the repo page's Runs tab).
    #[test]
    fn the_ci_page_replays_ci_page_view() {
        let fixture: Value = serde_json::from_str(VIEW_FIXTURE).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        assert!(cases.iter().any(|case| case["repositoryId"].is_string()), "the repo page's Runs tab cases");
        for case in cases {
            let world = world(case["world"].as_str().unwrap());
            let phone = case["phone"].as_bool() == Some(true);
            // #EXP-2169: a `repositoryId` case = the repo page's Runs tab
            // (#EXP-2214: its inner tab, the merge queue first).
            if let Some(repository_id) = case["repositoryId"].as_str() {
                let tab = case["tab"].as_str().and_then(CiTab::parse).unwrap_or_default();
                let got = normalized(&repo_runs_tree(&world, repository_id, tab, phone));
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
