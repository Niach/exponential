//! Round 5 (`packages/exponential-ui/docs/round-5-contract.md`, VAPP-162)
//! through the painter in a headless gpui window: QrCode (the core's
//! matrix, the lighter colour on the plate, the label under it), QrScanner
//! (the placeholder without a camera, the host feed hook, `scanned` once per
//! distinct value), VoiceWave `bars` (the core's rects, no child), the
//! Segmented rail + count badge, Button `hold` (interactions-round1.json
//! `buttonHold` replayed through pointer and keys), the serif family, and
//! the new macros (MessageBubble, CallSurface, Plan) painting from natives.

use std::cell::RefCell;
use std::rc::Rc;

use eui_core::NestedNode;
use eui_gpui::host::{ActionEvent, HostPlugin};
use eui_gpui::paint::{natives, qr};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{div, prelude::*, px, App, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext, Window};
use serde_json::{json, Value};

#[derive(Clone, Default)]
struct Recorder {
    actions: Rc<RefCell<Vec<ActionEvent>>>,
    /// Some = a camera host: the feed asks it saw (component id, formats).
    camera: Option<Rc<RefCell<Vec<(String, Vec<String>)>>>>,
}

impl HostPlugin for Recorder {
    fn on_action(&self, event: &ActionEvent, _cx: &mut App) {
        self.actions.borrow_mut().push(event.clone());
    }
    fn qr_scanner_feed(&self, component_id: &str, formats: &[String], _window: &mut Window, _cx: &mut App) -> Option<gpui::AnyElement> {
        let asks = self.camera.as_ref()?;
        asks.borrow_mut().push((component_id.to_string(), formats.to_vec()));
        Some(div().size_full().bg(gpui::black()).into_any_element())
    }
}

struct Pane {
    view: Entity<SurfaceView>,
}

impl Render for Pane {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div().size_full().child(self.view.clone())
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

fn mounted(cx: &mut TestAppContext, tree: Value, log: Recorder, mode: eui_core::theme::Mode) -> (Entity<SurfaceView>, &mut VisualTestContext) {
    cx.update(gpui_component::init);
    let host = Rc::new(log);
    let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), mode, host, ..Default::default() };
    let (pane, vcx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| SurfaceView::new(options, window, cx));
        view.update(cx, |v, cx| {
            let out = v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
            assert!(out.issues.is_empty(), "{:?}", out.issues);
            v.record_bounds(true);
        });
        Pane { view }
    });
    draw(vcx);
    draw(vcx);
    let view = pane.read_with(vcx, |p, _| p.view.clone());
    (view, vcx)
}

fn surface(cx: &mut TestAppContext, tree: Value) -> (Entity<SurfaceView>, Recorder, &mut VisualTestContext) {
    let log = Recorder::default();
    let (view, vcx) = mounted(cx, tree, log.clone(), eui_core::theme::Mode::Light);
    (view, log, vcx)
}

fn frame(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> eui_core::surface::Frame {
    view.read_with(cx, |v, _| v.frame(v.index_of(id).unwrap_or_else(|| panic!("no {id}"))).unwrap())
}

fn painted(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> gpui::Bounds<gpui::Pixels> {
    view.read_with(cx, |v, _| v.painted_bounds(id)).unwrap_or_else(|| panic!("{id} not painted"))
}

fn specimen(id: &str) -> Value {
    let doc: Value = serde_json::from_str(include_str!("../../../../../packages/exponential-ui/fixtures/specimens.json")).unwrap();
    doc["specimens"].as_array().unwrap().iter().find(|s| s["id"] == id).unwrap_or_else(|| panic!("no specimen {id}"))["node"].clone()
}

fn caption_lh() -> f32 {
    // `$type.lineHeight.xs` in the neutral theme (the QrCode label, the badge).
    16.0
}

#[gpui::test]
fn a_qr_code_paints_the_cores_matrix_on_a_light_plate_with_its_label(cx: &mut TestAppContext) {
    qr::record_qr(true);
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "alignItems": "flex-start", "gap": 16}, "children": [
        {"id": "pair", "component": "QrCode", "props": {"value": "spezl://pair?host=macbook.local&key=Zm9vYmFy", "label": "Scan to pair"}},
        {"id": "small", "component": "QrCode", "props": {"value": "HELLO", "level": "H", "size": 120}},
        {"id": "empty", "component": "QrCode", "props": {"value": ""}}
    ]});
    let (view, _log, cx) = surface(cx, tree);
    let (pair, small, empty) = (frame(&view, cx, "pair"), frame(&view, cx, "small"), frame(&view, cx, "empty"));
    assert_eq!(pair.h, 160.0 + 8.0 + caption_lh(), "the plate, labelGap, one caption line");
    assert!(pair.w >= 160.0);
    assert_eq!((small.w, small.h), (120.0, 120.0), "no label: just the plate");
    assert_eq!((empty.w, empty.h), (160.0, 160.0), "an empty value keeps the empty plate");
    let painted = qr::take_qr();
    let get = |id: &str| painted.iter().rev().find(|p| p.id == id).cloned().unwrap_or_else(|| panic!("{id} not painted"));
    let want = eui_core::qr_code::qr_matrix("HELLO", eui_core::qr_code::QrLevel::H).unwrap();
    let s = get("small");
    assert_eq!((s.modules, s.size), (want.size, 120.0));
    assert_eq!(s.dark, want.rows.iter().map(|r| r.bytes().filter(|b| *b == b'1').count()).sum::<usize>(), "every dark module of the core");
    assert!(qr::luminance(s.plate) > qr::luminance(s.module), "dark modules on a light plate");
    assert_eq!(get("empty").modules, 0, "no matrix: the plate only");
    assert!(get("pair").modules >= 25, "a longer value takes a larger version");
    let a11y = view.read_with(cx, |v, _| v.accessible_info("pair")).expect("an img");
    assert_eq!((a11y.role, a11y.label.as_deref()), (gpui::Role::Image, Some("Scan to pair")));
    assert!(view.read_with(cx, |v, _| v.accessible_info("small")).is_none(), "no label = decorative");
    qr::record_qr(false);
}

#[gpui::test]
fn a_dark_theme_still_paints_dark_modules_on_the_lighter_plate(cx: &mut TestAppContext) {
    qr::record_qr(true);
    let tree = json!({"id": "root", "component": "Box", "children": [{"id": "q", "component": "QrCode", "props": {"value": "HELLO"}}]});
    let (_view, _cx) = mounted(cx, tree, Recorder::default(), eui_core::theme::Mode::Dark);
    let p = qr::take_qr().into_iter().rev().find(|p| p.id == "q").expect("painted");
    assert!(qr::luminance(p.plate) > qr::luminance(p.module), "the lighter colour fills the plate in dark mode too");
    qr::record_qr(false);
}

fn scanner_tree() -> Value {
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "alignItems": "flex-start", "gap": 16}, "children": [
        {"id": "scan", "component": "QrScanner", "props": {"hint": "Point at the code on your computer", "formats": ["qr"]}, "style": {"width": 320}, "on": {"scanned": {"event": {"name": "paired", "context": {"code": {"path": "/x"}}}}}},
        {"id": "off", "component": "QrScanner", "props": {"hint": "Paused", "disabled": true}, "style": {"width": 320}, "on": {"scanned": {"event": {"name": "paired"}}}}
    ]})
}

#[gpui::test]
fn a_qr_scanner_without_a_camera_shows_the_placeholder_chrome(cx: &mut TestAppContext) {
    let (view, _log, cx) = surface(cx, scanner_tree());
    let f = frame(&view, cx, "scan");
    assert_eq!(f.w, 320.0, "the author's width");
    // The 280 px viewfinder (maxWidth), the root gap, the hint's lines (the
    // test text system has no Geist, so the line count is the stand-in's).
    let hint = f.h - (280.0 + 12.0);
    assert!(hint >= 20.0 && (hint % 20.0).abs() < 1e-3, "viewfinder + gap + whole hint lines: {}", f.h);
    let off = frame(&view, cx, "off");
    assert_eq!(off.h, 280.0 + 12.0 + 20.0, "a one-word hint is one line");
    let a11y = view.read_with(cx, |v, _| v.accessible_info("scan")).expect("an img");
    assert_eq!((a11y.role, a11y.label.as_deref()), (gpui::Role::Image, eui_core::strings::default_string("scanQrCode")), "the default label");
}

#[gpui::test]
fn the_host_camera_feeds_the_viewfinder_and_scanned_fires_once_per_value(cx: &mut TestAppContext) {
    let asks = Rc::new(RefCell::new(Vec::new()));
    let log = Recorder { camera: Some(asks.clone()), ..Default::default() };
    let (view, cx) = mounted(cx, scanner_tree(), log.clone(), eui_core::theme::Mode::Light);
    assert!(asks.borrow().iter().any(|(id, f)| id == "scan" && f == &vec!["qr".to_string()]), "the enabled scanner asks the host for its feed");
    assert!(!asks.borrow().iter().any(|(id, _)| id == "off"), "a disabled scanner stops the camera");
    let fire = |cx: &mut VisualTestContext, id: &str, v: &str| cx.update(|_, cx| view.update(cx, |view, cx| view.qr_scanned(id, v, cx)));
    assert!(fire(cx, "scan", "spezl://pair?key=1"));
    assert!(!fire(cx, "scan", "spezl://pair?key=1"), "a code held in view reports once");
    assert!(fire(cx, "scan", "spezl://pair?key=2"), "a new value reports");
    assert!(!fire(cx, "off", "spezl://pair?key=1"), "disabled stops reporting");
    let actions = log.actions.borrow();
    let paired: Vec<&ActionEvent> = actions.iter().filter(|a| a.name == "paired").collect();
    assert_eq!(paired.len(), 2);
    assert_eq!(view.read_with(cx, |v, _| v.announcement()).map(|a| a.0.to_string()).as_deref(), Some("spezl://pair?key=2"), "the decoded value is announced");
}

#[gpui::test]
fn voice_wave_bars_paint_the_cores_rects_and_no_child(cx: &mut TestAppContext) {
    natives::record_voice_wave(true);
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "alignItems": "flex-start"}, "children": [
        {"id": "md", "component": "VoiceWave", "props": {"variant": "bars", "size": "md", "activity": "speaking", "level": 0.7, "bands": [0.9, 0.5, 1, 0.6], "label": "Voice"}},
        {"id": "xl", "component": "VoiceWave", "props": {"variant": "bars", "size": "xl", "activity": "idle"}, "children": [{"id": "kid", "component": "Text", "props": {"text": "Child"}}]}
    ]});
    let settings = eui_core::surface::SurfaceSettings { reduced_motion: true, ..Default::default() };
    cx.update(gpui_component::init);
    let host = Rc::new(Recorder::default());
    let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), host, settings: Some(settings), ..Default::default() };
    let (pane, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| SurfaceView::new(options, window, cx));
        view.update(cx, |v, cx| {
            v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
            v.record_bounds(true);
        });
        Pane { view }
    });
    draw(cx);
    draw(cx);
    let view = pane.read_with(cx, |p, _| p.view.clone());
    let (md, xl) = (frame(&view, cx, "md"), frame(&view, cx, "xl"));
    assert_eq!((md.w, md.h), (117.0, 24.0), "voiceWave.bars.sizes.md");
    assert_eq!((xl.w, xl.h), (195.0, 56.0), "voiceWave.bars.sizes.xl");
    let bars = natives::take_voice_wave_bars();
    let m = bars.iter().rev().find(|b| b.id == "md").expect("md painted");
    let want = eui_core::voice_wave::voice_wave_bars(&eui_core::voice_wave::VoiceWaveBarsInput {
        size: "md".into(),
        activity: eui_core::voice_wave::Activity::Speaking,
        level: Some(0.7),
        bands: Some(vec![0.9, 0.5, 1.0, 0.6]),
        elapsed: Some(0.0),
        reduced_motion: Some(true),
    });
    assert_eq!(m.bars.len(), 20);
    for (got, w) in m.bars.iter().zip(&want.bars) {
        assert_eq!(*got, (w.x as f32, w.y as f32, w.width as f32, w.height as f32, w.radius as f32));
    }
    assert!(natives::take_voice_wave().iter().all(|r| r.id != "md" && r.id != "xl"), "no rings for bars");
    assert!(view.read_with(cx, |v, _| v.painted_bounds("kid")).is_none(), "the child stays unpainted");
    natives::record_voice_wave(false);
}

#[gpui::test]
fn a_vertical_bar_is_the_rail_with_count_badges(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "height": 400}, "children": [
        {"id": "rail", "component": "Segmented", "props": {"variant": "bar", "orientation": "vertical", "value": "chats", "items": [
            {"label": "Chats", "value": "chats", "icon": "message-circle", "count": 3},
            {"label": "Voices", "value": "voices", "icon": "mic"},
            {"label": "Library", "value": "library", "icon": "layers", "count": 120},
            {"label": "Friends", "value": "friends", "icon": "users"}]},
         "on": {"change": {"event": {"name": "go"}}}},
        {"id": "seg", "component": "Segmented", "props": {"orientation": "vertical", "value": "a", "items": [{"label": "Alpha", "value": "a"}, {"label": "B", "value": "b"}]}}
    ]});
    let (view, log, cx) = surface(cx, tree);
    let rail = frame(&view, cx, "rail");
    assert_eq!((rail.w, rail.h), (72.0, 4.0 * 56.0), "72 wide, items × $control.tabBar, not stretched");
    let seg = frame(&view, cx, "seg");
    assert!(seg.h >= 2.0 * 32.0 && seg.h > seg.w / 2.0, "a vertical segmented stacks its segments: {seg:?}");
    // The third item's centre: items stack top to bottom, 56 each.
    let b = painted(&view, cx, "rail");
    cx.simulate_click(gpui::point(b.origin.x + px(36.0), b.origin.y + px(56.0 * 2.5)), Modifiers::default());
    draw(cx);
    let go = log.actions.borrow().iter().find(|a| a.name == "go").cloned().expect("change fired");
    assert_eq!(go.context["value"], json!("library"));
    assert_eq!(eui_core::segmented::item_accessibility_label("Library", eui_core::segmented::option_badge(Some("bar"), &json!({"count": 120})).as_deref()), "Library, 99+");
}

fn hold_fixture() -> Vec<Value> {
    let doc: Value = serde_json::from_str(include_str!("../../../../../packages/exponential-ui/fixtures/interactions-round1.json")).unwrap();
    doc["buttonHold"].as_array().unwrap().clone()
}

/// interactions-round1.json `buttonHold`, every case through the painter:
/// pointer steps as window mouse events at the button's painted box,
/// keys through the focused button, `blur` by moving focus away.
#[gpui::test]
fn button_hold_replays_the_fixture_through_pointer_and_keys(cx: &mut TestAppContext) {
    use eui_core::hold::HoldInput;
    cx.update(gpui_component::init);
    let cases = hold_fixture();
    assert_eq!(cases.len(), 10);
    for c in &cases {
        let name = c["name"].as_str().unwrap().to_string();
        let mut props = json!({"label": "Hold to talk", "holdingLabel": "Holding…", "hold": true}).as_object().cloned().unwrap();
        for (k, v) in c.get("props").and_then(Value::as_object).into_iter().flatten() {
            if v.is_null() {
                props.remove(k);
            } else {
                props.insert(k.clone(), v.clone());
            }
        }
        let on = json!({"press": {"event": {"name": "press"}}, "holdStart": {"event": {"name": "holdStart"}}, "holdEnd": {"event": {"name": "holdEnd"}}, "holdCancel": {"event": {"name": "holdCancel"}}});
        let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "alignItems": "flex-start", "padding": 60, "gap": 8}, "children": [
            {"id": "b", "component": "Button", "props": props, "on": on, "style": {"width": 120, "height": 40}},
            {"id": "other", "component": "Button", "props": {"label": "Other"}, "on": {"press": {"event": {"name": "other"}}}}
        ]});
        let log = Recorder::default();
        let host = Rc::new(log.clone());
        let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), host, ..Default::default() };
        let (pane, vcx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| SurfaceView::new(options, window, cx));
            view.update(cx, |v, cx| {
                v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
                v.record_bounds(true);
            });
            Pane { view }
        });
        draw(vcx);
        draw(vcx);
        let view = pane.read_with(vcx, |p, _| p.view.clone());
        let origin = painted(&view, vcx, "b").origin;
        let at = |s: &Value| gpui::point(origin.x + px(s["x"].as_f64().unwrap_or(0.0) as f32), origin.y + px(s["y"].as_f64().unwrap_or(0.0) as f32));
        let keyed = c["steps"].as_array().unwrap().iter().any(|s| matches!(s["do"].as_str(), Some("keydown" | "keyup" | "blur")));
        if keyed {
            let focused = vcx.update(|window, cx| view.update(cx, |v, cx| v.focus_next(false, window, cx)));
            assert_eq!(view.read_with(vcx, |v, _| focused.and_then(|i| v.surface().layout_node(i).map(|n| n.id.clone()))).as_deref(), Some("b"), "{name}: the hold button takes focus");
        }
        for (k, step) in c["steps"].as_array().unwrap().iter().enumerate() {
            let before = log.actions.borrow().len();
            match step["do"].as_str().unwrap() {
                "pointerdown" => vcx.simulate_mouse_down(at(step), MouseButton::Left, Modifiers::default()),
                "pointermove" => vcx.simulate_mouse_move(at(step), MouseButton::Left, Modifiers::default()),
                "pointerup" => vcx.simulate_mouse_up(at(step), MouseButton::Left, Modifiers::default()),
                "pointercancel" => {
                    let i = view.read_with(vcx, |v, _| v.index_of("b").unwrap());
                    vcx.update(|_, cx| view.update(cx, |v, cx| v.hold_input(i, HoldInput::PointerCancel, cx)));
                }
                "keydown" => {
                    let key = match step["key"].as_str().unwrap() {
                        " " => "space",
                        "Enter" => "enter",
                        "Escape" => "escape",
                        other => panic!("{other}"),
                    };
                    if step["repeat"].as_bool() == Some(true) {
                        // A repeat: a second key down on the held button
                        // through the window (the view drops gpui's
                        // `is_held` repeats before they reach the hold; a
                        // fresh down on an active hold fires nothing either).
                        vcx.simulate_keystrokes(key);
                    } else {
                        vcx.update(|window, cx| view.update(cx, |v, cx| v.handle_key(key, false, None, window, cx)));
                    }
                }
                "keyup" => {
                    let key = match step["key"].as_str().unwrap() {
                        " " => "space",
                        "Enter" => "enter",
                        other => panic!("{other}"),
                    };
                    vcx.update(|_, cx| view.update(cx, |v, cx| v.key_up(key, cx)));
                }
                "blur" => {
                    vcx.update(|window, cx| view.update(cx, |v, cx| v.focus_next(false, window, cx)));
                }
                other => panic!("{name}: unknown step {other}"),
            }
            draw(vcx);
            let fired: Vec<String> = log.actions.borrow()[before..].iter().map(|a| a.name.clone()).collect();
            let want: Vec<String> = step["events"].as_array().unwrap().iter().map(|e| e.as_str().unwrap().to_string()).collect();
            assert_eq!(fired, want, "{name} step {k}: events");
            let (pressed, label) = view.read_with(vcx, |v, _| {
                let i = v.index_of("b").unwrap();
                let n = v.surface().layout_node(i).unwrap();
                let mut s: Vec<String> = n.part_query.as_ref().map(|q| q.states.clone()).unwrap_or_default();
                s.extend(v.placed_nodes().iter().find(|p| p.id == "b" && !p.removed).map(|p| p.states.clone()).unwrap_or_default());
                s.extend(v.node_states("b"));
                (s.iter().any(|x| x == "pressed"), n.props.get("label").and_then(Value::as_str).unwrap_or("").to_string())
            });
            assert_eq!(pressed, step["pressed"].as_bool().unwrap(), "{name} step {k}: pressed");
            assert_eq!(label, step["label"].as_str().unwrap(), "{name} step {k}: label");
        }
    }
}

#[gpui::test]
fn the_serif_family_maps_to_a_system_serif(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "serif", "component": "Text", "props": {"text": "The quick brown fox", "variant": "lead"}, "style": {"fontFamily": "$type.family.serif"}},
        {"id": "sans", "component": "Text", "props": {"text": "The quick brown fox", "variant": "lead"}}
    ]});
    let (view, _log, cx) = surface(cx, tree);
    let (serif, sans) = view.read_with(cx, |v, _| (v.node_font("serif"), v.node_font("sans")));
    let (serif, sans) = (serif.expect("serif font"), sans.expect("sans font"));
    assert_ne!(serif.family, sans.family, "ui-serif is not the sans");
    assert!(["New York", "Georgia", "Times New Roman", "DejaVu Serif", "Liberation Serif", "Noto Serif", ".SystemUIFont"].contains(&serif.family.as_ref()), "{}", serif.family);
}

/// The round-5 macros paint from natives: each specimen lays out with
/// every visible node inside the surface and no Unknown placeholder.
#[gpui::test]
fn the_new_macro_specimens_paint_from_natives(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    for id in ["exponential-ui-message-bubble", "exponential-ui-call-surface", "exponential-ui-plan", "exponential-ui-qr-code", "exponential-ui-qr-scanner"] {
        let tree = specimen(id);
        let host = Rc::new(Recorder::default());
        let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), host, ..Default::default() };
        let (pane, vcx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| SurfaceView::new(options, window, cx));
            view.update(cx, |v, cx| {
                let out = v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
                assert!(out.issues.is_empty(), "{:?}", out.issues);
            });
            Pane { view }
        });
        draw(vcx);
        let view = pane.read_with(vcx, |p, _| p.view.clone());
        let (unknown, empty_leaves) = view.read_with(vcx, |v, _| {
            let nodes = v.placed_nodes();
            let unknown = nodes.iter().filter(|n| !n.removed && n.component == "Unknown").count();
            let empty: Vec<String> = nodes
                .iter()
                .filter(|n| !n.removed && !n.hidden && n.kind == eui_core::layout_tree::NodeKind::Leaf && matches!(n.component.as_str(), "QrCode" | "QrScanner" | "Text" | "Button" | "Icon"))
                .filter(|n| v.frame(n.index).is_some_and(|f| f.w <= 0.0 || f.h <= 0.0))
                .map(|n| n.id.clone())
                .collect();
            (unknown, empty)
        });
        assert_eq!(unknown, 0, "{id}: no Unknown placeholder");
        assert!(empty_leaves.is_empty(), "{id}: zero-sized leaves {empty_leaves:?}");
    }
}

/// A CallSurface's round buttons hold their 24 px Icon centred in the 56 px
/// box (the core centres a Button's one custom child, like the web).
#[gpui::test]
fn a_call_buttons_icon_sits_centred(cx: &mut TestAppContext) {
    let (view, _log, cx) = surface(cx, specimen("exponential-ui-call-surface"));
    for (button, icon) in [("c0-call.controls.cancel.end", "c0-call.controls.cancel.end.endIcon"), ("c1-call.controls.muteTip.mute", "c1-call.controls.muteTip.mute.muteIcon")] {
        let (b, i) = (frame(&view, cx, button), frame(&view, cx, icon));
        assert_eq!((b.w, b.h, i.w, i.h), (56.0, 56.0, 24.0, 24.0), "{button}");
        assert_eq!((i.x - b.x, i.y - b.y), (16.0, 16.0), "{icon} centred");
    }
}

/// N-12: the `DockCard` macro on the gpui painter (Swift
/// `testDockCardExpandsToTriggerOverABottomDrawerWithItsFooter`, Compose
/// `DockCardTest`): the core expands it to a pressable trigger row over a
/// bottom `Drawer`; the specimen (open, with a footer slot) paints the
/// collapsed card and the sheet's body with its footer under it.
#[gpui::test]
fn the_dock_card_specimen_paints_its_trigger_and_the_open_bottom_drawer_with_its_footer(cx: &mut TestAppContext) {
    let (view, _log, cx) = surface(cx, specimen("exponential-ui-dock-card"));
    view.update(cx, |v, cx| v.advance_clock(std::time::Duration::from_secs(1), cx));
    draw(cx);
    let unknown = view.read_with(cx, |v, _| v.placed_nodes().iter().filter(|n| !n.removed && n.component == "Unknown").count());
    assert_eq!(unknown, 0, "no Unknown placeholder");
    for part in ["trigger", "trigger.icon", "trigger.body.header.label", "trigger.body.header.caption", "trigger.body.header.meta", "trigger.body.row.title", "trigger.body.row.chevron"] {
        painted(&view, cx, &format!("c0-dock-card.{part}"));
    }
    let trigger_layer = view.read_with(cx, |v, _| v.placed_nodes().iter().find(|n| n.id == "c0-dock-card.trigger").map(|n| n.layer));
    assert_eq!(trigger_layer, Some(0), "the collapsed card sits in the base layer");
    let drawer = view.read_with(cx, |v, _| v.layers().iter().find(|l| l.kind == "Drawer").cloned()).expect("the open card's sheet is a Drawer layer");
    assert_eq!(drawer.owner, "c0-dock-card.sheet");
    assert_eq!(drawer.position, "bottom");
    assert!(drawer.modal);
    let (footer_part, footer_parent) = view.read_with(cx, |v, _| {
        let nodes = v.placed_nodes();
        let part = nodes.iter().find(|n| n.id == "c0-dock-card.sheet.footer").map(|n| (n.index, n.part.clone()));
        (part, nodes.iter().find(|n| n.id == "c0-footer").and_then(|n| n.parent))
    });
    let (footer_index, part) = footer_part.expect("the Drawer's footer part");
    assert_eq!(part.as_deref(), Some("footer"));
    assert_eq!(footer_parent, Some(footer_index), "the `footer` slot lands in the Drawer's own footer part");
    for id in ["c0-p", "c0-footer", "c0-no", "c0-yes"] {
        let layer = view.read_with(cx, |v, _| v.placed_nodes().iter().find(|n| n.id == id).map(|n| n.layer));
        assert_eq!(layer, Some(drawer.layer), "{id} rides the sheet");
        painted(&view, cx, id);
    }
    let (body, yes) = (painted(&view, cx, "c0-p"), painted(&view, cx, "c0-yes"));
    assert!(yes.origin.y >= body.origin.y + body.size.height, "the footer sits under the body: {yes:?} vs {body:?}");
}
