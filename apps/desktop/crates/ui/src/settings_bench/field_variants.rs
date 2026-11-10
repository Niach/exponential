//! #EXP-1629: the IDE's chromeless group fields vs genUI Textarea
//! `row`/`code` and Input `code`/`bare`: the agent instructions (`row`, 4–16
//! rows), the skill description (`row`, empty, 2–6), the SKILL.md / prompt
//! body (`code`, 14–40), the skill name after its glyph (Input `code`), the
//! agent / board / prompt name (Input `bare`) and the team prompt while it
//! loads (`code`, disabled, 10–40). Shoot `settings_fields_ide`, then
//! `settings_fields_eui` with `EXP_SETTINGS_BEFORE` at the first: 0 px.
use super::*;
use gpui_component::input::{InputState, Textarea, TextareaState};
use serde_json::json;

const AREA: &str = "Triage new issues every morning.\nLabel them and ping the owner of the board.";
const CODE: &str = "---\nname: review\n---\n# Review\nRun the checks, then summarise the diff.";
const NAME: &str = "review-pr";

pub(super) struct Ide {
    area: gpui::Entity<TextareaState>,
    empty: gpui::Entity<TextareaState>,
    code: gpui::Entity<TextareaState>,
    name: gpui::Entity<InputState>,
    plain: gpui::Entity<InputState>,
    off: gpui::Entity<TextareaState>,
}

impl Ide {
    fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let area = cx.new(|cx| crate::controls::web_textarea(4, 16, window, cx).placeholder("Instructions"));
        area.update(cx, |s, cx| s.set_value(AREA, window, cx));
        let empty = cx.new(|cx| crate::controls::web_textarea(2, 6, window, cx).placeholder("What it does"));
        let code = cx.new(|cx| crate::controls::web_textarea(14, 40, window, cx).placeholder("SKILL.md"));
        code.update(cx, |s, cx| s.set_value(CODE, window, cx));
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("name"));
        name.update(cx, |s, cx| s.set_value(NAME, window, cx));
        let plain = cx.new(|cx| InputState::new(window, cx).placeholder("Agent name"));
        plain.update(cx, |s, cx| s.set_value("Triage", window, cx));
        let off = cx.new(|cx| crate::controls::web_textarea(10, 40, window, cx));
        off.update(cx, |s, cx| s.set_value("You are the team's agent.", window, cx));
        Self { area, empty, code, name, plain, off }
    }
}

impl Render for Ide {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let name_row = crate::surface::glass_row_shell().child(gpui_component::Icon::from(crate::icons::registry::UI_DOCS).text_color(muted)).child(
            div().flex_1().min_w_0().font_family(theme::terminal::FONT_FAMILY).child(crate::controls::glass_input(&self.name, window, cx).appearance(false).h_auto().px_0().py_0()),
        );
        let plain_row = crate::surface::glass_row_shell().gap_2().child(
            div().flex_1().min_w_0().child(crate::surface::glass_row_input(crate::controls::glass_input(&self.plain, window, cx)).text_left()),
        );
        gpui_component::v_flex().w_full().gap_4()
            .child(crate::surface::glass_group().child(Textarea::new(&self.area).appearance(false).w_full().px_4().py_3()))
            .child(crate::surface::glass_group().child(Textarea::new(&self.empty).appearance(false).w_full().px_4().py_3().text_sm()))
            .child(crate::surface::glass_group().child(Textarea::new(&self.code).appearance(false).w_full().min_h(px(256.)).px_4().py_3().text_xs().font_family(theme::terminal::FONT_FAMILY)))
            .child(crate::surface::glass_group_rows(vec![name_row, plain_row]))
            .child(crate::surface::glass_group().child(Textarea::new(&self.off).appearance(false).w_full().px_4().py_3().font_family(theme::terminal::FONT_FAMILY).text_xs().disabled(true)))
    }
}

pub(super) struct Eui {
    view: gpui::Entity<eui_gpui::view::SurfaceView>,
}

impl Eui {
    fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let ink = crate::settings::doc_ink(cx);
        let state = json!({"area": AREA, "empty": "", "code": CODE, "name": NAME, "plain": "Triage", "off": "You are the team's agent."});
        let binding = cx.new(|_| eui_gpui::binding::Binding::new("/pane", state, |s: &serde_json::Value, _| s.clone()));
        let group = |id: &str, child: serde_json::Value| json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "borderRadius": ink.0, "backgroundColor": ink.1, "overflow": "hidden"}, "children": [child]});
        let glyph = ink.2 * 1.0;
        let shell = |id: &str, gap: f32, children: Vec<serde_json::Value>| json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": gap, "paddingHorizontal": ink.2, "paddingVertical": ink.2 * 0.75}, "children": children});
        let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.2, "width": "100%"}, "children": [
            group("g1", json!({"id": "area", "component": "Textarea", "props": {"label": "Instructions", "name": "area", "variant": "row", "rows": 4, "autosize": true, "maxRows": 16, "placeholder": "Instructions", "value": {"@path": "/pane/area"}}})),
            group("g2", json!({"id": "empty", "component": "Textarea", "props": {"label": "Description", "name": "empty", "variant": "row", "rows": 2, "autosize": true, "maxRows": 6, "placeholder": "What it does", "value": {"@path": "/pane/empty"}}})),
            group("g3", json!({"id": "code", "component": "Textarea", "props": {"label": "Body", "name": "code", "variant": "code", "rows": 14, "autosize": true, "maxRows": 40, "placeholder": "SKILL.md", "value": {"@path": "/pane/code"}}})),
            {"id": "g4", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "borderRadius": ink.0, "backgroundColor": ink.1, "overflow": "hidden"}, "children": [
                shell("r1", ink.2 * 0.75, vec![
                    json!({"id": "r1_icon", "component": "Icon", "props": {"name": "ui-docs"}, "style": {"width": glyph, "height": glyph, "color": ink.3, "flexShrink": 0}}),
                    json!({"id": "name", "component": "Input", "props": {"label": "Name", "name": "name", "variant": "code", "placeholder": "name", "value": {"@path": "/pane/name"}}, "style": {"flexGrow": 1, "minWidth": 0}}),
                ]),
                {"id": "div", "component": "Box", "style": {"height": 1, "flexShrink": 0, "backgroundColor": ink.4}},
                shell("r2", ink.2 * 0.5, vec![
                    json!({"id": "plain", "component": "Input", "props": {"label": "Name", "name": "plain", "variant": "bare", "placeholder": "Agent name", "value": {"@path": "/pane/plain"}}, "style": {"flexGrow": 1, "minWidth": 0}}),
                ]),
            ]},
            group("g5", json!({"id": "off", "component": "Textarea", "props": {"label": "Prompt", "name": "off", "variant": "code", "rows": 10, "autosize": true, "maxRows": 40, "disabled": true, "value": {"@path": "/pane/off"}}})),
        ]});
        let view = crate::settings::doc_mount("bench-fields", tree, &binding, window, cx);
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
fn settings_fields_ide() {
    shoot_pane("fields", Ide::new, |_, _| {});
}

#[test]
#[ignore]
fn settings_fields_eui() {
    shoot_pane("fields", Eui::new, |_, _| {});
}
