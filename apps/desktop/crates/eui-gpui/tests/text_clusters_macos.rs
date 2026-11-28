//! YO-30 on macOS: `/ˈʃpɛtsl̩/` in Nunito through gpui's REAL CoreText text
//! system. Nunito lacks U+0329, so `cluster_runs` must shape the `l̩`
//! cluster whole in one face of the sans chain that has both characters,
//! and the mark's ink must sit over the l (never a bar after it).

//!
//! `harness = false` (Cargo.toml): gpui-pre's Mac platform must be created on
//! the process main thread, and libtest runs `#[test]` fns on worker threads.
//! `main` runs the check itself; a `--list` (cargo-nextest's discovery) lists
//! nothing, like `terminal`'s exit_hook, so it runs under plain `cargo test`.

#[cfg(target_os = "macos")]
use std::borrow::Cow;
#[cfg(target_os = "macos")]
use std::sync::Arc;

#[cfg(target_os = "macos")]
use eui_gpui::fallback::{cluster_runs, default_chain};
#[cfg(target_os = "macos")]
use gpui::{px, Font, FontStyle, FontWeight, SharedString, TextRun, TextSystem, WindowTextSystem};

fn main() {
    if std::env::args().any(|arg| arg == "--list") {
        return;
    }
    #[cfg(target_os = "macos")]
    {
        ipa_mark_attaches_on_coretext();
        println!("test ipa_mark_attaches_on_coretext ... ok");
        key_symbols_paint_in_inter_symbols();
        println!("test key_symbols_paint_in_inter_symbols ... ok");
    }
}

#[cfg(target_os = "macos")]
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

/// The bundled `Inter Symbols` registers itself and heads the chain: a ⌘
/// in Nunito (which has none) shapes in it, never in an OS face
/// (`EXP_INTER_SYMBOLS` = a face file to load instead of the bundled ones).
#[cfg(target_os = "macos")]
fn key_symbols_paint_in_inter_symbols() {
    use eui_gpui::symbol_fonts::{register_symbol_fonts, INTER_SYMBOLS_FAMILY};
    let platform = gpui_platform::current_platform(true).text_system();
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/conformance/fonts");
    platform.add_fonts(vec![Cow::Owned(std::fs::read(format!("{dir}/Nunito-Regular.ttf")).expect("Nunito"))]).expect("Nunito loads");
    if let Ok(file) = std::env::var("EXP_INTER_SYMBOLS") {
        platform.add_fonts(vec![Cow::Owned(std::fs::read(file).expect("the face"))]).expect("the face loads");
    }
    let ts = Arc::new(TextSystem::new(platform));
    if std::env::var("EXP_INTER_SYMBOLS").is_err() {
        register_symbol_fonts(&ts);
        register_symbol_fonts(&ts);
    }
    let window_ts = WindowTextSystem::new(ts.clone());
    let bare = |family: &str| Font { family: family.to_string().into(), features: Default::default(), fallbacks: None, weight: FontWeight(400.0), style: FontStyle::Normal };
    let symbols = ts.resolve_font(&bare(INTER_SYMBOLS_FAMILY));
    assert_eq!(ts.get_font_for_id(symbols).map(|f| f.family.to_string()).as_deref(), Some(INTER_SYMBOLS_FAMILY), "Inter Symbols loads");
    let font = Font { fallbacks: Some(default_chain()), ..bare("Nunito") };
    let text = "\u{2318}S";
    let run = TextRun { len: text.len(), font, color: gpui::black(), background_color: None, underline: None, strikethrough: None };
    let runs = cluster_runs(&ts, text, &[run], &default_chain());
    let line = window_ts.shape_line(SharedString::from(text), px(16.0), &runs, None);
    let cmd: Vec<_> = line.runs.iter().filter(|r| r.glyphs.iter().any(|g| g.index == 0)).map(|r| ts.get_font_for_id(r.font_id).map(|f| f.family.to_string())).collect();
    assert_eq!(cmd, vec![Some(INTER_SYMBOLS_FAMILY.to_string())], "the command key shapes in Inter Symbols");
    let symbols_advance = ts.advance(symbols, px(16.0), '\u{2318}').expect("Inter Symbols has the command key");
    let shaped: Vec<_> = line.runs.iter().filter(|r| r.glyphs.iter().any(|g| g.index == 0)).map(|r| ts.advance(r.font_id, px(16.0), '\u{2318}').ok()).collect();
    assert_eq!(shaped, vec![Some(symbols_advance)], "at Inter Symbols' advance");
}
