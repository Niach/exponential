//! `eui-motion-wasm`: the browser (and server-side validation) facade over
//! eui-motion. Frames cross as a `Float32Array` in the packed display-list
//! encoding (`eui-motion/README.md` § Packed frames).

use eui_motion::{display::DisplayList, validate, Limits, Motion};
use serde_json::{json, Map, Value};
use wasm_bindgen::prelude::*;

fn issues_json(issues: &[eui_motion::Issue]) -> String {
    serde_json::to_string(issues).unwrap_or_else(|_| "[]".into())
}

/// A loaded animation; `sample(frame)` returns the packed frame.
#[wasm_bindgen]
pub struct MotionPlayer {
    motion: Motion,
    list: DisplayList,
    packed: Vec<f32>,
}

#[wasm_bindgen]
impl MotionPlayer {
    /// Load Lottie JSON (`slots` = a JSON object of slot overrides, or "").
    /// Throws a JSON array of issues when refused.
    #[wasm_bindgen(constructor)]
    pub fn new(source: &str, slots: &str) -> Result<MotionPlayer, JsValue> {
        let slots: Map<String, Value> = if slots.is_empty() { Map::new() } else { serde_json::from_str(slots).unwrap_or_default() };
        let motion = Motion::load_with(source.as_bytes(), &Limits::default(), &slots).map_err(|i| JsValue::from_str(&issues_json(&i)))?;
        Ok(Self { motion, list: DisplayList::default(), packed: Vec::new() })
    }

    pub fn width(&self) -> f32 {
        self.motion.size().0
    }
    pub fn height(&self) -> f32 {
        self.motion.size().1
    }
    #[wasm_bindgen(js_name = frameRate)]
    pub fn frame_rate(&self) -> f64 {
        self.motion.frame_rate()
    }
    #[wasm_bindgen(js_name = inPoint)]
    pub fn in_point(&self) -> f64 {
        self.motion.frames().0
    }
    #[wasm_bindgen(js_name = outPoint)]
    pub fn out_point(&self) -> f64 {
        self.motion.frames().1
    }
    /// Markers as JSON `[{name, time, duration}]` (frames).
    pub fn markers(&self) -> String {
        serde_json::to_string(self.motion.markers()).unwrap_or_else(|_| "[]".into())
    }
    /// Image asset locations, in `Image` op order.
    pub fn images(&self) -> Vec<String> {
        self.motion.images().into_iter().map(|(_, location)| location).collect()
    }
    /// The non-fatal findings as a JSON array.
    pub fn warnings(&self) -> String {
        issues_json(&self.motion.warnings)
    }
    /// Register a font (TTF/OTF bytes) for text layers without glyphs.
    #[wasm_bindgen(js_name = addFont)]
    pub fn add_font(&mut self, family: &str, data: Vec<u8>) {
        self.motion.add_font(family, data);
    }
    #[wasm_bindgen(js_name = needsFonts)]
    pub fn needs_fonts(&self) -> bool {
        self.motion.needs_fonts()
    }

    /// Sample `frame` into the packed f32 encoding (a copy into JS memory).
    pub fn sample(&mut self, frame: f64) -> js_sys::Float32Array {
        self.motion.sample(frame, &mut self.list);
        self.list.encode(&mut self.packed);
        js_sys::Float32Array::from(self.packed.as_slice())
    }
}

/// Validate a Lottie JSON string: `{"errors":[…],"warnings":[…]}`.
#[wasm_bindgen]
pub fn validate(source: &str) -> String {
    let report = validate::check(source.as_bytes(), &Limits::default());
    json!({ "errors": report.errors, "warnings": report.warnings }).to_string()
}
