//! Round 8 through the gpui painter (`packages/exponential-ui/docs/
//! round-8-contract.md` §2–4; the Canvas is `tests/canvas.rs`): a mixed
//! Checkbox paints the checked look, a Progress `value` sets tabular
//! figures (measurer and painter alike), and a `collapse: auto` Stepper
//! follows its PARENT's width (`@container`) in gpui's own layout pass.

use eui_core::NestedNode;
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{TestAppContext, VisualTestContext};
use serde_json::{json, Value};

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

fn surface(cx: &mut TestAppContext, tree: Value) -> (gpui::Entity<SurfaceView>, &mut VisualTestContext) {
    cx.update(gpui_component::init);
    let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), mode: eui_core::theme::Mode::Light, ..Default::default() };
    let (view, vcx) = cx.add_window_view(move |window, cx| SurfaceView::new(options, window, cx));
    view.update(vcx, |v, cx| {
        v.record_bounds(true);
        let out = v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
        assert!(out.issues.is_empty(), "{:?}", out.issues);
    });
    draw(vcx);
    draw(vcx);
    (view, vcx)
}

fn shown(view: &gpui::Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> bool {
    view.read_with(cx, |v, _| {
        let i = v.index_of(id).unwrap_or_else(|| panic!("no {id}"));
        v.surface().layout_node(i).is_some_and(|n| !n.hidden) && v.painted_bounds(id).is_some_and(|b| f32::from(b.size.height) > 0.0)
    })
}

#[gpui::test]
fn a_mixed_checkbox_paints_the_checked_fill(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "off", "component": "Checkbox", "props": {"label": "Off", "name": "off"}},
        {"id": "on", "component": "Checkbox", "props": {"label": "On", "name": "on", "checked": true}},
        {"id": "mixed", "component": "Checkbox", "props": {"label": "Mixed", "name": "mixed", "indeterminate": true}}
    ]});
    let (view, cx) = surface(cx, tree);
    let fill = |cx: &mut VisualTestContext, id: &str| view.read_with(cx, |v, _| v.painted_colors(id)).unwrap_or_else(|| panic!("{id} painted")).0;
    let (off, on, mixed) = (fill(cx, "off.box"), fill(cx, "on.box"), fill(cx, "mixed.box"));
    assert_ne!(off, on);
    assert_eq!(mixed, on, "the mixed box wears the checked look");
    assert_eq!(view.read_with(cx, |v, _| v.toggled_state("mixed")), Some(gpui::Toggled::Mixed));
    assert_eq!(view.read_with(cx, |v, _| v.toggled_state("on")), Some(gpui::Toggled::True));
}

#[test]
fn a_progress_value_is_set_in_tabular_figures() {
    assert!(eui_gpui::measure::tabular_text("upload.header.value", None, None));
    assert!(!eui_gpui::measure::tabular_text("upload.header.label", None, None));
}

/// `fontVariantNumeric: tabular-nums` paints (and, through the same
/// `Fonts::for_style`, measures) with the `tnum` feature, inherited from a
/// Box like CSS; a Text without it keeps the font's own figures.
#[gpui::test]
fn font_variant_numeric_tabular_nums_paints_with_tnum(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "age", "component": "Text", "props": {"text": "10m"}, "style": {"fontVariantNumeric": "tabular-nums"}},
        {"id": "col", "component": "Box", "style": {"fontVariantNumeric": "tabular-nums"}, "children": [
            {"id": "kid", "component": "Text", "props": {"text": "3h"}}
        ]},
        {"id": "plain", "component": "Text", "props": {"text": "11"}}
    ]});
    let (view, cx) = surface(cx, tree);
    let font = |cx: &mut VisualTestContext, id: &str| view.read_with(cx, |v, _| v.node_font(id)).unwrap_or_else(|| panic!("{id} has a font"));
    let tnum = |f: &gpui::Font| eui_gpui::measure::with_tabular_nums(f.clone()).features == f.features;
    assert!(tnum(&font(cx, "age")), "the styled Text");
    assert!(tnum(&font(cx, "kid")), "inherited from the Box");
    assert!(!tnum(&font(cx, "plain")), "no key: the font's own figures");
}

#[gpui::test]
fn a_progress_with_value_text_and_detail_paints_its_header_and_caption(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "style": {"width": 360}, "children": [
        {"id": "up", "component": "Progress", "props": {"value": 41, "label": "Uploading", "valueText": "41 MB/s · 12 min", "detail": "3 of 7 files"}}
    ]});
    let (view, cx) = surface(cx, tree);
    for id in ["up.header", "up.header.label", "up.header.value", "up.detail"] {
        assert!(shown(&view, cx, id), "{id} paints");
    }
    // The value sits at the row's end.
    let (row, value) = view.read_with(cx, |v, _| (v.painted_bounds("up.header").unwrap(), v.painted_bounds("up.header.value").unwrap()));
    assert!((f32::from(row.right()) - f32::from(value.right())).abs() < 0.5, "{row:?} {value:?}");
}

fn stepper_in(width: f32) -> Value {
    json!({"id": "root", "component": "Box", "style": {"width": 1200}, "children": [{"id": "col", "component": "Box", "style": {"width": width}, "children": [
        {"id": "wizard", "component": "Stepper", "props": {"steps": [{"label": "Account"}, {"label": "Team"}, {"label": "Done"}], "current": 1, "collapse": "auto"}}
    ]}]})
}

#[gpui::test]
fn an_auto_stepper_follows_its_parent_width(cx: &mut TestAppContext) {
    let (view, cx) = surface(cx, stepper_in(400.0));
    assert!(!shown(&view, cx, "wizard.full"), "a narrow column collapses");
    assert!(shown(&view, cx, "wizard.compact"));
    let line = view.update(cx, |v, _| v.surface_mut().nodes().into_iter().find(|n| n.id == "wizard.compact.line.1").map(|n| n.props["text"].clone()));
    assert_eq!(line, Some(json!("Step 2 of 3 · Team")));
}

#[gpui::test]
fn an_auto_stepper_stays_full_in_a_wide_column(cx: &mut TestAppContext) {
    let (view, cx) = surface(cx, stepper_in(800.0));
    assert!(shown(&view, cx, "wizard.full"));
    assert!(!shown(&view, cx, "wizard.compact"));
}
