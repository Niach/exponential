//! VAPP-166 on macOS: the spezl drafts' gpui harness painted a VARIABLE
//! font (Nunito[wght]) at its default weight whatever the requested one.
//! Through gpui's REAL CoreText text system holding only
//! `conformance/fonts/Nunito-Variable.ttf`, a line at 400 / 600 / 800 must
//! be wider than at 200 (the default instance) and as wide as the static
//! Nunito-Regular / SemiBold / ExtraBold (within 1 %), like
//! `tests/variable_fonts.rs` on Linux. The patch: vendor/gpui-pre-apple.
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
fn widths(files: &[&str], weights: &[f32]) -> Vec<f32> {
    let platform = gpui_platform::current_platform(true).text_system();
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/conformance/fonts");
    platform.add_fonts(files.iter().map(|f| Cow::Owned(std::fs::read(format!("{dir}/{f}")).expect("font"))).collect()).expect("the fonts load");
    let ts = Arc::new(TextSystem::new(platform));
    let window_ts = WindowTextSystem::new(ts);
    let text = "spezl · Calling MacBook Pro";
    weights.iter().map(|&weight| {
        let font = Font { family: "Nunito".into(), features: Default::default(), fallbacks: None, weight: FontWeight(weight), style: FontStyle::Normal };
        let run = TextRun { len: text.len(), font, color: gpui::black(), background_color: None, underline: None, strikethrough: None };
        f32::from(window_ts.shape_line(SharedString::from(text), px(24.0), &[run], None).width)
    }).collect()
}

#[cfg(target_os = "macos")]
fn variable_weights_differ_on_coretext() {
    let variable = widths(&["Nunito-Variable.ttf"], &[200.0, 400.0, 600.0, 800.0]);
    let fixed = widths(&["Nunito-Regular.ttf", "Nunito-SemiBold.ttf", "Nunito-ExtraBold.ttf"], &[400.0, 600.0, 800.0]);
    eprintln!("VAPP-166 macOS: Nunito[wght] 200/400/600/800 {variable:?}, static 400/600/800 {fixed:?}");
    assert!(variable[2] > variable[0] + 1.0, "600 paints heavier than 200: {variable:?}");
    assert!(variable[3] > variable[2] + 1.0, "800 paints heavier than 600: {variable:?}");
    for (i, w) in [400, 600, 800].iter().enumerate() {
        let (v, s) = (variable[i + 1], fixed[i]);
        assert!((v - s).abs() <= s * 0.01, "{w}: variable {v} vs static {s}");
    }
}
