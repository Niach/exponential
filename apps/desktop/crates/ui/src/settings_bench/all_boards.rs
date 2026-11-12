use super::*;

#[test]
#[ignore]
fn settings_all_boards_put_away() {
    shoot_pane("all-boards-put-away", AllBoardsPane::for_shots, |pane, cx| {
        let archived = serde_json::from_value(serde_json::json!([
            {"id": "b1", "name": "Old roadmap", "color": "#3b82f6", "icon": "rocket", "archivedAt": "2026-06-30"},
            {"id": "b2", "name": "Experiments", "icon": "flask-conical"},
        ]))
        .expect("archived boards");
        let trash = serde_json::from_value(serde_json::json!([
            {"id": "b3", "name": "Scratch", "color": "#ef4444", "icon": "code", "purgeAt": "2020-01-01T00:00:00Z"},
            {"id": "b4", "name": "Duplicates"},
        ]))
        .expect("trashed boards");
        let issues = serde_json::from_value(serde_json::json!([
            {"id": "i1", "identifier": "EXP-42", "title": "Flaky login redirect", "boardId": null, "purgeAt": (chrono::Utc::now() + chrono::Duration::hours(9 * 24 - 1)).to_rfc3339()},
            {"id": "i2", "identifier": "EXP-7", "title": "Old onboarding copy", "purgeAt": null},
        ]))
        .expect("trashed issues");
        pane.show_with_issues(archived, trash, issues, cx)
    });
}

#[test]
#[ignore]
fn settings_all_boards_empty() {
    shoot_pane("all-boards-empty", AllBoardsPane::for_shots, |pane, cx| pane.show(Vec::new(), Vec::new(), cx));
}
