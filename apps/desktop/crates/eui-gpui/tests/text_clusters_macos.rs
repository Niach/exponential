//! YO-30 on macOS: `/ˈʃpɛtsl̩/` in Nunito through gpui's REAL CoreText text
//! system. Nunito lacks U+0329, so `cluster_runs` must shape the `l̩`
//! cluster whole in one face of the sans chain that has both characters,
//! and the mark's ink must sit over the l (never a bar after it).

#![cfg(target_os = "macos")]

use std::borrow::Cow;
use std::sync::Arc;

use eui_gpui::fallback::{cluster_runs, default_chain};
use gpui::{px, Font, FontStyle, FontWeight, SharedString, TextRun, TextSystem, WindowTextSystem};

#[test]
fn ipa_mark_attaches_on_coretext() {
    let platform = gpui_platform::current_platform(true).text_system();
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/conformance/fonts");
    let fonts = ["Nunito-Regular.ttf", "Nunito-Bold.ttf"].iter().map(|f| Cow::Owned(std::fs::read(format!("{dir}/{f}")).expect("Nunito"))).collect();
    platform.add_fonts(fonts).expect("Nunito loads");
    let ts = Arc::new(TextSystem::new(platform.clone()));
    let window_ts = WindowTextSystem::new(ts.clone());

    let text = "/ˈʃpɛtsl̩/";
    let font = Font { family: "Nunito".into(), features: Default::default(), fallbacks: None, weight: FontWeight(400.0), style: FontStyle::Normal };
    assert!(ts.advance(ts.resolve_font(&font), px(16.0), '\u{329}').is_err(), "Nunito has no U+0329");
    let run = TextRun { len: text.len(), font, color: gpui::black(), background_color: None, underline: None, strikethrough: None };
    let runs = cluster_runs(&ts, text, &[run], &default_chain());
    let start = text.find("l\u{329}").unwrap();
    let end = start + "l\u{329}".len();
    let mut at = 0;
    let face: Vec<SharedString> = runs.iter().filter_map(|r| {
        let hit = at < end && at + r.len > start;
        at += r.len;
        hit.then(|| r.font.family.clone())
    }).collect();
    eprintln!("YO-30 macOS: runs {:?}", runs.iter().map(|r| (r.len, r.font.family.to_string())).collect::<Vec<_>>());
    assert_eq!(face.len(), 1, "the l̩ cluster is ONE run: {face:?}");
    assert_ne!(face[0].as_ref(), "Nunito", "the l̩ cluster leaves Nunito");

    let size = px(48.0);
    let line = window_ts.shape_line(SharedString::from(text), size, &runs, None);
    let mine: Vec<_> = line.runs.iter().flat_map(|r| r.glyphs.iter().map(move |g| (r.font_id, g.index, g.id, f32::from(g.position.x)))).filter(|g| g.1 >= start && g.1 < end).collect();
    assert!(mine.len() >= 2 && mine.iter().all(|g| g.0 == mine[0].0), "base and mark share one font: {mine:?}");
    let ink = |g: &(gpui::FontId, usize, gpui::GlyphId, f32)| {
        let b = platform.typographic_bounds(g.0, g.2).expect("glyph bounds");
        let scale = f32::from(size) / ts.units_per_em(g.0) as f32;
        (g.3 + b.origin.x * scale, g.3 + (b.origin.x + b.size.width) * scale)
    };
    let (b0, b1) = ink(&mine[0]);
    for g in &mine[1..] {
        let (m0, m1) = ink(g);
        eprintln!("YO-30 macOS: face {:?} base ink {b0:.2}..{b1:.2}, mark ink {m0:.2}..{m1:.2}", face[0]);
        let mid = (m0 + m1) / 2.0;
        assert!(mid >= b0 - 0.5 && mid <= b1 + 0.5, "mark ink {m0}..{m1} centres over the l's {b0}..{b1}");
    }
}
