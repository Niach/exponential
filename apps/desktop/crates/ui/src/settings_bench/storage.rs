use super::*;

/// A Storage list from JSON (the rows the pictures show).
fn storage_list(rows: serde_json::Value, total: i64) -> api::attachments::AttachmentsListForTeamOutput {
    serde_json::from_value(serde_json::json!({"attachments": rows, "totalBytes": total, "draftBytes": 0})).expect("a storage list")
}

fn storage_rows() -> serde_json::Value {
    let row = |id: &str, name: &str, kind: &str, bytes: i64, image: bool, referenced: bool, uploader: Option<&str>| {
        serde_json::json!({"id": id, "issueId": format!("issue-{id}"), "boardId": "board", "uploaderId": uploader, "filename": name, "contentType": kind, "sizeBytes": bytes, "createdAt": "2026-07-01", "isImage": image, "referenced": referenced})
    };
    serde_json::json!([
        row("a1", "screenshot-of-the-settings-pane-after-the-move.png", "image/png", 412_877, true, false, None),
        row("a2", "design-notes.pdf", "application/pdf", 1_048_576, false, true, Some("user-x")),
        row("a3", "walkthrough.mp4", "video/mp4", 23_456_789, false, true, None),
        row("a4", "archive.zip", "application/zip", 2_048, false, false, None),
    ])
}

fn storage(name: &str, load: impl FnOnce() -> StorageLoad + 'static) {
    shoot_pane(name, StoragePane::for_shots, move |pane, cx| pane.show(load(), cx));
}

#[test]
#[ignore]
fn settings_storage_rows() {
    storage("storage-rows", || StorageLoad::Ready(storage_list(storage_rows(), 24_920_290), None));
}

#[test]
#[ignore]
fn settings_storage_meter() {
    storage("storage-meter", || {
        let plan = serde_json::from_value(serde_json::json!({"plan": "pro", "limits": {"storageMb": 1024.0}, "usage": {"storageMb": 23.8}})).expect("a plan");
        StorageLoad::Ready(storage_list(storage_rows(), 24_920_290), Some(plan))
    });
}

#[test]
#[ignore]
fn settings_storage_empty() {
    storage("storage-empty", || StorageLoad::Ready(storage_list(serde_json::json!([]), 0), None));
}

#[test]
#[ignore]
fn settings_storage_failed() {
    storage("storage-failed", || StorageLoad::Failed);
}
