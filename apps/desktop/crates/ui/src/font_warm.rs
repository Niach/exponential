//! #LOGS-6: the first issue a process opens paid for loading font families
//! and building their fallback cascades (the editor's Inter with its Tibetan
//! fallbacks, JetBrains Mono for code, genUI's sans and mono chains, every
//! weight and style) inside its first frame. A background thread resolves
//! those faces and shapes a line in each at the sizes the pages use shortly
//! after launch, so the first open finds them loaded. The shaping cache is
//! per window; what this warms is the process-wide font state below it.

use std::sync::Arc;
use std::time::Duration;

use gpui::{px, App, Font, FontStyle, FontWeight, TextRun, WindowTextSystem};

/// Launch keeps the main thread busy first; the warm-up waits it out (it
/// takes the text system's lock while it loads a family).
const DELAY: Duration = Duration::from_secs(2);
/// The text sizes the issue and session pages set.
const SIZES: [f32; 7] = [12., 13., 14., 15., 16., 20., 24.];
const SAMPLE: &str = "Quick brown fox — 0123456789 <800ms> ≤ ≥ → ✓ • … “é” ü ß";

pub(crate) fn warm(cx: &App) {
    let text_system = cx.text_system().clone();
    std::thread::Builder::new()
        .name("font-warm".into())
        .spawn(move || {
            std::thread::sleep(DELAY);
            let _span = tracing::trace_span!("font.warm").entered();
            let sans = theme::tokens::typography::FONT_FAMILY;
            let mono = theme::terminal::FONT_FAMILY;
            let mut fonts: Vec<Font> = Vec::new();
            for family in [sans, mono] {
                for weight in [400., 500., 600., 700.] {
                    for style in [FontStyle::Normal, FontStyle::Italic] {
                        let with_weight = |font: Font| Font { weight: FontWeight(weight), style, ..font };
                        fonts.push(with_weight(gpui::font(family)));
                        fonts.push(with_weight(gpui_markdown_editor::editor_text_font(family)));
                    }
                }
            }
            let shaper = WindowTextSystem::new(Arc::clone(&text_system));
            for font in &fonts {
                shape(&shaper, font, &SIZES);
            }
            for font in eui_gpui::measure::prewarm_fonts(&text_system, sans) {
                shape(&shaper, &font, &[13., 14.]);
            }
        })
        .ok();
}

fn shape(shaper: &WindowTextSystem, font: &Font, sizes: &[f32]) {
    let run = TextRun {
        len: SAMPLE.len(),
        font: font.clone(),
        color: gpui::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    for size in sizes {
        shaper.layout_line(SAMPLE, px(*size), std::slice::from_ref(&run), None);
    }
}
