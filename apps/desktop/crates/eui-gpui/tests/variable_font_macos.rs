//! VAPP-166 on macOS: the spezl drafts' gpui harness painted a VARIABLE
//! font (Nunito[wght]) at its default weight whatever the requested one.
//! Through gpui's REAL CoreText text system holding only
//! `conformance/fonts/Nunito-Variable.ttf`, a line at 600 / 800 must be
//! wider than at 200 (the default instance).
//!
//! `harness = false`: gpui-pre's Mac platform must be created on the
//! process main thread (see `text_clusters_macos.rs`).

#[cfg(target_os = "macos")]
use std::borrow::Cow;
#[cfg(target_os = "macos")]
use std::sync::Arc;

#[cfg(target_os = "macos")]
use gpui::{px, Font, FontStyle, FontWeight, SharedString, TextRun, TextSystem, WindowTextSystem};

fn main() {
    if std::env::args().any(|arg| arg == "--list") {
        return;
    }
    #[cfg(target_os = "macos")]
    {
        variable_weights_differ_on_coretext();
        println!("test variable_weights_differ_on_coretext ... ok");
    }
}

#[cfg(target_os = "macos")]
fn variable_weights_differ_on_coretext() {
    let platform = gpui_platform::current_platform(true).text_system();
    let file = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/conformance/fonts/Nunito-Variable.ttf");
    platform.add_fonts(vec![Cow::Owned(std::fs::read(file).expect("Nunito-Variable"))]).expect("Nunito[wght] loads");
    let ts = Arc::new(TextSystem::new(platform.clone()));
    let window_ts = WindowTextSystem::new(ts.clone());
    let text = "spezl · Calling MacBook Pro";
    let width = |weight: f32| {
        let font = Font { family: "Nunito".into(), features: Default::default(), fallbacks: None, weight: FontWeight(weight), style: FontStyle::Normal };
        let run = TextRun { len: text.len(), font, color: gpui::black(), background_color: None, underline: None, strikethrough: None };
        f32::from(window_ts.shape_line(SharedString::from(text), px(24.0), &[run], None).width)
    };
    let widths: Vec<(f32, f32)> = [200.0, 400.0, 600.0, 800.0].into_iter().map(|w| (w, width(w))).collect();
    eprintln!("VAPP-166 macOS: Nunito[wght] widths {widths:?}");
    assert!(widths[2].1 > widths[0].1 + 1.0, "600 paints heavier than 200: {widths:?}");
    assert!(widths[3].1 > widths[2].1 + 1.0, "800 paints heavier than 600: {widths:?}");
}
