//! Tier A drawing (#EDE-16): a small RGBA canvas and a text rasterizer on
//! swash with the embedded Inter font, for what the compositor draws
//! itself without gpui (window frames, the recovery screen). Colours come
//! from the design tokens (`packages/design-tokens/tokens.json`,
//! `palette`), fixed here in sRGB; never agent-editable.

use std::{cell::RefCell, collections::HashMap};

use swash::{
    scale::{image::Image, Render, ScaleContext, Source},
    zeno::Format,
    FontRef,
};

/// Inter Medium (OFL), the IDE's own file.
const FONT: &[u8] = include_bytes!("../../../../desktop/assets/fonts/Inter-Medium.ttf");

/// The palette's sRGB bytes (oklch converted; `tests::colours_match_the_tokens`).
pub mod colour {
    pub type Rgba = [u8; 4];
    pub const BACKGROUND: Rgba = [10, 10, 10, 255];
    pub const CARD: Rgba = [23, 23, 23, 255];
    pub const SECONDARY: Rgba = [38, 38, 38, 255];
    pub const FOREGROUND: Rgba = [250, 250, 250, 255];
    pub const MUTED_FOREGROUND: Rgba = [161, 161, 161, 255];
    pub const DESTRUCTIVE: Rgba = [255, 100, 103, 255];
    pub const RING: Rgba = [115, 115, 115, 255];
    /// `border` is white at 10 % over the card.
    pub const BORDER_ON_CARD: Rgba = [46, 46, 46, 255];
}

/// RGBA8, straight alpha while drawing; [`Canvas::premultiplied`] hands
/// smithay what `Abgr8888` memory buffers expect.
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height, data: vec![0; (width * height * 4) as usize] }
    }

    pub fn fill(&mut self, colour: [u8; 4]) {
        for px in self.data.chunks_exact_mut(4) {
            px.copy_from_slice(&colour);
        }
    }

    /// Blends `colour` at `alpha` (0..=255) over the pixel.
    pub fn blend(&mut self, x: i32, y: i32, colour: [u8; 4], alpha: u8) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 || alpha == 0 {
            return;
        }
        let i = ((y as u32 * self.width + x as u32) * 4) as usize;
        let a = (alpha as u32 * colour[3] as u32) / 255;
        let dst = &mut self.data[i..i + 4];
        let da = dst[3] as u32;
        let out_a = a + da * (255 - a) / 255;
        for c in 0..3 {
            let src = colour[c] as u32;
            let d = dst[c] as u32;
            dst[c] = if out_a == 0 { 0 } else { ((src * a + d * da * (255 - a) / 255) / out_a) as u8 };
        }
        dst[3] = out_a as u8;
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, colour: [u8; 4]) {
        for yy in y.max(0)..(y + h).min(self.height as i32) {
            for xx in x.max(0)..(x + w).min(self.width as i32) {
                self.blend(xx, yy, colour, 255);
            }
        }
    }

    /// A rectangle with rounded top corners (the title bar), anti-aliased.
    pub fn fill_rounded_top(&mut self, w: i32, h: i32, radius: f32, colour: [u8; 4]) {
        for y in 0..h {
            for x in 0..w {
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                let coverage = if fy < radius && (fx < radius || fx > w as f32 - radius) {
                    let (cx, cy) = (if fx < radius { radius } else { w as f32 - radius }, radius);
                    let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
                    (radius - d + 0.5).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                self.blend(x, y, colour, (coverage * 255.0) as u8);
            }
        }
    }

    /// An anti-aliased line `thickness` px wide.
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, thickness: f32, colour: [u8; 4]) {
        let (minx, maxx) = (x0.min(x1) - thickness, x0.max(x1) + thickness);
        let (miny, maxy) = (y0.min(y1) - thickness, y0.max(y1) + thickness);
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len2 = (dx * dx + dy * dy).max(1e-6);
        for y in miny.floor() as i32..=maxy.ceil() as i32 {
            for x in minx.floor() as i32..=maxx.ceil() as i32 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let t = (((px - x0) * dx + (py - y0) * dy) / len2).clamp(0.0, 1.0);
                let d = ((px - (x0 + t * dx)).powi(2) + (py - (y0 + t * dy)).powi(2)).sqrt();
                let coverage = (thickness / 2.0 - d + 0.5).clamp(0.0, 1.0);
                self.blend(x, y, colour, (coverage * 255.0) as u8);
            }
        }
    }

    /// The bytes for an `Abgr8888` memory buffer.
    pub fn premultiplied(&self) -> Vec<u8> {
        let mut out = self.data.clone();
        for px in out.chunks_exact_mut(4) {
            let a = px[3] as u32;
            for c in 0..3 {
                px[c] = (px[c] as u32 * a / 255) as u8;
            }
        }
        out
    }
}

/// A canvas imported as a texture, kept per renderer context and content
/// key; a new key re-imports under a fresh element id so the damage
/// tracker repaints it.
pub struct TextureCache {
    key: u64,
    context: Option<String>,
    id: smithay::backend::renderer::element::Id,
    texture: Option<Box<dyn std::any::Any>>,
    size: (i32, i32),
}

impl Default for TextureCache {
    fn default() -> Self {
        Self { key: 0, context: None, id: smithay::backend::renderer::element::Id::new(), texture: None, size: (0, 0) }
    }
}

impl TextureCache {
    /// The element for `draw()`'s canvas (drawn only when `key` or the
    /// renderer changed) at `at` (physical px) with `scale`.
    pub fn element<R>(&mut self, renderer: &mut R, key: u64, scale: i32, at: smithay::utils::Point<f64, smithay::utils::Physical>, draw: impl FnOnce() -> Canvas) -> Option<smithay::backend::renderer::element::texture::TextureRenderElement<R::TextureId>>
    where
        R: smithay::backend::renderer::Renderer + smithay::backend::renderer::ImportMem,
        R::TextureId: Clone + 'static,
    {
        use smithay::backend::renderer::element::{texture::TextureRenderElement, Id, Kind};
        let context = format!("{:?}", renderer.context_id());
        let fresh = self.key != key || self.context.as_deref() != Some(&context) || self.texture.as_ref().and_then(|t| t.downcast_ref::<R::TextureId>()).is_none();
        if fresh {
            let canvas = draw();
            let texture = renderer.import_memory(&canvas.premultiplied(), smithay::backend::allocator::Fourcc::Abgr8888, (canvas.width as i32, canvas.height as i32).into(), false).ok()?;
            if self.key != key {
                self.id = Id::new();
            }
            self.key = key;
            self.context = Some(context);
            self.size = (canvas.width as i32, canvas.height as i32);
            self.texture = Some(Box::new(texture));
        }
        let texture = self.texture.as_ref()?.downcast_ref::<R::TextureId>()?.clone();
        Some(TextureRenderElement::from_static_texture(self.id.clone(), renderer.context_id(), at, texture, scale, smithay::utils::Transform::Normal, None, None, None, None, Kind::Unspecified))
    }
}

/// A content key from anything hashable.
pub fn key_of(value: &impl std::hash::Hash) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut h);
    h.finish()
}

thread_local! {
    static SCALER: RefCell<ScaleContext> = RefCell::new(ScaleContext::new());
    static GLYPHS: RefCell<HashMap<(char, u32), Option<Image>>> = RefCell::new(HashMap::new());
}

fn font() -> FontRef<'static> {
    FontRef::from_index(FONT, 0).expect("the embedded Inter font")
}

/// The width of `text` at `size` px.
pub fn measure(text: &str, size: f32) -> f32 {
    let font = font();
    let charmap = font.charmap();
    let metrics = font.glyph_metrics(&[]).scale(size);
    text.chars().map(|c| metrics.advance_width(charmap.map(c))).sum()
}

/// `text` cut to fit `max_width` with an ellipsis.
pub fn fit(text: &str, size: f32, max_width: f32) -> String {
    if measure(text, size) <= max_width {
        return text.to_string();
    }
    let mut out = String::new();
    for c in text.chars() {
        out.push(c);
        if measure(&format!("{out}…"), size) > max_width {
            out.pop();
            break;
        }
    }
    format!("{}…", out.trim_end())
}

/// Draws `text` with its baseline at `y` starting at `x`; returns the width.
pub fn draw(canvas: &mut Canvas, x: f32, y: f32, text: &str, size: f32, colour: [u8; 4]) -> f32 {
    let font = font();
    let charmap = font.charmap();
    let metrics = font.glyph_metrics(&[]).scale(size);
    let mut pen = x;
    for c in text.chars() {
        let id = charmap.map(c);
        let image = GLYPHS.with(|g| {
            g.borrow_mut()
                .entry((c, size.to_bits()))
                .or_insert_with(|| {
                    SCALER.with(|s| {
                        let mut ctx = s.borrow_mut();
                        let mut scaler = ctx.builder(font).size(size).hint(true).build();
                        Render::new(&[Source::Outline]).format(Format::Alpha).render(&mut scaler, id)
                    })
                })
                .clone()
        });
        if let Some(image) = image {
            let (w, h) = (image.placement.width as i32, image.placement.height as i32);
            for gy in 0..h {
                for gx in 0..w {
                    let alpha = image.data[(gy * w + gx) as usize];
                    canvas.blend(pen.round() as i32 + image.placement.left + gx, y.round() as i32 - image.placement.top + gy, colour, alpha);
                }
            }
        }
        pen += metrics.advance_width(id);
    }
    pen - x
}

/// The font's ascent at `size` (where a baseline sits below a top edge).
pub fn ascent(size: f32) -> f32 {
    font().metrics(&[]).scale(size).ascent
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_draws_and_measures() {
        let mut c = Canvas::new(120, 30);
        c.fill(colour::CARD);
        let w = draw(&mut c, 4.0, 20.0, "Files", 14.0, colour::FOREGROUND);
        assert!(w > 20.0 && w < 60.0, "{w}");
        assert!((measure("Files", 14.0) - w).abs() < 0.01);
        // Some pixel in the text run got lighter than the card.
        assert!(c.data.chunks_exact(4).any(|px| px[0] > 100));
        assert_eq!(fit("A very long window title indeed", 14.0, 60.0).chars().last(), Some('…'));
        assert_eq!(fit("Short", 14.0, 600.0), "Short");
    }

    #[test]
    fn shapes_blend_with_coverage() {
        let mut c = Canvas::new(20, 20);
        c.fill_rounded_top(20, 20, 6.0, colour::CARD);
        assert_eq!(c.data[3], 0, "the corner pixel is outside the radius");
        assert_eq!(c.data[(10 * 20 + 10) * 4 + 3], 255);
        c.line(2.0, 2.0, 18.0, 18.0, 2.0, colour::FOREGROUND);
        assert!(c.data[(10 * 20 + 10) * 4] > 200);
        let pre = c.premultiplied();
        assert_eq!(&pre[..4], &[0, 0, 0, 0]);
    }

    /// The sRGB bytes above are the tokens' oklch values.
    #[test]
    fn colours_match_the_tokens() {
        let tokens: serde_json::Value = serde_json::from_str(include_str!("../../../../../packages/design-tokens/tokens.json")).unwrap();
        let palette = &tokens["palette"];
        let srgb = |key: &str| -> [u8; 4] {
            let text = palette[key].as_str().unwrap();
            let inner = text.trim_start_matches("oklch(").trim_end_matches(')');
            let parts: Vec<f64> = inner.split('/').next().unwrap().split_whitespace().map(|p| p.parse().unwrap()).collect();
            let (l, c, h) = (parts[0], parts[1], parts[2]);
            let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
            let (l_, m_, s_) = (l + 0.3963377774 * a + 0.2158037573 * b, l - 0.1055613458 * a - 0.0638541728 * b, l - 0.0894841775 * a - 1.2914855480 * b);
            let (ll, mm, ss) = (l_.powi(3), m_.powi(3), s_.powi(3));
            let lin = [4.0767416621 * ll - 3.3077115913 * mm + 0.2309699292 * ss, -1.2684380046 * ll + 2.6097574011 * mm - 0.3413193965 * ss, -0.0041960863 * ll - 0.7034186147 * mm + 1.7076147010 * ss];
            let gamma = |c: f64| {
                let c = c.clamp(0.0, 1.0);
                if c > 0.0031308 { 1.055 * c.powf(1.0 / 2.4) - 0.055 } else { 12.92 * c }
            };
            [(gamma(lin[0]) * 255.0).round() as u8, (gamma(lin[1]) * 255.0).round() as u8, (gamma(lin[2]) * 255.0).round() as u8, 255]
        };
        assert_eq!(srgb("background"), colour::BACKGROUND);
        assert_eq!(srgb("card"), colour::CARD);
        assert_eq!(srgb("secondary"), colour::SECONDARY);
        assert_eq!(srgb("foreground"), colour::FOREGROUND);
        assert_eq!(srgb("mutedForeground"), colour::MUTED_FOREGROUND);
        assert_eq!(srgb("destructive"), colour::DESTRUCTIVE);
        assert_eq!(srgb("ring"), colour::RING);
    }
}
