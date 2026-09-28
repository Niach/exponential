//! VAPP-4 spike core: a `Box` tree with StyleX-subset style objects (JSON) is
//! resolved on the client (surface-width `@media`, `:pressed`), converted to
//! taffy styles, laid out with a host-supplied `Measure`, and returned as
//! absolutely positioned frames in pre-order (= paint order = a11y order).
//!
//! Throwaway: nothing here is product code. The shape of the fixture and the
//! whitelist in [`WHITELIST`] are the findings that outlive the spike.

pub mod bench;
pub mod measure;
pub mod style;
pub mod surface;
pub mod tracks;
pub mod tree;

pub use measure::{FixedMeasure, Measure, MeasureRequest};
pub use style::{StyleContext, TextStyle, Visual};
pub use surface::{Frame, LayoutResult, PlacedNode, Surface};
pub use taffy;
pub use taffy::AvailableSpace;
pub use tree::{Kind, NodeSpec};

/// The kitchen-sink fixture shared by all four clients
/// (`packages/domain-contract/fixtures/vapp-kitchen-sink.json`).
pub const KITCHEN_SINK_JSON: &str =
    include_str!("../../../../../packages/domain-contract/fixtures/vapp-kitchen-sink.json");

/// The candidate v1 style-property whitelist (layout + visual, no transitions,
/// no transforms). `style::unknown_keys` reports anything outside it.
pub const WHITELIST: &[&str] = &[
    // display + flow
    "display",
    "flexDirection",
    "flexWrap",
    "justifyContent",
    "alignItems",
    "alignContent",
    "alignSelf",
    "justifySelf",
    "flexGrow",
    "flexShrink",
    "flexBasis",
    "gap",
    "rowGap",
    "columnGap",
    "direction",
    "overflow",
    // sizing
    "width",
    "height",
    "minWidth",
    "minHeight",
    "maxWidth",
    "maxHeight",
    "aspectRatio",
    // positioning
    "position",
    "top",
    "right",
    "bottom",
    "left",
    "inset",
    // spacing
    "padding",
    "paddingTop",
    "paddingRight",
    "paddingBottom",
    "paddingLeft",
    "paddingHorizontal",
    "paddingVertical",
    "margin",
    "marginTop",
    "marginRight",
    "marginBottom",
    "marginLeft",
    "marginHorizontal",
    "marginVertical",
    // grid
    "gridTemplateColumns",
    "gridTemplateRows",
    "gridTemplateAreas",
    "gridArea",
    "gridColumn",
    "gridRow",
    // visual (painted, not laid out; borderWidth DOES take layout space)
    "backgroundColor",
    "color",
    "borderWidth",
    "borderColor",
    "borderRadius",
    "opacity",
    "boxShadow",
    "fontSize",
    "fontWeight",
    "lineHeight",
    "textAlign",
];

/// Convenience: parse + resolve + lay out in one call.
pub fn layout_once(
    tree_json: &str,
    viewport_width: f32,
    viewport_height: f32,
    pressed: &[String],
    measure: &mut dyn Measure,
) -> Result<(Surface, LayoutResult), String> {
    let mut surface = Surface::new(tree_json)?;
    surface.set_viewport(viewport_width, viewport_height);
    surface.set_pressed(pressed);
    let result = surface.layout(measure);
    Ok((surface, result))
}
