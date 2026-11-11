//! #EXP-1629: the genUI Select `button` menu's metrics against web's picker
//! (`packages/ui` Combobox): the menu open, a status list at web's `sm` width.

use super::*;
use serde_json::json;

pub(super) struct Open {
    view: gpui::Entity<eui_gpui::view::SurfaceView>,
    wait: Option<u8>,
}

impl Open {
    fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let binding = cx.new(|_| eui_gpui::binding::Binding::new("/pane", json!({}), |s: &serde_json::Value, _| s.clone()));
        let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "height": 360, "alignItems": "flex-end"}, "children": [
            {"id": "target", "component": "Select", "style": {"width": 176}, "props": {"label": "When a PR opens", "name": "target", "triggerVariant": "button", "align": "end", "popupWidth": 224, "value": "review", "options": [
                {"label": "Backlog", "value": "backlog", "icon": "status-backlog", "iconColor": "$color.mutedForeground"},
                {"label": "In Progress", "value": "progress", "icon": "status-in-progress", "iconColor": "$color.warning"},
                {"label": "In Review", "value": "review", "icon": "status-in-review", "iconColor": "$color.success"},
                {"label": "Done", "value": "done", "icon": "status-done", "iconColor": "$color.info"},
                {"label": "Do nothing", "value": "none"},
            ]}},
        ]});
        let view = crate::settings::doc_mount_lifted("bench-select-button-menu", tree, &binding, window, cx);
        Self { view, wait: Some(2) }
    }
}

impl Render for Open {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        match self.wait {
            Some(0) => {
                self.wait = None;
                self.view.update(cx, |view, cx| {
                    let trigger = view.index_of("target.trigger").expect("the trigger");
                    view.fire(trigger, "press", None, cx);
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
fn settings_select_button_menu() {
    shoot_pane_settled("select-button-menu", Open::new, |_, _| {}, Some(std::time::Duration::from_millis(1200)));
}
