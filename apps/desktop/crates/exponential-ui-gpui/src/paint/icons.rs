//! Icons. Catalog `Icon` names are registry CONCEPTS resolved through
//! `HostPlugin::icon` (an SVG asset path); the painter's OWN chrome (a
//! checkbox check, a select chevron, the composer's send) uses
//! gpui-component's `IconName`, so a host without a registry still gets
//! working controls. An unknown name paints the placeholder circle.

use gpui::{div, prelude::*, px, svg, AnyElement, Hsla, SharedString};
use gpui_component::{Icon, IconName};

use crate::host::HostPlugin;

/// The painter's own glyphs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Check,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    Close,
    Search,
    Calendar,
    Send,
    Attach,
    Spinner,
    Play,
    Stop,
    Image,
    Placeholder,
}

impl Glyph {
    /// The gpui-component icon behind a glyph (`None` = drawn with divs).
    pub fn icon_name(self) -> Option<IconName> {
        Some(match self {
            Glyph::Check => IconName::Check,
            Glyph::ChevronDown => IconName::ChevronDown,
            Glyph::ChevronLeft => IconName::ChevronLeft,
            Glyph::ChevronRight => IconName::ChevronRight,
            Glyph::Close => IconName::Close,
            Glyph::Search => IconName::Search,
            Glyph::Calendar => IconName::Calendar,
            Glyph::Send => IconName::ArrowUp,
            Glyph::Attach => IconName::Plus,
            Glyph::Spinner => IconName::LoaderCircle,
            Glyph::Play => IconName::Play,
            Glyph::Image => IconName::GalleryVerticalEnd,
            Glyph::Stop | Glyph::Placeholder => return None,
        })
    }

    /// The core concept names the macros use for their own chrome.
    pub fn for_concept(name: &str) -> Option<Glyph> {
        Some(match name {
            "ui-check" => Glyph::Check,
            "ui-chevron-down" => Glyph::ChevronDown,
            "ui-chevron-left" => Glyph::ChevronLeft,
            "ui-chevron-right" => Glyph::ChevronRight,
            "ui-close" => Glyph::Close,
            "ui-search" => Glyph::Search,
            "ui-send" => Glyph::Send,
            "ui-icon-placeholder" => Glyph::Placeholder,
            _ => return None,
        })
    }
}

/// A chrome glyph `size` px square in `color`.
pub fn glyph(g: Glyph, size: f32, color: Hsla) -> AnyElement {
    match g.icon_name() {
        Some(name) => Icon::new(name).size(px(size)).text_color(color).into_any_element(),
        None if g == Glyph::Stop => div().size(px(size)).flex().items_center().justify_center().child(div().size(px(size * 0.6)).rounded(px(2.0)).bg(color)).into_any_element(),
        None => placeholder(size, color),
    }
}

/// The placeholder circle (an unknown icon name).
pub fn placeholder(size: f32, color: Hsla) -> AnyElement {
    let ring = (size * 0.75).max(4.0);
    div().size(px(size)).flex().items_center().justify_center().child(div().size(px(ring)).rounded_full().border(px(1.5)).border_color(color)).into_any_element()
}

/// A registry concept name: the host's SVG, else a built-in glyph, else the
/// placeholder.
pub fn concept(host: &dyn HostPlugin, name: &str, size: f32, color: Hsla) -> AnyElement {
    if let Some(path) = host.icon(name) {
        return svg().path(path).size(px(size)).flex_none().text_color(color).into_any_element();
    }
    match Glyph::for_concept(name) {
        Some(g) => glyph(g, size, color),
        None => placeholder(size, color),
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
