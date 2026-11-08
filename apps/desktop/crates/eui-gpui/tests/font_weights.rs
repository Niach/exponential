//! #FEED-209: a recipe's `fontWeight: 500` (Table `headerCell`, a totals
//! row's `cell`) paints at 400 on gpui. Inter at weight 500 must resolve,
//! in gpui's real Linux text system holding the conformance fonts, to a
//! face of its own: Inter-Medium, never the regular face.

#![cfg(any(target_os = "linux", target_os = "freebsd"))]

#[path = "../examples/conformance_dump.rs"]
#[allow(dead_code)]
mod conformance_dump;

use std::borrow::Cow;
use std::sync::Arc;

use gpui::{Font, FontId, FontStyle, FontWeight, PlatformTextSystem};

fn system(only: Option<&str>) -> Arc<dyn PlatformTextSystem> {
    let fonts = conformance_dump::font_set(&conformance_dump::manifest());
    let platform = Arc::new(gpui_wgpu::CosmicTextSystem::new_without_system_fonts(&fonts.default));
    let files = fonts.files.iter().filter(|(p, _)| only.is_none_or(|f| p.ends_with(f) || !p.contains("Inter-")));
    platform.add_fonts(files.map(|(_, b)| Cow::Owned(b.clone())).collect()).expect("the conformance fonts load");
    platform
}

fn inter(weight: f32) -> Font {
    Font { family: "Inter".into(), features: Default::default(), fallbacks: None, weight: FontWeight(weight), style: FontStyle::Normal }
}

/// The face's advances (font units) over glyphs whose widths move with weight.
fn advances(ts: &Arc<dyn PlatformTextSystem>, id: FontId) -> Vec<f32> {
    "mwMWabg0".chars().map(|c| ts.advance(id, ts.glyph_for_char(id, c).expect("glyph")).expect("advance").width).collect()
}

#[test]
fn inter_500_resolves_to_inter_medium() {
    let ts = system(None);
    let regular = ts.font_id(&inter(400.0)).expect("Inter 400");
    let medium = ts.font_id(&inter(500.0)).expect("Inter 500");
    assert_ne!(medium, regular, "Inter at 500 resolves to a face of its own, not the regular face");

    // Which face: the same advances as a system holding ONLY Inter-Medium.
    let only_medium = system(Some("Inter-Medium.ttf"));
    let only_regular = system(Some("Inter-Regular.ttf"));
    let want = advances(&only_medium, only_medium.font_id(&inter(500.0)).unwrap());
    assert_ne!(want, advances(&only_regular, only_regular.font_id(&inter(400.0)).unwrap()), "Medium and Regular differ in advances (the probe sees the face)");
    assert_eq!(advances(&ts, medium), want, "Inter at 500 is Inter-Medium");
    assert_eq!(advances(&ts, regular), advances(&only_regular, only_regular.font_id(&inter(400.0)).unwrap()), "Inter at 400 is Inter-Regular");
}
