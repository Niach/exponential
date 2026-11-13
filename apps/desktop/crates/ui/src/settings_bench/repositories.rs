//! #EXP-1629: Settings → Repositories in every state (owner rows with the
//! branch picker, chips, sharer and both access banners; a member's
//! read-only rows; busy; loading; failed; empty; no team; each GitHub
//! status line; the Local account's folders; the branch picker open).
use super::*;
use crate::settings::repositories::{RepoShot, RepositoriesPane};
use serde_json::{json, Value};

const INSTALL: &str = "https://github.com/apps/exponential/installations/new";

fn installation(id: i64, login: &str, org: bool, suspended: bool) -> Value {
    json!({"installationId": id, "accountLogin": login, "accountType": if org { "Organization" } else { "User" }, "manageUrl": format!("https://github.com/settings/installations/{id}"), "suspended": suspended})
}

fn linked(installations: Vec<Value>) -> Value {
    json!({"configured": true, "connectConfigured": true, "linked": true, "login": "dana-scully", "installed": !installations.is_empty(), "installUrl": INSTALL, "installations": installations})
}

fn suspended() -> Value {
    linked(vec![installation(1, "acme", true, false), installation(2, "dana-scully", false, true)])
}

fn rows() -> Value {
    json!([
        {"id": "r1", "fullName": "acme/web", "defaultBranch": "main", "githubDefaultBranch": "main", "private": true, "installationId": 1,
            "sharedBy": {"id": "u-me", "name": "Dana Scully"}, "boards": [{"id": "b1", "name": "Web"}, {"id": "b2", "name": "Design"}, {"id": "b3", "name": "Ops"}]},
        {"id": "r2", "fullName": "dana-scully/dotfiles", "defaultBranch": "release/2026-11", "githubDefaultBranch": "main", "private": false, "installationId": 2,
            "sharedBy": {"id": "u-sam", "email": "sam@acme.dev"}, "boards": []},
        {"id": "r3", "fullName": "acme/legacy-monolith-with-a-rather-long-repository-name", "defaultBranch": "develop", "private": true, "installationId": 1,
            "inaccessibleAt": "2026-11-01T10:00:00Z", "boards": [{"id": "b1", "name": "Web"}]},
    ])
}

fn boards() -> Vec<domain::rows::Board> {
    let board = |id: &str, name: &str, color: Option<&str>, icon: Option<&str>| {
        let mut board = domain::rows::Board::seeded(id, "t", name);
        board.color = color.map(str::to_string);
        board.icon = icon.map(str::to_string);
        board
    };
    vec![board("b1", "Web", Some("#3b82f6"), Some("rocket")), board("b2", "Design", None, Some("bug"))]
}

fn shot(team: bool, owner: bool, busy: bool) -> RepoShot {
    RepoShot { team: team.then(|| "t".to_string()), owner, me: Some("u-me".into()), local: None, boards: boards(), busy, branches: Vec::new(), pulls: Vec::new() }
}

fn repos(name: &str, shot: RepoShot, status: Option<Value>, list: Option<Result<Value, String>>, clicks: &'static [(f32, f32)]) {
    shoot_pane_clicked(name, RepositoriesPane::for_shots, move |pane, cx| pane.show(shot, status, list, cx), None, clicks);
}

#[test]
#[ignore]
fn settings_repositories_owner() {
    repos("repositories-owner", shot(true, true, false), Some(suspended()), Some(Ok(rows())), &[]);
}

#[test]
#[ignore]
fn settings_repositories_member() {
    repos("repositories-member", shot(true, false, false), Some(suspended()), Some(Ok(rows())), &[]);
}

#[test]
#[ignore]
fn settings_repositories_busy() {
    repos("repositories-busy", shot(true, true, true), Some(suspended()), Some(Ok(rows())), &[]);
}

#[test]
#[ignore]
fn settings_repositories_loading() {
    repos("repositories-loading", shot(true, true, false), None, None, &[]);
}

#[test]
#[ignore]
fn settings_repositories_failed() {
    repos("repositories-failed", shot(true, true, false), Some(linked(vec![installation(1, "acme", true, false)])), Some(Err("offline".into())), &[]);
}

#[test]
#[ignore]
fn settings_repositories_empty() {
    repos("repositories-empty", shot(true, true, false), Some(linked(vec![installation(1, "acme", true, false)])), Some(Ok(json!([]))), &[]);
}

#[test]
#[ignore]
fn settings_repositories_no_team() {
    repos("repositories-no-team", shot(false, true, false), None, None, &[]);
}

#[test]
#[ignore]
fn settings_repositories_status_failed() {
    repos("repositories-status-failed", shot(true, true, false), None, Some(Ok(json!([]))), &[]);
}

#[test]
#[ignore]
fn settings_repositories_status_not_configured() {
    repos("repositories-status-not-configured", shot(true, true, false), Some(json!({"configured": false})), Some(Ok(json!([]))), &[]);
}

#[test]
#[ignore]
fn settings_repositories_status_not_linked() {
    repos("repositories-status-not-linked", shot(true, true, false), Some(json!({"configured": true, "connectConfigured": true})), Some(Ok(json!([]))), &[]);
}

#[test]
#[ignore]
fn settings_repositories_status_expired() {
    repos("repositories-status-expired", shot(true, true, false), Some(json!({"configured": true, "linked": true, "needsReconnect": true, "login": "dana-scully"})), Some(Ok(json!([]))), &[]);
}

#[test]
#[ignore]
fn settings_repositories_status_linked() {
    repos("repositories-status-linked", shot(true, true, false), Some(linked(vec![installation(1, "acme", true, false), installation(3, "dana-scully", false, false)])), Some(Ok(json!([]))), &[]);
}

#[test]
#[ignore]
fn settings_repositories_status_not_installed() {
    repos("repositories-status-not-installed", shot(true, true, false), Some(linked(vec![])), Some(Ok(json!([]))), &[]);
}

#[test]
#[ignore]
fn settings_repositories_status_suspended() {
    repos("repositories-status-suspended", shot(true, true, false), Some(suspended()), Some(Ok(json!([]))), &[]);
}

#[test]
#[ignore]
fn settings_repositories_status_busy() {
    repos("repositories-status-busy", shot(true, true, true), Some(json!({"configured": true, "linked": true, "needsReconnect": true})), Some(Ok(json!([]))), &[]);
}

fn local(rows: Vec<api::repositories::LocalRepository>) -> RepoShot {
    RepoShot { local: Some(rows), ..shot(false, true, false) }
}

#[test]
#[ignore]
fn settings_repositories_local() {
    let folder = |id: &str, name: &str, branch: &str, path: &str| api::repositories::LocalRepository { id: id.into(), full_name: name.into(), default_branch: branch.into(), path: path.into() };
    let rows = vec![folder("l1", "web", "main", "/home/dana/code/web"), folder("l2", "a-folder-with-a-rather-long-name-to-see-it-truncate", "develop", "/home/dana/code/clients/acme/a-folder-with-a-rather-long-name-to-see-it-truncate")];
    repos("repositories-local", local(rows), None, None, &[]);
}

#[test]
#[ignore]
fn settings_repositories_local_empty() {
    repos("repositories-local-empty", local(Vec::new()), None, None, &[]);
}

/// A click at the first row's branch pill: `EXP_REPOS_CLICK=x,y`
/// overrides [`BRANCH`].
fn branch_click() -> &'static [(f32, f32)] {
    let point = std::env::var("EXP_REPOS_CLICK").ok().and_then(|v| v.split_once(',').and_then(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?)))).unwrap_or(BRANCH);
    Box::leak(Box::new([point]))
}

#[test]
#[ignore]
fn settings_repositories_branch_open() {
    let shot = RepoShot { branches: vec![("r1".into(), vec!["main".into(), "develop".into(), "feature/branch-picker".into(), "release/2026-11".into()])], ..shot(true, true, false) };
    repos("repositories-branch-open", shot, Some(suspended()), Some(Ok(rows())), branch_click());
}

#[test]
#[ignore]
fn settings_repositories_branch_loading() {
    repos("repositories-branch-loading", shot(true, true, false), Some(suspended()), Some(Ok(rows())), branch_click());
}

/// The first row's branch pill (window points), read off `repositories-owner`.
const BRANCH: (f32, f32) = (580., 291.);

/// #EXP-1616: a synced PR of `repo` (`checks` = its `checks_state`).
pub(super) fn pull(id: &str, repo: &str, number: i64, title: &str, head: &str, base: &str, state: &str, checks: &str) -> domain::rows::PullRequestRow {
    serde_json::from_value(json!({"id": id, "repository_id": repo, "number": number, "title": title, "head": head, "base": base, "state": state, "checks_state": checks,
        "url": format!("https://example.com/pull/{number}")})).expect("a pull request row")
}

/// #EXP-1616: both providers, each with its checks chip.
#[test]
#[ignore]
fn settings_repositories_providers() {
    let rows = json!([
        {"id": "r1", "fullName": "acme/web", "provider": "github", "defaultBranch": "main", "githubDefaultBranch": "main", "private": true, "installationId": 1,
            "sharedBy": {"id": "u-me", "name": "Dana Scully"}, "boards": [{"id": "b1", "name": "Web"}]},
        {"id": "r4", "fullName": "acme/engine", "provider": "ecode", "defaultBranch": "main", "private": true, "boards": [{"id": "b2", "name": "Design"}]},
        {"id": "r5", "fullName": "acme/docs", "provider": "ecode", "defaultBranch": "main", "private": true, "boards": []},
    ]);
    let pulls = vec![
        pull("p1", "r1", 41, "Retry", "exp/retry", "main", "open", "success"),
        pull("p2", "r4", 7, "Parser", "exp/parser", "main", "open", "pending"),
        pull("p3", "r4", 8, "Lexer", "exp/lexer", "exp/parser", "draft", "failure"),
    ];
    repos("repositories-providers", RepoShot { pulls, ..shot(true, true, false) }, Some(linked(vec![installation(1, "acme", true, false)])), Some(Ok(rows)), &[]);
}
