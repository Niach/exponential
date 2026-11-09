//! VAPP-217 (eApps U2): `record-form` (General components): THE Exponential UI
//! RecordForm macro, painted by the gpui painter from the catalog specimen
//! (`packages/exponential-ui/fixtures/specimens.json` `exponential-ui-record-form`),
//! so the twin cannot drift from the web entry
//! (`apps/styleguide/src/entries/record-form.tsx`). Under it, round 9 §6.6: a
//! field's one message per check severity (the web's `ExponentialUiFieldStates`;
//! error = the invalid state, warning and info advisory).

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use serde_json::{json, Value};

use crate::exponential_ui_screen::{specimen_node, styleguide_surface, styleguide_surface_in};

pub(crate) const ID: &str = "record-form";
pub(crate) const OWNER: &str = "VAPP-217";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let tree = specimen_node("exponential-ui-record-form", "specimen_record_form").expect("the RecordForm specimen");
    let states = field_states(window, cx);
    div().w_full().flex().flex_col().gap(px(24.0)).child(styleguide_surface("sg-record-form", tree, window, cx)).child(states)
}

/// One Input per severity. Its Form's unseen submit reveals the messages
/// right after the first layout (a still photograph, nothing to press).
fn field_states(window: &mut Window, cx: &mut App) -> gpui::Entity<eui_gpui::view::SurfaceView> {
    let field = |id: &str, label: &str, value: &str, path: &str| json!({"id": id, "component": "Input", "props": {"label": label, "name": id, "value": value, "validateOn": "submit", "checks": [{"condition": {"@path": path}}]}});
    let fields: Vec<Value> = vec![field("email", "Email", "ada@", "/error"), field("password", "Password", "hunter2", "/warning"), field("nickname", "Nickname", "ada", "/info")];
    let tree = json!({"id": "field-states", "component": "Form", "props": {"name": "field-states"}, "children": [
        {"id": "fields", "component": "Stack", "props": {"gap": "md"}, "children": fields},
        {"id": "reveal", "component": "Button", "props": {"label": "Check", "submit": true}, "style": {"display": "none"}}
    ]});
    let data = json!({
        "error": {"valid": false, "severity": "error", "message": "Not an email"},
        "warning": {"valid": false, "severity": "warning", "message": "Weak: add a few more characters"},
        "info": {"valid": false, "severity": "info", "message": "Shown on your profile"}
    });
    let view = styleguide_surface_in("sg-field-states", tree, None, Some(data), window, cx);
    let revealed = window.use_keyed_state("sg-field-states-revealed", cx, |_, _| false);
    if !*revealed.read(cx) {
        let view = view.clone();
        window.on_next_frame(move |_, cx| {
            let shown = view.update(cx, |v, cx| {
                let Some(reveal) = v.index_of("reveal") else { return false };
                v.surface_mut().event(reveal, "press", None);
                cx.notify();
                true
            });
            revealed.update(cx, |r, cx| {
                *r = shown;
                cx.notify();
            });
        });
    }
    view
}
