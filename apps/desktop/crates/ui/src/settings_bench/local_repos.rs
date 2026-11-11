use super::*;

fn local_repos_rows() -> Vec<(String, Option<u64>, Vec<(String, Option<String>)>)> {
    vec![
        ("acme/web".into(), Some(1_288_490_189), vec![("exp-session-1a2b".into(), Some("exp/session-1a2b".into())), ("detached".into(), None)]),
        ("acme/api".into(), Some(314_572_800), vec![("fix-login".into(), Some("exp/fix-login".into()))]),
        ("acme/docs".into(), Some(52_428_800), vec![]),
    ]
}

fn local_shot(repos: Option<Vec<(String, Option<u64>, Vec<(String, Option<String>)>)>>) -> LocalShot {
    LocalShot { repos, disk: Some((400_000_000_000, 500_000_000_000)), in_use: Vec::new(), busy: None, message: None }
}

fn local_repos(name: &str, shot: LocalShot) {
    shoot_pane(name, LocalReposPane::for_shots, move |pane, cx| pane.show(shot, cx));
}

#[test]
#[ignore]
fn settings_local_repos_rows() {
    local_repos("local-repos-rows", local_shot(Some(local_repos_rows())));
}

#[test]
#[ignore]
fn settings_local_repos_busy() {
    let shot = LocalShot {
        disk: Some((4_000_000_000, 500_000_000_000)),
        in_use: vec!["acme/api".into()],
        busy: Some("acme/web".into()),
        message: Some(("acme/docs".into(), "Removed 2 worktrees · deleted 3 branches".into())),
        ..local_shot(Some(local_repos_rows()))
    };
    local_repos("local-repos-busy", shot);
}

#[test]
#[ignore]
fn settings_local_repos_measuring() {
    let rows = local_repos_rows().into_iter().map(|(name, _, worktrees)| (name, None, worktrees)).collect();
    local_repos("local-repos-measuring", local_shot(Some(rows)));
}

#[test]
#[ignore]
fn settings_local_repos_no_worktrees() {
    local_repos("local-repos-no-worktrees", local_shot(Some(vec![("acme/docs".into(), Some(52_428_800), vec![])])));
}

#[test]
#[ignore]
fn settings_local_repos_empty() {
    local_repos("local-repos-empty", local_shot(Some(Vec::new())));
}

#[test]
#[ignore]
fn settings_local_repos_loading() {
    local_repos("local-repos-loading", local_shot(None));
}
