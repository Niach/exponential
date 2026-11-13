//! #EXP-1616: the repo page (repo-page.json) in its tabs: the PR tree
//! (checks chips, merge-train pills, a draft), the branches, the settings.
use super::*;
use super::repositories::pull;
use crate::settings::repo_page::{RepoPage, RepoPageShot};
use serde_json::json;

fn page(name: &str, tab: &'static str, owner: bool) {
    let pulls = vec![
        pull("p1", "r1", 10, "Root: the eCode server", "exp/root", "main", "open", "success"),
        pull("p2", "r1", 11, "First child: git-remote-exp", "exp/c1", "exp/root", "open", "failure"),
        pull("p3", "r1", 13, "Grandchild", "exp/g", "exp/c1", "open", "none"),
        pull("p4", "r1", 12, "Second child", "exp/c2", "exp/root", "draft", "pending"),
        pull("p5", "r1", 20, "Other: the repo page", "exp/other", "main", "open", "pending"),
        pull("p6", "r1", 3, "Merged long ago", "exp/old", "main", "merged", "success"),
    ];
    let shot = RepoPageShot {
        repo: json!({"id": "r1", "fullName": "acme/engine", "provider": "ecode", "defaultBranch": "main", "boards": []}),
        owner,
        tab,
        pulls,
        branches: Some(vec![("exp/c1", "aaaaaaaaaaaa"), ("main", "0123456789ab"), ("exp/root", "bbbbbbbbbbbb"), ("Zeta", "cccccccccccc")]),
        settings: Some(json!({"id": "r1", "fullName": "acme/engine", "remoteUrl": "exp://acme/engine", "defaultBranch": "main", "mirrorUrl": "https://github.com/acme/engine.git", "mirrorEnabled": true,
            "backups": [{"id": "k1", "createdAt": "2026-11-10T08:00:00Z", "bytes": 48213, "refs": 4}, {"id": "k2", "createdAt": "2026-11-12T08:00:00Z", "bytes": 51022, "refs": 5}]})),
    };
    shoot_pane(name, RepoPage::for_shots, move |page, cx| page.show(shot, cx));
}

#[test]
#[ignore]
fn settings_repo_page_pulls() {
    page("repo-page-pulls", "pulls", true);
}

#[test]
#[ignore]
fn settings_repo_page_branches() {
    page("repo-page-branches", "branches", true);
}

#[test]
#[ignore]
fn settings_repo_page_settings() {
    page("repo-page-settings", "settings", true);
}

#[test]
#[ignore]
fn settings_repo_page_runs() {
    page("repo-page-runs", "runs", false);
}
