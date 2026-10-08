//! Content painters of the measured leaves that need no interaction state of
//! the view (the interactive ones — fields, sliders, toggle groups, carousel
//! dots, the composer — live in `view::paint`). Each returns the element
//! drawn INSIDE the leaf's frame div. Every one mirrors in RTL.

use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts::PI;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use exponential_ui::measure::TextStyle;
use exponential_ui::surface::PlacedNode;
use exponential_ui::theme::{Mode, ResolvedTheme};
use gpui::{
    canvas, div, img, point, prelude::*, px, Animation, AnimationExt as _, AnyElement, App, Bounds, Corners, Div, Font, Hsla, Image, ImageFormat, ImageSource, ObjectFit, PathBuilder, Pixels, Point,
    RenderImage, SharedString, TextRun, Transformation, Window,
};
use gpui_component::Icon;
use serde_json::{Map, Value};

use super::color::{color_of, hsl};
use super::icons::{self, Glyph};
use super::parts::{control, part_props, px_prop, spacing, theme_color};
use super::{Decoration, PaintStyle};
use crate::host::HostPlugin;
use crate::measure::{display_text, unknown_label};

/// Everything a leaf painter reads.
pub struct LeafCx<'a> {
    pub node: &'a PlacedNode,
    pub w: f32,
    pub h: f32,
    pub style: &'a PaintStyle,
    /// The inherited text colour.
    pub ink: Hsla,
    pub text_style: &'a TextStyle,
    pub font: Font,
    pub theme: Option<&'a ResolvedTheme>,
    pub mode: Mode,
    pub host: &'a dyn HostPlugin,
    /// The node's interaction states (hover / pressed / focus / …).
    pub states: &'a [String],
    /// The owner's props for a synthetic part (else the node's own).
    pub owner_props: &'a Map<String, Value>,
    /// The surface direction is right-to-left.
    pub rtl: bool,
    /// The platform asked for reduced motion.
    pub reduced_motion: bool,
    /// The leaf's corner radii after its clipping ancestors' (an image in a
    /// rounded card clips to the card's corners).
    pub radii: [f32; 4],
    /// The surface's number / date formatter (chart ticks and values).
    pub formatter: &'a dyn exponential_ui::format::Formatter,
}

impl LeafCx<'_> {
    pub fn str(&self, key: &str) -> &str {
        self.node.props.get(key).and_then(Value::as_str).unwrap_or("")
    }

    pub fn bool(&self, key: &str) -> bool {
        matches!(self.node.props.get(key), Some(Value::Bool(true))) || self.node.props.get(key).and_then(Value::as_str) == Some("true")
    }

    pub fn num(&self, key: &str) -> Option<f64> {
        match self.node.props.get(key)? {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.trim().parse().ok(),
            _ => None,
        }
    }

    pub fn has_state(&self, s: &str) -> bool {
        self.node.states.iter().any(|x| x == s) || self.states.iter().any(|x| x == s)
    }

    /// The content box inside padding + border: (x, y, w, h).
    pub fn inner(&self) -> (f32, f32, f32, f32) {
        let [t, r, b, l] = self.style.insets();
        (l, t, (self.w - l - r).max(0.0), (self.h - t - b).max(0.0))
    }

    /// An absolutely placed div over the content box with the leaf's text style.
    pub fn content(&self) -> Div {
        let (x, y, w, h) = self.inner();
        self.typed(div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h)))
    }

    pub fn typed(&self, d: Div) -> Div {
        let d = d.font(self.font.clone()).text_size(px(self.text_style.font_size)).line_height(px(self.text_style.line_height)).text_color(self.ink);
        match self.style.decoration {
            Some(Decoration::Underline) => d.underline().text_decoration_color(self.ink),
            Some(Decoration::LineThrough) => d.line_through(),
            None => d,
        }
    }

    /// A row in reading order (reversed in RTL).
    pub fn row<E: Styled>(&self, d: E) -> E {
        if self.rtl {
            d.flex().flex_row_reverse()
        } else {
            d.flex().flex_row()
        }
    }

    pub fn owner_component(&self) -> &str {
        self.node.owner_component.as_deref().unwrap_or(&self.node.component)
    }

    pub fn part(&self, component: &str, part: &str, states: &[String]) -> PaintStyle {
        PaintStyle::from_visual(&exponential_ui::style::visual(&part_props(self.theme, self.mode, component, part, self.owner_props, states), exponential_ui::style::BoxKind::Leaf))
    }

    pub fn part_props(&self, component: &str, part: &str, states: &[String]) -> Map<String, Value> {
        part_props(self.theme, self.mode, component, part, self.owner_props, states)
    }

    pub fn theme_color(&self, name: &str) -> Option<Hsla> {
        theme_color(self.theme, self.mode, name)
    }

    /// The physical text alignment: the style's, else the `align` prop
    /// (start/end by the direction), else the direction's start.
    pub fn align(&self) -> &'static str {
        resolve_align(self.style.text_align.as_deref().or_else(|| self.node.props.get("align").and_then(Value::as_str)), self.rtl)
    }
}

/// `left | right | center` for a (possibly logical) alignment.
pub fn resolve_align(a: Option<&str>, rtl: bool) -> &'static str {
    match a {
        Some("center") => "center",
        Some("right") => "right",
        Some("left") | Some("justify") => "left",
        Some("end") => {
            if rtl {
                "left"
            } else {
                "right"
            }
        }
        _ => {
            if rtl {
                "right"
            } else {
                "left"
            }
        }
    }
}

/// Apply a physical alignment to a text box.
pub fn aligned(d: Div, a: &str) -> Div {
    match a {
        "center" => d.text_center(),
        "right" => d.text_right(),
        _ => d.text_left(),
    }
}

/// A box painted from a part style (bg, border, radius, shadow, opacity).
pub fn styled_box<E: Styled + gpui::prelude::FluentBuilder>(d: E, s: &PaintStyle, w: f32, h: f32) -> E {
    let r = s.radius().min(w.min(h) / 2.0);
    let bw = s.border.iter().copied().fold(0.0, f32::max);
    d.when_some(s.bg, |d, bg| d.bg(bg))
        .when(bw > 0.0, |d| d.border(px(bw)).border_color(s.border_color.unwrap_or(gpui::transparent_black())))
        .rounded(px(r))
        .when(!s.shadows.is_empty(), |d| d.shadow(s.shadows.clone()))
        .when_some(s.opacity, |d, o| d.opacity(o))
}

/// A line's shaped width plus its `letter-spacing`.
pub fn tracked_width(window: &Window, font: &Font, size: f32, text: &str, ls: f32) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let run = TextRun { len: text.len(), font: font.clone(), color: gpui::black(), background_color: None, underline: None, strikethrough: None };
    f32::from(window.text_system().shape_line(SharedString::from(text.to_string()), px(size), &[run], None).width) + crate::measure::tracking_width(text, ls)
}

/// One line [`tracked_text`] painted (with [`record_tracked`] on): where
/// its glyphs went, so a test checks the PAINT, not the layout frame.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackedLine {
    pub text: String,
    /// The box the line painted in (window px).
    pub left: f32,
    pub room: f32,
    /// The first glyph's x (window px) and the extent painted: the last
    /// glyph's x plus its shaped advance and tracking (the ellipsis end when
    /// cut).
    pub start: f32,
    pub end: f32,
    /// The last painted glyph's x minus its untracked shaped x (= `ls` ×
    /// the characters before it).
    pub last_shift: f32,
    pub cut: bool,
    pub underline: Option<(f32, f32)>,
}

thread_local! {
    static TRACKED: RefCell<Option<Vec<TrackedLine>>> = const { RefCell::new(None) };
}

/// Record every line [`tracked_text`] paints on this thread (a test aid).
pub fn record_tracked(on: bool) {
    TRACKED.with(|t| *t.borrow_mut() = on.then(Vec::new));
}

/// The lines painted since the last call (recording stays on).
pub fn take_tracked() -> Vec<TrackedLine> {
    TRACKED.with(|t| t.borrow_mut().as_mut().map(std::mem::take).unwrap_or_default())
}

/// What [`tracked_text`] paints.
#[derive(Clone)]
pub struct Tracked {
    pub font: Font,
    pub size: f32,
    pub line_height: f32,
    pub ink: Hsla,
    pub decoration: Option<Decoration>,
    /// px after every character.
    pub ls: f32,
    pub lines: Vec<String>,
    /// `left | center | right`.
    pub align: &'static str,
    /// The last line ends in an ellipsis when it overflows.
    pub ellipsize: bool,
}

/// Text with CSS `letter-spacing`. gpui text runs have no tracking, so each
/// line is shaped once (kerning kept) and its glyphs are painted one by one
/// at their shaped positions plus `ls` per preceding character: the painted
/// width is exactly the measured one ([`tracked_width`]). Lines stack at
/// `line_height` (CSS half-leading), aligned in the element's box.
pub fn tracked_text(t: Tracked) -> gpui::Canvas<()> {
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, _, window, cx| {
            window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
                let size = px(t.size);
                let lh = px(t.line_height);
                let ellipsis_w = if t.ellipsize { tracked_width(window, &t.font, t.size, "…", t.ls) } else { 0.0 };
                for (k, line) in t.lines.iter().enumerate() {
                    if line.is_empty() {
                        continue;
                    }
                    let run = TextRun { len: line.len(), font: t.font.clone(), color: t.ink, background_color: None, underline: None, strikethrough: None };
                    let shaped = window.text_system().shape_line(SharedString::from(line.clone()), size, &[run], None);
                    let full = f32::from(shaped.width) + crate::measure::tracking_width(line, t.ls);
                    let room = f32::from(bounds.size.width);
                    let cut = t.ellipsize && k + 1 == t.lines.len() && full > room + 0.5;
                    let used = if cut { room } else { full };
                    let x0 = f32::from(bounds.origin.x)
                        + match t.align {
                            "center" => (room - used) / 2.0,
                            "right" => room - used,
                            _ => 0.0,
                        };
                    let top = bounds.origin.y + lh * k as f32;
                    let pad = (lh - shaped.ascent - shaped.descent) / 2.0;
                    let baseline = top + pad + shaped.ascent;
                    // Byte index → tracked characters before it (a bidi
                    // mark takes no letter-spacing).
                    let char_at: HashMap<usize, usize> = line
                        .char_indices()
                        .scan(0usize, |n, (b, c)| {
                            let before = *n;
                            *n += usize::from(crate::measure::is_tracked(c));
                            Some((b, before))
                        })
                        .collect();
                    let glyphs: Vec<(gpui::FontId, &gpui::ShapedGlyph)> = shaped.runs.iter().flat_map(|r| r.glyphs.iter().map(move |g| (r.font_id, g))).collect();
                    let mut end_x = x0;
                    let mut painted: Option<(f32, f32)> = None;
                    for (gi, (font_id, g)) in glyphs.iter().enumerate() {
                        let before = char_at.get(&g.index).copied().unwrap_or(0) as f32;
                        let x = x0 + f32::from(g.position.x) + t.ls * before;
                        painted = Some((painted.map_or(x, |p| p.0), x - x0 - f32::from(g.position.x)));
                        if cut {
                            let next = glyphs.get(gi + 1).map(|(_, n)| x0 + f32::from(n.position.x) + t.ls * char_at.get(&n.index).copied().unwrap_or(0) as f32).unwrap_or(x0 + full);
                            if next - x0 > room - ellipsis_w {
                                break;
                            }
                            end_x = next;
                        }
                        let origin = point(px(x), baseline);
                        let _ = if g.is_emoji { window.paint_emoji(origin, *font_id, g.id, size) } else { window.paint_glyph(origin, *font_id, g.id, size, t.ink) };
                    }
                    if cut {
                        let dots = TextRun { len: "…".len(), font: t.font.clone(), color: t.ink, background_color: None, underline: None, strikethrough: None };
                        let shaped_dots = window.text_system().shape_line(SharedString::from("…"), size, &[dots], None);
                        let _ = shaped_dots.paint(point(px(end_x), top), lh, gpui::TextAlign::Left, None, window, cx);
                    }
                    let width = px(if cut { room } else { full - t.ls.max(0.0) });
                    TRACKED.with(|log| {
                        if let Some(log) = log.borrow_mut().as_mut() {
                            let (start, last_shift) = painted.unwrap_or((x0, 0.0));
                            log.push(TrackedLine {
                                text: line.clone(),
                                left: f32::from(bounds.origin.x),
                                room,
                                start,
                                end: if cut { end_x + ellipsis_w } else { x0 + full },
                                last_shift,
                                cut,
                                underline: (t.decoration == Some(Decoration::Underline)).then(|| (x0, x0 + f32::from(width))),
                            });
                        }
                    });
                    match t.decoration {
                        Some(Decoration::Underline) => window.paint_underline(point(px(x0), baseline + shaped.descent * 0.618), width, &gpui::UnderlineStyle { thickness: px(1.0), color: Some(t.ink), wavy: false }),
                        Some(Decoration::LineThrough) => window.paint_strikethrough(point(px(x0), top + (shaped.ascent * 0.5 + pad + shaped.ascent) * 0.5), width, &gpui::StrikethroughStyle { thickness: px(1.0), color: Some(t.ink) }),
                        None => {}
                    }
                }
            });
        },
    )
}

impl LeafCx<'_> {
    /// The [`Tracked`] spec of this leaf's text (its font, ink, decoration
    /// and `letterSpacing`).
    pub fn tracked(&self, lines: Vec<String>, align: &'static str, ellipsize: bool) -> Tracked {
        Tracked { font: self.font.clone(), size: self.text_style.font_size, line_height: self.text_style.line_height, ink: self.ink, decoration: self.style.decoration, ls: self.style.letter_spacing, lines, align, ellipsize }
    }
}

/// Text lines (already broken by the measurer's rules) in the content box:
/// one line truncates with an ellipsis; a `lines` clamp shows `n` lines, the
/// last one ellipsized. `letterSpacing` paints through [`tracked_text`].
pub fn text_lines(cx: &LeafCx, lines: &[String], clamp: Option<usize>) -> AnyElement {
    let a = cx.align();
    let lh = cx.text_style.line_height;
    // Round 2 §2: every line shapes with the node's paragraph direction.
    let marked: Vec<String> = lines.iter().map(|l| crate::text::with_paragraph_direction(l, cx.rtl).into_owned()).collect();
    let lines = &marked[..];
    if cx.style.letter_spacing != 0.0 {
        let (x, y, w, h) = cx.inner();
        let shown: Vec<String> = match clamp {
            Some(1) => vec![lines.join(" ")],
            Some(n) if lines.len() > n && n > 0 => {
                let mut v: Vec<String> = lines[..n - 1].to_vec();
                v.push(lines[n - 1..].join(" "));
                v
            }
            _ => lines.to_vec(),
        };
        return tracked_text(cx.tracked(shown, a, clamp.is_some())).absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h)).into_any_element();
    }
    match clamp {
        // One ellipsized line: 1 px of slack on the trailing side, so a line
        // the measurer fitted exactly (sub-pixel float noise) never ellipsizes.
        Some(1) => {
            let (x, y, w, h) = cx.inner();
            let x = match a {
                "right" => x - 1.0,
                "center" => x - 0.5,
                _ => x,
            };
            aligned(cx.typed(div().absolute().left(px(x)).top(px(y)).w(px(w + 1.0)).h(px(h))), a).truncate().child(SharedString::from(lines.join(" "))).into_any_element()
        }
        Some(n) if lines.len() > n && n > 1 => {
            let mut col = cx.content().flex().flex_col().overflow_hidden();
            for (k, line) in lines.iter().enumerate().take(n) {
                let text = if k + 1 == n { lines[k..].join(" ") } else { line.clone() };
                let row = aligned(div().w_full().h(px(lh)).flex_none().whitespace_nowrap(), a);
                col = col.child(if k + 1 == n { row.truncate() } else { row.overflow_hidden() }.child(SharedString::from(text)));
            }
            col.into_any_element()
        }
        _ => aligned(cx.content(), a).whitespace_nowrap().child(SharedString::from(lines.join("\n"))).into_any_element(),
    }
}

/// The rotating loader glyph.
pub fn spinner_glyph(id: impl Into<gpui::ElementId>, size: f32, color: Hsla) -> AnyElement {
    Icon::new(gpui_component::IconName::LoaderCircle)
        .size(px(size))
        .text_color(color)
        .with_animation(id, Animation::new(Duration::from_millis(800)).repeat(), |icon, delta| icon.transform(Transformation::rotate(gpui::percentage(delta))))
        .into_any_element()
}

/// Button / Toggle content: spinner or icon, then the label (mirrored in RTL).
pub fn button(cx: &LeafCx, component: &str, window: &Window) -> AnyElement {
    let label = cx.str("label");
    let icon = cx.str("icon");
    let loading = cx.bool("loading");
    let icon_only = cx.str("size") == "icon";
    // The icon recipe for the BUTTON's own props (as the measurer reads
    // it): a part button (a FileUpload remove) is not sized by its owner's.
    let icon_size = px_prop(&part_props(cx.theme, cx.mode, component, "icon", &cx.node.props, &[]), "width").unwrap_or_else(|| control(cx.theme, "iconSm", 16.0));
    let mut row = cx.row(cx.content()).items_center().justify_center().gap(px(cx.style.gap)).whitespace_nowrap();
    if loading {
        row = row.child(spinner_glyph(SharedString::from(format!("{}.spinner", cx.node.id)), icon_size, cx.ink));
    } else if !icon.is_empty() {
        row = row.child(icons::concept_mirrored(cx.host, icon, icon_size, cx.ink, cx.rtl));
    } else if icon_only {
        row = row.child(icons::placeholder(icon_size, cx.ink));
    }
    if !icon_only && !label.is_empty() {
        let shown = crate::text::transform(label, cx.style.text_transform.as_deref()).into_owned();
        if cx.style.letter_spacing != 0.0 {
            let w = tracked_width(window, &cx.font, cx.text_style.font_size, &shown, cx.style.letter_spacing);
            row = row.child(tracked_text(cx.tracked(vec![shown], "left", true)).flex_shrink(1.0).min_w_0().w(px(w)).h(px(cx.text_style.line_height)));
        } else {
            row = row.child(SharedString::from(shown));
        }
    }
    row.into_any_element()
}

/// A Link: underlined label (`letterSpacing` painted per glyph, as
/// measured).
pub fn link(cx: &LeafCx) -> AnyElement {
    let label = if cx.str("label").is_empty() { cx.str("href") } else { cx.str("label") };
    let shown = crate::text::transform(label, cx.style.text_transform.as_deref()).into_owned();
    if cx.style.letter_spacing != 0.0 {
        let t = Tracked { decoration: Some(Decoration::Underline), ..cx.tracked(vec![shown], "center", true) };
        let (x, y, w, h) = cx.inner();
        let top = y + ((h - cx.text_style.line_height) / 2.0).max(0.0);
        return tracked_text(t).absolute().left(px(x)).top(px(top)).w(px(w)).h(px(cx.text_style.line_height.min(h.max(0.0)))).into_any_element();
    }
    cx.row(cx.content()).items_center().justify_center().whitespace_nowrap().underline().text_decoration_color(cx.ink).child(SharedString::from(shown)).into_any_element()
}

/// An `Icon` leaf (its style's rotation turns the glyph: the Collapsible
/// chevron).
pub fn icon(cx: &LeafCx) -> AnyElement {
    let (_, _, w, h) = cx.inner();
    let name = if cx.str("name").is_empty() { "ui-icon-placeholder" } else { cx.str("name") };
    // The glyph is the theme's Icon/root recipe size (`size: sm` →
    // `$control.iconSm` in the built-in themes; a remove button's box is 36,
    // its glyph 16), never larger than the box.
    let recipe = part_props(cx.theme, cx.mode, "Icon", "root", &cx.node.props, cx.states);
    let size = px_prop(&recipe, "width").or_else(|| px_prop(&recipe, "height")).unwrap_or(f32::INFINITY).min(w.min(h)).max(1.0);
    let rotate = cx.style.transform.rotate;
    let glyph = if rotate.abs() > 0.01 { icons::concept_rotated(cx.host, name, size, cx.ink, rotate.to_radians()) } else { icons::concept_mirrored(cx.host, name, size, cx.ink, cx.rtl) };
    cx.content().flex().items_center().justify_center().child(glyph).into_any_element()
}

/// Initials of a name (two words max, upper-cased).
pub fn initials(name: &str) -> String {
    name.split_whitespace().take(2).filter_map(|w| w.chars().next()).flat_map(char::to_uppercase).collect()
}

/// The avatar seed hue (`seedHue` of the React renderer).
pub fn seed_hue(seed: &str) -> f32 {
    let mut h: u32 = 0;
    for c in seed.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(c as u32);
    }
    (h % 360) as f32
}

/// Standard base64 (padding optional, whitespace skipped).
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut bits = 0;
    for b in s.bytes() {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

thread_local! {
    static DATA_IMAGES: RefCell<HashMap<String, Option<Arc<Image>>>> = RefCell::new(HashMap::new());
}

/// A `data:image/<type>;base64,…` URI decoded once (cached per process).
pub fn data_image(src: &str) -> Option<Arc<Image>> {
    let rest = src.strip_prefix("data:")?;
    if let Some(hit) = DATA_IMAGES.with(|m| m.borrow().get(src).cloned()) {
        return hit;
    }
    let decoded = (|| {
        let (meta, data) = rest.split_once(',')?;
        let mime = meta.split(';').next().unwrap_or("");
        let format = match mime {
            "image/png" => ImageFormat::Png,
            "image/jpeg" | "image/jpg" => ImageFormat::Jpeg,
            "image/webp" => ImageFormat::Webp,
            "image/gif" => ImageFormat::Gif,
            "image/svg+xml" => ImageFormat::Svg,
            "image/bmp" => ImageFormat::Bmp,
            "image/tiff" => ImageFormat::Tiff,
            _ => return None,
        };
        let bytes = if meta.ends_with(";base64") { base64_decode(data)? } else { percent_decode(data) };
        Some(Arc::new(Image::from_bytes(format, bytes)))
    })();
    DATA_IMAGES.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() > 256 {
            m.clear();
        }
        m.insert(src.to_string(), decoded.clone());
    });
    decoded
}

fn percent_decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

/// An image source: `data:` decoded, a URL loaded, an absolute path read
/// from disk, anything else the host app's embedded asset of that path.
pub fn image_source(src: &str) -> Option<ImageSource> {
    if src.is_empty() {
        return None;
    }
    if src.starts_with("data:") {
        return data_image(src).map(ImageSource::from);
    }
    if src.starts_with('/') && !src.starts_with("//") {
        return Some(ImageSource::from(PathBuf::from(src)));
    }
    Some(ImageSource::from(src.to_string()))
}

/// `Avatar`: the image when the host resolves one, else tinted initials.
pub fn avatar(cx: &LeafCx) -> AnyElement {
    let name = cx.str("name");
    let seed = if cx.str("seed").is_empty() { name } else { cx.str("seed") };
    let fallback = cx.part("Avatar", "fallback", &[]);
    let (bg, fg) = if seed.is_empty() {
        (fallback.bg, fallback.color.unwrap_or(cx.ink))
    } else {
        let hue = seed_hue(seed);
        (Some(hsl(hue, 0.7, 0.55, 0.22)), hsl(hue, 0.6, if cx.mode == Mode::Dark { 0.72 } else { 0.38 }, 1.0))
    };
    let letters = {
        let s = initials(name);
        if s.is_empty() {
            "?".to_string()
        } else {
            s
        }
    };
    let size = cx.w.min(cx.h);
    let fs = px_prop(&cx.part_props("Avatar", "fallback", &[]), "fontSize").unwrap_or((size * 0.4).round());
    let initials_el = move || {
        div().size_full().flex().items_center().justify_center().rounded_full().when_some(bg, |d, bg| d.bg(bg)).text_color(fg).text_size(px(fs)).child(SharedString::from(letters.clone())).into_any_element()
    };
    let src = cx.str("src");
    let mut out = div().size_full().rounded_full().overflow_hidden();
    match crate::media::image_source(cx.host, src) {
        Some(source) => {
            let fb = initials_el.clone();
            out = out.child(img(source).size_full().rounded_full().object_fit(ObjectFit::Cover).with_fallback(fb).with_loading(initials_el));
        }
        None => out = out.child(initials_el()),
    }
    out.into_any_element()
}

/// What an `Image` paints without a loadable source (round 2 §7): the
/// `Image/fallback` part fills the box (its recipe colours) with the glyph
/// (`fallback`, else the builtin `Image.fallback`) at `$control.iconLg`,
/// centred. The alt text stays the accessible name. Owned, so the image
/// loader's fallback / loading closures can paint it later.
#[derive(Clone)]
pub struct ImageFallback {
    bg: Option<Hsla>,
    ink: Hsla,
    size: f32,
    name: String,
    path: Option<SharedString>,
}

impl ImageFallback {
    pub fn of(cx: &LeafCx) -> ImageFallback {
        let part = cx.part("Image", "fallback", &[]);
        let ink = part.color.or_else(|| cx.theme_color("mutedForeground")).unwrap_or(cx.ink);
        let bg = part.bg.or_else(|| cx.theme_color("muted"));
        let size = cx.theme.and_then(|t| t.tokens.control.get("iconLg").copied()).unwrap_or(24.0) as f32;
        let name = match cx.str("fallback") {
            "" => exponential_ui::layout_tree::builtin_icon("Image.fallback").unwrap_or("ui-icon-placeholder").to_string(),
            f => f.to_string(),
        };
        let path = cx.host.icon(&name);
        ImageFallback { bg, ink, size, name, path }
    }

    pub fn element(&self) -> AnyElement {
        let glyph = match &self.path {
            Some(p) => gpui::svg().path(p.clone()).size(px(self.size)).text_color(self.ink).into_any_element(),
            None => icons::glyph(Glyph::for_concept(&self.name).unwrap_or(Glyph::Image), self.size, self.ink),
        };
        div().size_full().flex().items_center().justify_center().when_some(self.bg, |d, bg| d.bg(bg)).child(glyph).into_any_element()
    }
}

pub fn image_placeholder(cx: &LeafCx) -> AnyElement {
    ImageFallback::of(cx).element()
}

fn rounded<E: Styled>(e: E, r: [f32; 4]) -> E {
    e.rounded_tl(px(r[0])).rounded_tr(px(r[1])).rounded_br(px(r[2])).rounded_bl(px(r[3]))
}

/// Where an image of `image` size lands in the box `(x, y, w, h)` for a CSS
/// `object-fit` and an `object-position` of `focal` (fractions 0..1).
pub fn focal_bounds(fit: &str, bx: (f32, f32, f32, f32), image: (f32, f32), focal: (f32, f32)) -> (f32, f32, f32, f32) {
    let (x, y, w, h) = bx;
    let (iw, ih) = image;
    if fit == "fill" || iw <= 0.0 || ih <= 0.0 {
        return bx;
    }
    let contain = (w / iw).min(h / ih);
    let scale = match fit {
        "contain" => contain,
        "none" => 1.0,
        "scaleDown" => contain.min(1.0),
        _ => (w / iw).max(h / ih),
    };
    let (dw, dh) = (iw * scale, ih * scale);
    let (fx, fy) = (focal.0.clamp(0.0, 1.0), focal.1.clamp(0.0, 1.0));
    (x + (w - dw) * fx, y + (h - dh) * fy, dw, dh)
}

/// The decoded image of a source if it loaded (`Some(Err)` = it failed;
/// `None` = still loading, the view repaints when it lands).
fn render_image(source: &ImageSource, window: &mut Window, cx: &mut App) -> Option<Result<Arc<RenderImage>, ()>> {
    match source {
        ImageSource::Resource(r) => window.use_asset::<gpui::ImgResourceLoader>(r, cx).map(|r| r.map_err(|_| ())),
        ImageSource::Image(i) => {
            let out = i.clone().use_render_image(window, cx);
            // A data URI decodes synchronously-ish; a failure stays `None`
            // from gpui, so an undecodable payload shows the placeholder.
            out.map(Ok)
        }
        ImageSource::Render(r) => Some(Ok(r.clone())),
        // The host's media loader (VAPP-91: a request with headers).
        ImageSource::Custom(load) => load(window, cx).map(|r| r.map_err(|_| ())),
    }
}

/// `Image`: data URIs, URLs, files and host assets through gpui `img()`
/// (the placeholder while loading and on failure), clipped to the leaf's
/// (and its rounded ancestors') corners. `focalX` / `focalY` (CSS
/// `object-position`) place the decoded image with [`focal_bounds`].
pub fn image(cx: &LeafCx, window: &mut Window, app: &mut App) -> AnyElement {
    let src = cx.str("src");
    let Some(source) = crate::media::image_source(cx.host, src) else {
        return rounded(div().size_full().overflow_hidden(), cx.radii).child(image_placeholder(cx)).into_any_element();
    };
    let fit_name = match cx.str("fit") {
        f @ ("contain" | "fill" | "none" | "scaleDown") => f,
        _ => "cover",
    };
    let fit = match fit_name {
        "contain" => ObjectFit::Contain,
        "fill" => ObjectFit::Fill,
        "none" => ObjectFit::None,
        "scaleDown" => ObjectFit::ScaleDown,
        _ => ObjectFit::Cover,
    };
    let focal = (cx.num("focalX").unwrap_or(0.5) as f32, cx.num("focalY").unwrap_or(0.5) as f32);
    let centred = (focal.0 - 0.5).abs() < 1e-3 && (focal.1 - 0.5).abs() < 1e-3;
    if !centred && fit_name != "fill" {
        // gpui's `object_fit` always centres: paint the decoded image at its
        // focal bounds, the leaf's rounded corners clipping it.
        return match render_image(&source, window, app) {
            Some(Ok(data)) => {
                let radii = cx.radii;
                let fit_name = fit_name.to_string();
                canvas(
                    |_, _, _| {},
                    move |bounds: Bounds<Pixels>, _, window, _| {
                        let size = data.size(0);
                        let (bx, by, bw, bh) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y), f32::from(bounds.size.width), f32::from(bounds.size.height));
                        let (x, y, w, h) = focal_bounds(&fit_name, (bx, by, bw, bh), (size.width.0 as f32, size.height.0 as f32), focal);
                        let image_bounds = Bounds { origin: point(px(x), px(y)), size: gpui::size(px(w), px(h)) };
                        let corners = Corners { top_left: px(radii[0]), top_right: px(radii[1]), bottom_right: px(radii[2]), bottom_left: px(radii[3]) };
                        let _ = window.paint_image(bounds, image_bounds, corners, data.clone(), 0, false);
                    },
                )
                .size_full()
                .into_any_element()
            }
            _ => rounded(div().size_full().overflow_hidden(), cx.radii).child(image_placeholder(cx)).into_any_element(),
        };
    }
    let (fa, fb) = (ImageFallback::of(cx), ImageFallback::of(cx));
    rounded(div().size_full().overflow_hidden(), cx.radii)
        .child(rounded(img(source).size_full().object_fit(fit), cx.radii).with_fallback(move || fa.element()).with_loading(move || fb.element()))
        .into_any_element()
}

/// `m:ss` / `h:mm:ss`.
pub fn fmt_duration(ms: f64) -> String {
    let total = (ms / 1000.0).max(0.0).round() as u64;
    let (h, m, s) = (total / 3600, (total / 60) % 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// `Video`: the poster (or a dark tint) with a play button and the duration.
pub fn video(cx: &LeafCx) -> AnyElement {
    let poster = cx.str("poster");
    let mut out = rounded(div().size_full().relative().overflow_hidden().bg(gpui::black().opacity(0.85)), cx.radii);
    if let Some(source) = crate::media::image_source(cx.host, poster) {
        out = out.child(img(source).absolute().size_full().object_fit(ObjectFit::Cover).with_fallback(|| div().into_any_element()));
    }
    let white = gpui::white();
    out = out.child(div().absolute().size_full().flex().items_center().justify_center().child(div().size(px(44.0)).rounded_full().bg(white.opacity(0.18)).flex().items_center().justify_center().child(icons::glyph(Glyph::Play, 20.0, white))));
    if let Some(ms) = cx.num("durationMs") {
        let d = div().absolute().bottom(px(6.0)).px(px(6.0)).rounded(px(4.0)).bg(gpui::black().opacity(0.6)).text_color(white).text_size(px(12.0)).child(SharedString::from(fmt_duration(ms)));
        out = out.child(if cx.rtl { d.left(px(8.0)) } else { d.right(px(8.0)) });
    }
    out.into_any_element()
}

/// `AudioPlayer`: the title line over a controls bar.
pub fn audio(cx: &LeafCx) -> AnyElement {
    let title = cx.str("title");
    let gap = spacing(cx.theme, "xs");
    let muted = cx.theme_color("muted").unwrap_or(cx.ink.opacity(0.1));
    let muted_fg = cx.theme_color("mutedForeground").unwrap_or(cx.ink.opacity(0.6));
    let track = cx.part("AudioPlayer", "track", &[]);
    let duration = cx.num("durationMs").map(fmt_duration).unwrap_or_else(|| "0:00".into());
    let mut col = cx.content().flex().flex_col().gap(px(gap));
    if !title.is_empty() {
        col = col.child(aligned(div().truncate(), cx.align()).text_color(track.color.unwrap_or(cx.ink)).child(SharedString::from(title.to_string())));
    }
    // The controls row: `AudioPlayer/controls` height, else `$control.row`.
    let row_h = px_prop(&cx.part_props("AudioPlayer", "controls", &[]), "height").unwrap_or_else(|| control(cx.theme, "row", 40.0));
    col.child(
        cx.row(div())
            .h(px(row_h))
            .flex_none()
            .w_full()
            .rounded(px(row_h / 2.0))
            .bg(muted)
            .items_center()
            .gap(px(8.0))
            .px(px(8.0))
            .child(div().size(px(28.0)).rounded_full().bg(cx.ink).flex().items_center().justify_center().child(icons::glyph(Glyph::Play, 14.0, cx.theme_color("background").unwrap_or(gpui::white()))))
            .child(div().flex_1().h(px(4.0)).rounded_full().bg(muted_fg.opacity(0.35)))
            .child(div().text_size(px(12.0)).text_color(muted_fg).child(SharedString::from(format!("0:00 / {duration}")))),
    )
    .into_any_element()
}

/// `Spinner`.
pub fn spinner(cx: &LeafCx) -> AnyElement {
    let (_, _, w, h) = cx.inner();
    cx.content().flex().items_center().justify_center().child(spinner_glyph(SharedString::from(format!("{}.spin", cx.node.id)), w.min(h).max(1.0), cx.ink)).into_any_element()
}

fn arc_points(center: Point<Pixels>, r: f32, from: f32, sweep: f32) -> Vec<Point<Pixels>> {
    let steps = ((sweep.abs() / (2.0 * PI)) * 64.0).ceil().max(2.0) as usize;
    (0..=steps)
        .map(|i| {
            let a = from + sweep * i as f32 / steps as f32;
            point(center.x + px(r * a.cos()), center.y + px(r * a.sin()))
        })
        .collect()
}

pub(crate) fn stroke_arc(window: &mut gpui::Window, center: Point<Pixels>, r: f32, from: f32, sweep: f32, width: f32, color: Hsla) {
    if sweep.abs() < 0.001 || r <= 0.0 {
        return;
    }
    let mut b = PathBuilder::stroke(px(width));
    let pts = arc_points(center, r, from, sweep);
    b.move_to(pts[0]);
    for p in &pts[1..] {
        b.line_to(*p);
    }
    if let Ok(path) = b.build() {
        window.paint_path(path, color);
    }
}

/// `Ring`: track circle + value arc from 12 o'clock (counter-clockwise in
/// RTL), label centred.
pub fn ring(cx: &LeafCx) -> AnyElement {
    let value = cx.num("value").unwrap_or(0.0).clamp(0.0, 1.0) as f32;
    let track = cx.part_props("Ring", "track", &[]);
    let fill = cx.part_props("Ring", "fill", &[]);
    let stroke = px_prop(&track, "borderWidth").unwrap_or(2.0);
    let fill_stroke = px_prop(&fill, "borderWidth").unwrap_or(stroke);
    let track_color = color_of(track.get("color").and_then(Value::as_str)).unwrap_or(cx.ink.opacity(0.15));
    let fill_color = color_of(fill.get("color").and_then(Value::as_str)).unwrap_or(cx.ink);
    let label = cx.str("label").to_string();
    let label_style = cx.part("Ring", "label", &[]);
    let size = cx.w.min(cx.h);
    let dir = if cx.rtl { -1.0 } else { 1.0 };
    let mut out = div().size_full().relative().child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let c = bounds.center();
                let r = size / 2.0 - stroke.max(fill_stroke) / 2.0;
                stroke_arc(window, c, r, 0.0, 2.0 * PI, stroke, track_color);
                stroke_arc(window, c, r, -PI / 2.0, dir * 2.0 * PI * value, fill_stroke, fill_color);
            },
        )
        .absolute()
        .size_full(),
    );
    if !label.is_empty() {
        let fs = px_prop(&cx.part_props("Ring", "label", &[]), "fontSize").unwrap_or(12.0);
        out = out.child(div().absolute().size_full().flex().items_center().justify_center().whitespace_nowrap().text_size(px(fs)).text_color(label_style.color.unwrap_or(cx.ink)).child(SharedString::from(label)));
    }
    out.into_any_element()
}

/// `TreeGuides`: 16 px columns, 1 px lines at x = 7, the elbow at mid-height
/// (mirrored in RTL).
pub fn tree_guides(cx: &LeafCx) -> AnyElement {
    let depth = cx.num("depth").unwrap_or(0.0).max(0.0) as usize;
    let elbow_at = cx.num("elbowAt").map(|v| v as i64).unwrap_or(depth as i64 - 1);
    let tee = cx.bool("tee");
    let pass: Vec<i64> = cx.node.props.get("passThrough").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).map(|v| v as i64).collect()).unwrap_or_default();
    let line = cx.part_props("TreeGuides", "line", &[]);
    let color = color_of(line.get("color").and_then(Value::as_str)).unwrap_or(cx.ink.opacity(0.25));
    let lw = px_prop(&line, "width").unwrap_or(1.0);
    let h = cx.h;
    let w = cx.w;
    let mx = |x: f32, width: f32| if cx.rtl { w - x - width } else { x };
    let mut out = div().size_full().relative();
    for i in 0..depth {
        let x = i as f32 * 16.0 + 7.0;
        if pass.contains(&(i as i64)) {
            out = out.child(div().absolute().left(px(mx(x, lw))).top_0().w(px(lw)).h(px(h)).bg(color));
        }
        if i as i64 == elbow_at {
            out = out.child(div().absolute().left(px(mx(x, lw))).top_0().w(px(lw)).h(px(if tee { h } else { h / 2.0 })).bg(color));
            out = out.child(div().absolute().left(px(mx(x, 16.0 - 7.0))).top(px((h / 2.0).floor())).w(px(16.0 - 7.0)).h(px(lw)).bg(color));
        }
    }
    out.into_any_element()
}

/// The `Unknown` placeholder note.
pub fn unknown(cx: &LeafCx) -> AnyElement {
    let label = cx.part_props("Unknown", "label", &[]);
    let color = color_of(label.get("color").and_then(Value::as_str)).or_else(|| cx.theme_color("destructive")).unwrap_or(cx.ink);
    let fs = px_prop(&label, "fontSize").unwrap_or(12.0);
    let lh = px_prop(&label, "lineHeight").unwrap_or(16.0);
    let family = label.get("fontFamily").and_then(Value::as_str).map(|f| cx.host.font_family(f));
    aligned(cx.content(), cx.align())
        .text_color(color)
        .text_size(px(fs))
        .line_height(px(lh))
        .when_some(family, |d, f| d.font_family(f))
        .child(SharedString::from(unknown_label(&cx.node.props, &cx.node.component)))
        .into_any_element()
}

/// A Checkbox `box` (or a Table `checkbox`): the check glyph when checked.
pub fn check_box(cx: &LeafCx, checked: bool) -> AnyElement {
    if !checked {
        return div().into_any_element();
    }
    let check = cx.part_props("Checkbox", "check", &[]);
    let size = px_prop(&check, "width").unwrap_or(12.0);
    let color = color_of(check.get("color").and_then(Value::as_str)).or(cx.style.color).unwrap_or(cx.ink);
    div().size_full().flex().items_center().justify_center().child(icons::concept(cx.host, "ui-check", size, color)).into_any_element()
}

/// Where a Switch thumb sits: the end side when checked (the left in RTL).
pub fn switch_thumb_x(checked: bool, w: f32, size: f32, inset: f32, rtl: bool) -> f32 {
    let end = checked != rtl;
    if end {
        w - size - inset
    } else {
        inset
    }
}

/// A Switch `track`: the thumb, travelled to the end side when checked.
pub fn switch_thumb(cx: &LeafCx, checked: bool) -> AnyElement {
    let states: Vec<String> = if checked { vec!["checked".into()] } else { vec![] };
    let mut props = cx.owner_props.clone();
    props.insert("checked".into(), Value::Bool(checked));
    let tp = part_props(cx.theme, cx.mode, "Switch", "thumb", &props, &states);
    let thumb = PaintStyle::from_visual(&exponential_ui::style::visual(&tp, exponential_ui::style::BoxKind::Leaf));
    let size = px_prop(&tp, "width").unwrap_or((cx.h - 4.0).max(4.0));
    let inset = ((cx.h - size) / 2.0).max(0.0);
    let x = switch_thumb_x(checked, cx.w, size, inset, cx.rtl);
    let fallback = cx.theme_color("background").unwrap_or(gpui::white());
    let style = PaintStyle { bg: thumb.bg.or(Some(fallback)), radii: [size / 2.0; 4], ..thumb };
    div().size_full().relative().child(styled_box(div().absolute().left(px(x)).top(px(inset)).size(px(size)), &style, size, size)).into_any_element()
}

/// A picker trigger's content: the core's text (muted when it is the
/// placeholder) and the trigger glyph at the end side.
pub fn trigger_content(cx: &LeafCx, label: &str, is_placeholder: bool, icon: &str) -> AnyElement {
    let ph = cx.part(cx.owner_component(), "placeholder", &[]);
    let muted = ph.color.or_else(|| cx.theme_color("mutedForeground")).unwrap_or(cx.ink.opacity(0.6));
    let color = if is_placeholder { muted } else { cx.ink };
    let icon = if icon.is_empty() { "ui-chevron-down" } else { icon };
    let shown = crate::text::transform(label, cx.style.text_transform.as_deref()).into_owned();
    let text = if cx.style.letter_spacing != 0.0 {
        tracked_text(Tracked { ink: color, ..cx.tracked(vec![shown], cx.align(), true) }).min_w_0().flex_1().h(px(cx.text_style.line_height)).into_any_element()
    } else {
        aligned(div().min_w_0().flex_1().truncate(), cx.align()).text_color(color).child(SharedString::from(shown)).into_any_element()
    };
    cx.row(cx.content())
        .items_center()
        .justify_between()
        .gap(px(spacing(cx.theme, "sm")))
        .child(text)
        .child(div().flex_none().opacity(0.6).child(icons::concept(cx.host, icon, 16.0, cx.ink)))
        .into_any_element()
}

/// Slider geometry along the track: `(fill_x, fill_w, thumb_x)` (the fill
/// grows from the end side in RTL).
pub fn slider_geometry(fraction: f32, w: f32, thumb: f32, rtl: bool) -> (f32, f32, f32) {
    let f = fraction.clamp(0.0, 1.0);
    if rtl {
        (w * (1.0 - f), w * f, (w - thumb) * (1.0 - f))
    } else {
        (0.0, w * f, (w - thumb) * f)
    }
}

/// The value bar of a Slider `track` (range + thumb at `fraction`).
pub fn slider(cx: &LeafCx, fraction: f32) -> AnyElement {
    let track_props = cx.part_props("Slider", "track", &[]);
    let bar_h = px_prop(&track_props, "height").unwrap_or(6.0).min(cx.h.max(1.0));
    let track = PaintStyle::from_visual(&exponential_ui::style::visual(&track_props, exponential_ui::style::BoxKind::Leaf));
    let range = cx.part("Slider", "range", &[]);
    let thumb_states: Vec<String> = cx.states.iter().filter(|s| matches!(s.as_str(), "focus" | "focus-visible" | "hover" | "pressed")).cloned().collect();
    let thumb_props = cx.part_props("Slider", "thumb", &thumb_states);
    let thumb = PaintStyle::from_visual(&exponential_ui::style::visual(&thumb_props, exponential_ui::style::BoxKind::Leaf));
    let t = px_prop(&thumb_props, "width").unwrap_or(16.0).min(cx.h.max(1.0));
    let (fill_x, fill_w, thumb_x) = slider_geometry(fraction, cx.w, t, cx.rtl);
    let y = (cx.h - bar_h) / 2.0;
    let primary = cx.theme_color("primary").unwrap_or(cx.ink);
    let muted = cx.theme_color("muted").unwrap_or(cx.ink.opacity(0.15));
    let radius = (bar_h / 2.0).min(track.radius().max(bar_h / 2.0));
    let thumb_style = PaintStyle { bg: thumb.bg.or(Some(gpui::white())), radii: [t / 2.0; 4], ..thumb };
    div()
        .size_full()
        .relative()
        .child(div().absolute().left_0().top(px(y)).w(px(cx.w)).h(px(bar_h)).rounded(px(radius)).bg(track.bg.unwrap_or(muted)))
        .child(div().absolute().left(px(fill_x)).top(px(y)).w(px(fill_w)).h(px(bar_h)).rounded(px(radius)).bg(range.bg.unwrap_or(primary)))
        .child(styled_box(div().absolute().left(px(thumb_x)).top(px((cx.h - t) / 2.0)).size(px(t)), &thumb_style, t, t))
        .into_any_element()
}

/// `Skeleton`: the box (background + radius) pulses like the web's
/// `xui-pulse` (opacity 1 → .5 → 1 over 2 s); static under reduced motion.
pub fn skeleton(cx: &LeafCx) -> AnyElement {
    if cx.reduced_motion {
        return div().into_any_element();
    }
    let bg = cx.style.bg.unwrap_or(cx.ink.opacity(0.1));
    let r = cx.radii;
    rounded(div().absolute().size_full().bg(bg), r)
        .with_animation(SharedString::from(format!("{}.pulse", cx.node.id)), Animation::new(Duration::from_millis(2000)).repeat(), |d, t| {
            // 0 → .5 → 1 maps to opacity 1 → .5 → 1 (eased like CSS).
            let phase = if t < 0.5 { t * 2.0 } else { (1.0 - t) * 2.0 };
            d.opacity(1.0 - 0.5 * super::cubic_bezier([0.4, 0.0, 0.6, 1.0], phase))
        })
        .into_any_element()
}

/// A Table cell's boolean (a tick, or a muted dash).
pub fn bool_cell(cx: &LeafCx, value: bool) -> AnyElement {
    let muted = cx.theme_color("mutedForeground").unwrap_or(cx.ink.opacity(0.6));
    let inner = if value { icons::concept(cx.host, "ui-check", 16.0, cx.ink) } else { div().child("–").text_color(muted).into_any_element() };
    let d = cx.content().flex().items_center();
    let d = match cx.align() {
        "center" => d.justify_center(),
        "right" => d.justify_end(),
        _ => d.justify_start(),
    };
    d.child(inner).into_any_element()
}

/// A Table cell's badge (the value in a pill).
pub fn badge_cell(cx: &LeafCx, text: &str) -> AnyElement {
    let muted = cx.theme_color("secondary").or_else(|| cx.theme_color("muted")).unwrap_or(cx.ink.opacity(0.1));
    let fg = cx.theme_color("secondaryForeground").unwrap_or(cx.ink);
    let d = cx.content().flex().items_center();
    let d = match cx.align() {
        "center" => d.justify_center(),
        "right" => d.justify_end(),
        _ => d.justify_start(),
    };
    d.child(div().px(px(8.0)).rounded_full().bg(muted).text_color(fg).text_size(px((cx.text_style.font_size - 2.0).max(10.0))).whitespace_nowrap().child(SharedString::from(text.to_string()))).into_any_element()
}

/// The value of a cell as text (`display_text`).
pub fn cell_text(v: Option<&Value>) -> String {
    display_text(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avatar_initials_and_hues() {
        assert_eq!(initials("Alex Chen"), "AC");
        assert_eq!(initials("ada lovelace byron"), "AL");
        assert_eq!(initials(""), "");
        assert_eq!(seed_hue("Alex Chen"), seed_hue("Alex Chen"));
        assert!(seed_hue("x") < 360.0);
    }

    #[test]
    fn durations_format() {
        assert_eq!(fmt_duration(12_000.0), "0:12");
        assert_eq!(fmt_duration(1_860_000.0), "31:00");
        assert_eq!(fmt_duration(3_725_000.0), "1:02:05");
    }

    #[test]
    fn rtl_mirrors_the_controls() {
        assert_eq!(slider_geometry(0.25, 200.0, 16.0, false), (0.0, 50.0, 46.0));
        assert_eq!(slider_geometry(0.25, 200.0, 16.0, true), (150.0, 50.0, 138.0));
        assert_eq!(switch_thumb_x(true, 32.0, 16.0, 2.0, false), 14.0);
        assert_eq!(switch_thumb_x(true, 32.0, 16.0, 2.0, true), 2.0);
        assert_eq!(switch_thumb_x(false, 32.0, 16.0, 2.0, true), 14.0);
        assert_eq!(resolve_align(None, true), "right");
        assert_eq!(resolve_align(Some("end"), true), "left");
        assert_eq!(resolve_align(Some("end"), false), "right");
        assert_eq!(resolve_align(Some("center"), true), "center");
    }

    #[test]
    fn focal_points_place_the_image_like_object_position() {
        // A 200×100 image covering a 100×100 box: scaled to 200×100, the
        // focal point picks which part shows.
        assert_eq!(focal_bounds("cover", (0.0, 0.0, 100.0, 100.0), (200.0, 100.0), (0.0, 0.5)), (0.0, 0.0, 200.0, 100.0));
        assert_eq!(focal_bounds("cover", (0.0, 0.0, 100.0, 100.0), (200.0, 100.0), (1.0, 0.5)), (-100.0, 0.0, 200.0, 100.0));
        assert_eq!(focal_bounds("cover", (10.0, 0.0, 100.0, 100.0), (200.0, 100.0), (0.5, 0.5)), (-40.0, 0.0, 200.0, 100.0));
        // Contain letterboxes toward the focal edge.
        assert_eq!(focal_bounds("contain", (0.0, 0.0, 100.0, 100.0), (200.0, 100.0), (0.5, 0.0)), (0.0, 0.0, 100.0, 50.0));
        assert_eq!(focal_bounds("contain", (0.0, 0.0, 100.0, 100.0), (200.0, 100.0), (0.5, 1.0)), (0.0, 50.0, 100.0, 50.0));
        // none = natural size; scaleDown never enlarges; fill ignores focal.
        assert_eq!(focal_bounds("none", (0.0, 0.0, 100.0, 100.0), (40.0, 20.0), (0.0, 0.0)), (0.0, 0.0, 40.0, 20.0));
        assert_eq!(focal_bounds("scaleDown", (0.0, 0.0, 100.0, 100.0), (40.0, 20.0), (1.0, 1.0)), (60.0, 80.0, 40.0, 20.0));
        assert_eq!(focal_bounds("fill", (0.0, 0.0, 100.0, 100.0), (40.0, 20.0), (1.0, 1.0)), (0.0, 0.0, 100.0, 100.0));
    }

    #[test]
    fn data_uris_decode() {
        assert_eq!(base64_decode("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64_decode("aGVsbG8").unwrap(), b"hello");
        assert!(base64_decode("@@").is_none());
        let png = "data:image/png;base64,iVBORw0KGgo=";
        assert!(data_image(png).is_some());
        assert!(data_image("data:text/plain;base64,aGk=").is_none());
        assert!(image_source("https://x.test/a.png").is_some());
        assert!(image_source("").is_none());
    }
}
