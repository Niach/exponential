//! #EXP-1629: genUI Select `row` disabled: the IDE's Effort row locked by
//! ultracode (value + chevron dimmed) over a live row.

use super::*;
use serde_json::json;

pub(super) struct Rows {
    view: gpui::Entity<eui_gpui::view::SurfaceView>,
}

impl Rows {
    fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let ink = crate::settings::doc_ink(cx);
        let binding = cx.new(|_| eui_gpui::binding::Binding::new("/pane", json!({}), |s: &serde_json::Value, _| s.clone()));
        let options = json!([{"label": "High", "value": "high"}, {"label": "Max", "value": "max"}]);
        let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "borderRadius": ink.0, "backgroundColor": ink.1, "overflow": "hidden"}, "children": [
            {"id": "live", "component": "Select", "props": {"label": "Effort", "name": "live", "triggerVariant": "row", "value": "high", "options": options}},
            {"id": "divider", "component": "Box", "style": {"height": 1, "backgroundColor": ink.4}},
            {"id": "locked", "component": "Select", "props": {"label": "Effort", "name": "locked", "triggerVariant": "row", "description": "ultracode sets effort", "disabled": true, "value": "max", "options": options}},
        ]});
        let view = crate::settings::doc_mount_lifted("bench-select-row-disabled", tree, &binding, window, cx);
        Self { view }
    }
}

impl Render for Rows {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div().w_full().child(self.view.clone())
    }
}

#[test]
#[ignore]
fn settings_select_row_disabled() {
    shoot_pane("select-row-disabled", Rows::new, |_, _| {});
}
