//! Per-window repository resolver (masterplan v4 §4.2).
//!
//! The five trunk/IDE surfaces — the git bar (§4.3), run bar (§7.5), file tree
//! (§4.5), terminal-dock `+` shell (§4.6) and Source Control screen (§4.4) — all
//! need the same project → repo resolution off the server-only
//! `repositories.list` (never synced). Rather than each firing its own identical
//! network call on every project switch (five concurrent identical queries),
//! this ONE per-window entity runs `repositories.list` once per active
//! workspace, caches the parsed rows, and exposes the resolution as gpui state
//! the surfaces `cx.observe` + read.
//!
//! The v4 model is one repo per project via `projects.repositoryId`; the server
//! returns each repo with the `projects[]` it backs (the web `repositories.list`
//! loader — `[{ id, name, slug }]`). A project resolves to the repo whose
//! `projects[]` contains its id; the workspace-level trunk surface resolves the
//! first project's repo, else the sole workspace repo.
//!
//! Consumer-driven, like every other load gate in this crate: a surface calls
//! [`RepoResolver::ensure_loaded`] at render time (idempotent — one fetch per
//! workspace) and reads [`RepoResolver::lookup_project`] /
//! [`RepoResolver::lookup_workspace_trunk`]. The fetch keys on the active
//! workspace, so switching projects within a workspace reuses the cache; the
//! surfaces `cx.observe(&resolver, …)` to re-render when the fetch lands.
//!
//! The cache is NOT restart-scoped (EXP-139): the resolver observes the synced
//! projects collection and refetches when the workspace's
//! (project → `repository_id`) mapping changes — linking/unlinking a repo on
//! the web (or another client) reaches the trunk surfaces live. The refetch is
//! stale-while-revalidate: the old rows keep serving until the fresh ones land.

use std::collections::HashMap;

use gpui::{
    App, AppContext as _, Context, Entity, Global, SharedString, Subscription, Window, WindowId,
};
use serde::{Deserialize, Serialize};
use sync::Store;

use crate::navigation::{self, Navigation};
use crate::queries;

/// A resolved repository row: the ids the launcher / token paths need plus the
/// project ids it backs (the resolution key).
#[derive(Clone)]
pub struct ResolvedRepo {
    /// `repositories.id` — the input to `repositories.installationToken`.
    pub repository_id: String,
    /// `owner/name` — the clone-root key + the remote's redaction anchor.
    pub full_name: String,
    /// The repo's default branch as the server reported it — `repositories.list`
    /// resolves + heals it against GitHub (L30). `None` only when the API omits
    /// it; NEVER fabricated as `main` here, so a use site that needs a concrete
    /// branch resolves it at use-time from a freshly-minted installation token
    /// (whose `default_branch` is looked up live), not a stale/assumed default.
    pub default_branch: Option<String>,
    /// The project ids this repo backs (`projects.repositoryId` == this repo).
    project_ids: Vec<String>,
}

/// The outcome of a project / trunk lookup against the cached rows.
pub enum RepoLookup {
    /// `repositories.list` has not resolved yet (or the workspace isn't known).
    Loading,
    /// Resolved: this scope is backed by a repo.
    Found(ResolvedRepo),
    /// Resolved, but no repo backs this scope.
    NotFound,
    /// `repositories.list` failed.
    Error(SharedString),
}

/// The single fetch's lifecycle for the active workspace.
enum State {
    Idle,
    Loading,
    Ready(Vec<ResolvedRepo>),
    Error(SharedString),
}

/// One per window, shared by all five trunk/IDE surfaces.
pub struct RepoResolver {
    nav: Entity<Navigation>,
    /// The workspace the cached `state` belongs to; a switch re-fetches.
    workspace_id: Option<String>,
    state: State,
    /// Stale-fetch guard — bumped per fetch so a superseded workspace's
    /// in-flight result is dropped.
    generation: u64,
    /// The synced (project → repository) links the current fetch was started
    /// under (EXP-139) — a mismatch means a repo was linked, unlinked, or
    /// retargeted since, so the cached mapping is stale and must refetch.
    fetched_links: Option<Vec<(String, String)>>,
    _subscriptions: Vec<Subscription>,
}

impl RepoResolver {
    fn new(nav: Entity<Navigation>, cx: &mut Context<Self>) -> Self {
        // EXP-139: `projects.repository_id` is Electric-synced and live, but
        // this cache is built from the server-only `repositories.list` — watch
        // the synced rows so a link change on any client invalidates it.
        let projects = Store::global(cx).collections().projects.clone();
        let subscriptions = vec![cx.observe(&projects, |this: &mut Self, _, cx| {
            this.refresh_if_links_changed(cx);
        })];
        Self {
            nav,
            workspace_id: None,
            state: State::Idle,
            generation: 0,
            fetched_links: None,
            _subscriptions: subscriptions,
        }
    }

    /// Kick the single `repositories.list` fetch for the active workspace when
    /// first needed or after a workspace switch. Idempotent — surfaces call it
    /// at render time; only one network call runs per workspace.
    pub fn ensure_loaded(&mut self, cx: &mut Context<Self>) {
        let workspace_id = navigation::active_workspace_id(&self.nav, cx);
        if workspace_id.as_deref() != self.workspace_id.as_deref() {
            self.workspace_id = workspace_id;
            self.state = State::Idle;
        }
        if matches!(self.state, State::Idle) {
            self.start_fetch(cx);
        }
    }

    /// Refetch when the synced (project → repository) links no longer match
    /// what the cached rows were fetched under (EXP-139). An `Idle` cache
    /// refetches on the next `ensure_loaded` anyway, and a `Loading` fetch
    /// re-checks on completion, so only settled states act here.
    fn refresh_if_links_changed(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.state, State::Ready(_) | State::Error(_)) {
            return;
        }
        let Some(workspace_id) = self.workspace_id.clone() else {
            return;
        };
        if self.fetched_links.as_ref() != Some(&links_snapshot(&workspace_id, cx)) {
            self.start_fetch(cx);
        }
    }

    fn start_fetch(&mut self, cx: &mut Context<Self>) {
        let Some(workspace_id) = self.workspace_id.clone() else {
            return; // workspace not synced yet — a surface re-drives on notify
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };

        // Stale-while-revalidate: a link-change refetch keeps serving the old
        // rows until the fresh ones land; only a cold or switched cache shows
        // the Loading state.
        if !matches!(self.state, State::Ready(_)) {
            self.state = State::Loading;
        }
        self.fetched_links = Some(links_snapshot(&workspace_id, cx));
        self.generation += 1;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    fetch_repos(&trpc, &workspace_id).map_err(|err| err.to_string())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return; // superseded by a workspace switch
                }
                match result {
                    Ok(repos) => this.state = State::Ready(repos),
                    Err(err) => {
                        log::warn!("[ui] repo resolver: repositories.list failed: {err}");
                        // A failed REVALIDATION keeps the still-serviceable
                        // rows (the stale-while-revalidate promise) — only a
                        // cold load may surface the error state, else every
                        // trunk surface would regress on a network blip.
                        if !matches!(this.state, State::Ready(_)) {
                            this.state = State::Error(err.into());
                        }
                    }
                }
                cx.notify();
                // A link that changed while this fetch was in flight still
                // lands: compare once more now that the state settled.
                this.refresh_if_links_changed(cx);
            });
        })
        .detach();
    }

    /// Resolve a project's repo (git bar / run bar / file tree / `+` shell
    /// scope): the repo whose `projects[]` contains `project_id`.
    pub fn lookup_project(&self, project_id: &str) -> RepoLookup {
        match &self.state {
            State::Idle | State::Loading => RepoLookup::Loading,
            State::Error(msg) => RepoLookup::Error(msg.clone()),
            State::Ready(repos) => match repos
                .iter()
                .find(|repo| repo.project_ids.iter().any(|id| id == project_id))
            {
                Some(repo) => RepoLookup::Found(repo.clone()),
                None => RepoLookup::NotFound,
            },
        }
    }

    /// Resolve the workspace trunk repo (Source Control scope): the
    /// `first_project`'s repo, else the sole repo in the workspace.
    pub fn lookup_workspace_trunk(&self, first_project: Option<&str>) -> RepoLookup {
        match &self.state {
            State::Idle | State::Loading => RepoLookup::Loading,
            State::Error(msg) => RepoLookup::Error(msg.clone()),
            State::Ready(repos) => {
                let chosen = first_project
                    .and_then(|project_id| {
                        repos
                            .iter()
                            .find(|repo| repo.project_ids.iter().any(|id| id == project_id))
                    })
                    .or(if repos.len() == 1 {
                        repos.first()
                    } else {
                        None
                    });
                match chosen {
                    Some(repo) => RepoLookup::Found(repo.clone()),
                    None => RepoLookup::NotFound,
                }
            }
        }
    }
}

/// A workspace's (project → repository) links from the SYNCED projects rows
/// (EXP-139) — the live signal that a server-only `repositories.list` cache
/// is stale. Shared with the settings Projects/Repositories panes, which
/// cache the same server read. Sorted, so collection iteration order can
/// never look like a link change. Repo-less projects are omitted (unrelated
/// project churn must not force refetches); archived projects are omitted
/// because the server's `projects[]` mapping hides them too, so archiving or
/// unarchiving a LINKED project correctly counts as a mapping change.
pub(crate) fn links_snapshot(workspace_id: &str, cx: &App) -> Vec<(String, String)> {
    let store = Store::global(cx);
    let projects = store.collections().projects.read(cx);
    let mut links: Vec<(String, String)> = projects
        .iter()
        .filter(|project| {
            project.workspace_id == workspace_id && project.archived_at.is_none()
        })
        .filter_map(|project| {
            project
                .repository_id
                .clone()
                .map(|repository_id| (project.id.clone(), repository_id))
        })
        .collect();
    links.sort();
    links
}

/// One `repositories.list` row (the new `projects[]` shape).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepoRow {
    id: String,
    full_name: String,
    #[serde(default)]
    default_branch: Option<String>,
    #[serde(default)]
    projects: Vec<ProjectRef>,
}

#[derive(Deserialize)]
struct ProjectRef {
    id: String,
}

/// `repositories.list` → resolved rows (the new `projects[]` shape). Blocking;
/// the caller runs it off the foreground.
fn fetch_repos(
    trpc: &api::TrpcClient,
    workspace_id: &str,
) -> Result<Vec<ResolvedRepo>, api::ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        workspace_id: &'a str,
    }

    let rows: Vec<RepoRow> =
        trpc.query_with_input("repositories.list", &Input { workspace_id })?;
    Ok(rows.into_iter().map(resolved_from_row).collect())
}

/// One `repositories.list` row → [`ResolvedRepo`]. Pure so the branch-handling
/// (L30: keep the server value, never fabricate `main`) is unit-testable.
fn resolved_from_row(row: RepoRow) -> ResolvedRepo {
    ResolvedRepo {
        repository_id: row.id,
        full_name: row.full_name,
        // Keep the server-healed value; an empty/absent field becomes `None` so
        // no consumer inherits a fabricated `main` (they resolve via a token).
        default_branch: row.default_branch.filter(|branch| !branch.is_empty()),
        project_ids: row.projects.into_iter().map(|project| project.id).collect(),
    }
}

// ---------------------------------------------------------------------------
// Per-window registry (mirrors navigation::nav_for_window)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct ResolverRegistry {
    by_window: HashMap<WindowId, Entity<RepoResolver>>,
}

impl Global for ResolverRegistry {}

/// The window's repo resolver, created on first access (shared by all five
/// trunk/IDE surfaces in the window). The nav entity it observes is looked up
/// through the same window registry, so every surface + the resolver agree on
/// the active workspace/project scope.
pub fn repo_resolver_for_window(window: &Window, cx: &mut App) -> Entity<RepoResolver> {
    let window_id = window.window_handle().window_id();
    if let Some(existing) = cx
        .try_global::<ResolverRegistry>()
        .and_then(|registry| registry.by_window.get(&window_id).cloned())
    {
        return existing;
    }
    let nav = navigation::nav_for_window(window, cx);
    let resolver = cx.new(|cx| RepoResolver::new(nav, cx));
    cx.default_global::<ResolverRegistry>()
        .by_window
        .insert(window_id, resolver.clone());
    resolver
}

/// Drop a closed window's entry (called from the `Workspace` release hook —
/// entities die with the window; the registry must not leak handles).
pub fn remove_window(window_id: WindowId, cx: &mut App) {
    if let Some(registry) = cx.try_global::<ResolverRegistry>() {
        if registry.by_window.contains_key(&window_id) {
            cx.global_mut::<ResolverRegistry>()
                .by_window
                .remove(&window_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(default_branch: Option<&str>) -> RepoRow {
        RepoRow {
            id: "repo-1".to_string(),
            full_name: "acme/web".to_string(),
            default_branch: default_branch.map(str::to_string),
            projects: vec![ProjectRef {
                id: "proj-1".to_string(),
            }],
        }
    }

    #[test]
    fn keeps_the_server_default_branch_verbatim() {
        // A `master`-default repo must round-trip as `master`,
        // never be rewritten to `main`.
        let resolved = resolved_from_row(row(Some("master")));
        assert_eq!(resolved.default_branch.as_deref(), Some("master"));
        assert_eq!(resolved.repository_id, "repo-1");
        assert_eq!(resolved.project_ids, vec!["proj-1".to_string()]);
    }

    #[test]
    fn absent_or_empty_branch_is_none_never_main() {
        // L30: an absent/empty API value must NOT become a fabricated `main` —
        // it stays `None` so the use site resolves a live value from a token.
        assert_eq!(resolved_from_row(row(None)).default_branch, None);
        assert_eq!(resolved_from_row(row(Some(""))).default_branch, None);
    }
}
