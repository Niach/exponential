//! Icons. Catalog `Icon` names are registry CONCEPTS resolved through
//! `HostPlugin::icon` (an SVG asset path). Without a host registry the
//! painter still draws every glyph the core's `builtinIcons` table names
//! (`ui-check`, `ui-chevron-*`, `ui-copy`, `upload`, `ui-clock`, the toast
//! tones…) with gpui-component's `IconName`, so its own chrome works in any
//! host. An unknown name paints the placeholder circle. Direction-bound
//! glyphs (chevrons, back/forward arrows) mirror in RTL.

use gpui::{div, prelude::*, px, svg, AnyElement, Hsla, Radians, SharedString, Transformation};
use gpui_component::{Icon, IconName};

use crate::host::HostPlugin;

/// The painter's own glyphs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Check,
    ChevronDown,
    ChevronUp,
    ChevronLeft,
    ChevronRight,
    ChevronsUpDown,
    Close,
    Search,
    Calendar,
    Clock,
    Send,
    Attach,
    Spinner,
    Play,
    Stop,
    Image,
    Copy,
    Upload,
    File,
    Plus,
    Minus,
    Info,
    Success,
    Warning,
    Error,
    Back,
    Star,
    Placeholder,
}

impl Glyph {
    /// The gpui-component icon behind a glyph (`None` = drawn with divs).
    pub fn icon_name(self) -> Option<IconName> {
        Some(match self {
            Glyph::Check => IconName::Check,
            Glyph::ChevronDown => IconName::ChevronDown,
            Glyph::ChevronUp => IconName::ChevronUp,
            Glyph::ChevronLeft => IconName::ChevronLeft,
            Glyph::ChevronRight => IconName::ChevronRight,
            Glyph::ChevronsUpDown => IconName::ChevronsUpDown,
            Glyph::Close => IconName::Close,
            Glyph::Search => IconName::Search,
            Glyph::Calendar => IconName::Calendar,
            Glyph::Send => IconName::ArrowUp,
            Glyph::Attach | Glyph::Plus => IconName::Plus,
            Glyph::Minus => IconName::Minus,
            Glyph::Spinner => IconName::LoaderCircle,
            Glyph::Play => IconName::Play,
            Glyph::Image => IconName::GalleryVerticalEnd,
            Glyph::Copy => IconName::Copy,
            Glyph::Upload => IconName::ArrowUp,
            Glyph::File => IconName::File,
            Glyph::Info => IconName::Info,
            Glyph::Success => IconName::CircleCheck,
            Glyph::Warning => IconName::TriangleAlert,
            Glyph::Error => IconName::CircleX,
            Glyph::Back => IconName::ArrowLeft,
            Glyph::Star => IconName::Star,
            Glyph::Stop | Glyph::Clock | Glyph::Placeholder => return None,
        })
    }

    /// The core concept names (`builtinIcons`, the macros' chrome).
    pub fn for_concept(name: &str) -> Option<Glyph> {
        Some(match name {
            "ui-check" => Glyph::Check,
            "ui-chevron-down" => Glyph::ChevronDown,
            "ui-chevron-up" => Glyph::ChevronUp,
            "ui-chevron-left" => Glyph::ChevronLeft,
            "ui-chevron-right" => Glyph::ChevronRight,
            "ui-selector" => Glyph::ChevronsUpDown,
            "ui-close" => Glyph::Close,
            "ui-search" | "search" => Glyph::Search,
            "calendar" | "ui-calendar" => Glyph::Calendar,
            "ui-clock" => Glyph::Clock,
            "ui-send" => Glyph::Send,
            "ui-attach" => Glyph::Attach,
            "ui-stop" => Glyph::Stop,
            "ui-copy" => Glyph::Copy,
            "upload" => Glyph::Upload,
            "ui-file" => Glyph::File,
            "ui-add" => Glyph::Plus,
            "ui-minus" => Glyph::Minus,
            "ui-info" => Glyph::Info,
            "ui-success" => Glyph::Success,
            "ui-warning" => Glyph::Warning,
            "ui-error" => Glyph::Error,
            "ui-back" => Glyph::Back,
            "ui-star" | "star" => Glyph::Star,
            "ui-image" => Glyph::Image,
            "ui-icon-placeholder" => Glyph::Placeholder,
            _ => return None,
        })
    }

    /// The glyph read in the other direction (RTL).
    pub fn mirrored(self) -> Glyph {
        match self {
            Glyph::ChevronLeft => Glyph::ChevronRight,
            Glyph::ChevronRight => Glyph::ChevronLeft,
            g => g,
        }
    }
}

/// A chrome glyph `size` px square in `color`.
pub fn glyph(g: Glyph, size: f32, color: Hsla) -> AnyElement {
    match g.icon_name() {
        Some(name) => Icon::new(name).size(px(size)).text_color(color).into_any_element(),
        None if g == Glyph::Stop => div().size(px(size)).flex().items_center().justify_center().child(div().size(px(size * 0.6)).rounded(px(2.0)).bg(color)).into_any_element(),
        None if g == Glyph::Clock => clock(size, color),
        None => placeholder(size, color),
    }
}

/// A clock face (no `IconName` for it): a ring and two hands.
fn clock(size: f32, color: Hsla) -> AnyElement {
    let ring = (size * 0.8).max(6.0);
    let stroke = (size / 12.0).max(1.0);
    let c = size / 2.0;
    div()
        .size(px(size))
        .relative()
        .child(div().absolute().left(px(c - ring / 2.0)).top(px(c - ring / 2.0)).size(px(ring)).rounded_full().border(px(stroke)).border_color(color))
        .child(div().absolute().left(px(c - stroke / 2.0)).top(px(c - ring * 0.3)).w(px(stroke)).h(px(ring * 0.3)).bg(color))
        .child(div().absolute().left(px(c - stroke / 2.0)).top(px(c - stroke / 2.0)).w(px(ring * 0.25)).h(px(stroke)).bg(color))
        .into_any_element()
}

/// The placeholder circle (an unknown icon name).
pub fn placeholder(size: f32, color: Hsla) -> AnyElement {
    let ring = (size * 0.75).max(4.0);
    div().size(px(size)).flex().items_center().justify_center().child(div().size(px(ring)).rounded_full().border(px(1.5)).border_color(color)).into_any_element()
}

/// A registry concept name: the host's SVG, else a built-in glyph, else the
/// placeholder.
pub fn concept(host: &dyn HostPlugin, name: &str, size: f32, color: Hsla) -> AnyElement {
    concept_mirrored(host, name, size, color, false)
}

/// [`concept`], with direction-bound glyphs mirrored in RTL (a host SVG of a
/// chevron/arrow is flipped by a half turn about its vertical axis).
pub fn concept_mirrored(host: &dyn HostPlugin, name: &str, size: f32, color: Hsla, rtl: bool) -> AnyElement {
    let directional = rtl && matches!(name, "ui-chevron-left" | "ui-chevron-right" | "ui-back" | "ui-forward" | "ui-arrow-left" | "ui-arrow-right");
    if let Some(path) = host.icon(name) {
        let s = svg().path(path).size(px(size)).flex_none().text_color(color);
        return if directional { s.with_transformation(Transformation::scale(gpui::size(-1.0, 1.0))).into_any_element() } else { s.into_any_element() };
    }
    match Glyph::for_concept(name) {
        Some(g) if directional && g == Glyph::Back => Icon::new(IconName::ArrowRight).size(px(size)).text_color(color).into_any_element(),
        Some(g) => glyph(if directional { g.mirrored() } else { g }, size, color),
        None => placeholder(size, color),
    }
}

/// [`concept`] turned by `radians` (a style `transform: rotate(…)`).
pub fn concept_rotated(host: &dyn HostPlugin, name: &str, size: f32, color: Hsla, radians: f32) -> AnyElement {
    if let Some(path) = host.icon(name) {
        return svg().path(path).size(px(size)).flex_none().text_color(color).with_transformation(Transformation::rotate(Radians(radians))).into_any_element();
    }
    match Glyph::for_concept(name).and_then(Glyph::icon_name) {
        Some(n) => Icon::new(n).size(px(size)).text_color(color).rotate(Radians(radians)).into_any_element(),
        None => concept(host, name, size, color),
    }
}

/// Whether a concept resolves to something other than the placeholder.
pub fn is_known(host: &dyn HostPlugin, name: &str) -> bool {
    host.icon(name).is_some() || Glyph::for_concept(name).is_some_and(|g| g != Glyph::Placeholder)
}

/// Shorthand for a label string.
pub fn label(text: &str) -> SharedString {
    SharedString::from(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_icon_slot_has_a_glyph() {
        use exponential_ui::generated::catalog::{BUILTIN_ICON_NAMES, BUILTIN_ICON_SLOTS};
        for (slot, name) in BUILTIN_ICON_SLOTS.iter().zip(BUILTIN_ICON_NAMES) {
            assert!(Glyph::for_concept(name).is_some(), "{slot} → {name} has no built-in glyph");
        }
        assert_eq!(Glyph::ChevronLeft.mirrored(), Glyph::ChevronRight);
    }
}
