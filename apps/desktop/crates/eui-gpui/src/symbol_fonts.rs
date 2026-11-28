//! `Inter Symbols`: the bundled key-symbol fallback (packages/exponential-ui/
//! fonts/inter-symbols.json). Geist and Nunito have no ⌘, so every OS
//! substituted its own face and a Kbd "⌘S" measured differently per
//! platform. Every renderer names this family right AFTER the theme family,
//! ahead of any OS fallback (CSS: `"Geist", "Inter Symbols", ui-sans-serif,
//! …`); here it is the first entry of the measurer's and painter's glyph
//! fallback chains ([`crate::measure`]). The SDK registers the faces itself
//! ([`register_symbol_fonts`]), no host setup. Inter subset, SIL OFL 1.1
//! (`assets/fonts/OFL-Inter.txt`).

use std::borrow::Cow;
use std::sync::{Arc, Mutex, Weak};

use gpui::TextSystem;

/// The family name every renderer puts after the theme family.
pub const INTER_SYMBOLS_FAMILY: &str = "Inter Symbols";

/// The four faces (400, 500, 600, 700), byte-identical to
/// packages/exponential-ui/fonts/InterSymbols-*.ttf.
pub const INTER_SYMBOLS_FACES: [&[u8]; 4] = [
    include_bytes!("../assets/fonts/InterSymbols-Regular.ttf"),
    include_bytes!("../assets/fonts/InterSymbols-Medium.ttf"),
    include_bytes!("../assets/fonts/InterSymbols-SemiBold.ttf"),
    include_bytes!("../assets/fonts/InterSymbols-Bold.ttf"),
];

/// Registers the `Inter Symbols` faces with `text_system`, once per text
/// system (a second call is a no-op). The SDK calls it before it resolves a
/// font ([`crate::measure::Fonts::new`], [`crate::measure::prewarm_fonts`]), so a host never has to; `add_fonts` clears gpui's
/// resolved-font cache, so a lookup that missed earlier re-resolves. A text
/// system that already lists the family (the conformance fonts) is left as
/// it is on Linux, where listing families is cheap.
pub fn register_symbol_fonts(text_system: &Arc<TextSystem>) {
    register(text_system, Some(text_system));
}

struct Registered {
    addr: usize,
    /// `None` = registered through a bare `&TextSystem` (prewarm).
    weak: Option<Weak<TextSystem>>,
}

static REGISTERED: Mutex<Vec<Registered>> = Mutex::new(Vec::new());

pub(crate) fn register(text_system: &TextSystem, arc: Option<&Arc<TextSystem>>) {
    let addr = text_system as *const TextSystem as usize;
    let mut done = REGISTERED.lock().unwrap_or_else(|e| e.into_inner());
    done.retain(|r| r.weak.as_ref().map_or(true, |w| w.strong_count() > 0));
    if let Some(r) = done.iter_mut().find(|r| r.addr == addr) {
        if r.weak.is_none() {
            r.weak = arc.map(Arc::downgrade);
        }
        return;
    }
    done.push(Registered { addr, weak: arc.map(Arc::downgrade) });
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    if text_system.all_font_names().iter().any(|f| f == INTER_SYMBOLS_FAMILY) {
        return;
    }
    if let Err(err) = text_system.add_fonts(INTER_SYMBOLS_FACES.iter().map(|b| Cow::Borrowed(*b)).collect()) {
        tracing::warn!("eui-gpui: the Inter Symbols faces did not load: {err:#}");
    }
}
