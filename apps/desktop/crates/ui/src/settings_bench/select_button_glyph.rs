//! #EXP-1629: genUI Select `button` trigger: the picked option's glyph leads
//! in its tint (tokens, a host colour bound from data); no glyph, bare.

use super::*;
use serde_json::json;

pub(super) struct Rows {
    view: gpui::Entity<eui_gpui::view::SurfaceView>,
}

impl Rows {
    fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let binding = cx.new(|_| eui_gpui::binding::Binding::new("/pane", json!({"blocked": "#ef4444ff"}), |s: &serde_json::Value, _| s.clone()));
        let options = json!([
            {"label": "In Review", "value": "review", "icon": "status-in-review", "iconColor": "$color.success"},
            {"label": "Blocked", "value": "blocked", "icon": "status-in-progress", "iconColor": {"@path": "/pane/blocked"}},
            {"label": "Do nothing", "value": "none"},
        ]);
        let select = |id: &str, value: &str| json!({"id": id, "component": "Select", "style": {"width": 176}, "props": {"label": id, "name": id, "triggerVariant": "button", "value": value, "options": options}});
        let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": 12, "width": "100%", "alignItems": "flex-end"}, "children": [select("review", "review"), select("blocked", "blocked"), select("none", "none")]});
        let view = crate::settings::doc_mount_lifted("bench-select-button-glyph", tree, &binding, window, cx);
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
fn settings_select_button_glyph() {
    shoot_pane("select-button-glyph", Rows::new, |_, _| {});
}
