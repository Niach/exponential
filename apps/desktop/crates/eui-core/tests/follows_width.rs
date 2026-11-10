//! #EXP-1629: an autosize Textarea's field and the Composer grow with their
//! text wrapped at the width they are LAID OUT at. Their intrinsic width is
//! fixed (min == max), which alone would answer every width from the
//! max-content height: the core asks the measurer at the real width.

use eui_core::measure::{HeightRequest, Intrinsics, LeafRequest, Measure};
use eui_core::surface::{Surface, SurfaceOptions};
use eui_core::theme::Mode;
use eui_core::themes::builtin_theme;
use eui_core::types::NestedNode;
use serde_json::{json, Value};

/// The measured leaf's index, the widths it was asked at and its laid-out
/// frame (width, height).
type Laid = (Vec<(String, f32)>, (f32, f32));

/// 8 px per char, 20 px rows; a fixed 160 px intrinsic width; a width-less
/// pass counts the hard lines only (as the native measurers do).
struct Field {
    asked: Vec<(String, f32)>,
    leaf: Option<u32>,
}

const CHARS: f32 = 12_000.0;

fn rows_at(width: f32) -> f32 {
    (CHARS * 8.0 / width).ceil()
}

impl Measure for Field {
    fn measure_id(&self) -> u64 {
        1629
    }
    fn measure_intrinsics(&mut self, leaves: &[LeafRequest]) -> Vec<Intrinsics> {
        leaves.iter().map(|l| if l.part == Some("field") || l.component == "Composer" { self.leaf = Some(l.index); Intrinsics { min_content_width: 160.0, max_content_width: 160.0, height_at_max_content: 20.0, baseline: None } } else { Intrinsics::default() }).collect()
    }
    fn measure_heights(&mut self, leaves: &[LeafRequest], requests: &[HeightRequest]) -> Vec<f32> {
        requests
            .iter()
            .map(|r| {
                let leaf = leaves.iter().find(|l| l.index == r.index).expect("the leaf");
                self.asked.push((leaf.id.to_string(), r.width));
                rows_at(r.width) * 20.0
            })
            .collect()
    }
}

fn laid(field: Value) -> Laid {
    let tree: NestedNode = serde_json::from_value(json!({"id": "root", "component": "Box", "style": {"width": 400}, "children": [field]})).unwrap();
    let options = SurfaceOptions { theme: Some(builtin_theme("neutral").unwrap()), mode: Mode::Light, ..SurfaceOptions::default() };
    let mut s = Surface::new("t", options);
    assert!(s.set_nested(tree).issues.is_empty());
    s.set_viewport(400.0, 0.0, None);
    let mut m = Field { asked: Vec::new(), leaf: None };
    let out = s.layout(&mut m);
    let leaf = m.leaf.expect("the field was measured");
    let f = out.frames.iter().find(|f| f.index == leaf).expect("the field was laid out");
    (m.asked, (f.w, f.h))
}

#[test]
fn an_autosize_textarea_field_is_measured_at_its_laid_out_width() {
    let (asked, (w, h)) = laid(json!({"id": "t", "component": "Textarea", "style": {"width": 360}, "props": {"label": "Prompt", "name": "p", "variant": "code", "autosize": true, "rows": 10, "maxRows": 40}}));
    assert!(w > 160.0, "the field stretches past its 160 px intrinsic width: {w}");
    assert!(asked.iter().any(|(_, at)| (*at - w).abs() < 0.01), "asked at its laid width {w}: {asked:?}");
    assert_eq!(h, rows_at(w) * 20.0, "as tall as its rows at {w} px");
}

#[test]
fn a_fixed_textarea_is_never_asked_at_a_width() {
    let (asked, (_, h)) = laid(json!({"id": "t", "component": "Textarea", "style": {"width": 360}, "props": {"label": "Prompt", "name": "p", "variant": "code", "rows": 10}}));
    assert!(asked.is_empty(), "{asked:?}");
    assert_eq!(h, 20.0, "the max-content height");
}

#[test]
fn a_composer_is_measured_at_its_laid_out_width() {
    let (asked, (w, h)) = laid(json!({"id": "c", "component": "Composer", "style": {"width": 360}, "props": {"placeholder": "Message"}}));
    assert!(asked.iter().any(|(id, at)| id == "c" && (*at - w).abs() < 0.01), "asked at its laid width {w}: {asked:?}");
    assert_eq!(h, rows_at(w) * 20.0);
}
