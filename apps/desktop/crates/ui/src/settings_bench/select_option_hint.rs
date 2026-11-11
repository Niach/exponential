//! #EXP-1629: genUI Select option `hint`: a `button` Select's menu open, each
//! scale with its small muted hint at the row's end.

use super::*;
use serde_json::json;

pub(super) struct Open {
    view: gpui::Entity<eui_gpui::view::SurfaceView>,
    wait: Option<u8>,
}

impl Open {
    fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let binding = cx.new(|_| eui_gpui::binding::Binding::new("/pane", json!({}), |s: &serde_json::Value, _| s.clone()));
        let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "height": 320, "alignItems": "flex-end"}, "children": [
            {"id": "scale", "component": "Select", "style": {"width": 176}, "props": {"label": "Estimate scale", "name": "scale", "triggerVariant": "button", "align": "end", "value": "tshirt", "options": [
                {"label": "Not in use", "value": "none"},
                {"label": "Exponential", "value": "exponential", "hint": "1, 2, 4, 8, 16 points"},
                {"label": "Fibonacci", "value": "fibonacci", "hint": "1, 2, 3, 5, 8 points"},
                {"label": "T-shirt", "value": "tshirt", "hint": "XS, S, M, L, XL"},
            ]}},
        ]});
        let view = crate::settings::doc_mount_lifted("bench-select-option-hint", tree, &binding, window, cx);
        Self { view, wait: Some(2) }
    }
}

impl Render for Open {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        match self.wait {
            Some(0) => {
                self.wait = None;
                self.view.update(cx, |view, cx| {
                    let trigger = view.index_of("scale.trigger").expect("the trigger");
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
fn settings_select_option_hint() {
    shoot_pane_settled("select-option-hint", Open::new, |_, _| {}, Some(std::time::Duration::from_millis(1200)));
}
