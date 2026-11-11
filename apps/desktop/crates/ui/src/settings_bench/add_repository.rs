//! #EXP-1629: Settings → Repositories → Add repository in every state (the
//! dialog's body at its real box: loading, failed, not configured, each
//! missing prerequisite, ready with its search / lookup / banner / notice).
use super::*;
use crate::github_connect::GithubReposResult;
use crate::settings::add_repository_dialog::{AddRepoShot, AddRepositoryDialogView};
use serde_json::{json, Value};

/// The dialog body's box in the default window: the 416 × 480 content box
/// less the panel's edge and the shell's padding.
const BODY: (f32, f32) = (386., 466.);

/// The dialog's content at its body box.
struct Body {
    view: gpui::Entity<AddRepositoryDialogView>,
}

impl Render for Body {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "AddRepoBody").entered();
        div().w(px(BODY.0)).h(px(BODY.1)).child(self.view.clone())
    }
}

const INSTALL: &str = "https://github.com/apps/exponential/installations/new";

fn installation(id: i64, login: Option<&str>, suspended: bool) -> Value {
    json!({"installationId": id, "accountLogin": login, "accountType": "Organization", "manageUrl": format!("https://github.com/settings/installations/{id}"), "suspended": suspended})
}

fn repo(name: &str, private: bool) -> Value {
    json!({"fullName": name, "private": private, "defaultBranch": "main"})
}

fn repos() -> Vec<Value> {
    vec![
        repo("acme/web", true),
        repo("acme/legacy-monolith-with-a-rather-long-repository-name-that-truncates", false),
        repo("dana-scully/dotfiles", false),
        repo("acme/infrastructure", true),
    ]
}

fn result(repos: Vec<Value>, installations: Vec<Value>, install_url: bool, has_more: bool) -> GithubReposResult {
    serde_json::from_value(json!({"configured": true, "connectConfigured": true, "linked": true, "login": "dana-scully", "installed": true,
        "installUrl": if install_url { Some(INSTALL) } else { None }, "repos": repos, "hasMore": has_more, "installations": installations})).expect("a result")
}

fn two_accounts() -> Vec<Value> {
    vec![installation(1, Some("acme"), false), installation(2, Some("dana-scully"), false)]
}

fn ready() -> GithubReposResult {
    result(repos(), two_accounts(), true, false)
}

fn shot(load: Option<Option<GithubReposResult>>) -> AddRepoShot {
    AddRepoShot { load, query: "", lookup: "", lookup_busy: false, lookup_error: None, plan_limited: None }
}

fn add_repo(name: &str, shot: AddRepoShot) {
    let name = format!("add-repo-{name}");
    shoot_pane_acted(
        &name,
        |window, cx| {
            // Reduce motion freezes the pulse and the spinner at their first frame.
            cx.set_reduce_motion(true);
            Body { view: cx.new(|cx| AddRepositoryDialogView::for_shots(window, cx)) }
        },
        |_, _| {},
        None,
        &[],
        move |body, window, cx| body.view.update(cx, |view, cx| view.show(shot, window, cx)),
    );
}

fn status(json: Value) -> Option<Option<GithubReposResult>> {
    Some(Some(serde_json::from_value(json).expect("a result")))
}

#[test]
#[ignore]
fn settings_add_repo_loading() {
    add_repo("loading", shot(None));
}

#[test]
#[ignore]
fn settings_add_repo_failed() {
    add_repo("failed", shot(Some(None)));
}

#[test]
#[ignore]
fn settings_add_repo_not_configured() {
    add_repo("not-configured", shot(status(json!({"configured": false}))));
}

#[test]
#[ignore]
fn settings_add_repo_not_linked() {
    add_repo("not-linked", shot(status(json!({"configured": true, "connectConfigured": true, "installUrl": INSTALL}))));
}

#[test]
#[ignore]
fn settings_add_repo_expired() {
    add_repo("expired", shot(status(json!({"configured": true, "linked": true, "needsReconnect": true, "login": "dana-scully", "installUrl": INSTALL}))));
}

#[test]
#[ignore]
fn settings_add_repo_not_installed() {
    add_repo("not-installed", shot(status(json!({"configured": true, "linked": true, "login": "dana-scully", "installUrl": INSTALL}))));
}

#[test]
#[ignore]
fn settings_add_repo_not_installed_no_url() {
    add_repo("not-installed-no-url", shot(status(json!({"configured": true, "linked": true, "login": "dana-scully"}))));
}

#[test]
#[ignore]
fn settings_add_repo_ready() {
    add_repo("ready", shot(Some(Some(ready()))));
}

/// More rows than the list's window: it scrolls (the footer stays pinned).
#[test]
#[ignore]
fn settings_add_repo_scrolls() {
    let many = (0..24).map(|i| repo(&format!("acme/service-{i:02}"), i % 3 == 0)).collect();
    add_repo("scrolls", shot(Some(Some(result(many, two_accounts(), true, false)))));
}

#[test]
#[ignore]
fn settings_add_repo_search_typed() {
    add_repo("search-typed", AddRepoShot { query: "acme", ..shot(Some(Some(ready()))) });
}

#[test]
#[ignore]
fn settings_add_repo_search_no_match() {
    add_repo("search-no-match", AddRepoShot { query: "nothing-matches", ..shot(Some(Some(ready()))) });
}

#[test]
#[ignore]
fn settings_add_repo_none_pushable() {
    add_repo("none-pushable", shot(Some(Some(result(Vec::new(), vec![installation(1, Some("acme"), false)], true, false)))));
}

#[test]
#[ignore]
fn settings_add_repo_suspended_empty() {
    add_repo("suspended-empty", shot(Some(Some(result(Vec::new(), vec![installation(1, Some("acme"), true), installation(7, None, true)], true, false)))));
}

#[test]
#[ignore]
fn settings_add_repo_suspended_list() {
    add_repo("suspended-list", shot(Some(Some(result(repos(), vec![installation(1, Some("acme"), false), installation(2, Some("dana-scully"), true)], true, false)))));
}

#[test]
#[ignore]
fn settings_add_repo_cap_note() {
    add_repo("cap-note", shot(Some(Some(result(repos(), two_accounts(), true, true)))));
}

#[test]
#[ignore]
fn settings_add_repo_no_install_no_links() {
    add_repo("no-install-no-links", shot(Some(Some(result(repos(), Vec::new(), false, false)))));
}

#[test]
#[ignore]
fn settings_add_repo_lookup_typed() {
    add_repo("lookup-typed", AddRepoShot { lookup: "acme/private-repo", ..shot(Some(Some(ready()))) });
}

#[test]
#[ignore]
fn settings_add_repo_lookup_busy() {
    add_repo("lookup-busy", AddRepoShot { lookup: "acme/private-repo", lookup_busy: true, ..shot(Some(Some(ready()))) });
}

#[test]
#[ignore]
fn settings_add_repo_lookup_error() {
    add_repo("lookup-error", AddRepoShot { lookup: "acme/private-repo", lookup_error: Some("The GitHub App isn't installed on acme."), ..shot(Some(Some(ready()))) });
}

#[test]
#[ignore]
fn settings_add_repo_plan_limit() {
    add_repo("plan-limit", AddRepoShot { plan_limited: Some("Your plan allows 3 repositories."), ..shot(Some(Some(ready()))) });
}
