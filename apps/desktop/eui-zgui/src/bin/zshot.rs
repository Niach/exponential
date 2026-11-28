//! One specimen painted by eui-zgui (wgpu offscreen) to a PNG.
//!
//! ```text
//! zshot <specimen-id> <out.png> [WxH] [scale]     # default 720 x content height, scale 2
//! zshot all <dir> [sizes.json]                     # the 20 trial specimens; sizes from the gpui reference
//! ```
use std::cell::RefCell;
use std::rc::Rc;

use eui_core::theme::Mode;
use eui_zgui::{Doc, fonts, host, specimens};

fn render(id: &str, out: &str, size: Option<(f32, f32)>, scale: f64) {
    let fonts = fonts::fonts();
    let sp = specimens::load(id);
    let theme = eui_core::themes::builtin_theme("exponential");
    let (w, h) = size.unwrap_or((720.0, 0.0));
    let mut doc = Doc::new("shot", &fonts, theme, Mode::Dark, w, h);
    doc.surface.set_forced_states(&sp.states);
    doc.surface.set_nested(sp.node);
    doc.pass();
    let h = if h > 0.0 { h } else { doc.last.as_ref().map(|o| o.surface_height).unwrap_or(400.0).ceil() };
    if doc.height == 0.0 {
        doc.height = h;
        doc.surface.set_viewport(w, h, None);
        doc.pass();
    }
    doc.pass();
    let doc = Rc::new(RefCell::new(doc));
    let d = doc.clone();
    let img = host::shot(&fonts, w, h, scale, move || d.borrow().view());
    img.save(out).expect("png");
    eprintln!("zgui {id} {}x{} → {out}", img.width(), img.height());
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let size = |s: Option<&String>| s.and_then(|s| s.split_once('x')).and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)));
    match a.first().map(String::as_str) {
        Some("all") => {
            let dir = a.get(1).cloned().unwrap_or("shots-zgui".into());
            std::fs::create_dir_all(&dir).unwrap();
            let sizes: serde_json::Value = a.get(2).and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
            for id in specimens::TRIAL {
                let wh = sizes.get(id).and_then(|v| Some((v[0].as_f64()? as f32, v[1].as_f64()? as f32)));
                // Each in its own process: one zgui runtime per process thread.
                let st = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([id.to_string(), format!("{dir}/{id}.png"), wh.map(|(w, h)| format!("{w}x{h}")).unwrap_or("720x0".into())])
                    .status()
                    .unwrap();
                if !st.success() {
                    eprintln!("zgui {id} FAILED {st}");
                }
            }
        }
        Some(id) => render(id, a.get(1).map(String::as_str).unwrap_or("zgui.png"), size(a.get(2)), a.get(3).and_then(|s| s.parse().ok()).unwrap_or(2.0)),
        None => eprintln!("usage: zshot <id> <out.png> [WxH] [scale] | zshot all <dir> [sizes.json]"),
    }
}
