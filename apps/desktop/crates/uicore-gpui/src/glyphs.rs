//! #EDE-65: the rail's glyphs, painted the same in both hosts: the IDE's
//! rail (#EXP-1724) and eDE's sidebar, which is that rail for good. A
//! document names them; [`paint`] draws the names a host does not own:
//! `agent:<cli>` (the agent's mark, Claude in its brand colour),
//! `ide:dot:<tone>` (the 8 px status dot) and `ide:badge:dot:<tone>|<glyph>`
//! (a glyph wearing its dot in the corner).

use gpui::{div, prelude::*, px, AnyElement, Div, Hsla, SharedString};

/// The status dot (the sidebar's trailing dot).
pub fn dot(color: Hsla) -> Div {
    div().size(px(theme::tokens::sidebar::TRAILING_DOT)).flex_shrink_0().rounded_full().bg(color)
}

/// A dot's colour by tone (`amber` needs you, `green` busy, `primary` unread).
pub fn dot_color(tone: &str) -> Option<Hsla> {
    let palette = theme::exponential_dark();
    match tone {
        "amber" => Some(palette.warning),
        "green" => Some(palette.success),
        "primary" => Some(theme::tokens::PRIMARY.to_hsla()),
        _ => None,
    }
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
pub fn paint(name: &str, size: f32, color: Hsla, icon: &dyn Fn(&str) -> Option<SharedString>) -> Option<AnyElement> {
    if let Some(agent) = name.strip_prefix("agent:") {
        return agent_mark(agent, size, color);
    }
    if let Some(tone) = name.strip_prefix("ide:dot:") {
        return Some(dot(dot_color(tone)?).into_any_element());
    }
    let (tone, glyph) = name.strip_prefix("ide:badge:dot:")?.split_once('|')?;
    let inner = paint(glyph, size, color, icon).or_else(|| Some(gpui::svg().path(icon(glyph)?).size(px(size)).text_color(color).into_any_element()))?;
    Some(cornered(inner, dot(dot_color(tone)?).into_any_element(), size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tones_are_the_ides() {
        let palette = theme::exponential_dark();
        assert_eq!(dot_color("amber"), Some(palette.warning));
        assert_eq!(dot_color("green"), Some(palette.success));
        assert_eq!(dot_color("primary"), Some(theme::tokens::PRIMARY.to_hsla()));
        assert_eq!(dot_color("red"), None);
    }
}
