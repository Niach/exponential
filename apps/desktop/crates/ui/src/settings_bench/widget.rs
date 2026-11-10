use super::*;

fn widget_rows() -> Vec<api::widgets::WidgetConfigSummary> {
    serde_json::from_value(serde_json::json!([
        {"id": "w1", "name": "Docs site", "enabled": true, "submissionCount": 12, "boardName": "Feedback", "allowedDomains": ["docs.example.com", "example.com"], "publicKey": "pk_1"},
        {"id": "w2", "name": "", "enabled": false, "submissionCount": 1, "allowedDomains": []},
    ]))
    .expect("widget rows")
}

#[test]
#[ignore]
fn settings_widget_rows() {
    shoot_pane("widget-rows", WidgetPane::for_shots, |pane, cx| pane.show(Some(widget_rows()), cx));
}

#[test]
#[ignore]
fn settings_widget_empty() {
    shoot_pane("widget-empty", WidgetPane::for_shots, |pane, cx| pane.show(Some(Vec::new()), cx));
}

#[test]
#[ignore]
fn settings_widget_failed() {
    shoot_pane("widget-failed", WidgetPane::for_shots, |pane, cx| pane.show(None, cx));
}
