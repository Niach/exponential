//! #EXP-1629: genUI Select option `iconColor`: a status picker's row Select
//! (`triggerIcon`) open, each glyph in its tint (tokens, and one host
//! colour bound from data), the chosen one's leading the value.

use super::*;
use serde_json::json;

pub(super) struct Open {
    view: gpui::Entity<eui_gpui::view::SurfaceView>,
    wait: Option<u8>,
}

impl Open {
    fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let ink = crate::settings::doc_ink(cx);
        let binding = cx.new(|_| eui_gpui::binding::Binding::new("/pane", json!({"blocked": "#ef4444ff"}), |s: &serde_json::Value, _| s.clone()));
        let options = json!([
            {"label": "Backlog", "value": "backlog", "icon": "status-backlog", "iconColor": "$color.mutedForeground"},
            {"label": "In Progress", "value": "progress", "icon": "status-in-progress", "iconColor": "$color.warning"},
            {"label": "Blocked", "value": "blocked", "icon": "status-in-progress", "iconColor": {"@path": "/pane/blocked"}},
            {"label": "In Review", "value": "review", "icon": "status-in-review", "iconColor": "$color.success"},
            {"label": "Do nothing", "value": "none", "icon": "ui-minus", "iconColor": "$color.mutedForeground"},
        ]);
        let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "borderRadius": ink.0, "backgroundColor": ink.1, "overflow": "hidden"}, "children": [
            {"id": "pr", "component": "Select", "props": {"label": "When a PR opens, move issues to", "name": "pr", "triggerVariant": "row", "triggerIcon": true, "popupMinWidth": 170, "value": "blocked", "options": options}},
        ]});
        let view = crate::settings::doc_mount_lifted("bench-select-icon-color", tree, &binding, window, cx);
        Self { view, wait: Some(2) }
    }
}

impl Render for Open {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        match self.wait {
            Some(0) => {
                self.wait = None;
                self.view.update(cx, |view, cx| {
                    if let Some(trigger) = view.index_of("pr.trigger") {
                        view.fire(trigger, "press", None, cx);
                    }
                });
            }
            Some(n) => {
                self.wait = Some(n - 1);
                cx.notify();
            }
            None if !self.view.read(cx).layers().is_empty() => {
                self.view.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx));
            }
            None => {}
        }
        div().w_full().child(self.view.clone())
    }
}

#[test]
#[ignore]
fn settings_select_icon_color() {
    shoot_pane_settled("select-icon-color", Open::new, |_, _| {}, Some(std::time::Duration::from_millis(1200)));
}
