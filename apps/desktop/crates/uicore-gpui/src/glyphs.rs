//! #EDE-65: the rail's glyphs, painted the same in both hosts: the IDE's
//! rail (#EXP-1724) and eDE's sidebar, which is that rail for good. A
//! document names them; [`paint`] draws the names a host does not own:
//! `agent:<cli>` (the agent's mark, Claude in its brand colour),
//! `ide:dot:<tone>` (the 8 px status dot), `ide:badge:dot:<tone>|<glyph>`
//! (a glyph wearing its dot in the corner) and
//! `ide:badge:count:<tone>:<label>|<glyph>` (THE count badge there).

use std::sync::RwLock;

use eui_core::theme::{Mode, ResolvedTheme};
use eui_gpui::controls::{count_badge, BadgeTone};
use gpui::{div, prelude::*, px, AnyElement, App, Div, Hsla, SharedString};

/// #EDE-235: the colours the glyphs paint in. The IDE keeps its own
/// palette; eDE's shell hands the theme its ui directory holds
/// ([`set_palette`]), so a theme edit recolours the rail's dots and badges too.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub success: Hsla,
    pub muted: Hsla,
    pub muted_foreground: Hsla,
}

impl Palette {
    /// The IDE's palette (`theme::exponential_dark`).
    pub fn ide() -> Self {
        let p = theme::exponential_dark();
        Self { success: p.success, muted: p.muted, muted_foreground: p.muted_foreground }
    }

    /// The dark colours of an eUI theme; a role it lacks keeps the IDE's.
    pub fn of(theme: &ResolvedTheme) -> Self {
        let colors = &theme.modes.get(Mode::Dark).color;
        let ide = Self::ide();
        let pick = |role: &str, or: Hsla| colors.get(role).and_then(|c| eui_gpui::paint::color::parse_hex(c)).unwrap_or(or);
        Self { success: pick("success", ide.success), muted: pick("muted", ide.muted), muted_foreground: pick("mutedForeground", ide.muted_foreground) }
    }
}

static PALETTE: RwLock<Option<Palette>> = RwLock::new(None);

/// The glyphs' colours from now on (eDE: the ui directory's theme).
pub fn set_palette(palette: Palette) {
    *PALETTE.write().unwrap_or_else(|p| p.into_inner()) = Some(palette);
}

/// The colours the glyphs paint in now: a host's, else the IDE's.
pub fn palette() -> Palette {
    PALETTE.read().unwrap_or_else(|p| p.into_inner()).unwrap_or_else(Palette::ide)
}

/// The status dot (the sidebar's trailing dot).
pub fn dot(color: Hsla) -> Div {
    div().size(px(theme::tokens::sidebar::TRAILING_DOT)).flex_shrink_0().rounded_full().bg(color)
}

/// A dot's colour by tone (`green` = online, or a chat at work).
pub fn dot_color(tone: &str) -> Option<Hsla> {
    let palette = palette();
    match tone {
        "green" => Some(palette.success),
        _ => None,
    }
}

/// `<tone>:<label>` back to THE count badge's count and tone (`99+` reads as
/// past the cap).
pub fn parse_count(rest: &str) -> Option<(usize, BadgeTone)> {
    let (tone, label) = rest.split_once(':')?;
    let tone = match tone {
        "attention" => BadgeTone::Attention,
        "muted" => BadgeTone::Muted,
        _ => return None,
    };
    let count = match label.strip_suffix('+') {
        Some(cap) => cap.parse::<usize>().ok()? + 1,
        None => label.parse().ok()?,
    };
    Some((count, tone))
}

/// THE count badge in the rail's own colours and type: eDE's shell never sets
/// gpui-component's theme, whose default is the light one in the system font.
fn rail_count(count: usize, tone: BadgeTone, cx: &App) -> Option<Div> {
    let badge = count_badge(count, tone, cx)?.font_family("Inter");
    Some(match tone {
        BadgeTone::Muted => {
            let palette = palette();
            badge.bg(palette.muted).text_color(palette.muted_foreground)
        }
        _ => badge,
    })
}

/// `inner` at `size` with `corner` over its top-right corner.
pub fn cornered(inner: AnyElement, corner: AnyElement, size: f32) -> AnyElement {
    div().relative().size(px(size)).child(inner).child(div().absolute().top(px(-6.)).right(px(-8.)).child(corner)).into_any_element()
}

/// An agent's mark (`claude`, `codex`): the asset the IDE's run mark draws.
pub fn agent_mark(agent: &str, size: f32, color: Hsla) -> Option<AnyElement> {
    let (path, tint) = match agent {
        "claude" => ("icons/claude.svg", theme::CLAUDE_BRAND),
        "codex" => ("icons/codex.svg", color),
        _ => return None,
    };
    Some(gpui::svg().path(SharedString::from(path)).size(px(size)).flex_shrink_0().text_color(tint).into_any_element())
}

/// The glyph a rail name stands for; `icon` resolves a plain icon name.
pub fn paint(name: &str, size: f32, color: Hsla, icon: &dyn Fn(&str) -> Option<SharedString>, cx: &App) -> Option<AnyElement> {
    if let Some(agent) = name.strip_prefix("agent:") {
        return agent_mark(agent, size, color);
    }
    if let Some(tone) = name.strip_prefix("ide:dot:") {
        return Some(dot(dot_color(tone)?).into_any_element());
    }
    let (badge, glyph) = name.strip_prefix("ide:badge:")?.split_once('|')?;
    let corner = match badge.strip_prefix("count:") {
        Some(rest) => {
            let (count, tone) = parse_count(rest)?;
            rail_count(count, tone, cx)?.into_any_element()
        }
        None => dot(dot_color(badge.strip_prefix("dot:")?)?).into_any_element(),
    };
    let inner = paint(glyph, size, color, icon, cx).or_else(|| Some(gpui::svg().path(icon(glyph)?).size(px(size)).text_color(color).into_any_element()))?;
    Some(cornered(inner, corner, size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_count_parses_back() {
        assert_eq!(parse_count("attention:3").map(|(n, t)| (n, t == BadgeTone::Attention)), Some((3, true)));
        assert_eq!(parse_count("muted:99+").map(|(n, t)| (n, t == BadgeTone::Muted)), Some((100, true)));
        assert!(parse_count("amber:3").is_none());
    }

    #[test]
    fn the_tones_are_the_ides() {
        let palette = theme::exponential_dark();
        assert_eq!(Palette::ide().success, palette.success);
        assert_eq!(dot_color("red"), None);
    }

    /// #EDE-235: a theme's dark roles, the IDE's for a role it lacks.
    #[test]
    fn a_theme_gives_its_palette() {
        let mut t = (*eui_core::themes::builtin_theme("exponential").expect("the theme")).clone();
        t.modes.dark.color.insert("muted".into(), "#2f2320".into());
        t.modes.dark.color.shift_remove("success");
        let p = Palette::of(&t);
        assert_eq!(p.muted, eui_gpui::paint::color::parse_hex("#2f2320").unwrap());
        assert_eq!(p.success, Palette::ide().success);
    }
}
