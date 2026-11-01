//! `eui-motion-wasm`: the browser (and server-side validation) facade over
//! eui-motion. Frames cross as a `Float32Array` in the packed display-list
//! encoding (`eui-motion/README.md` § Packed frames).

use eui_motion::{display::DisplayList, validate, Limits, Motion};
use wasm_bindgen::prelude::*;

/// A loaded animation; `sample(frame)` returns the packed frame.
#[wasm_bindgen]
pub struct MotionPlayer {
    motion: Motion,
    list: DisplayList,
    packed: Vec<f32>,
}

#[wasm_bindgen]
impl MotionPlayer {
    /// Load a Lottie JSON string; throws a JSON array of issues when refused.
    #[wasm_bindgen(constructor)]
    pub fn new(source: &str) -> Result<MotionPlayer, JsValue> {
        let motion = Motion::load(source.as_bytes())
            .map_err(|issues| JsValue::from_str(&serde_json::to_string(&issues).unwrap_or_default()))?;
        Ok(Self { motion, list: DisplayList::default(), packed: Vec::new() })
    }

    pub fn width(&self) -> f32 { self.motion.size().0 }
    pub fn height(&self) -> f32 { self.motion.size().1 }
    #[wasm_bindgen(js_name = frameRate)]
    pub fn frame_rate(&self) -> f64 { self.motion.frame_rate() }
    #[wasm_bindgen(js_name = inPoint)]
    pub fn in_point(&self) -> f64 { self.motion.frames().0 }
    #[wasm_bindgen(js_name = outPoint)]
    pub fn out_point(&self) -> f64 { self.motion.frames().1 }
    /// Image asset locations, in `Image` op order.
    pub fn images(&self) -> Vec<String> {
        (0..self.motion.images().len()).filter_map(|i| self.motion.image_location(i)).collect()
    }
    /// The non-fatal findings as a JSON array.
    pub fn warnings(&self) -> String { serde_json::to_string(&self.motion.warnings).unwrap_or_default() }

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
    serde_json::json!({ "errors": report.errors, "warnings": report.warnings }).to_string()
}
