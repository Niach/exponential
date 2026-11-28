//! Criterion 3: two eUI surfaces (two `Doc`s, two eui-core `Surface`s) in
//! ONE process and ONE zgui window, each updated on its own; the pixels of
//! the other one never move. Real wgpu offscreen frames, read back.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use eui_core::theme::Mode;
use eui_zgui::{Doc, fonts, host};
use serde_json::json;
use zgui::view::View as _;

fn comps(text: &str) -> Vec<eui_core::types::FlatComponent> {
    serde_json::from_value(json!([
        {"id": "root", "component": "Box", "style": {"padding": 16, "flexDirection": "column", "gap": 8}, "children": ["t", "b"]},
        {"id": "t", "component": "Text", "text": text, "variant": "title"},
        {"id": "b", "component": "Button", "label": "Press", "variant": "default"}
    ]))
    .unwrap()
}

fn region(img: &zgui_render_wgpu::Pixels, x0: i32, w: i32, h: i32) -> Vec<[u8; 4]> {
    let mut v = Vec::new();
    for y in 0..h {
        for x in x0..x0 + w {
            v.push(img.rgba(x, y));
        }
    }
    v
}

#[test]
fn two_documents_update_independently() {
    let fonts = fonts::fonts();
    let theme = eui_core::themes::builtin_theme("exponential");
    let (w, h) = (240.0_f32, 120.0_f32);
    let mk = |id: &str, text: &str| {
        let mut d = Doc::new(id, &fonts, theme.clone(), Mode::Dark, w, h);
        d.surface.set_components(comps(text));
        d.pass();
        Rc::new(RefCell::new(d))
    };
    let (a, b) = (mk("a", "Alpha"), mk("b", "Bravo"));
    let (a2, b2) = (a.clone(), b.clone());
    let readback: host::Readback = Arc::new(Mutex::new(None));
    let rt = host::runtime(&fonts, w * 2.0, h, host::Paint::Offscreen(readback.clone()), move |cx| {
        let root = zgui::elements::r#box()
            .style_text(Some(format!("position:relative;width:{}px;height:{h}px", w * 2.0)))
            .child(a2.borrow().view())
            .child(zgui::elements::r#box().style_text(Some(format!("position:absolute;left:{w}px;top:0px;width:{w}px;height:{h}px"))).child(b2.borrow().view()));
        Box::new(zgui::view::AnyView::new(root).build(cx))
    });
    let mut harness = host::open(rt, w * 2.0, h, 1.0);
    host::frame(&mut harness);
    let grab = |harness: &mut zgui_platform_headless::Harness<zgui::runtime::Runtime>| {
        host::frame(harness);
        readback.lock().unwrap().take().or_else(|| {
            host::frame(harness);
            readback.lock().unwrap().take()
        })
    };
    let first = grab(&mut harness).expect("a first frame");
    let (wa, ww, hh) = (0, w as i32, h as i32);
    let (a0, b0) = (region(&first, wa, ww, hh), region(&first, ww, ww, hh));
    assert!(a0.iter().any(|p| p[0] > 128), "surface A painted text");
    assert!(b0.iter().any(|p| p[0] > 128), "surface B painted text");

    // Update A only.
    a.borrow_mut().surface.update_components(comps("Alpha changed, much longer"));
    let sa = a.borrow_mut().pass();
    assert!(sa.restyled >= 1);
    let after = grab(&mut harness).expect("a frame after A changed");
    assert_ne!(region(&after, wa, ww, hh), a0, "A repainted");
    assert_eq!(region(&after, ww, ww, hh), b0, "B untouched by A's update");

    // Update B only.
    b.borrow_mut().surface.update_components(comps("Bravo two"));
    b.borrow_mut().pass();
    let after2 = grab(&mut harness).expect("a frame after B changed");
    assert_eq!(region(&after2, wa, ww, hh), region(&after, wa, ww, hh), "A untouched by B's update");
    assert_ne!(region(&after2, ww, ww, hh), b0, "B repainted");
    std::mem::forget(harness);
}
