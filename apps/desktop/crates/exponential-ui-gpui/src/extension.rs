//! Extension natives (`component: "Extension"` nodes, keyed on their
//! `extension_kind`) are painted by a host-registered [`ExtensionPainter`]
//! (`SurfaceView::register_painter`). Its `measure` answers the leaf's size
//! in-process (the same per-request contract as the text measure); `paint`
//! returns the element drawn INSIDE the node's frame (the frame div itself,
//! with the recipe chrome, is the painter's).

use std::sync::Arc;

use exponential_ui::measure::{LeafRequest, TextStyle};
use exponential_ui::style::Visual;
use exponential_ui::surface::PlacedNode;
use exponential_ui::theme::{Mode, ResolvedTheme};

/// Everything a painter gets for one extension node.
pub struct PaintContext<'a> {
    pub node: &'a PlacedNode,
    /// The node's RESOLVED visual (colours `#rrggbb[aa]`, px numbers).
    pub visual: &'a Visual,
    pub text_style: &'a TextStyle,
    /// The frame size (border box).
    pub width: f32,
    pub height: f32,
    /// A container extension's children, already painted at their frames
    /// (relative to this node); add them to the returned element.
    pub children: Vec<gpui::AnyElement>,
    pub theme: Option<&'a Arc<ResolvedTheme>>,
    pub mode: Mode,
    /// Fire an interaction on this node (`press`, `change` + payload…): it
    /// routes through the core exactly like a native's event.
    pub emit: EmitFn<'a>,
}

/// The `emit` callback of a [`PaintContext`].
pub type EmitFn<'a> = Box<dyn Fn(&str, Option<serde_json::Value>, &mut gpui::Window, &mut gpui::App) + 'a>;

/// Paints (and measures) one extension native kind.
pub trait ExtensionPainter: 'static {
    /// The border-box size at `wrap_width` (`None` = max-content, `Some(0.0)`
    /// = min-content). `None` = (0, 0): the node's style sizes it.
    fn measure(
        &self,
        _leaf: &LeafRequest,
        _wrap_width: Option<f32>,
        _window: &mut gpui::Window,
        _cx: &mut gpui::App,
    ) -> Option<(f32, f32)> {
        None
    }

    fn paint(&self, ctx: PaintContext, window: &mut gpui::Window, cx: &mut gpui::App) -> gpui::AnyElement;
}
