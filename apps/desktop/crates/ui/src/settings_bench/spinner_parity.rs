//! #EXP-1629: the IDE's spinner (`feedback::spinner`, muted) vs genUI
//! `Spinner` at the same sizes (xs = the 10 px inline one, sm, default, lg)
//! and phase (Reduce motion freezes both at their start frame). Shoot
//! `settings_spinner_ide`, then `settings_spinner_eui` with
//! `EXP_SETTINGS_BEFORE` at the first: 0 px.
use super::*;
use serde_json::json;

const SIZES: [(&str, f32); 4] = [("xs", 10.), ("sm", 16.), ("default", 20.), ("lg", 24.)];

pub(super) struct Ide;

impl Render for Ide {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        SIZES.iter().fold(gpui_component::h_flex().items_start().gap(px(16.)), |row, (_, side)| row.child(crate::feedback::spinner(*side, cx)))
    }
}

pub(super) struct Eui {
    view: gpui::Entity<eui_gpui::view::SurfaceView>,
}

impl Eui {
    fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let binding = cx.new(|_| eui_gpui::binding::Binding::new("/pane", json!({}), |s: &serde_json::Value, _| s.clone()));
        let children: Vec<serde_json::Value> = SIZES.iter().map(|(size, _)| json!({"id": format!("spin_{size}"), "component": "Spinner", "props": {"size": size}})).collect();
        let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "flex-start", "gap": 16, "width": "100%"}, "children": children});
        let view = crate::settings::doc_mount("bench-spinner", tree, &binding, window, cx);
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
fn settings_spinner_ide() {
    shoot_pane("spinner", |_, cx| {
        cx.set_reduce_motion(true);
        Ide
    }, |_, _| {});
}

#[test]
#[ignore]
fn settings_spinner_eui() {
    shoot_pane("spinner", |window, cx| {
        cx.set_reduce_motion(true);
        Eui::new(window, cx)
    }, |_, _| {});
}
