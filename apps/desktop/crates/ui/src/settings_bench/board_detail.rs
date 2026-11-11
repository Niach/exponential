use super::*;

fn board(repository: Option<&str>, branch: Option<&str>) -> domain::rows::Board {
    serde_json::from_value(serde_json::json!({
        "id": "board-web", "team_id": "team-acme", "name": "Web app", "prefix": "WEB", "color": "#3b82f6", "icon": "rocket",
        "repository_id": repository, "default_branch": branch,
    }))
    .expect("a board")
}

fn board_repos() -> serde_json::Value {
    serde_json::json!([
        {"id": "r1", "fullName": "acme/web", "defaultBranch": "main", "githubDefaultBranch": "main", "private": true},
        {"id": "r2", "fullName": "acme/api", "defaultBranch": "develop", "private": false},
    ])
}

fn board_detail(name: &str, shot: BoardShot) {
    shoot_pane(name, BoardDetailPane::for_shots, move |pane, cx| pane.show(shot, cx));
}

#[test]
#[ignore]
fn settings_board_unlinked() {
    board_detail("board-unlinked", BoardShot { board: board(None, None), repos: Some(board_repos()), failed: false, press: Vec::new() });
}

#[test]
#[ignore]
fn settings_board_linked() {
    board_detail("board-linked", BoardShot { board: board(Some("r1"), None), repos: Some(board_repos()), failed: false, press: Vec::new() });
}

#[test]
#[ignore]
fn settings_board_pinned() {
    board_detail("board-pinned", BoardShot { board: board(Some("r2"), Some("release/1.2")), repos: Some(board_repos()), failed: false, press: Vec::new() });
}

/// The identity row's ColorPicker open: the label palette's 28 px cells.
#[test]
#[ignore]
fn settings_board_color_open() {
    board_detail("board-color-open", BoardShot { board: board(Some("r1"), None), repos: Some(board_repos()), failed: false, press: vec!["color_picker.trigger".into()] });
}

#[test]
#[ignore]
fn settings_board_loading() {
    board_detail("board-loading", BoardShot { board: board(Some("r1"), None), repos: None, failed: false, press: Vec::new() });
}

#[test]
#[ignore]
fn settings_board_unavailable() {
    board_detail("board-unavailable", BoardShot { board: board(Some("gone"), None), repos: Some(board_repos()), failed: false, press: Vec::new() });
}

#[test]
#[ignore]
fn settings_board_failed_plain() {
    let plain = domain::rows::Board { icon: None, color: None, prefix: None, ..board(None, None) };
    board_detail("board-failed-plain", BoardShot { board: plain, repos: None, failed: true, press: Vec::new() });
}
