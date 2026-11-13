//! #EXP-1616 THE REPO PAGE (repo-page.json, its `$comment` is the ×4 spec):
//! Settings › Repositories › one repository. A genUI document: the `AppBar`
//! (the repo's name, its provider `Badge`, back to Repositories) over the
//! `Tabs{pill}` of `domain::repo_page::REPO_PAGE.tabs`:
//!
//! - Pull requests: `Section{tree}` of `repo_pr_tree` rows over the synced
//!   `pull_requests` (`Row{depth, identifier, title, subtitle}`, the checks
//!   `Chip{dot}` and the merge-train pill as its trailing meta);
//! - Runs: the `EmptyState` until eCode CI (E4) fills it;
//! - Branches: `repo_branches` rows (`repositories.branches` for eCode,
//!   `repositories.listBranches` names for GitHub);
//! - Settings: `SettingsRow`s (provider, default branch, clone URL,
//!   backups + Restore… behind an `AlertDialog`, mirror `Switch` + `Input`).
//!
//! The tree is rebuilt whenever its data moves (the voice pane's way); the
//! tab, the mirror draft, the switch and the alert stay bound at `/pane`.

use std::rc::Rc;

use gpui::{div, App, AppContext as _, ClipboardItem, Entity, IntoElement, ParentElement, Render, Styled, Subscription, Window};
use serde_json::{json, Value};
use sync::Store;

use eui_gpui::binding::Binding;
use eui_gpui::view::SurfaceView;

use domain::repo_page::{repo_branches, repo_pr_tree, RepoBranch, RepoPr, REPO_PAGE};

use crate::navigation::{active_team_id, Navigation};
use crate::queries;

use super::doc::{failed_state, mount_with, IdeToasts, Ink};
use super::repositories::{fetch_repositories, RepoRow};
use super::SettingsSection;

/// The dev drive's prefix: `name:<name>` = the repo whose full name ends in
/// `/<name>` (`EXP_DEV_SETTINGS=repo:<name>`).
pub(crate) const BY_NAME: &str = "name:";

/// What the page fetched (`None` = still loading).
#[derive(Clone, Default)]
struct Fetched {
    /// The resolved repository (`Err` = the list failed or no such repo).
    repo: Option<Result<RepoRow, String>>,
    branches: Option<Result<Vec<RepoBranch>, String>>,
    /// eCode only (`repositories.settings`).
    settings: Option<Result<api::repositories::RepoSettings, String>>,
}

/// A pinned state for the headless bench.
#[cfg(test)]
pub(crate) struct RepoPageShot {
    pub repo: Value,
    pub owner: bool,
    pub tab: &'static str,
    pub pulls: Vec<domain::rows::PullRequestRow>,
    pub branches: Option<Vec<(&'static str, &'static str)>>,
    pub settings: Option<Value>,
}

pub struct RepoPage {
    nav: Entity<Navigation>,
    /// The repository id, or [`BY_NAME`]`<name>`.
    key: String,
    fetched: Fetched,
    started: bool,
    /// The data the current tree was built from.
    built: Option<Value>,
    #[cfg(test)]
    shot: Option<(bool, Vec<domain::rows::PullRequestRow>)>,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    _subscriptions: Vec<Subscription>,
}

const PATH: &str = "/pane";

impl RepoPage {
    pub fn new(nav: Entity<Navigation>, key: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let pulls = Store::global(cx).collections().pull_requests.clone();
        let subscriptions = vec![cx.observe(&pulls, |_, _, cx| cx.notify()), cx.observe(&nav, |_, _, cx| cx.notify())];
        let page = cx.weak_entity();
        let handle = window.window_handle();
        let tab = domain::repo_page::repo_tab(None);
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, json!({"tab": tab, "mirrorUrl": "", "mirrorEnabled": false, "restoreOpen": false}), |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for name in ["back", "retry", "tab", "pr", "copy", "branch", "restore", "restore_confirm", "restore_cancel", "mirror_edit", "mirror_save", "mirror_toggle"] {
                let page = page.clone();
                binding.on_action(name, move |_, event, cx| {
                    let key = event.context.get("key").and_then(Value::as_str).unwrap_or_default().to_string();
                    let payload = event.payload.clone().unwrap_or(Value::Null);
                    let page = page.clone();
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = page.update(cx, |page, cx| page.action(name, &key, &payload, window, cx));
                        });
                    });
                    None
                });
            }
            binding
        });
        let view = mount_with("settings-repo", document(&Ink::of(cx), &json!({"phase": "loading"})), &binding, Rc::new(super::doc::ide_plugin()), true, window, cx);
        Self { nav, key, fetched: Fetched::default(), started: false, built: None, #[cfg(test)] shot: None, binding, view, _subscriptions: subscriptions }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new(crate::navigation::nav_for_window(window, cx), "r1".into(), window, cx)
    }

    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: RepoPageShot, cx: &mut gpui::Context<Self>) {
        self.started = true;
        self.fetched.repo = Some(Ok(serde_json::from_value(shot.repo).expect("a repository row")));
        self.fetched.branches = shot.branches.map(|list| Ok(list.into_iter().map(|(name, sha)| RepoBranch { name: name.into(), sha: sha.into() }).collect()));
        self.fetched.settings = shot.settings.map(|settings| Ok(serde_json::from_value(settings).expect("repository settings")));
        if let Some(Ok(settings)) = &self.fetched.settings {
            let (url, enabled) = (settings.mirror_url.clone().unwrap_or_default(), settings.mirror_enabled);
            self.binding.update(cx, |binding, cx| binding.update_state(|state| {
                state["mirrorUrl"] = json!(url);
                state["mirrorEnabled"] = json!(enabled);
            }, cx));
        }
        let tab = shot.tab;
        self.binding.update(cx, |binding, cx| binding.update_state(|state| state["tab"] = json!(tab), cx));
        self.shot = Some((shot.owner, shot.pulls));
        cx.notify();
    }

    fn pinned(&self) -> bool {
        #[cfg(test)]
        if self.shot.is_some() {
            return true;
        }
        false
    }

    fn owner(&self, cx: &App) -> bool {
        #[cfg(test)]
        if let Some((owner, _)) = &self.shot {
            return *owner;
        }
        active_team_id(&self.nav, cx).is_some_and(|team| super::is_owner(cx, &team))
    }

    fn pull_rows(&self, repository_id: &str, cx: &App) -> Vec<domain::rows::PullRequestRow> {
        #[cfg(test)]
        if let Some((_, pulls)) = &self.shot {
            return pulls.iter().filter(|pull| pull.repository_id.as_deref() == Some(repository_id)).cloned().collect();
        }
        Store::global(cx).collections().pull_requests.read(cx).iter().filter(|pull| pull.repository_id.as_deref() == Some(repository_id)).cloned().collect()
    }

    fn repo(&self) -> Option<&RepoRow> {
        self.fetched.repo.as_ref().and_then(|repo| repo.as_ref().ok())
    }

    /// The first load: resolve the repository off `repositories.list`, then
    /// its branches (and an eCode repo's settings).
    fn ensure_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        if self.started || self.pinned() {
            return;
        }
        let (Some(team_id), Some(trpc)) = (active_team_id(&self.nav, cx), queries::trpc_client(cx)) else { return };
        self.started = true;
        let key = self.key.clone();
        cx.spawn(async move |this, cx| {
            let fetched = cx
                .background_executor()
                .spawn(async move {
                    let repo = fetch_repositories(&trpc, &team_id).map_err(|err| err.user_message()).and_then(|rows| {
                        rows.into_iter()
                            .find(|row| match key.strip_prefix(BY_NAME) {
                                Some(name) => row.full_name.ends_with(&format!("/{name}")),
                                None => row.id == key,
                            })
                            .ok_or_else(|| "This repository is gone.".to_string())
                    });
                    let mut fetched = Fetched { repo: Some(repo.clone()), ..Default::default() };
                    if let Ok(repo) = repo {
                        fetched.branches = Some(load_branches(&trpc, &repo));
                        if repo.provider() == "ecode" {
                            fetched.settings = Some(api::repositories::settings(&trpc, &repo.id).map_err(|err| err.user_message()));
                        }
                    }
                    fetched
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(Ok(settings)) = &fetched.settings {
                    let (url, enabled) = (settings.mirror_url.clone().unwrap_or_default(), settings.mirror_enabled);
                    this.binding.update(cx, |binding, cx| binding.update_state(|state| {
                        state["mirrorUrl"] = json!(url);
                        state["mirrorEnabled"] = json!(enabled);
                    }, cx));
                }
                this.fetched = fetched;
                cx.notify();
            });
        })
        .detach();
    }

    /// Re-read the branches and settings after a write.
    fn reload(&mut self, cx: &mut gpui::Context<Self>) {
        let (Some(repo), Some(trpc)) = (self.repo().cloned(), queries::trpc_client(cx)) else { return };
        cx.spawn(async move |this, cx| {
            let (branches, settings) = cx
                .background_executor()
                .spawn(async move {
                    let settings = (repo.provider() == "ecode").then(|| api::repositories::settings(&trpc, &repo.id).map_err(|err| err.user_message()));
                    (load_branches(&trpc, &repo), settings)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.fetched.branches = Some(branches);
                if settings.is_some() {
                    this.fetched.settings = settings;
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn action(&mut self, name: &str, key: &str, payload: &Value, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let value = payload.get("value").and_then(Value::as_str).map(str::to_string);
        match name {
            "back" => crate::sidebar::select_settings_section(window, cx, SettingsSection::Repositories),
            "retry" => {
                self.started = false;
                self.fetched = Fetched::default();
                cx.notify();
            }
            "tab" => {
                let tab = domain::repo_page::repo_tab(value.as_deref());
                self.binding.update(cx, |binding, cx| binding.update_state(|state| state["tab"] = json!(tab), cx));
            }
            "pr" => {
                let url = self.repo().map(|repo| self.pull_rows(&repo.id, cx)).unwrap_or_default().into_iter().find(|pull| pull.id == key).and_then(|pull| pull.url);
                if let Some(url) = url {
                    super::open_url(cx, url);
                }
            }
            "copy" => {
                if let Some(url) = self.clone_url() {
                    cx.write_to_clipboard(ClipboardItem::new_string(url));
                }
            }
            "branch" => {
                let (Some(repo), Some(branch)) = (self.repo().cloned(), value) else { return };
                if branch == repo.default_branch {
                    return;
                }
                self.write("set", "the default branch", cx, move |trpc| api::repositories::set_default_branch(trpc, &repo.id, Some(&branch)));
            }
            "restore" => {
                let backup = key.to_string();
                self.binding.update(cx, |binding, cx| binding.update_state(|state| {
                    state["restoreOpen"] = json!(true);
                    state["restoreId"] = json!(backup);
                }, cx));
            }
            "restore_cancel" => self.binding.update(cx, |binding, cx| binding.update_state(|state| state["restoreOpen"] = json!(false), cx)),
            "restore_confirm" => {
                let backup = self.binding.read(cx).state()["restoreId"].as_str().unwrap_or_default().to_string();
                self.binding.update(cx, |binding, cx| binding.update_state(|state| state["restoreOpen"] = json!(false), cx));
                let Some(repo) = self.repo().cloned() else { return };
                if backup.is_empty() {
                    return;
                }
                self.write("restore", "the backup", cx, move |trpc| api::repositories::restore(trpc, &repo.id, &backup));
            }
            "mirror_edit" => {
                let url = value.unwrap_or_default();
                self.binding.update(cx, |binding, cx| binding.update_state(|state| state["mirrorUrl"] = json!(url), cx));
            }
            "mirror_toggle" | "mirror_save" => {
                if name == "mirror_toggle" {
                    let checked = payload.get("checked").and_then(Value::as_bool).unwrap_or(false);
                    self.binding.update(cx, |binding, cx| binding.update_state(|state| state["mirrorEnabled"] = json!(checked), cx));
                }
                if name == "mirror_save" {
                    if let Some(url) = value {
                        self.binding.update(cx, |binding, cx| binding.update_state(|state| state["mirrorUrl"] = json!(url), cx));
                    }
                }
                let Some(repo) = self.repo().cloned() else { return };
                let state = self.binding.read(cx).state();
                let url = state["mirrorUrl"].as_str().unwrap_or_default().trim().to_string();
                let enabled = state["mirrorEnabled"].as_bool().unwrap_or(false);
                self.write("save", "the mirror", cx, move |trpc| api::repositories::set_mirror(trpc, &repo.id, (!url.is_empty()).then_some(url.as_str()), enabled));
            }
            _ => {}
        }
    }

    /// One owner write off the executor: a toast on failure, a reload after.
    fn write(&mut self, verb: &'static str, thing: &'static str, cx: &mut gpui::Context<Self>, run: impl FnOnce(&api::TrpcClient) -> Result<(), api::ApiError> + Send + 'static) {
        let Some(trpc) = queries::trpc_client(cx) else { return };
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { run(&trpc) }).await;
            let _ = this.update(cx, |this, cx| {
                if let Err(err) = &result {
                    crate::action_error::failed_action_err(verb, thing, err, cx);
                }
                this.reload(cx);
            });
        })
        .detach();
    }

    /// `exp://<team>/<name>` (the server's remote URL when it sent one).
    fn clone_url(&self) -> Option<String> {
        let repo = self.repo()?;
        (repo.provider() == "ecode").then(|| {
            self.fetched
                .settings
                .as_ref()
                .and_then(|settings| settings.as_ref().ok())
                .and_then(|settings| settings.remote_url.clone())
                .filter(|url| !url.is_empty())
                .unwrap_or_else(|| format!("exp://{}", repo.full_name))
        })
    }

    /// The data the tree is built from.
    fn data(&self, cx: &App) -> Value {
        let repo = match &self.fetched.repo {
            None => return json!({"phase": "loading"}),
            Some(Err(_)) => return json!({"phase": "failed"}),
            Some(Ok(repo)) => repo,
        };
        let copy = &REPO_PAGE.copy;
        let ecode = repo.provider() == "ecode";
        let owner = self.owner(cx);
        let rows = self.pull_rows(&repo.id, cx);
        let prs: Vec<RepoPr> = rows.iter().map(RepoPr::from_row).collect();
        let pulls: Vec<Value> = repo_pr_tree(&prs)
            .into_iter()
            .map(|row| {
                let pill = rows.iter().find(|pull| pull.id == row.id).map(domain::repo_page::merge_pill_label).unwrap_or_default();
                json!({"key": row.id, "identifier": row.identifier, "title": row.title, "subtitle": row.subtitle, "depth": row.depth, "checks": row.checks, "pill": pill})
            })
            .collect();
        let default_branch = self
            .fetched
            .settings
            .as_ref()
            .and_then(|settings| settings.as_ref().ok())
            .map(|settings| settings.default_branch.clone())
            .filter(|branch| !branch.is_empty())
            .unwrap_or_else(|| repo.default_branch.clone());
        let branches = match &self.fetched.branches {
            None => Value::Null,
            Some(Err(_)) => json!([]),
            Some(Ok(list)) => repo_branches(list, &default_branch, &prs)
                .into_iter()
                .map(|row| json!({"name": row.name, "sha": row.sha, "isDefault": row.is_default, "pr": row.pr_number.map(|n| format!("#{n}"))}))
                .collect(),
        };
        let mut names: Vec<String> = match &self.fetched.branches {
            Some(Ok(list)) => list.iter().map(|branch| branch.name.clone()).collect(),
            _ => Vec::new(),
        };
        if !names.contains(&default_branch) {
            names.insert(0, default_branch.clone());
        }
        let settings = self.fetched.settings.as_ref().and_then(|settings| settings.as_ref().ok());
        let mut backups: Vec<&api::repositories::RepoBackup> = settings.map(|settings| settings.backups.iter().collect()).unwrap_or_default();
        backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        let backups: Vec<Value> = backups
            .into_iter()
            .map(|backup| {
                let refs = if backup.refs == 1 { "1 ref".to_string() } else { format!("{} refs", backup.refs) };
                json!({"key": backup.id, "title": backup.created_at.get(..16).unwrap_or(&backup.created_at).replace('T', " "), "subtitle": format!("{refs} \u{b7} {}", crate::issue_files::format_bytes(backup.bytes))})
            })
            .collect();
        json!({
            "phase": "ready",
            "name": repo.full_name.rsplit('/').next().unwrap_or(&repo.full_name),
            "provider": copy.provider.label(repo.provider()),
            "ecode": ecode,
            "owner": owner,
            "pulls": pulls,
            "branches": branches,
            "defaultBranch": default_branch,
            "branchNames": names,
            "cloneUrl": self.clone_url(),
            "backups": backups,
        })
    }
}

/// An eCode repo's heads (`repositories.branches`); a GitHub repo's names
/// (`repositories.listBranches`, no sha).
fn load_branches(trpc: &api::TrpcClient, repo: &RepoRow) -> Result<Vec<RepoBranch>, String> {
    if repo.provider() == "ecode" {
        api::repositories::branch_heads(trpc, &repo.id)
            .map(|heads| heads.into_iter().map(|head| RepoBranch { name: head.name, sha: head.sha }).collect())
            .map_err(|err| err.user_message())
    } else {
        api::repositories::list_branches(trpc, &repo.id)
            .map(|out| out.branches.into_iter().map(|name| RepoBranch { name, sha: String::new() }).collect())
            .map_err(|err| err.user_message())
    }
}

impl Render for RepoPage {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "RepoPage").entered();
        self.ensure_loaded(cx);
        let data = self.data(cx);
        if self.built.as_ref() != Some(&data) {
            let tree: eui_core::NestedNode = serde_json::from_value(document(&Ink::of(cx), &data)).expect("the repo page parses");
            self.view.update(cx, |view, cx| {
                view.set_nested(tree, cx);
            });
            self.built = Some(data);
        }
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// The checks `Chip{dot}` of `state` (`None` = no chip).
fn checks_chip(id: &str, state: Option<&str>) -> Option<Value> {
    let state = state?;
    let chip = REPO_PAGE.copy.checks.of(state)?;
    let dot = match chip.tone.as_str() {
        "green" => "$color.success",
        "amber" => "$color.warning",
        _ => "$color.destructive",
    };
    Some(json!({"id": id, "component": "Chip", "props": {"label": chip.label, "dot": dot}, "style": {"flexShrink": 0, "alignSelf": "center"}}))
}

fn empty(id: &str, icon: &str, title: &str) -> Value {
    json!({"id": id, "component": "EmptyState", "props": {"title": title, "icon": icon, "size": "list"}})
}

fn column(id: &str, children: Vec<Value>) -> Value {
    json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": 12, "width": "100%", "minWidth": 0, "paddingTop": 12}, "children": children})
}

/// The Pull requests tab: the PR tree, or its empty state.
fn pulls_tab(data: &Value) -> Value {
    let rows = data["pulls"].as_array().cloned().unwrap_or_default();
    if rows.is_empty() {
        return column("pulls", vec![empty("pulls_empty", "pr-open", &REPO_PAGE.copy.pulls.empty)]);
    }
    let rows: Vec<Value> = rows
        .iter()
        .enumerate()
        .map(|(ix, row)| {
            let id = format!("pr_{ix}");
            let mut meta: Vec<Value> = checks_chip(&format!("{id}_checks"), row["checks"].as_str()).into_iter().collect();
            if let Some(pill) = row["pill"].as_str().filter(|pill| !pill.is_empty()) {
                meta.push(json!({"id": format!("{id}_pill"), "component": "Badge", "props": {"text": pill, "variant": "outline"}, "style": {"flexShrink": 0, "alignSelf": "center"}}));
            }
            json!({"id": id, "component": "Row",
                "props": {"title": row["title"], "identifier": row["identifier"], "subtitle": row["subtitle"], "depth": row["depth"], "pressable": true},
                "on": {"press": {"event": {"name": "pr", "context": {"key": row["key"]}}}},
                "slots": {"trailing": {"id": format!("{id}_meta"), "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": 6, "flexShrink": 0}, "children": meta}}})
        })
        .collect();
    column("pulls", vec![json!({"id": "pulls_tree", "component": "Section", "props": {"tree": true}, "children": rows})])
}

/// The Branches tab: the skeleton, the empty state or the rows.
fn branches_tab(data: &Value) -> Value {
    let Some(rows) = data["branches"].as_array() else {
        return column("branches", vec![json!({"id": "branches_skeleton", "component": "Skeleton", "props": {"rows": crate::feedback::SKELETON_ROWS}})]);
    };
    if rows.is_empty() {
        return column("branches", vec![empty("branches_empty", "ui-branch", &REPO_PAGE.copy.branches.empty)]);
    }
    let rows: Vec<Value> = rows
        .iter()
        .enumerate()
        .map(|(ix, row)| {
            let id = format!("branch_{ix}");
            let mut props = json!({"title": row["name"]});
            if row["sha"].as_str().is_some_and(|sha| !sha.is_empty()) {
                props["subtitle"] = row["sha"].clone();
            }
            if let Some(pr) = row["pr"].as_str() {
                props["meta"] = json!(pr);
            }
            let mut node = json!({"id": id, "component": "Row", "props": props});
            if row["isDefault"].as_bool() == Some(true) {
                node["slots"] = json!({"trailing": {"id": format!("{id}_default"), "component": "Badge", "props": {"text": REPO_PAGE.copy.branches.default, "variant": "secondary"}, "style": {"alignSelf": "center"}}});
            }
            node
        })
        .collect();
    column("branches", vec![json!({"id": "branches_list", "component": "Section", "children": rows})])
}

/// A `SettingsRow` over `children`.
fn setting(id: &str, label: &str, children: Vec<Value>) -> Value {
    json!({"id": id, "component": "SettingsRow", "props": {"label": label}, "children": children})
}

/// The Settings tab: provider, default branch, then an eCode repo's clone
/// URL, backups and mirror.
fn settings_tab(data: &Value) -> Value {
    let copy = &REPO_PAGE.copy.settings;
    let owner = data["owner"].as_bool() == Some(true);
    let mut rows = vec![setting("set_provider", &copy.provider, vec![json!({"id": "set_provider_badge", "component": "Badge", "props": {"text": data["provider"], "variant": "secondary"}})])];
    let options: Vec<Value> = data["branchNames"].as_array().into_iter().flatten().map(|name| json!({"label": name, "value": name})).collect();
    rows.push(setting("set_branch", &copy.default_branch, vec![json!({"id": "set_branch_select", "component": "Select",
        "props": {"name": "defaultBranch", "label": copy.default_branch, "value": data["defaultBranch"], "options": options, "disabled": !owner, "triggerVariant": "pill"},
        "on": {"change": {"event": {"name": "branch"}}}})]));
    if data["ecode"].as_bool() == Some(true) {
        rows.push(setting("set_clone", &copy.clone_url, vec![json!({"id": "set_clone_row", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": 8, "minWidth": 0},
            "children": [
                {"id": "set_clone_url", "component": "Text", "props": {"text": data["cloneUrl"], "variant": "code", "lines": 1}},
                {"id": "set_clone_copy", "component": "Button", "props": {"icon": "ui-copy", "variant": "ghost", "size": "icon", "label": copy.clone_url}, "on": {"press": {"event": {"name": "copy"}}}},
            ]})]));
        let backups = data["backups"].as_array().cloned().unwrap_or_default();
        let backup_rows: Vec<Value> = if backups.is_empty() {
            vec![json!({"id": "set_backups_none", "component": "Text", "props": {"text": copy.no_backups, "variant": "muted"}})]
        } else {
            let rows: Vec<Value> = backups
                .iter()
                .enumerate()
                .map(|(ix, backup)| {
                    let id = format!("backup_{ix}");
                    let mut node = json!({"id": id, "component": "Row", "props": {"title": backup["title"], "subtitle": backup["subtitle"]}});
                    if owner {
                        node["slots"] = json!({"trailing": {"id": format!("{id}_restore"), "component": "Button", "props": {"label": copy.restore, "variant": "outline", "size": "sm"},
                            "on": {"press": {"event": {"name": "restore", "context": {"key": backup["key"]}}}}}});
                    }
                    node
                })
                .collect();
            vec![json!({"id": "set_backups_list", "component": "Section", "style": {"width": "100%"}, "children": rows})]
        };
        rows.push(json!({"id": "set_backups", "component": "SettingsRow", "props": {"label": copy.backups, "stackBelow": "xl"}, "children": backup_rows}));
        rows.push(setting("set_mirror", &copy.mirror, vec![json!({"id": "set_mirror_switch", "component": "Switch", "props": {"label": "", "name": "mirror", "checked": {"@path": "/pane/mirrorEnabled"}, "disabled": !owner},
            "on": {"change": {"event": {"name": "mirror_toggle"}}}})]));
        rows.push(setting("set_mirror_url", &copy.mirror_url, vec![json!({"id": "set_mirror_input", "component": "Input", "props": {"label": copy.mirror_url, "name": "mirrorUrl", "placeholder": "https://github.com/acme/repo.git", "value": {"@path": "/pane/mirrorUrl"}, "disabled": !owner, "size": "sm", "variant": "box"},
            "style": {"width": 320, "maxWidth": "100%"},
            "on": {"change": {"event": {"name": "mirror_edit"}}, "submit": {"event": {"name": "mirror_save"}}}})]));
    }
    column("settings", rows)
}

/// The page's tree for `data` (`phase`: loading | failed | ready).
fn document(ink: &Ink, data: &Value) -> Value {
    let root = |children: Vec<Value>| json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "gap": 8}, "children": children});
    match data["phase"].as_str() {
        Some("ready") => {}
        Some("failed") => return root(vec![failed_state(ink, "failed", "settings-repositories", &REPO_PAGE.copy.pulls.empty, json!(true))]),
        _ => return root(vec![json!({"id": "skeleton", "component": "Skeleton", "props": {"rows": crate::feedback::SKELETON_ROWS}})]),
    }
    let bar = json!({"id": "bar", "component": "AppBar", "props": {"title": data["name"], "back": true}, "on": {"back": {"event": {"name": "back"}}},
        "slots": {"trailing": {"id": "bar_provider", "component": "Badge", "props": {"text": data["provider"], "variant": "secondary"}, "style": {"alignSelf": "center"}}}});
    let tabs: Vec<Value> = REPO_PAGE.tabs.iter().map(|tab| json!({"label": tab.label, "value": tab.id})).collect();
    let panels: Vec<Value> = REPO_PAGE
        .tabs
        .iter()
        .map(|tab| match tab.id.as_str() {
            "pulls" => pulls_tab(data),
            "branches" => branches_tab(data),
            "settings" => settings_tab(data),
            _ => column("runs", vec![empty("runs_empty", "ci-queued", &REPO_PAGE.copy.runs.empty)]),
        })
        .collect();
    let tabs = json!({"id": "tabs", "component": "Tabs", "props": {"tabs": tabs, "value": {"@path": "/pane/tab"}, "variant": "pill"}, "on": {"change": {"event": {"name": "tab"}}}, "children": panels});
    let restore = json!({"id": "restore_alert", "component": "AlertDialog",
        "props": {"open": {"@path": "/pane/restoreOpen"}, "title": REPO_PAGE.copy.settings.restore_title, "confirmLabel": REPO_PAGE.copy.settings.restore_confirm, "destructive": true},
        "on": {"confirm": {"event": {"name": "restore_confirm"}}, "cancel": {"event": {"name": "restore_cancel"}}}});
    root(vec![bar, tabs, restore])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready() -> Value {
        json!({"phase": "ready", "name": "web", "provider": "Exponential", "ecode": true, "owner": true,
            "pulls": [{"key": "p1", "identifier": "#2", "title": "Root", "subtitle": "exp/a → main", "depth": 0, "checks": "success", "pill": "Checks running"},
                      {"key": "p2", "identifier": "#3", "title": "Child", "subtitle": "exp/b → exp/a", "depth": 1, "checks": null, "pill": "Draft"}],
            "branches": [{"name": "main", "sha": "0123456", "isDefault": true, "pr": null}],
            "defaultBranch": "main", "branchNames": ["main"], "cloneUrl": "exp://acme/web",
            "backups": [{"key": "b1", "title": "2026-11-12 10:00", "subtitle": "3 refs · 4 KB"}]})
    }

    #[test]
    fn the_document_raises_no_unexpected_issue() {
        let ink = super::super::readiness::tests::test_ink();
        let mut bare = ready();
        bare["pulls"] = json!([]);
        bare["branches"] = json!([]);
        bare["backups"] = json!([]);
        bare["ecode"] = json!(false);
        let mut skeleton = ready();
        skeleton["branches"] = Value::Null;
        for data in [ready(), bare, skeleton, json!({"phase": "loading"}), json!({"phase": "failed"})] {
            let tree = document(&ink, &data);
            let bound = super::super::doc::bound_values(&tree);
            let mut surface = eui_core::surface::Surface::new("settings-repo", Default::default());
            let outcome = surface.set_nested(serde_json::from_value(tree).expect("parses"));
            let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
            assert!(issues.is_empty(), "{issues:?}");
        }
    }
}
