//! #EXP-1616 THE REPO PAGE (repo-page.json, its `$comment` is the ×4 spec):
//! Settings › Repositories › one repository. A genUI document: the `AppBar`
//! (the repo's name, its provider `Badge`, back to Repositories) over the
//! `Tabs{pill}` of `domain::repo_page::REPO_PAGE.tabs`:
//!
//! - Pull requests: `Section{tree}` of `repo_pr_tree` rows over the synced
//!   `pull_requests` (`Row{depth, identifier, title, subtitle}`, the checks
//!   `Chip{dot}` and the merge-train pill as its trailing meta);
//! - Runs (#EXP-1617 E4, repo-ci.json): a `Table` of `repo_ci::run_rows`
//!   over the synced `ci_runs` / `ci_jobs` (the glyph `Icon` tinted by its
//!   tone), a row opens the CI run page; none = the `EmptyState`;
//! - Branches: `repo_branches` rows (`repositories.branches` for eCode,
//!   `repositories.listBranches` names for GitHub);
//! - Settings: `SettingsRow`s (provider, default branch, clone URL,
//!   backups + Restore… behind an `AlertDialog`, mirror `Switch` + `Input`),
//!   then the `CI` `Section` (repo-ci.json): the pipelines' `Row`s with the
//!   checks `Chip{dot}`, the required checks `ChipInput`, the secrets `Table`
//!   (delete behind an `AlertDialog`, Add secret = a `Dialog` with a password
//!   `Input`) and the GitHub-only `Switch`es. Only the repo's managers (its
//!   sharer or a team owner) change anything (`ci.repoSettings`,
//!   `ci.setRepoConfig`, `ci.setSecret`, `ci.deleteSecret`).
//!
//! The tree is rebuilt whenever its data moves (the voice pane's way); the
//! tab, the mirror draft, the switch and the alert stay bound at `/pane`.

use std::rc::Rc;

use gpui::{div, App, AppContext as _, ClipboardItem, Entity, IntoElement, ParentElement, Render, Styled, Subscription, Window};
use serde_json::{json, Value};
use sync::Store;

use eui_gpui::binding::Binding;
use eui_gpui::view::SurfaceView;

use domain::repo_ci;
use domain::repo_page::{repo_branches, repo_pr_tree, RepoBranch, RepoPr, REPO_PAGE};
use domain::rows::{CiJobRow, CiRunRow, PullRequestRow};

use crate::navigation::{active_team_id, Navigation};
use crate::queries;

use super::doc::{failed_state, mount_with, IdeToasts, Ink};
use super::repositories::{fetch_repositories, RepoRow};
use super::SettingsSection;

/// The dev drive's prefix: `name:<name>` = the repo whose full name ends in
/// `/<name>` (`EXP_DEV_SETTINGS=repo:<name>`; `repo:<name>:<tab>` opens
/// that tab, the view-catalog's `repo-runs` / `repo-ci` drives).
pub(crate) const BY_NAME: &str = "name:";

/// What the page fetched (`None` = still loading).
#[derive(Clone, Default)]
struct Fetched {
    /// The resolved repository (`Err` = the list failed or no such repo).
    repo: Option<Result<RepoRow, String>>,
    branches: Option<Result<Vec<RepoBranch>, String>>,
    /// eCode only (`repositories.settings`).
    settings: Option<Result<api::repositories::RepoSettings, String>>,
    /// #EXP-1617: `ci.repoSettings` (the CI config and the secret names).
    ci: Option<Result<api::ci::RepoCiSettings, String>>,
}

/// A pinned state (the headless bench, the `repo-runs` / `repo-ci`
/// specimens).
pub(crate) struct RepoPageShot {
    pub repo: Value,
    pub owner: bool,
    pub tab: &'static str,
    pub pulls: Vec<PullRequestRow>,
    pub branches: Option<Vec<(&'static str, &'static str)>>,
    pub settings: Option<Value>,
    pub runs: Vec<CiRunRow>,
    pub jobs: Vec<CiJobRow>,
    /// `ci.repoSettings`'s answer.
    pub ci: Option<Value>,
    /// The clock of a running run's duration (`None` = the real one).
    pub now_ms: Option<i64>,
}

/// The rows a pinned page shows instead of the synced ones.
struct Pinned {
    owner: bool,
    pulls: Vec<PullRequestRow>,
    runs: Vec<CiRunRow>,
    jobs: Vec<CiJobRow>,
    now_ms: Option<i64>,
}

pub struct RepoPage {
    nav: Entity<Navigation>,
    /// The repository id, or [`BY_NAME`]`<name>`.
    key: String,
    fetched: Fetched,
    started: bool,
    /// The data the current tree was built from.
    built: Option<Value>,
    shot: Option<Pinned>,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    _subscriptions: Vec<Subscription>,
}

const PATH: &str = "/pane";

impl RepoPage {
    pub fn new(nav: Entity<Navigation>, key: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let mut subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        if let Some(store) = Store::try_global(cx) {
            let collections = store.collections().clone();
            subscriptions.push(cx.observe(&collections.pull_requests, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&collections.ci_runs, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&collections.ci_jobs, |_, _, cx| cx.notify()));
        }
        let page = cx.weak_entity();
        let handle = window.window_handle();
        // `name:<name>:<tab>` (the dev drive) opens on that tab.
        let (key, tab) = match key.strip_prefix(BY_NAME).and_then(|rest| rest.split_once(':')) {
            Some((name, tab)) => (format!("{BY_NAME}{name}"), domain::repo_page::repo_tab(Some(tab))),
            None => (key, domain::repo_page::repo_tab(None)),
        };
        let binding = cx.new(|_| {
            let mut binding = Binding::new(
                PATH,
                json!({"tab": tab, "mirrorUrl": "", "mirrorEnabled": false, "restoreOpen": false,
                    "required": [], "githubStatuses": false, "actionsRunners": false,
                    "secretOpen": false, "secretName": "", "secretValue": "", "secretInvalid": false, "deleteOpen": false, "deleteName": ""}),
                |state: &Value, _| state.clone(),
            );
            binding.set_toasts(Rc::new(IdeToasts));
            for name in [
                "back", "retry", "tab", "pr", "copy", "branch", "restore", "restore_confirm", "restore_cancel", "mirror_edit", "mirror_save", "mirror_toggle",
                "run", "required", "ci_switch", "secret_add", "secret_edit", "secret_save", "secret_cancel", "secret_dialog", "secret_delete", "delete_confirm", "delete_cancel",
            ] {
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
        Self { nav, key, fetched: Fetched::default(), started: false, built: None, shot: None, binding, view, _subscriptions: subscriptions }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new(crate::navigation::nav_for_window(window, cx), "r1".into(), window, cx)
    }

    /// The `repo-runs` / `repo-ci` specimens: `acme/engine` over
    /// repo-ci.json's runs, on `tab`.
    pub(crate) fn specimen(tab: &'static str, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let mut page = Self::new(crate::navigation::nav_for_window(window, cx), "r1".into(), window, cx);
        let case = &repo_ci::REPO_CI["runRows"][0];
        let mut runs = repo_ci::fixture_runs(&case["runs"]);
        for run in &mut runs {
            run.repository_id = Some("r1".into());
        }
        let mut jobs = repo_ci::fixture_jobs(&case["jobs"]);
        for (index, (job, check)) in jobs.iter_mut().zip(["test (linux)", "build", "lint", "build", "build", "build"]).enumerate() {
            job.id = format!("j{index}");
            job.check = check.into();
        }
        page.show(
            RepoPageShot {
                repo: json!({"id": "r1", "fullName": "acme/engine", "provider": "github", "defaultBranch": "main", "boards": []}),
                owner: true,
                tab,
                pulls: Vec::new(),
                branches: Some(vec![("main", "")]),
                settings: None,
                runs,
                jobs,
                ci: Some(json!({"config": {"requiredChecks": ["build"], "githubStatuses": true, "actionsRunners": false},
                    "secrets": [{"name": "NPM_TOKEN", "updatedAt": "2026-11-12T09:00:00.000Z"}, {"name": "SENTRY_AUTH_TOKEN", "updatedAt": "2026-11-13T08:15:00.000Z"}]})),
                now_ms: case["now"].as_str().and_then(domain::instant::parse_instant),
            },
            cx,
        );
        page
    }

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
        self.fetched.ci = shot.ci.map(|ci| Ok(serde_json::from_value(ci).expect("CI settings")));
        self.adopt_ci(cx);
        let tab = shot.tab;
        self.binding.update(cx, |binding, cx| binding.update_state(|state| state["tab"] = json!(tab), cx));
        // A pinned clock: the Started / Updated relative times read off it.
        if let Some(now) = shot.now_ms {
            self.view.update(cx, |view, cx| view.set_clock(Some(now as f64), cx));
        }
        self.shot = Some(Pinned { owner: shot.owner, pulls: shot.pulls, runs: shot.runs, jobs: shot.jobs, now_ms: shot.now_ms });
        cx.notify();
    }

    fn pinned(&self) -> bool {
        self.shot.is_some()
    }

    fn owner(&self, cx: &App) -> bool {
        if let Some(pinned) = &self.shot {
            return pinned.owner;
        }
        active_team_id(&self.nav, cx).is_some_and(|team| super::is_owner(cx, &team))
    }

    /// #EXP-1617: the repo's managers = its sharer or a team owner.
    fn manager(&self, cx: &App) -> bool {
        if self.owner(cx) {
            return true;
        }
        if self.pinned() {
            return false;
        }
        let me = queries::active_account(cx).map(|account| account.user_id);
        self.repo().and_then(|repo| repo.shared_by.as_ref()).is_some_and(|shared| me.as_deref() == Some(shared.id.as_str()))
    }

    fn pull_rows(&self, repository_id: &str, cx: &App) -> Vec<PullRequestRow> {
        if let Some(pinned) = &self.shot {
            return pinned.pulls.iter().filter(|pull| pull.repository_id.as_deref() == Some(repository_id)).cloned().collect();
        }
        Store::global(cx).collections().pull_requests.read(cx).iter().filter(|pull| pull.repository_id.as_deref() == Some(repository_id)).cloned().collect()
    }

    /// #EXP-1617: the repository's synced CI runs and their jobs.
    fn ci_rows(&self, repository_id: &str, cx: &App) -> (Vec<CiRunRow>, Vec<CiJobRow>) {
        let (runs, jobs): (Vec<CiRunRow>, Vec<CiJobRow>) = match &self.shot {
            Some(pinned) => (pinned.runs.clone(), pinned.jobs.clone()),
            None => match Store::try_global(cx) {
                Some(store) => {
                    let collections = store.collections();
                    (collections.ci_runs.read(cx).iter().cloned().collect(), collections.ci_jobs.read(cx).iter().cloned().collect())
                }
                None => (Vec::new(), Vec::new()),
            },
        };
        let runs: Vec<CiRunRow> = runs.into_iter().filter(|run| run.repository_id.as_deref() == Some(repository_id)).collect();
        let jobs = jobs.into_iter().filter(|job| runs.iter().any(|run| run.id == job.run_id)).collect();
        (runs, jobs)
    }

    fn now_ms(&self) -> i64 {
        self.shot.as_ref().and_then(|pinned| pinned.now_ms).unwrap_or_else(crate::session_data::now_ms)
    }

    /// The loaded CI config into the bound controls (required checks, the
    /// switches).
    fn adopt_ci(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(Ok(ci)) = &self.fetched.ci else { return };
        let config = ci.config.clone();
        self.binding.update(cx, |binding, cx| binding.update_state(|state| {
            state["required"] = json!(config.required_checks.clone().unwrap_or_default());
            state["githubStatuses"] = json!(config.github_statuses.unwrap_or(false));
            state["actionsRunners"] = json!(config.actions_runners.unwrap_or(false));
        }, cx));
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
                        fetched.ci = Some(api::ci::repo_settings(&trpc, &repo.id).map_err(|err| err.user_message()));
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
                this.adopt_ci(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Re-read the branches and settings after a write.
    fn reload(&mut self, cx: &mut gpui::Context<Self>) {
        let (Some(repo), Some(trpc)) = (self.repo().cloned(), queries::trpc_client(cx)) else { return };
        cx.spawn(async move |this, cx| {
            let (branches, settings, ci) = cx
                .background_executor()
                .spawn(async move {
                    let settings = (repo.provider() == "ecode").then(|| api::repositories::settings(&trpc, &repo.id).map_err(|err| err.user_message()));
                    let ci = api::ci::repo_settings(&trpc, &repo.id).map_err(|err| err.user_message());
                    (load_branches(&trpc, &repo), settings, ci)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.fetched.branches = Some(branches);
                if settings.is_some() {
                    this.fetched.settings = settings;
                }
                // A failed re-read keeps what the page shows.
                if ci.is_ok() || this.fetched.ci.is_none() {
                    this.fetched.ci = Some(ci);
                    this.adopt_ci(cx);
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
            _ => self.ci_action(name, key, payload, window, cx),
        }
    }

    /// #EXP-1617: the Runs tab and the CI section's events.
    fn ci_action(&mut self, name: &str, key: &str, payload: &Value, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let value = payload.get("value").and_then(Value::as_str).map(str::to_string);
        let set = |page: &mut Self, cx: &mut gpui::Context<Self>, apply: &dyn Fn(&mut Value)| page.binding.update(cx, |binding, cx| binding.update_state(|state| apply(state), cx));
        match name {
            // A table row: the run page.
            "run" => {
                let run_id = payload.get("key").and_then(Value::as_str).filter(|id| !id.is_empty()).unwrap_or(key).to_string();
                if !run_id.is_empty() && !self.pinned() {
                    crate::navigation::navigate(window, cx, crate::navigation::Screen::CiRun { run_id, job: None });
                }
            }
            "required" => {
                let Some(values) = payload.get("values").and_then(Value::as_array) else { return };
                let mut checks: Vec<String> = Vec::new();
                for entry in values.iter().filter_map(Value::as_str).map(str::trim).filter(|entry| !entry.is_empty()) {
                    if !checks.iter().any(|check| check == entry) {
                        checks.push(entry.to_string());
                    }
                }
                let bound = json!(checks.clone());
                set(self, cx, &|state| state["required"] = bound.clone());
                let Some(repo) = self.repo().cloned() else { return };
                let input = api::ci::RepoCiConfigInput { repository_id: repo.id, required_checks: Some(checks), ..Default::default() };
                self.write("save", "the required checks", cx, move |trpc| api::ci::set_repo_config(trpc, &input));
            }
            "ci_switch" => {
                let checked = payload.get("checked").and_then(Value::as_bool).unwrap_or(false);
                let field = key.to_string();
                let Some(repo) = self.repo().cloned() else { return };
                let mut input = api::ci::RepoCiConfigInput { repository_id: repo.id, ..Default::default() };
                match field.as_str() {
                    "githubStatuses" => input.github_statuses = Some(checked),
                    "actionsRunners" => input.actions_runners = Some(checked),
                    _ => return,
                }
                set(self, cx, &|state| state[field.as_str()] = json!(checked));
                self.write("save", "the CI setting", cx, move |trpc| api::ci::set_repo_config(trpc, &input));
            }
            "secret_add" => set(self, cx, &|state| {
                state["secretOpen"] = json!(true);
                state["secretName"] = json!("");
                state["secretValue"] = json!("");
                state["secretInvalid"] = json!(false);
            }),
            "secret_edit" => {
                let text = value.unwrap_or_default();
                let field = if key == "value" { "secretValue" } else { "secretName" };
                set(self, cx, &|state| {
                    state[field] = json!(text);
                    if field == "secretName" {
                        state["secretInvalid"] = json!(false);
                    }
                });
            }
            "secret_cancel" => set(self, cx, &|state| state["secretOpen"] = json!(false)),
            "secret_dialog" => {
                if payload.get("open").and_then(Value::as_bool) == Some(false) {
                    set(self, cx, &|state| state["secretOpen"] = json!(false));
                }
            }
            "secret_save" => {
                let state = self.binding.read(cx).state();
                let (name, secret) = (state["secretName"].as_str().unwrap_or_default().trim().to_string(), state["secretValue"].as_str().unwrap_or_default().to_string());
                if !repo_ci::secret_name(&name) {
                    set(self, cx, &|state| state["secretInvalid"] = json!(true));
                    return;
                }
                if secret.is_empty() {
                    return;
                }
                set(self, cx, &|state| {
                    state["secretOpen"] = json!(false);
                    state["secretValue"] = json!("");
                });
                let Some(repo) = self.repo().cloned() else { return };
                self.write("save", "the secret", cx, move |trpc| api::ci::set_secret(trpc, &repo.id, &name, &secret));
            }
            "secret_delete" => {
                let secret = key.to_string();
                set(self, cx, &|state| {
                    state["deleteOpen"] = json!(true);
                    state["deleteName"] = json!(secret.clone());
                });
            }
            "delete_cancel" => set(self, cx, &|state| state["deleteOpen"] = json!(false)),
            "delete_confirm" => {
                let secret = self.binding.read(cx).state()["deleteName"].as_str().unwrap_or_default().to_string();
                set(self, cx, &|state| state["deleteOpen"] = json!(false));
                let Some(repo) = self.repo().cloned() else { return };
                if secret.is_empty() {
                    return;
                }
                self.write("delete", "the secret", cx, move |trpc| api::ci::delete_secret(trpc, &repo.id, &secret));
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
                let pill = rows.iter().find(|pull| pull.id == row.id).map(domain::repo_page::merge_pill);
                let (label, glyph) = pill.map_or((String::new(), ""), |p| (p.label, p.glyph.id()));
                json!({"key": row.id, "identifier": row.identifier, "title": row.title, "subtitle": row.subtitle, "depth": row.depth, "checks": row.checks, "pill": label, "pillGlyph": glyph})
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
        let (runs, jobs) = self.ci_rows(&repo.id, cx);
        let run_rows: Vec<Value> = repo_ci::run_rows(&runs, &jobs, self.now_ms())
            .into_iter()
            .map(|row| {
                json!({"id": row.id, "glyph": row.glyph.concept(), "tone": icon_tone(row.tone), "statusLabel": row.status_label,
                    "run": format!("{} {}", row.identifier, row.title), "commit": row.commit, "ref": row.r#ref, "trigger": row.trigger,
                    "duration": row.duration.unwrap_or_default(), "started": row.started_at})
            })
            .collect();
        let pipelines: Vec<Value> = repo_ci::pipeline_rows(&runs).into_iter().map(|row| json!({"name": row.name, "state": row.state})).collect();
        let ci = match &self.fetched.ci {
            None => Value::Null,
            Some(Err(_)) => json!({"failed": true}),
            Some(Ok(ci)) => {
                let required = ci.config.required_checks.clone().unwrap_or_default();
                let suggestions: Vec<Value> = repo_ci::required_suggestions(&runs, &jobs, &required).into_iter().map(|name| json!({"label": name, "value": name})).collect();
                let secrets: Vec<Value> = ci.secrets.iter().map(|secret| json!({"name": secret.name, "updated": secret.updated_at})).collect();
                json!({"suggestions": suggestions, "secrets": secrets})
            }
        };
        json!({
            "phase": "ready",
            "name": repo.full_name,
            "provider": copy.provider.label(repo.provider()),
            "ecode": ecode,
            "owner": owner,
            "manager": self.manager(cx),
            "runs": run_rows,
            "pipelines": pipelines,
            "ci": ci,
            "switches": repo_ci::switches(repo.provider()),
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
                // The merge-train pill: its glyph (the spinner = the
                // `ci-running` concept) then its label, as on web and phones.
                let glyph = match row["pillGlyph"].as_str().unwrap_or_default() {
                    "spinner" => "ci-running",
                    other => other,
                };
                let mut badge = json!({"id": format!("{id}_pill"), "component": "Badge", "props": {"text": pill, "variant": "outline"}, "style": {"flexShrink": 0, "alignSelf": "center"}});
                if !glyph.is_empty() {
                    badge["props"]["icon"] = json!(glyph);
                }
                meta.push(badge);
            }
            json!({"id": id, "component": "Row",
                "props": {"title": row["title"], "identifier": row["identifier"], "subtitle": row["subtitle"], "depth": row["depth"], "pressable": true},
                "on": {"press": {"event": {"name": "pr", "context": {"key": row["key"]}}}},
                // THE PR node ×4 (web `PrNode`): a 12px ring in the success
                // tone before the identifier.
                "slots": {"leading": {"id": format!("{id}_node"), "component": "Box", "style": {"width": 12, "height": 12, "borderRadius": 9999, "borderWidth": 1.5, "borderColor": "$color.success", "flexShrink": 0, "alignSelf": "center"}},
                    "trailing": {"id": format!("{id}_meta"), "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": 6, "flexShrink": 0}, "children": meta}}})
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

/// A pr-checks tone as the `Icon`'s (ci_run's `ICON_TONE`).
fn icon_tone(tone: domain::pr_checks::CheckTone) -> &'static str {
    match tone {
        domain::pr_checks::CheckTone::Muted => "neutral",
        domain::pr_checks::CheckTone::Green => "success",
        domain::pr_checks::CheckTone::Red => "danger",
    }
}

/// #EXP-1617 the Runs tab: a `Table` of the runs (a row opens its run
/// page), or the empty state.
fn runs_tab(data: &Value) -> Value {
    let rows = data["runs"].as_array().cloned().unwrap_or_default();
    if rows.is_empty() {
        return column("runs", vec![empty("runs_empty", "ci-queued", &REPO_PAGE.copy.runs.empty)]);
    }
    let columns: Vec<Value> = repo_ci::run_columns()
        .into_iter()
        .map(|(key, label)| match key {
            "status" => json!({"key": key, "label": label, "type": "slot", "slot": "status", "width": 128}),
            "run" => json!({"key": key, "label": label, "truncate": true}),
            "commit" => json!({"key": key, "label": label, "width": 84}),
            "ref" => json!({"key": key, "label": label, "truncate": true}),
            "trigger" => json!({"key": key, "label": label, "width": 112}),
            "duration" => json!({"key": key, "label": label, "width": 88, "align": "end"}),
            _ => json!({"key": key, "label": label, "type": "relativeTime", "width": 144, "truncate": true}),
        })
        .collect();
    let status = json!({"id": "runs_status", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": 6, "minWidth": 0},
        "children": [
            {"id": "runs_status_glyph", "component": "Icon", "props": {"name": {"@path": "glyph"}, "tone": {"@path": "tone"}, "size": "sm"}},
            {"id": "runs_status_label", "component": "Text", "props": {"text": {"@path": "statusLabel"}, "lines": 1}},
        ]});
    column("runs", vec![json!({"id": "runs_table", "component": "Table", "props": {"columns": columns, "rows": rows, "density": "compact"},
        "on": {"rowPress": {"event": {"name": "run"}}}, "slots": {"status": status}})])
}

/// #EXP-1617 the Settings tab's `CI` `Section`: pipelines, required checks,
/// secrets, the GitHub-only switches.
fn ci_section(data: &Value) -> Value {
    let copy = |path: &str| repo_ci::copy(path);
    let manager = data["manager"].as_bool() == Some(true);
    let mut children = Vec::new();
    let pipelines = data["pipelines"].as_array().cloned().unwrap_or_default();
    if pipelines.is_empty() {
        children.push(json!({"id": "ci_pipelines_none", "component": "Text", "props": {"text": copy("ci.noPipeline"), "variant": "muted"}}));
    } else {
        let rows: Vec<Value> = pipelines
            .iter()
            .enumerate()
            .map(|(ix, row)| {
                let id = format!("ci_pipeline_{ix}");
                let mut node = json!({"id": id, "component": "Row", "props": {"title": row["name"]}});
                if let Some(chip) = checks_chip(&format!("{id}_checks"), row["state"].as_str()) {
                    node["slots"] = json!({"trailing": chip});
                }
                node
            })
            .collect();
        children.push(json!({"id": "ci_pipelines", "component": "Section", "style": {"width": "100%"}, "children": rows}));
    }
    let ci = &data["ci"];
    if ci.is_null() {
        children.push(json!({"id": "ci_skeleton", "component": "Skeleton", "props": {"rows": 2}}));
    } else if ci["failed"].as_bool() != Some(true) {
        children.push(json!({"id": "ci_required", "component": "SettingsRow", "props": {"label": copy("ci.required.label"), "description": copy("ci.required.description"), "stackBelow": "md"},
            "children": [{"id": "ci_required_input", "component": "ChipInput",
                "props": {"name": "requiredChecks", "values": {"@path": "/pane/required"}, "placeholder": copy("ci.required.placeholder"), "suggestions": ci["suggestions"], "disabled": !manager},
                "on": {"change": {"event": {"name": "required"}}}}]}));
        // The secrets (web `secretsNodes`): an h4 `Heading`, the `Table` (a
        // ghost "Delete" per row for managers) or the muted empty line, then
        // the outline Add secret.
        children.push(json!({"id": "ci_secrets_heading", "component": "Heading", "props": {"text": copy("ci.secrets.title"), "level": "h4"}}));
        let secrets = ci["secrets"].as_array().cloned().unwrap_or_default();
        if secrets.is_empty() {
            children.push(json!({"id": "ci_secrets_none", "component": "Text", "props": {"text": copy("ci.secrets.empty"), "variant": "muted"}}));
        } else {
            let mut columns = vec![
                json!({"key": "name", "label": copy("ci.secrets.columns.name"), "truncate": true}),
                json!({"key": "updated", "label": copy("ci.secrets.columns.updated"), "type": "relativeTime", "width": 160}),
            ];
            let mut table = json!({"id": "ci_secrets_table", "component": "Table", "props": {"rows": secrets, "rowKey": "name"}, "style": {"width": "100%"}});
            if manager {
                columns.push(json!({"key": "actions", "label": "", "type": "slot", "slot": "actions", "width": 104, "align": "end"}));
                table["slots"] = json!({"actions": {"id": "ci_secret_delete", "component": "Button", "props": {"label": copy("ci.secrets.delete"), "variant": "ghost", "size": "sm"},
                    "on": {"press": {"event": {"name": "secret_delete", "context": {"key": {"@path": "name"}}}}}}});
            }
            table["props"]["columns"] = json!(columns);
            children.push(table);
        }
        if manager {
            children.push(json!({"id": "ci_secret_add", "component": "Button", "props": {"label": copy("ci.secrets.add"), "variant": "outline", "size": "sm"},
                "style": {"alignSelf": "flex-start"}, "on": {"press": {"event": {"name": "secret_add"}}}}));
        }
        for switch in data["switches"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            children.push(json!({"id": format!("ci_{switch}"), "component": "SettingsRow", "props": {"label": copy(&format!("ci.{switch}.label")), "description": copy(&format!("ci.{switch}.description"))},
                "children": [{"id": format!("ci_{switch}_switch"), "component": "Switch", "props": {"label": "", "name": switch, "checked": {"@path": format!("/pane/{switch}")}, "disabled": !manager},
                    "on": {"change": {"event": {"name": "ci_switch", "context": {"key": switch}}}}}]}));
        }
    }
    json!({"id": "ci", "component": "Section", "props": {"title": copy("ci.section")}, "children": children})
}

/// The Add secret `Dialog`: name (validated), the value as a password,
/// Cancel / Save.
fn secret_dialog() -> Value {
    let copy = |path: &str| repo_ci::copy(path);
    json!({"id": "secret_dialog", "component": "Dialog", "props": {"title": copy("ci.secrets.add"), "open": {"@path": "/pane/secretOpen"}},
        "on": {"change": {"event": {"name": "secret_dialog"}}},
        "children": [{"id": "secret_form", "component": "Form", "props": {"name": "secret"}, "on": {"submit": {"event": {"name": "secret_save"}}},
            "children": [{"id": "secret_fields", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "gap": 12}, "children": [
                {"id": "secret_name", "component": "Input", "props": {"label": copy("ci.secrets.name"), "name": "name", "placeholder": "NPM_TOKEN", "value": {"@path": "/pane/secretName"}},
                    "on": {"change": {"event": {"name": "secret_edit", "context": {"key": "name"}}}}},
                {"id": "secret_invalid", "component": "Text", "visible": {"@path": "/pane/secretInvalid"}, "props": {"text": copy("ci.secrets.invalid"), "variant": "caption", "tone": "danger"}},
                {"id": "secret_value", "component": "Input", "props": {"type": "password", "label": copy("ci.secrets.value"), "name": "value", "value": {"@path": "/pane/secretValue"}},
                    "on": {"change": {"event": {"name": "secret_edit", "context": {"key": "value"}}}}},
                {"id": "secret_actions", "component": "FormActions", "props": {"submit": copy("ci.secrets.save"), "cancel": true}, "on": {"cancel": {"event": {"name": "secret_cancel"}}}},
            ]}]}]})
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
    rows.push(ci_section(data));
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
    // The bar holds the title only (repo-page.json): the provider lives in
    // the Settings tab.
    let bar = json!({"id": "bar", "component": "AppBar", "props": {"title": data["name"], "back": true}, "on": {"back": {"event": {"name": "back"}}}});
    let tabs: Vec<Value> = REPO_PAGE.tabs.iter().map(|tab| json!({"label": tab.label, "value": tab.id})).collect();
    let panels: Vec<Value> = REPO_PAGE
        .tabs
        .iter()
        .map(|tab| match tab.id.as_str() {
            "pulls" => pulls_tab(data),
            "branches" => branches_tab(data),
            "settings" => settings_tab(data),
            _ => runs_tab(data),
        })
        .collect();
    let tabs = json!({"id": "tabs", "component": "Tabs", "props": {"tabs": tabs, "value": {"@path": "/pane/tab"}, "variant": "pill"}, "on": {"change": {"event": {"name": "tab"}}}, "children": panels});
    let restore = json!({"id": "restore_alert", "component": "AlertDialog",
        "props": {"open": {"@path": "/pane/restoreOpen"}, "title": REPO_PAGE.copy.settings.restore_title, "confirmLabel": REPO_PAGE.copy.settings.restore_confirm, "destructive": true},
        "on": {"confirm": {"event": {"name": "restore_confirm"}}, "cancel": {"event": {"name": "restore_cancel"}}}});
    let delete = json!({"id": "secret_delete_alert", "component": "AlertDialog",
        "props": {"open": {"@path": "/pane/deleteOpen"}, "title": repo_ci::copy("ci.secrets.deleteTitle"), "confirmLabel": repo_ci::copy("ci.secrets.deleteConfirm"), "cancelLabel": repo_ci::copy("ci.secrets.cancel"), "destructive": true},
        "on": {"confirm": {"event": {"name": "delete_confirm"}}, "cancel": {"event": {"name": "delete_cancel"}}}});
    root(vec![bar, tabs, restore, delete, secret_dialog()])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready() -> Value {
        json!({"phase": "ready", "name": "web", "provider": "Exponential", "ecode": true, "owner": true,
            "pulls": [{"key": "p1", "identifier": "#2", "title": "Root", "subtitle": "exp/a → main", "depth": 0, "checks": "success", "pill": "Checks running", "pillGlyph": "spinner"},
                      {"key": "p2", "identifier": "#3", "title": "Child", "subtitle": "exp/b → exp/a", "depth": 1, "checks": null, "pill": "Draft", "pillGlyph": "pr-draft"}],
            "branches": [{"name": "main", "sha": "0123456", "isDefault": true, "pr": null}],
            "defaultBranch": "main", "branchNames": ["main"], "cloneUrl": "exp://acme/web",
            "backups": [{"key": "b1", "title": "2026-11-12 10:00", "subtitle": "3 refs · 4 KB"}],
            "manager": true, "switches": ["githubStatuses", "actionsRunners"],
            "runs": [{"id": "r12", "glyph": "ci-running", "tone": "neutral", "statusLabel": "Running", "run": "#12 ci", "commit": "ccccccc", "ref": "feat/x", "trigger": "Pull request", "duration": "5m 0s", "started": "2026-11-13T10:25:00.000Z"},
                     {"id": "r13", "glyph": "ci-queued", "tone": "neutral", "statusLabel": "Queued", "run": "#13 ci", "commit": "ddddddd", "ref": "main", "trigger": "Merge train", "duration": "", "started": null}],
            "pipelines": [{"name": "ci", "state": "pending"}, {"name": "nightly", "state": "failure"}],
            "ci": {"suggestions": [{"label": "lint", "value": "lint"}], "secrets": [{"name": "NPM_TOKEN", "updated": "2026-11-12T09:00:00.000Z"}]}})
    }

    #[test]
    fn the_document_raises_no_unexpected_issue() {
        let ink = super::super::readiness::tests::test_ink();
        let mut bare = ready();
        bare["pulls"] = json!([]);
        bare["branches"] = json!([]);
        bare["backups"] = json!([]);
        bare["ecode"] = json!(false);
        bare["runs"] = json!([]);
        bare["pipelines"] = json!([]);
        bare["ci"] = json!({"suggestions": [], "secrets": []});
        bare["manager"] = json!(false);
        bare["switches"] = json!([]);
        let mut skeleton = ready();
        skeleton["branches"] = Value::Null;
        skeleton["ci"] = Value::Null;
        let mut failed_ci = ready();
        failed_ci["ci"] = json!({"failed": true});
        for data in [ready(), bare, skeleton, failed_ci, json!({"phase": "loading"}), json!({"phase": "failed"})] {
            let tree = document(&ink, &data);
            let bound = super::super::doc::bound_values(&tree);
            let mut surface = eui_core::surface::Surface::new("settings-repo", Default::default());
            let outcome = surface.set_nested(serde_json::from_value(tree).expect("parses"));
            let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
            assert!(issues.is_empty(), "{issues:?}");
        }
    }
}
