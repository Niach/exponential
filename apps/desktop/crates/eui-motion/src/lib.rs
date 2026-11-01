//! # eui-motion
//!
//! Exponential UI Motion (EXP-1584): loads a Lottie animation, validates it
//! against the eUI limits (expressions refused) and samples any frame into
//! the flat [`DisplayList`] every eUI painter replays.
//!
//! ```
//! let json = r#"{"v":"5.7.0","fr":30,"ip":0,"op":30,"w":10,"h":10,"layers":[]}"#;
//! let motion = eui_motion::Motion::load(json.as_bytes()).unwrap();
//! let mut list = eui_motion::DisplayList::default();
//! motion.sample(0.0, &mut list);
//! assert_eq!(list.width, 10.0);
//! ```

pub mod display;
#[cfg(feature = "raster")]
pub mod raster;
mod sink;
pub mod validate;

pub use display::{DisplayList, Op};
pub use validate::{Issue, Limits};

use std::cell::RefCell;

/// A loaded, validated animation.
pub struct Motion {
    composition: velato::Composition,
    /// Image asset ids, in the order `Op::Image::asset` indexes them.
    images: Vec<String>,
    renderer: RefCell<velato::Renderer>,
    /// Non-fatal findings (unsupported features rendered approximately or skipped).
    pub warnings: Vec<Issue>,
}

impl Motion {
    /// Parse and validate with the default [`Limits`].
    pub fn load(source: &[u8]) -> Result<Self, Vec<Issue>> {
        Self::load_with(source, &Limits::default())
    }

    /// Parse and validate with explicit limits.
    pub fn load_with(source: &[u8], limits: &Limits) -> Result<Self, Vec<Issue>> {
        let report = validate::check(source, limits);
        if !report.errors.is_empty() {
            return Err(report.errors);
        }
        let composition = velato::Composition::from_slice(source).map_err(|e| {
            vec![Issue {
                code: "parse".into(),
                path: String::new(),
                message: e.to_string(),
            }]
        })?;
        let mut images: Vec<String> = composition.images.keys().cloned().collect();
        images.sort();
        Ok(Self {
            composition,
            images,
            renderer: RefCell::new(velato::Renderer::new()),
            warnings: report.warnings,
        })
    }

    /// Composition width × height.
    pub fn size(&self) -> (f32, f32) {
        (
            self.composition.width as f32,
            self.composition.height as f32,
        )
    }

    /// Frames per second.
    pub fn frame_rate(&self) -> f64 {
        self.composition.frame_rate
    }

    /// The active frame range `[in, out)`.
    pub fn frames(&self) -> (f64, f64) {
        (self.composition.frames.start, self.composition.frames.end)
    }

    /// Duration in seconds.
    pub fn duration(&self) -> f64 {
        let (a, b) = self.frames();
        if self.frame_rate() > 0.0 {
            (b - a) / self.frame_rate()
        } else {
            0.0
        }
    }

    /// Image asset ids in `Op::Image::asset` order.
    pub fn images(&self) -> &[String] {
        &self.images
    }

    /// Location (`u` + `p`, or the data URL) of image asset `index`.
    pub fn image_location(&self, index: usize) -> Option<String> {
        let id = self.images.get(index)?;
        self.composition.images.get(id).map(|i| i.location())
    }

    /// Sample `frame` (composition frames, fractional allowed) into `list`.
    pub fn sample(&self, frame: f64, list: &mut DisplayList) {
        list.clear();
        let (w, h) = self.size();
        list.width = w;
        list.height = h;
        let mut recorder = sink::Recorder {
            list,
            images: &self.images,
        };
        self.renderer.borrow_mut().append(
            &self.composition,
            frame,
            kurbo::Affine::IDENTITY,
            1.0,
            &mut recorder,
        );
    }
}
