//! YO-30: font fallback per grapheme CLUSTER, the way browsers, CoreText in
//! SwiftUI and Android's minikin pick it. gpui's platform shapers fall back
//! per CHARACTER: a combining mark the primary face lacks (Nunito has no
//! U+0329 in `/ˈʃpɛtsl̩/`) is shaped alone in another face, so its base and
//! the mark sit in two fonts and nothing attaches them (macOS paints a stray
//! bar after the base). Before shaping, every run is cut so a cluster whose
//! characters the run's face does not ALL cover is shaped whole in the first
//! face of the fallback chain that covers it: base and mark share one font
//! and its mark positioning. A cluster no face covers keeps the run's font
//! (the platform's own fallback, as before). Single-character clusters are
//! left to the platform: there is nothing to attach.

use gpui::{px, Font, FontFallbacks, FontId, TextRun, TextSystem};
use unicode_segmentation::UnicodeSegmentation;

/// Characters a face never needs a glyph for: joiners, variation
/// selectors, tags and the bidi controls.
fn ignorable(c: char) -> bool {
    matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{061C}' | '\u{FE00}'..='\u{FE0F}' | '\u{E0000}'..='\u{E007F}' | '\u{E0100}'..='\u{E01EF}')
}

/// The run's face without its cascade (what the face ITSELF covers).
fn bare(font: &Font, family: Option<&gpui::SharedString>) -> Font {
    Font { family: family.cloned().unwrap_or_else(|| font.family.clone()), fallbacks: None, ..font.clone() }
}

fn covers(ts: &TextSystem, id: FontId, cluster: &str) -> bool {
    cluster.chars().filter(|c| !ignorable(*c)).all(|c| ts.advance(id, px(16.0), c).is_ok())
}

/// The face a cluster of `font` shapes in: `None` = the run's own.
fn pick(ts: &TextSystem, font: &Font, chain: &FontFallbacks, cluster: &str) -> Option<Font> {
    if covers(ts, ts.resolve_font(&bare(font, None)), cluster) {
        return None;
    }
    chain.fallback_list().iter().find_map(|family| {
        let family = gpui::SharedString::from(family.clone());
        let id = ts.resolve_font(&bare(font, Some(&family)));
        // A family the system lacks resolves to gpui's last-resort stack.
        let loaded = family.starts_with('.') || ts.get_font_for_id(id).is_some_and(|f| f.family == family);
        (loaded && covers(ts, id, cluster)).then(|| Font { family, ..font.clone() })
    })
}

/// `runs` over `text`, cut so every multi-character cluster the run's face
/// cannot shape whole gets the first fallback face that can. `chain` is the
/// fallback list for runs whose font carries none.
pub fn cluster_runs(ts: &TextSystem, text: &str, runs: &[TextRun], chain: &FontFallbacks) -> Vec<TextRun> {
    if text.is_ascii() {
        return runs.to_vec();
    }
    let mut out = Vec::with_capacity(runs.len());
    let mut at = 0;
    for run in runs {
        let seg = text.get(at..at + run.len).unwrap_or("");
        at += run.len;
        if seg.is_ascii() {
            out.push(run.clone());
            continue;
        }
        let chain = run.font.fallbacks.as_ref().unwrap_or(chain);
        let (mut start, mut current): (usize, Option<Font>) = (0, None);
        for (i, cluster) in seg.grapheme_indices(true) {
            let multi = cluster.chars().filter(|c| !ignorable(*c)).nth(1).is_some();
            let face = if multi { pick(ts, &run.font, chain, cluster) } else { None };
            if face != current {
                if i > start {
                    out.push(TextRun { len: i - start, font: current.clone().unwrap_or_else(|| run.font.clone()), ..run.clone() });
                }
                (start, current) = (i, face);
            }
        }
        if seg.len() > start {
            out.push(TextRun { len: seg.len() - start, font: current.unwrap_or_else(|| run.font.clone()), ..run.clone() });
        }
    }
    out
}

/// The sans chain ([`crate::measure`]'s) for runs built without one
/// (markdown spans, code tokens).
pub fn default_chain() -> FontFallbacks {
    static CHAIN: std::sync::OnceLock<FontFallbacks> = std::sync::OnceLock::new();
    CHAIN.get_or_init(|| FontFallbacks::from_fonts(crate::measure::SANS_FALLBACKS.iter().map(|s| s.to_string()).collect())).clone()
}
