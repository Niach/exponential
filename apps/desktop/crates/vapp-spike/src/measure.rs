//! The host measure contract + the fixed fake measure used by geometry tests.

use serde_json::{Map, Value};
use taffy::prelude::*;

use crate::style::TextStyle;
use crate::tree::Kind;

/// One measure request for a leaf. `known_*` = the dimension taffy already
/// decided (measure only the other one); `available_*` = the constraint
/// (`Definite(w)`, `MinContent`, `MaxContent`). Both known ⇒ taffy never asks.
/// The result is the CONTENT box; taffy adds the leaf's padding/border itself,
/// so leaf styles carry no padding (native controls report border-box sizes).
#[derive(Debug, Clone)]
pub struct MeasureRequest<'a> {
    pub index: u32,
    pub id: &'a str,
    pub kind: Kind,
    pub props: &'a Map<String, Value>,
    pub text_style: TextStyle,
    pub known_width: Option<f32>,
    pub known_height: Option<f32>,
    pub available_width: AvailableSpace,
    pub available_height: AvailableSpace,
}

impl MeasureRequest<'_> {
    /// The width a host lays text out in: the known width, else the definite
    /// available width, else `None` (max-content).
    pub fn wrap_width(&self) -> Option<f32> {
        self.known_width.or(match self.available_width {
            AvailableSpace::Definite(w) => Some(w),
            AvailableSpace::MinContent => Some(0.0),
            AvailableSpace::MaxContent => None,
        })
    }

    pub fn text(&self) -> &str {
        self.props
            .get("text")
            .or_else(|| self.props.get("label"))
            .or_else(|| self.props.get("title"))
            .and_then(Value::as_str)
            .unwrap_or("")
    }
}

pub trait Measure {
    fn measure(&mut self, req: &MeasureRequest) -> Size<f32>;
}

impl<F: FnMut(&MeasureRequest) -> Size<f32>> Measure for F {
    fn measure(&mut self, req: &MeasureRequest) -> Size<f32> {
        self(req)
    }
}

/// The geometry-test measure every platform can reproduce exactly: text is
/// 8 px per character on ONE 20 px line (no wrapping, so min-content ==
/// max-content), controls have fixed token sizes. A known dimension wins.
pub struct FixedMeasure;

pub const FIXED_CHAR_WIDTH: f32 = 8.0;
pub const FIXED_LINE_HEIGHT: f32 = 20.0;

fn chars(s: &str) -> f32 {
    s.chars().count() as f32
}

impl FixedMeasure {
    pub fn intrinsic(kind: Kind, props: &Map<String, Value>) -> Size<f32> {
        let str_prop = |k: &str| props.get(k).and_then(Value::as_str).unwrap_or("");
        let (w, h) = match kind {
            Kind::Text => (FIXED_CHAR_WIDTH * chars(str_prop("text")), FIXED_LINE_HEIGHT),
            Kind::Markdown => {
                let text = str_prop("text");
                let lines = text.lines().count().max(1) as f32;
                let widest = text.lines().map(chars).fold(0.0, f32::max);
                (FIXED_CHAR_WIDTH * widest, FIXED_LINE_HEIGHT * lines)
            }
            Kind::Button => (FIXED_CHAR_WIDTH * chars(str_prop("label")) + 24.0, 36.0),
            Kind::Pill => (FIXED_CHAR_WIDTH * chars(str_prop("label")) + 20.0, 28.0),
            Kind::ListRow => (
                FIXED_CHAR_WIDTH * (chars(str_prop("title")) + chars(str_prop("meta"))) + 16.0,
                32.0,
            ),
            Kind::TextField => (160.0, 36.0),
            Kind::Textarea => (160.0, 72.0),
            Kind::Select => (160.0, 36.0),
            Kind::Toggle => (44.0, 24.0),
            Kind::Badge => (20.0, 20.0),
            Kind::Avatar => {
                let s = props.get("size").and_then(Value::as_f64).unwrap_or(32.0) as f32;
                (s, s)
            }
            Kind::Image => (320.0, 180.0),
            Kind::Divider => (0.0, 1.0),
            Kind::Progress => (160.0, 8.0),
            Kind::Box | Kind::Card => (0.0, 0.0),
        };
        Size { width: w, height: h }
    }
}

impl Measure for FixedMeasure {
    fn measure(&mut self, req: &MeasureRequest) -> Size<f32> {
        let i = Self::intrinsic(req.kind, req.props);
        Size {
            width: req.known_width.unwrap_or(i.width),
            height: req.known_height.unwrap_or(i.height),
        }
    }
}
