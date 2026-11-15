//! #VAPP-166: a variable font paints and measures at the weight asked for,
//! not its default instance, on gpui's Linux text system (the vendored
//! `gpui-pre-wgpu` patch). Google Fonts' `Nunito[wght].ttf` defaults to
//! wght 200; at 600 and 800 it must lay out like the static SemiBold and
//! ExtraBold instances the conformance set carries, and paint more ink.

#![cfg(any(target_os = "linux", target_os = "freebsd"))]

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;

use gpui::{point, px, Font, FontStyle, FontWeight, PlatformTextSystem, RenderGlyphParams, SharedString};

const SAMPLE: &str = "Hamburgefonstiv spezl";

fn font_file(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../packages/exponential-ui/conformance/fonts").join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn system(files: &[&str]) -> Arc<gpui_wgpu::CosmicTextSystem> {
    let ts = Arc::new(gpui_wgpu::CosmicTextSystem::new_without_system_fonts("Nunito"));
    ts.add_fonts(files.iter().map(|f| Cow::Owned(font_file(f))).collect()).expect("the fonts load");
    ts
}

fn font(weight: f32) -> Font {
    Font { family: SharedString::from("Nunito"), features: Default::default(), fallbacks: None, weight: FontWeight(weight), style: FontStyle::Normal }
}

fn width(ts: &gpui_wgpu::CosmicTextSystem, weight: f32) -> f32 {
    let id = ts.font_id(&font(weight)).expect("Nunito resolves");
    let line = ts.layout_line(SAMPLE, px(32.0), &[gpui::FontRun { len: SAMPLE.len(), font_id: id }]);
    f32::from(line.width)
}

/// The summed coverage of the sample's glyphs, rasterised at `weight`.
fn ink(ts: &gpui_wgpu::CosmicTextSystem, weight: f32) -> u64 {
    let id = ts.font_id(&font(weight)).expect("Nunito resolves");
    let line = ts.layout_line(SAMPLE, px(32.0), &[gpui::FontRun { len: SAMPLE.len(), font_id: id }]);
    let mut total = 0u64;
    for run in &line.runs {
        for g in &run.glyphs {
            let params = RenderGlyphParams { font_id: run.font_id, glyph_id: g.id, font_size: px(32.0), subpixel_variant: point(0, 0), scale_factor: 1.0, is_emoji: false, subpixel_rendering: false, dilation: 0 };
            let bounds = ts.glyph_raster_bounds(&params).expect("bounds");
            if bounds.size.width.0 == 0 || bounds.size.height.0 == 0 {
                continue;
            }
            let (_, bytes) = ts.rasterize_glyph(&params, bounds).expect("raster");
            total += bytes.iter().map(|&b| b as u64).sum::<u64>();
        }
    }
    total
}

#[test]
fn variable_font_lays_out_at_the_requested_weight() {
    let variable = system(&["Nunito-Variable.ttf"]);
    for (weight, file) in [(400.0, "Nunito-Regular.ttf"), (600.0, "Nunito-SemiBold.ttf"), (800.0, "Nunito-ExtraBold.ttf")] {
        let want = width(&system(&[file]), weight);
        let got = width(&variable, weight);
        assert!((got - want).abs() <= want * 0.01, "Nunito[wght] at {weight}: {got} px wide, the static instance {want} px");
        let id = variable.font_id(&font(weight)).unwrap();
        assert_eq!(variable.font_weight_and_style(id).unwrap().0, FontWeight(weight), "the instance reports its weight");
    }
    assert!(width(&variable, 200.0) < width(&variable, 600.0), "the default (200) instance is narrower than 600");
}

#[test]
fn variable_font_paints_at_the_requested_weight() {
    let variable = system(&["Nunito-Variable.ttf"]);
    let (light, semi, extra) = (ink(&variable, 200.0), ink(&variable, 600.0), ink(&variable, 800.0));
    assert!(light < semi && semi < extra, "more weight, more ink: 200 = {light}, 600 = {semi}, 800 = {extra}");
    let static_semi = ink(&system(&["Nunito-SemiBold.ttf"]), 600.0);
    let diff = (semi as f64 - static_semi as f64).abs() / static_semi as f64;
    assert!(diff < 0.03, "600 paints like the static SemiBold: {semi} vs {static_semi}");
}
