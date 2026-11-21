//! #EXP-2046: the FACTORY taskbar's clock sets its time in tabular figures,
//! so its width holds from minute to minute and nothing left of it moves:
//! an open popover anchored there (the Agent spaces list) stays open
//! (#EXP-1739 closes an overlay whose anchor moved; at 05:30 → 05:31 the
//! clock's text got 3 px narrower and the list closed by itself). Laid out
//! with gpui's real Linux text system holding the shell's own Inter.

#![cfg(any(target_os = "linux", target_os = "freebsd"))]

use std::borrow::Cow;
use std::rc::Rc;
use std::sync::Arc;

use eui_gpui::host::HostPlugin;
use gpui::{PlatformTextSystem, TestAppContext, VisualTestContext};
use serde_json::{json, Value};
use uicore_data::Feed;
use uicore_gpui::{Surfaces, WindowedHost};

struct Plugin;
impl HostPlugin for Plugin {}

fn node<'a>(n: &'a Value, id: &str) -> Option<&'a Value> {
    if n.get("id").and_then(Value::as_str) == Some(id) {
        return Some(n);
    }
    match n {
        Value::Object(m) => m.values().find_map(|v| node(v, id)),
        Value::Array(a) => a.iter().find_map(|v| node(v, id)),
        _ => None,
    }
}

/// A test app whose text system shapes with the shell's fonts
/// (`apps/desktop/assets/fonts`, what ede-shell embeds).
fn app() -> TestAppContext {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts");
    let files: Vec<Cow<'static, [u8]>> = std::fs::read_dir(&dir).expect("the shell's fonts").filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "ttf")).map(|p| Cow::Owned(std::fs::read(p).expect("a font"))).collect();
    let text = Arc::new(gpui_wgpu::CosmicTextSystem::new_without_system_fonts("Inter"));
    text.add_fonts(files).expect("the shell's fonts load");
    let cx = TestAppContext::build_with_text_system(gpui::TestDispatcher::new(0), None, text);
    cx.update(gpui_component::init);
    cx
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

/// Times whose digits differ in width in a proportional face (1 is narrow).
const TIMES: [&str; 6] = ["05:30", "05:31", "11:11", "00:00", "08:48", "10:01"];

/// The clock's width and the x of the button left of it, per time, with the
/// clock node `clock` at the right end of an 800 px bar (as the taskbar
/// sets it).
fn widths(clock: Value, cx: &mut TestAppContext) -> Vec<(f32, f32)> {
    let factory = uicore_docs::doc("taskbar").expect("the factory taskbar").json();
    let source = json!({
        "surface": factory["surface"],
        "data": {"time": TIMES[0]},
        "root": {"id": "taskbar", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "flex-end", "width": 800, "height": 48}, "children": [
            {"id": "anchor", "component": "Button", "props": {"icon": "nav-computer", "variant": "ghost", "size": "icon", "label": "Agent spaces"}},
            clock
        ]}
    });
    let doc = ede_ui::validate_document("taskbar", &source).expect("the document validates");
    let mut surfaces = Surfaces::new(Box::new(WindowedHost), eui_core::themes::builtin_theme("exponential").expect("the theme"), Rc::new(|_| Rc::new(Plugin) as Rc<dyn HostPlugin>), Rc::new(|_, _| {}), "test");
    assert!(cx.update(|cx| surfaces.open_layer(&doc, &Feed::new(), cx)), "the taskbar opened");
    let region = surfaces.region("taskbar").cloned().expect("the taskbar's region");
    let mut vcx = VisualTestContext::from_window(region.handle.into(), cx);
    let mut out = Vec::new();
    for time in TIMES {
        region.view.update(&mut vcx, |v, cx| v.set_data("/time", Some(json!(time)), cx).expect("the time"));
        draw(&mut vcx);
        let (clock, anchor) = region.view.read_with(&vcx, |v, _| {
            let frame = |id: &str| v.index_of(id).and_then(|i| v.frame(i)).unwrap_or_else(|| panic!("{id} laid out"));
            (frame("clock").w, frame("anchor").x)
        });
        out.push((clock, anchor));
    }
    out
}

#[test]
fn the_clocks_width_holds_from_minute_to_minute() {
    let mut app = app();
    let cx = &mut app;
    let factory = uicore_docs::doc("taskbar").expect("the factory taskbar").json();
    let clock = node(&factory["root"], "clock").expect("the factory clock").clone();
    assert_eq!(clock["style"]["fontVariantNumeric"], "tabular-nums", "the factory clock sets tabular figures");
    // These times do differ in this face: without tabular figures the clock
    // and the button left of it move.
    let mut proportional = clock.clone();
    proportional.as_object_mut().unwrap().remove("style");
    let moving = widths(proportional, cx);
    assert!(moving.iter().any(|w| (w.0 - moving[0].0).abs() > 0.5), "the times' widths differ in a proportional face: {moving:?}");
    // The factory clock: one width, the button left of it in one place.
    let held = widths(clock, cx);
    for (time, w) in TIMES.iter().zip(&held) {
        assert!((w.0 - held[0].0).abs() < 0.01, "{time}: the clock is {} wide, {} at {}", w.0, held[0].0, TIMES[0]);
        assert!((w.1 - held[0].1).abs() < 0.01, "{time}: the button left of the clock at x {}, {} at {}", w.1, held[0].1, TIMES[0]);
    }
}
