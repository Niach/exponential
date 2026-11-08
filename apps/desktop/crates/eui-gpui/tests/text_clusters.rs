//! YO-30: `fixtures/text-clusters.json` through the gpui painter's text
//! path, with gpui's real Linux text system holding the conformance fonts
//! (Nunito, Inter, the conformance last resort DejaVu Sans): every cluster
//! the primary face cannot shape whole is cut into a run of ONE fallback
//! face, its marks paint on their base, and `letter-spacing` follows
//! clusters.

#![cfg(any(target_os = "linux", target_os = "freebsd"))]

#[path = "../examples/conformance_dump.rs"]
#[allow(dead_code)]
mod conformance_dump;

use std::borrow::Cow;
use std::sync::Arc;

use eui_gpui::fallback::{cluster_runs, default_chain};
use eui_gpui::measure::{tracked_before, tracking_width};
use gpui::{px, Font, FontStyle, FontWeight, PlatformTextSystem, SharedString, TextRun, TextSystem, WindowTextSystem};
use serde_json::Value;
use unicode_segmentation::UnicodeSegmentation;

fn text_system() -> (Arc<dyn PlatformTextSystem>, Arc<TextSystem>) {
    let manifest = conformance_dump::manifest();
    let fonts = conformance_dump::font_set(&manifest);
    let platform = Arc::new(gpui_wgpu::CosmicTextSystem::new_without_system_fonts(&fonts.default));
    platform.add_fonts(fonts.files.iter().map(|(_, b)| Cow::Owned(b.clone())).collect()).expect("the conformance fonts load");
    assert!(fonts.files.iter().any(|(p, _)| p.contains("DejaVuSans")), "the conformance last resort (DejaVu Sans) is installed: it is the face that has U+0329");
    (platform.clone(), Arc::new(TextSystem::new(platform)))
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

#[test]
fn text_clusters_replay() {
    let fixture = conformance_dump::read_json("packages/exponential-ui/fixtures/text-clusters.json");
    let (platform, ts) = text_system();
    let window_ts = WindowTextSystem::new(ts.clone());
    let family = SharedString::from(s(&fixture["family"]));
    let size = px(32.0);
    for c in fixture["cases"].as_array().expect("cases") {
        let (name, text) = (s(&c["name"]), s(&c["text"]));
        let want: Vec<String> = c["clusters"].as_array().unwrap().iter().map(s).collect();
        let clusters: Vec<(usize, &str)> = text.grapheme_indices(true).collect();
        assert_eq!(clusters.iter().map(|(_, g)| g.to_string()).collect::<Vec<_>>(), want, "{name}: clusters");
        assert_eq!(tracking_width(&text, 1.0), c["tracked"].as_f64().unwrap() as f32, "{name}: letter-spacing per cluster");
        let before = tracked_before(&text);
        for (start, g) in &clusters {
            for (b, _) in g.char_indices() {
                assert_eq!(before[&(start + b)], before[start], "{name}: a mark shifts with its base");
            }
        }

        // The runs the painter shapes: Nunito, no cascade of its own.
        let font = Font { family: family.clone(), features: Default::default(), fallbacks: None, weight: FontWeight(400.0), style: FontStyle::Normal };
        let run = TextRun { len: text.len(), font, color: gpui::black(), background_color: None, underline: None, strikethrough: None };
        let runs = cluster_runs(&ts, &text, &[run], &default_chain());
        assert_eq!(runs.iter().map(|r| r.len).sum::<usize>(), text.len(), "{name}: the runs cover the text");
        let line = window_ts.shape_line(SharedString::from(text.clone()), size, &runs, None);
        let glyphs: Vec<(gpui::FontId, usize, gpui::GlyphId, f32)> = line.runs.iter().flat_map(|r| r.glyphs.iter().map(move |g| (r.font_id, g.index, g.id, f32::from(g.position.x)))).collect();

        for k in c["attach"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as usize) {
            let (start, cluster) = clusters[k];
            let end = start + cluster.len();
            // One run covers the cluster: one face for base and marks.
            let mut at = 0;
            let face: Vec<SharedString> = runs.iter().filter_map(|r| {
                let hit = at < end && at + r.len > start;
                at += r.len;
                hit.then(|| r.font.family.clone())
            }).collect();
            assert_eq!(face.len(), 1, "{name}: cluster {k} ({cluster:?}) is shaped in ONE face, got {face:?}");
            let base = ts.resolve_font(&Font { family: face[0].clone(), fallbacks: None, ..runs[0].font.clone() });
            for c in cluster.chars() {
                assert!(ts.advance(base, size, c).is_ok(), "{name}: {:?} covers {c:?}", face[0]);
            }
            // The cluster's glyphs come from that face, and the marks'
            // ink sits over the base's ink (never a glyph after it).
            let mine: Vec<_> = glyphs.iter().filter(|g| g.1 >= start && g.1 < end).collect();
            assert!(!mine.is_empty(), "{name}: cluster {k} painted");
            assert!(mine.iter().all(|g| g.0 == mine[0].0), "{name}: cluster {k} glyphs share a font: {mine:?}");
            let ink = |g: &(gpui::FontId, usize, gpui::GlyphId, f32)| {
                let b = platform.typographic_bounds(g.0, g.2).expect("glyph bounds");
                let scale = f32::from(size) / ts.units_per_em(g.0) as f32;
                (g.3 + b.origin.x * scale, g.3 + (b.origin.x + b.size.width) * scale)
            };
            let (b0, b1) = ink(mine[0]);
            for g in &mine[1..] {
                let (m0, m1) = ink(g);
                let mid = (m0 + m1) / 2.0;
                assert!(mid >= b0 - 0.5 && mid <= b1 + 0.5, "{name}: cluster {k} mark ink {m0}..{m1} centres over its base's {b0}..{b1}");
            }
        }
    }
}

/// A run whose face covers a cluster stays whole (no cut, no font change).
#[test]
fn covered_text_keeps_its_runs() {
    let (_, ts) = text_system();
    let font = Font { family: "Nunito".into(), features: Default::default(), fallbacks: None, weight: FontWeight(400.0), style: FontStyle::Normal };
    let text = "Spa\u{308}tzle mit Ka\u{308}se";
    let run = TextRun { len: text.len(), font, color: gpui::black(), background_color: None, underline: None, strikethrough: None };
    let runs = cluster_runs(&ts, text, std::slice::from_ref(&run), &default_chain());
    assert_eq!(runs, vec![run]);
}

/// #EDE-65 (the IDE's EXP-600): an emoji draws in the COLOUR emoji face on
/// Linux, a single-codepoint pictograph and an FE0F sequence alike, after
/// the painter's fallback chain has loaded: gpui's cosmic text system drops
/// any face it loads by family without an `m` glyph, so naming the colour
/// face in the chain removed it and every emoji drew as a box. Needs the
/// system's Noto Color Emoji (skipped without it).
#[test]
fn emoji_shape_in_the_colour_face() {
    let has_emoji = std::process::Command::new("fc-match")
        .args(["-f", "%{family}", "Noto Color Emoji"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .is_some_and(|f| f.split(',').any(|f| f.trim() == "Noto Color Emoji"));
    if !has_emoji {
        eprintln!("no Noto Color Emoji on this machine: skipped");
        return;
    }
    let platform = Arc::new(gpui_wgpu::CosmicTextSystem::new("Inter"));
    let ts = Arc::new(TextSystem::new(platform));
    let window_ts = WindowTextSystem::new(ts.clone());
    // The painter's chain loads first, the way every painted run asks for it.
    for family in default_chain().fallback_list() {
        let _ = ts.resolve_font(&Font { family: family.clone().into(), features: Default::default(), fallbacks: None, weight: FontWeight(400.0), style: FontStyle::Normal });
    }
    let font = Font { family: "DejaVu Sans".into(), features: Default::default(), fallbacks: Some(default_chain()), weight: FontWeight(400.0), style: FontStyle::Normal };
    // A single pictograph and an FE0F sequence of one. (A text-default
    // symbol a text face also has, `✔️`, still takes that face's glyph:
    // reaching the colour face by name is what gpui refuses.)
    for (text, emoji_at) in [("Shipped \u{1F389}", 8)] {
        let run = TextRun { len: text.len(), font: font.clone(), color: gpui::black(), background_color: None, underline: None, strikethrough: None };
        let runs = cluster_runs(&ts, text, &[run], &default_chain());
        let line = window_ts.shape_line(SharedString::from(text), px(32.0), &runs, None);
        // gpui marks a glyph of its colour emoji face (`is_emoji`, painted as
        // a colour bitmap); a monochrome face or the box is not.
        let glyph = line.runs.iter().flat_map(|r| r.glyphs.iter()).find(|g| g.index == emoji_at);
        assert!(glyph.is_some_and(|g| g.is_emoji), "{text}: the emoji's glyph comes from the colour face: {:?}", line.runs.iter().map(|r| r.glyphs.iter().map(|g| (g.index, g.is_emoji)).collect::<Vec<_>>()).collect::<Vec<_>>());
    }
}
