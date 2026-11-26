//! #EXP-2161: genUI Select option `section`: a `row` Select's popup open,
//! the member's own machine first, then each team's devices under its
//! muted heading (web's picker group heading).

use super::*;
use serde_json::json;

pub(super) struct Open {
    view: gpui::Entity<eui_gpui::view::SurfaceView>,
    wait: Option<u8>,
}

impl Open {
    fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let binding = cx.new(|_| eui_gpui::binding::Binding::new("/pane", json!({}), |s: &serde_json::Value, _| s.clone()));
        let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "height": 360}, "children": [
            {"id": "runs_on", "component": "Group", "children": [
                {"id": "device", "component": "Select", "props": {"label": "Device", "name": "device", "triggerVariant": "row", "popupMinWidth": 170, "value": "mint", "options": [
                    {"label": "build", "value": "acme:build", "icon": "server", "description": "Offline", "disabled": true, "section": "Acme"},
                    {"label": "mint", "value": "mint", "icon": "monitor"},
                    {"label": "unraid", "value": "acme:unraid", "icon": "server", "section": "Acme"},
                    {"label": "build", "value": "side:build", "icon": "server", "description": "Offline", "disabled": true, "section": "Side project"},
                ]}},
            ]},
        ]});
        let view = crate::settings::doc_mount_lifted("bench-select-option-section", tree, &binding, window, cx);
        Self { view, wait: Some(2) }
    }
}

impl Render for Open {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        match self.wait {
            Some(0) => {
                self.wait = None;
                self.view.update(cx, |view, cx| {
                    let trigger = view.index_of("device.trigger").expect("the trigger");
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
fn settings_select_option_section() {
    shoot_pane_settled("select-option-section", Open::new, |_, _| {}, Some(std::time::Duration::from_millis(1200)));
}
