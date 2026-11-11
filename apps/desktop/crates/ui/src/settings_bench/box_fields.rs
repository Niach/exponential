//! #EXP-1629: the IDE's boxed field (`glass_input` at `.web_input_sm()` and
//! `.web_input()`, a 176 px one beside a flex one, one empty with its
//! placeholder, one with a value) against genUI's Input `variant: "box"`
//! at `size` `sm` and `default`: the same picture.
use super::*;
use crate::controls::WebControl as _;
use gpui_component::input::InputState;
use serde_json::json;

const VALUE: &str = "ada@example.com";

pub(super) struct Ide {
    fields: Vec<gpui::Entity<InputState>>,
}

impl Ide {
    fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let mut fields = Vec::new();
        for _ in 0..2 {
            fields.push(cx.new(|cx| InputState::new(window, cx).placeholder("Name")));
            let value = cx.new(|cx| InputState::new(window, cx).placeholder("teammate@example.com"));
            value.update(cx, |s, cx| s.set_value(VALUE, window, cx));
            fields.push(value);
        }
        Self { fields }
    }
}

impl Render for Ide {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let f = &self.fields;
        let sm = gpui_component::h_flex().w_full().gap_2().items_center()
            .child(div().w(px(176.)).flex_shrink_0().child(crate::controls::glass_input(&f[0], window, cx).web_input_sm()))
            .child(div().flex_1().min_w_0().child(crate::controls::glass_input(&f[1], window, cx).web_input_sm()));
        let md = gpui_component::h_flex().w_full().gap_2().items_center()
            .child(div().w(px(176.)).flex_shrink_0().child(crate::controls::glass_input(&f[2], window, cx).web_input()))
            .child(div().flex_1().min_w_0().child(crate::controls::glass_input(&f[3], window, cx).web_input()));
        gpui_component::v_flex().w_full().gap_4().child(sm).child(md)
    }
}

pub(super) struct Eui {
    view: gpui::Entity<eui_gpui::view::SurfaceView>,
}

impl Eui {
    fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        // The IDE's `gap_2` / `gap_4`: half a rem, one rem.
        let rem = crate::settings::doc_ink(cx).2;
        let state = json!({"a": "", "b": VALUE, "c": "", "d": VALUE});
        let binding = cx.new(|_| eui_gpui::binding::Binding::new("/pane", state, |s: &serde_json::Value, _| s.clone()));
        let pair = |id: &str, size: &str, a: &str, b: &str| json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": rem * 0.5, "width": "100%"}, "children": [
            {"id": a, "component": "Input", "props": {"label": "Name", "name": a, "variant": "box", "size": size, "placeholder": "Name", "value": {"@path": format!("/pane/{a}")}}, "style": {"width": 176, "flexShrink": 0}},
            {"id": b, "component": "Input", "props": {"label": "Email", "name": b, "variant": "box", "size": size, "placeholder": "teammate@example.com", "value": {"@path": format!("/pane/{b}")}}, "style": {"flexGrow": 1, "minWidth": 0}},
        ]});
        let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": rem, "width": "100%"}, "children": [
            pair("sm", "sm", "a", "b"),
            pair("md", "default", "c", "d"),
        ]});
        let view = crate::settings::doc_mount("bench-box-fields", tree, &binding, window, cx);
        Self { view }
    }
}

impl Render for Eui {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div().w_full().child(self.view.clone())
    }
}

#[test]
#[ignore]
fn settings_box_fields_ide() {
    shoot_pane("box-fields", Ide::new, |_, _| {});
}

#[test]
#[ignore]
fn settings_box_fields_eui() {
    shoot_pane("box-fields", Eui::new, |_, _| {});
}
