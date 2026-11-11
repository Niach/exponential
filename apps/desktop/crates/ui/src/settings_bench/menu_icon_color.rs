//! #EXP-1629: genUI Menu item `iconColor`: the status picker's rows open,
//! each glyph in its tint (tokens, and one host colour bound from data).

use super::*;
use serde_json::json;

pub(super) struct Open {
    view: gpui::Entity<eui_gpui::view::SurfaceView>,
    wait: Option<u8>,
}

impl Open {
    fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let items = json!([
            {"label": "Backlog", "value": "backlog", "kind": "checkbox", "icon": "status-backlog", "iconColor": "$color.mutedForeground"},
            {"label": "In Progress", "value": "progress", "kind": "checkbox", "checked": true, "icon": "status-in-progress", "iconColor": "$color.warning"},
            {"label": "Blocked", "value": "blocked", "kind": "checkbox", "icon": "status-in-progress", "iconColor": "#ef4444ff"},
            {"label": "Done", "value": "done", "kind": "checkbox", "icon": "status-done", "iconColor": "$color.success"},
            {"label": "Do nothing", "value": "none", "kind": "checkbox", "icon": "ui-minus", "iconColor": "$color.mutedForeground"},
        ]);
        let binding = cx.new(|_| eui_gpui::binding::Binding::new("/pane", json!({"items": items}), |s: &serde_json::Value, _| s.clone()));
        let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "justifyContent": "flex-end", "width": "100%", "height": 320}, "children": [
            {"id": "menu", "component": "Menu", "props": {"items": {"@path": "/pane/items"}, "align": "end"}, "children": [
                {"id": "trigger", "component": "Button", "props": {"label": "In Progress", "icon": "status-in-progress", "variant": "outline"}},
            ]},
        ]});
        let view = crate::settings::doc_mount_lifted("bench-menu-icon-color", tree, &binding, window, cx);
        Self { view, wait: Some(2) }
    }
}

impl Render for Open {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        match self.wait {
            Some(0) => {
                self.wait = None;
                self.view.update(cx, |view, cx| {
                    if let Some(trigger) = view.index_of("trigger") {
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
fn settings_menu_icon_color() {
    shoot_pane_settled("menu-icon-color", Open::new, |_, _| {}, Some(std::time::Duration::from_millis(1200)));
}
