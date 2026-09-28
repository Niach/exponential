//! UniFFI facade over `vapp_spike` for the SwiftUI and Compose painters.
//!
//! Shape of the contract (see `spikes/vapp-layout/LANES.md`):
//! - `Surface` is parsed ONCE; `set_viewport` / `set_pressed` restyle only
//!   the nodes whose resolved style changed.
//! - `nodes()` hands the host the static per-node data (kind, props JSON) once.
//! - `layout(measure)` crosses back into the host for every leaf measurement
//!   with a small record carrying the node INDEX (never content), and returns
//!   one flat frame list in pre-order.

use std::sync::{Arc, Mutex};

use vapp_spike::measure::{Measure as CoreMeasure, MeasureRequest};
use vapp_spike::{AvailableSpace, FixedMeasure, Surface as CoreSurface};

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum VappError {
    #[error("invalid surface: {reason}")]
    Invalid { reason: String },
}

#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct FfiSize {
    pub width: f32,
    pub height: f32,
}

/// taffy's `AvailableSpace` flattened: `Definite` carries a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AvailMode {
    Definite,
    MinContent,
    MaxContent,
}

/// One measure request. `known_*` = already decided by taffy (measure only the
/// other axis); `available_*` = the constraint. The reply is the CONTENT box.
#[derive(Debug, Clone, uniffi::Record)]
pub struct MeasureCall {
    pub index: u32,
    pub known_width: Option<f32>,
    pub known_height: Option<f32>,
    pub available_width: Option<f32>,
    pub available_width_mode: AvailMode,
    pub available_height: Option<f32>,
    pub available_height_mode: AvailMode,
    /// Convenience: the width to lay text out in (known → definite → none).
    pub wrap_width: Option<f32>,
}

/// Implemented by the host (SwiftUI `sizeThatFits`, Compose intrinsics).
#[uniffi::export(with_foreign)]
pub trait Measure: Send + Sync {
    fn measure(&self, call: MeasureCall) -> FfiSize;
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct NodeInfo {
    pub index: u32,
    pub id: String,
    pub kind: String,
    pub depth: u32,
    pub parent: Option<u32>,
    pub is_container: bool,
    /// The node's `props` object as JSON (text, label, options…), parsed once by the host.
    pub props_json: String,
}

#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct FfiVisual {
    pub background_color: Option<String>,
    pub color: Option<String>,
    pub border_width: Option<f32>,
    pub border_color: Option<String>,
    pub border_radius: Option<f32>,
    pub opacity: Option<f32>,
    pub box_shadow: Option<String>,
    pub text_align: Option<String>,
    pub overflow_hidden: bool,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct PlacedFrame {
    pub index: u32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub visual: FfiVisual,
    pub font_size: f32,
    pub font_weight: u16,
    pub line_height: f32,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct LayoutResult {
    /// Pre-order = paint order = accessibility order.
    pub frames: Vec<PlacedFrame>,
    pub measure_calls: u32,
    /// Nanoseconds inside taffy INCLUDING the host measure callbacks.
    pub layout_ns: u64,
    pub surface_width: f32,
    pub surface_height: f32,
}

fn avail(a: AvailableSpace) -> (Option<f32>, AvailMode) {
    match a {
        AvailableSpace::Definite(v) => (Some(v), AvailMode::Definite),
        AvailableSpace::MinContent => (None, AvailMode::MinContent),
        AvailableSpace::MaxContent => (None, AvailMode::MaxContent),
    }
}

struct ForeignMeasure(Arc<dyn Measure>);

impl CoreMeasure for ForeignMeasure {
    fn measure(&mut self, req: &MeasureRequest) -> vapp_spike::taffy::geometry::Size<f32> {
        let (aw, awm) = avail(req.available_width);
        let (ah, ahm) = avail(req.available_height);
        let reply = self.0.measure(MeasureCall {
            index: req.index,
            known_width: req.known_width,
            known_height: req.known_height,
            available_width: aw,
            available_width_mode: awm,
            available_height: ah,
            available_height_mode: ahm,
            wrap_width: req.wrap_width(),
        });
        vapp_spike::taffy::geometry::Size { width: reply.width, height: reply.height }
    }
}

fn convert(r: vapp_spike::LayoutResult) -> LayoutResult {
    LayoutResult {
        frames: r
            .nodes
            .into_iter()
            .map(|n| PlacedFrame {
                index: n.index,
                x: n.frame.x,
                y: n.frame.y,
                width: n.frame.w,
                height: n.frame.h,
                visual: FfiVisual {
                    background_color: n.visual.background_color,
                    color: n.visual.color,
                    border_width: n.visual.border_width,
                    border_color: n.visual.border_color,
                    border_radius: n.visual.border_radius,
                    opacity: n.visual.opacity,
                    box_shadow: n.visual.box_shadow,
                    text_align: n.visual.text_align,
                    overflow_hidden: n.visual.overflow_hidden,
                },
                font_size: n.text_style.font_size,
                font_weight: n.text_style.font_weight,
                line_height: n.text_style.line_height,
            })
            .collect(),
        measure_calls: r.measure_calls,
        layout_ns: r.layout_ns,
        surface_width: r.surface_width,
        surface_height: r.surface_height,
    }
}

#[derive(uniffi::Object)]
pub struct Surface {
    inner: Mutex<CoreSurface>,
}

#[uniffi::export]
impl Surface {
    #[uniffi::constructor]
    pub fn new(tree_json: String) -> Result<Arc<Self>, VappError> {
        let inner = CoreSurface::new(&tree_json).map_err(|reason| VappError::Invalid { reason })?;
        Ok(Arc::new(Surface { inner: Mutex::new(inner) }))
    }

    pub fn node_count(&self) -> u32 {
        self.inner.lock().unwrap().node_count() as u32
    }

    /// Parse + tree build time of the constructor, in nanoseconds.
    pub fn build_ns(&self) -> u64 {
        self.inner.lock().unwrap().build_ns
    }

    pub fn nodes(&self) -> Vec<NodeInfo> {
        self.inner
            .lock()
            .unwrap()
            .nodes()
            .iter()
            .map(|n| NodeInfo {
                index: n.index,
                id: n.id.clone(),
                kind: n.kind.as_str().to_string(),
                depth: n.depth,
                parent: n.parent,
                is_container: n.kind.is_container(),
                props_json: serde_json::Value::Object(n.props.clone()).to_string(),
            })
            .collect()
    }

    /// Height ≤ 0 = "as tall as the content". Returns whether it changed.
    pub fn set_viewport(&self, width: f32, height: f32) -> bool {
        self.inner.lock().unwrap().set_viewport(width, height)
    }

    pub fn set_pressed(&self, ids: Vec<String>) -> bool {
        self.inner.lock().unwrap().set_pressed(&ids)
    }

    /// Off = fractional frames (Android rounds once itself, in px).
    pub fn set_rounding(&self, on: bool) {
        self.inner.lock().unwrap().set_rounding(on)
    }

    pub fn layout(&self, measure: Arc<dyn Measure>) -> LayoutResult {
        let mut m = ForeignMeasure(measure);
        convert(self.inner.lock().unwrap().layout(&mut m))
    }

    /// The fixed fake measure (8 px/char, 20 px lines) — the no-FFI baseline.
    pub fn layout_fixed(&self) -> LayoutResult {
        convert(self.inner.lock().unwrap().layout(&mut FixedMeasure))
    }
}

/// The shared kitchen-sink fixture.
#[uniffi::export]
pub fn kitchen_sink_json() -> String {
    vapp_spike::KITCHEN_SINK_JSON.to_string()
}

/// A synthetic ~n-node surface for the timing measurement.
#[uniffi::export]
pub fn bench_tree_json(n: u32) -> String {
    vapp_spike::bench::bench_tree(n as usize)
}

/// The fixed-measure intrinsic size of a leaf, so a host can run the
/// geometry-mode comparison without its own text engine.
#[uniffi::export]
pub fn fixed_intrinsic(kind: String, props_json: String) -> FfiSize {
    let kind = vapp_spike::Kind::parse(&kind).unwrap_or(vapp_spike::Kind::Box);
    let props: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&props_json).unwrap_or_default();
    let s = FixedMeasure::intrinsic(kind, &props);
    FfiSize { width: s.width, height: s.height }
}
