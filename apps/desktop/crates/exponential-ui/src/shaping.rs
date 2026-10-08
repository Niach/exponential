//! Rust-side text shaping (feature `shaping`): a [`Measure`] that answers
//! text leaves in-process on the system fonts through cosmic-text, so a host
//! on Android pays no JNA upcall for text at all; only CONTROLS it draws
//! itself still go through its own measurer (`fallback`). Prototype: the
//! control box comes from the recipe (`ControlBox`) and the label text is
//! shaped like a `Text`.

use std::collections::HashMap;

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight, Wrap};

use crate::measure::{HeightRequest, Intrinsics, LeafRequest, Measure};

pub struct ShapingMeasure {
    pub font_system: FontSystem,
    /// The measurer for non-text leaves (images, custom controls); `None` =
    /// the recipe box / a fixed estimate.
    pub fallback: Option<Box<dyn Measure>>,
    id: u64,
    cache: HashMap<(String, i64), (f32, f32)>,
}

impl ShapingMeasure {
    /// Loads the system fonts (slow once; keep the measurer alive).
    pub fn system() -> ShapingMeasure {
        ShapingMeasure { font_system: FontSystem::new(), fallback: None, id: 11, cache: HashMap::new() }
    }

    pub fn with_fallback(mut self, fallback: Box<dyn Measure>) -> Self {
        self.fallback = Some(fallback);
        self
    }

    fn is_text(leaf: &LeafRequest) -> bool {
        matches!(leaf.component, "Text" | "Markdown" | "Button" | "Link" | "Toggle") || leaf.part.is_some_and(|p| matches!(p, "tab" | "trigger" | "item" | "label" | "title" | "description"))
    }

    /// Shape `text` with the leaf's style at `width` (`None` = unbounded).
    fn shape(&mut self, leaf: &LeafRequest, width: Option<f32>) -> (f32, f32) {
        let text = leaf.text().to_string();
        let key = (format!("{}|{}|{}|{}", leaf.index, text, leaf.text_style.font_size, leaf.text_style.font_weight), width.map(|w| (w * 64.0) as i64).unwrap_or(-1));
        if let Some(hit) = self.cache.get(&key) {
            return *hit;
        }
        let ts = leaf.text_style;
        let metrics = Metrics::new(ts.font_size, ts.line_height);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        let family = ts.font_family.clone();
        let attrs = Attrs::new().weight(Weight(ts.font_weight)).family(match family.as_deref() {
            Some(f) if f.eq_ignore_ascii_case("ui-monospace") || f.eq_ignore_ascii_case("mono") => Family::Monospace,
            Some(f) => Family::Name(f),
            None => Family::SansSerif,
        });
        buffer.set_wrap(if width.is_some() { Wrap::WordOrGlyph } else { Wrap::None });
        buffer.set_size(width, None);
        buffer.set_text(&text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);
        let mut w: f32 = 0.0;
        let mut lines = 0usize;
        for run in buffer.layout_runs() {
            w = w.max(run.line_w);
            lines += 1;
        }
        let lines = match leaf.lines {
            Some(n) if n > 0 => lines.min(n as usize),
            _ => lines,
        };
        let h = lines.max(1) as f32 * ts.line_height;
        let c = leaf.control;
        let out = (w.ceil() + 2.0 * c.padding_horizontal + 2.0 * c.border_width, h + 2.0 * c.padding_vertical + 2.0 * c.border_width);
        self.cache.insert(key, out);
        out
    }
}

impl Measure for ShapingMeasure {
    fn measure_id(&self) -> u64 {
        self.id
    }

    fn measure_intrinsics(&mut self, leaves: &[LeafRequest]) -> Vec<Intrinsics> {
        let mut out = Vec::with_capacity(leaves.len());
        let mut deferred: Vec<usize> = Vec::new();
        for (k, leaf) in leaves.iter().enumerate() {
            if Self::is_text(leaf) {
                let (max_w, h) = self.shape(leaf, None);
                let (min_w, _) = self.shape(leaf, Some(1.0));
                let c = leaf.control;
                let clamp = |v: f32| c.width.unwrap_or(v).max(c.min_width.unwrap_or(0.0));
                out.push(Intrinsics { min_content_width: clamp(min_w), max_content_width: clamp(max_w), height_at_max_content: c.height.unwrap_or(h).max(c.min_height.unwrap_or(0.0)) });
            } else {
                out.push(Intrinsics::default());
                deferred.push(k);
            }
        }
        if !deferred.is_empty() {
            let subset: Vec<LeafRequest> = deferred.iter().map(|&k| leaves[k].clone()).collect();
            let answers = match &mut self.fallback {
                Some(f) => f.measure_intrinsics(&subset),
                None => subset
                    .iter()
                    .map(|l| {
                        let c = l.control;
                        let w = c.width.unwrap_or(c.min_width.unwrap_or(0.0));
                        let h = c.height.unwrap_or(c.min_height.unwrap_or(0.0));
                        Intrinsics { min_content_width: w, max_content_width: w, height_at_max_content: h }
                    })
                    .collect(),
            };
            for (j, &k) in deferred.iter().enumerate() {
                out[k] = answers[j];
            }
        }
        out
    }

    fn measure_heights(&mut self, leaves: &[LeafRequest], requests: &[HeightRequest]) -> Vec<f32> {
        requests
            .iter()
            .map(|r| {
                let Some(leaf) = leaves.iter().find(|l| l.index == r.index) else { return 0.0 };
                if Self::is_text(leaf) {
                    let c = leaf.control;
                    let inner = (r.width - 2.0 * c.padding_horizontal - 2.0 * c.border_width).max(1.0);
                    let (_, h) = self.shape(leaf, Some(inner));
                    c.height.unwrap_or(h)
                } else if let Some(f) = &mut self.fallback {
                    f.measure_heights(std::slice::from_ref(leaf), std::slice::from_ref(r)).first().copied().unwrap_or(0.0)
                } else {
                    leaf.control.height.unwrap_or(0.0)
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface::{Surface, SurfaceOptions};
    use crate::types::NestedNode;

    #[test]
    fn shapes_text_in_process_and_wraps_at_narrow_widths() {
        let tree: NestedNode = serde_json::from_value(serde_json::json!({
            "id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
            "children": [{"id": "p", "component": "Text", "props": {"text": "a paragraph of words that must wrap onto several lines at a narrow width"}}]
        }))
        .unwrap();
        let mut surface = Surface::new("shape", SurfaceOptions::default());
        surface.set_nested(tree);
        let mut measure = ShapingMeasure::system();
        surface.set_viewport(600.0, 0.0, None);
        let wide = surface.layout(&mut measure);
        surface.set_viewport(160.0, 0.0, None);
        let narrow = surface.layout(&mut measure);
        assert!(narrow.frames[1].h > wide.frames[1].h, "narrow {} vs wide {}", narrow.frames[1].h, wide.frames[1].h);
        assert!(narrow.upcalls <= 2);
    }
}
