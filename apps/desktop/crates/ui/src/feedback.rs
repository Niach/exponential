//! "Send Feedback" — open the PUBLIC feedback board in-app for members,
//! otherwise hand off to the cloud `/feedback` page in the browser.
//!
//! Since v7 the public board is a read-only feedback-type PROJECT inside an
//! otherwise-private workspace; workspace-level publicness and the
//! self-service join are gone. The desktop mirrors iOS/Android:
//!
//! 1. board already synced (member) → open its project board in-app;
//! 2. otherwise resolve the bootstrap board via the public
//!    `workspaces.getBySlug` query (slug `feedback`, seeded by
//!    `bootstrap-cloud.ts`) — a member whose sync merely lags waits for the
//!    workspace + board project to land, then opens in-app;
//! 3. everything else — signed out, a non-member (the mobile/desktop clients
//!    only sync membership-scoped shapes, so a non-member cannot render the
//!    board locally; the anonymous read-only view lives on the web), a
//!    self-hosted instance without the public board (NOT_FOUND), a non-public
//!    slug squatter, or a transport failure — falls back to the cloud
//!    `/feedback` page in the system browser.
//!
//! There is no join flow: public boards are read-only for non-members.
//! Anonymization semantics need no extra work here — public-board co-members
//! render via `domain::rows::member_fallback_label`, same as web/iOS/Android.

use gpui::{App, AsyncWindowContext, Window};
use sync::Store;

use crate::actions::{OpenProject, SendFeedback};
use crate::queries;

/// The bootstrap public feedback board's slug (web `/t/feedback`;
/// `bootstrap-cloud.ts` `PUBLIC_WORKSPACE_SLUG`).
const FEEDBACK_WORKSPACE_SLUG: &str = "feedback";

/// Register the App-global [`SendFeedback`] handler (call once from
/// `ui::init`).
pub fn init(cx: &mut App) {
    cx.on_action(|_: &SendFeedback, cx| open(cx));
}

/// Resolve the public feedback board and open it in-app; browser fallback
/// whenever that path is unavailable.
fn open(cx: &mut App) {
    if queries::active_account(cx).is_none() {
        // Signed out — the cloud page handles auth itself.
        browser_fallback(cx.background_executor());
        return;
    }
    crate::navigation::on_active_window(cx, |window, cx| {
        // Fast path: already a member — the board's project is in the synced
        // set (membership is exactly what makes it sync). Requiring a PUBLIC
        // project (not just the slugged workspace) preserves the squatter
        // guard: a private self-host workspace that owns the `feedback` slug
        // never opens through Send Feedback.
        let synced = Store::global(cx)
            .collections()
            .workspaces
            .read(cx)
            .iter()
            .find(|workspace| workspace.slug.as_deref() == Some(FEEDBACK_WORKSPACE_SLUG))
            .and_then(|workspace| {
                let projects = Store::global(cx)
                    .collections()
                    .projects_in_workspace(&workspace.id, cx);
                projects
                    .iter()
                    .find(|project| project.is_public == Some(true))
                    .map(|project| project.id.clone())
            });
        if let Some(project_id) = synced {
            open_board(window, cx, project_id);
            return;
        }
        resolve_and_route(window, cx);
    });
}

/// Open a project's board in-app — the canonical path (the `OpenProject`
/// handler resolves the project's workspace, switches the window's workspace
/// if needed, scopes to the project, and activates the All-Issues tool
/// window; there is no separate board screen).
fn open_board(window: &mut Window, cx: &mut App, project_id: String) {
    window.dispatch_action(Box::new(OpenProject { project_id }), cx);
}

/// Resolve path: `workspaces.getBySlug` → in-app (member of a board-hosting
/// workspace) after the workspace + board project sync in, or the browser
/// fallback for every other outcome.
fn resolve_and_route(window: &mut Window, cx: &mut App) {
    let Some(trpc) = queries::trpc_client(cx) else {
        browser_fallback(cx.background_executor());
        return;
    };
    let workspaces = Store::global(cx).collections().workspaces.clone();
    window
        .spawn(cx, async move |window| {
            let resolved = window
                .background_executor()
                .spawn(async move {
                    api::workspaces::workspaces_get_by_slug(&trpc, FEEDBACK_WORKSPACE_SLUG)
                })
                .await;

            let workspace = match resolved {
                Ok(workspace) if should_open_in_app(&workspace) => workspace,
                Ok(_) => {
                    // Non-member, or an instance whose `feedback` workspace has
                    // no public board — the cloud page serves the anonymous
                    // read-only view.
                    log::info!(
                        "[ui] feedback: `{FEEDBACK_WORKSPACE_SLUG}` not openable in-app (non-member or no public board); opening the browser"
                    );
                    browser_fallback_async(window);
                    return;
                }
                Err(err) => {
                    // NOT_FOUND (instance without the bootstrap board) or a
                    // transport failure — the cloud page always works.
                    log::info!("[ui] feedback: board lookup failed ({err}); opening the browser");
                    browser_fallback_async(window);
                    return;
                }
            };

            // Member whose sync merely lags (the fast path missed): wait for
            // the workspace and its board project to land, then open.
            queries::await_row_visible(&workspaces, &workspace.id, window).await;
            let project_id = await_board_project(&workspace.id, window).await;
            match project_id {
                Some(project_id) => {
                    let _ = window
                        .update(|window, cx| open_board(window, cx, project_id));
                }
                None => browser_fallback_async(window),
            }
        })
        .detach();
}

/// Bounded wait for the feedback board's project row to sync into the local
/// collection (a member's board arrives once membership rotates the shape
/// scope). Mirrors [`queries::await_row_visible`]'s shape but keys off a
/// predicate rather than an id. Returns the picked project id, or `None` on
/// timeout / closed window (→ browser fallback).
async fn await_board_project(workspace_id: &str, window: &mut AsyncWindowContext) -> Option<String> {
    const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);
    const POLL: std::time::Duration = std::time::Duration::from_millis(60);
    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        let picked = window
            .update(|_, cx| {
                let projects = Store::global(cx)
                    .collections()
                    .projects_in_workspace(workspace_id, cx);
                pick_board_project(&projects).map(|project| project.id.clone())
            })
            .ok()?; // window gone — nothing left to open
        if picked.is_some() {
            return picked;
        }
        if std::time::Instant::now() >= deadline {
            log::warn!("[ui] feedback: board project never synced for workspace {workspace_id}");
            return None;
        }
        window.background_executor().timer(POLL).await;
    }
}

/// Open the cloud `/feedback` page in the system browser — the fallback for
/// every non-in-app outcome (signed out, non-member, self-hosted instances
/// without a public board, transport failures).
fn browser_fallback(executor: &gpui::BackgroundExecutor) {
    let url = format!(
        "{}/feedback",
        crate::login::cloud_instance().trim_end_matches('/')
    );
    executor
        .spawn(async move {
            if let Err(err) = api::opener::open_in_browser(&url) {
                log::warn!("[ui] feedback: browser open failed: {err}");
            }
        })
        .detach();
}

fn browser_fallback_async(window: &AsyncWindowContext) {
    browser_fallback(window.background_executor());
}

/// In-app only for a member of a workspace that actually hosts a public board.
fn should_open_in_app(ws: &api::workspaces::WorkspaceBySlugOut) -> bool {
    ws.has_public_board && ws.membership.is_some()
}

/// The board project to open: prefer a public one, fall back to the first
/// (iOS/Android parity — `getBySlug` already vouched that the workspace hosts
/// a public board).
fn pick_board_project(projects: &[domain::rows::Project]) -> Option<&domain::rows::Project> {
    projects
        .iter()
        .find(|project| project.is_public == Some(true))
        .or_else(|| projects.first())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn by_slug(value: serde_json::Value) -> api::workspaces::WorkspaceBySlugOut {
        serde_json::from_value(value).unwrap()
    }

    fn project(id: &str, is_public: Option<bool>) -> domain::rows::Project {
        let mut obj = json!({ "id": id, "workspace_id": "w-1", "name": id });
        if let Some(public) = is_public {
            obj["is_public"] = json!(public);
        }
        serde_json::from_value(obj).unwrap()
    }

    #[test]
    fn should_open_in_app_requires_member_and_board() {
        // Member of a board-hosting workspace → in-app.
        assert!(should_open_in_app(&by_slug(json!({
            "id": "w-1", "hasPublicBoard": true, "membership": "member"
        }))));
        // Member but no public board → browser.
        assert!(!should_open_in_app(&by_slug(json!({
            "id": "w-1", "hasPublicBoard": false, "membership": "owner"
        }))));
        // Non-member of a board-hosting workspace → browser (the web serves
        // the anonymous read-only view).
        assert!(!should_open_in_app(&by_slug(json!({
            "id": "w-1", "hasPublicBoard": true, "membership": null
        }))));
        // Older server omits `hasPublicBoard` (defaults false) → browser.
        assert!(!should_open_in_app(&by_slug(json!({
            "id": "w-1", "membership": "member"
        }))));
    }

    #[test]
    fn pick_board_project_prefers_public_then_first() {
        let private = project("p-priv", Some(false));
        let public = project("p-pub", Some(true));

        // Prefers the public project even when a private one sorts first.
        let projects = vec![private.clone(), public.clone()];
        assert_eq!(pick_board_project(&projects).unwrap().id, "p-pub");

        // No public project → the first row (getBySlug already vouched).
        let projects = vec![project("p-a", Some(false)), project("p-b", None)];
        assert_eq!(pick_board_project(&projects).unwrap().id, "p-a");

        // Empty → None (→ browser fallback).
        assert!(pick_board_project(&[]).is_none());
    }
}
