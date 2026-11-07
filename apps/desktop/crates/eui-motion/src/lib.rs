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

pub mod anim;
pub mod canvas;
#[cfg(feature = "decompile")]
mod decompile;
pub mod dialect;
pub mod display;
pub mod dotlottie;
pub mod effects;
pub mod geom;
mod import;
pub mod model;
pub mod modifiers;
pub mod playback;
#[cfg(feature = "raster")]
pub mod raster;
pub mod render;
pub mod shapes;
pub mod state_machine;
pub mod text;
pub mod validate;

pub use canvas::Canvas;
pub use display::{DisplayList, Op};
pub use model::Marker;
pub use validate::{Issue, Limits};

/// A loaded, validated animation.
pub struct Motion {
    anim: model::Animation,
    /// Non-fatal findings (unsupported features rendered approximately or skipped).
    pub warnings: Vec<Issue>,
}

impl Motion {
    /// Parse and validate with the default [`Limits`].
    pub fn load(source: &[u8]) -> Result<Self, Vec<Issue>> {
        Self::load_with(source, &Limits::default(), &serde_json::Map::new())
    }

    /// Parse and validate with explicit limits and slot overrides (slot id →
    /// `{"p": property}` or a bare value, e.g. a resolved theme colour).
    pub fn load_with(source: &[u8], limits: &Limits, slots: &serde_json::Map<String, serde_json::Value>) -> Result<Self, Vec<Issue>> {
        let report = validate::check(source, limits);
        if !report.errors.is_empty() {
            return Err(report.errors);
        }
        let root: serde_json::Value = serde_json::from_slice(source).map_err(|e| {
            vec![Issue { code: "parse".into(), path: String::new(), message: e.to_string() }]
        })?;
        Ok(Self { anim: import::import(&root, slots), warnings: report.warnings })
    }

    /// Composition width × height.
    pub fn size(&self) -> (f32, f32) {
        (self.anim.width as f32, self.anim.height as f32)
    }

    /// Frames per second.
    pub fn frame_rate(&self) -> f64 {
        self.anim.frame_rate
    }

    /// The active frame range `[in, out)`.
    pub fn frames(&self) -> (f64, f64) {
        (self.anim.in_point, self.anim.out_point)
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

    /// Markers (named segments).
    pub fn markers(&self) -> &[Marker] {
        &self.anim.markers
    }

    /// Image assets in `Op::Image::asset` order: (id, location).
    pub fn images(&self) -> Vec<(String, String)> {
        self.anim.images.iter().map(|i| (i.id.clone(), i.location.clone())).collect()
    }

    /// Register a font file for text layers that ship no glyphs (`chars`).
    /// Hosts register the theme's fonts; the first registered font is the
    /// fallback for unknown families.
    pub fn add_font(&mut self, family: &str, data: Vec<u8>) {
        self.anim.font_book.add(family, data);
    }

    /// True when text layers have no glyphs to draw with: no `chars` in the
    /// file and no font registered with [`Motion::add_font`].
    pub fn needs_fonts(&self) -> bool {
        text::needs_fonts(&self.anim)
    }

    /// The composition's timeline (frame rate, range, markers) for [`playback`].
    pub fn timeline(&self) -> playback::Timeline {
        playback::Timeline {
            frame_rate: self.anim.frame_rate,
            in_point: self.anim.in_point,
            out_point: self.anim.out_point,
            markers: self.anim.markers.iter().map(|m| (m.name.clone(), m.time, m.duration)).collect(),
        }
    }

    /// The imported model.
    pub fn model(&self) -> &model::Animation {
        &self.anim
    }

    /// Sample `frame` (composition frames, fractional allowed) into `list`.
    pub fn sample(&self, frame: f64, list: &mut DisplayList) {
        render::render(&self.anim, frame, list);
    }
}
