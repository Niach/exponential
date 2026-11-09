//! Content painters of the measured leaves that need no interaction state of
//! the view (the interactive ones — fields, sliders, toggle groups, carousel
//! dots, the composer — live in `view::paint`). Each returns the element
//! drawn INSIDE the leaf's frame div. Every one mirrors in RTL.

use std::cell::RefCell;
use std::f32::consts::PI;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use eui_core::measure::TextStyle;
use eui_core::surface::PlacedNode;
use eui_core::theme::{Mode, ResolvedTheme};
use gpui::{
    canvas, div, img, point, prelude::*, px, Animation, AnimationExt as _, AnyElement, App, Bounds, Corners, Div, Font, Hsla, ImageSource, ObjectFit, PathBuilder, Pixels, Point,
    RenderImage, SharedString, TextRun, Transformation, Window,
};
use gpui_component::Icon;

use crate::controls::{SteppedAnimationExt as _, SMOOTH_STEP};
use serde_json::{Map, Value};

use super::color::color_of;
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
    /// EXP-1324: a shimmering Text's gradient now (`None` = plain ink;
    /// reduced motion never shimmers).
    pub shimmer: Option<ShimmerPaint>,
    /// The leaf's corner radii after its clipping ancestors' (an image in a
    /// rounded card clips to the card's corners).
    pub radii: [f32; 4],
    /// The surface's number / date formatter (chart ticks and values).
    pub formatter: &'a dyn eui_core::format::Formatter,
    /// The paint-only scale of the subtree the leaf sits in (1 = none;
    /// NEW-77: a VoiceWave's child); `w`, `h`, the style and the text style
    /// are already scaled, a recipe size read here is not.
    pub scale: f32,
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
        PaintStyle::from_visual(&eui_core::style::visual(&part_props(self.theme, self.mode, component, part, self.owner_props, states), eui_core::style::BoxKind::Leaf))
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
    let runs = crate::fallback::cluster_runs(window.text_system(), text, &[run], &crate::fallback::default_chain());
    f32::from(window.text_system().shape_line(SharedString::from(text.to_string()), px(size), &runs, None).width) + crate::measure::tracking_width(text, ls)
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
    /// Each painted glyph's colour (a shimmer varies them).
    pub inks: Vec<Hsla>,
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
    /// EXP-1324: glyphs take the shimmer gradient's colour at their place.
    pub shimmer: Option<ShimmerPaint>,
}

/// EXP-1324: the Text `shimmer` gradient at one instant (catalog/style.json
/// `textShimmer`). gpui cannot fill glyphs with a gradient, so each glyph
/// takes the gradient's colour at its centre: base @ ±spread, mid @
/// ±spread/2, highlight @ the gradient's centre, which slides from `-spread`
/// to `w + spread` (CSS background-position 100% → 0% of a `2w + 2·spread`
/// wide gradient) over `durationMs`; RTL slides the other way.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShimmerPaint {
    pub base: Hsla,
    pub mid: Hsla,
    pub highlight: Hsla,
    pub spread: f32,
    /// The CSS gradient angle (90deg + `angleDeg`).
    pub angle_deg: f32,
    /// 0 → 1 through one sweep.
    pub progress: f32,
    pub rtl: bool,
}

impl ShimmerPaint {
    /// The shimmer of `ink` at `progress` (0..1) of its sweep.
    pub fn new(ink: Hsla, dark: bool, progress: f32, rtl: bool) -> Self {
        let r = gpui::Rgba::from(ink);
        let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        let hex = format!("#{:02x}{:02x}{:02x}{:02x}", b(r.r), b(r.g), b(r.b), b(r.a));
        let s = eui_core::text_shimmer::text_shimmer(&hex, dark);
        let c = |h: &str| crate::paint::color::parse_hex(h).unwrap_or(ink);
        ShimmerPaint { base: c(&s.base), mid: c(&s.mid), highlight: c(&s.highlight), spread: s.spread_px as f32, angle_deg: 90.0 + s.angle_deg as f32, progress, rtl }
    }

    /// The sweep's progress `elapsed_ms` after it started (looping).
    pub fn progress_at(elapsed_ms: f64) -> f32 {
        let d = eui_core::text_shimmer::SHIMMER.duration_ms.max(1) as f64;
        (elapsed_ms.rem_euclid(d) / d) as f32
    }

    /// The colour at `(x, y)` in a `w × h` text box.
    pub fn color_at(&self, x: f32, y: f32, w: f32, h: f32) -> Hsla {
        let s = self.spread.max(1.0);
        let p = if self.rtl { 1.0 - self.progress } else { self.progress };
        let centre = -s + p * (w + 2.0 * s);
        // The point projected onto the gradient line (CSS angles: 90deg =
        // left → right, y grows downwards), about the box's centre.
        let a = self.angle_deg.to_radians();
        let u = w / 2.0 + (x - w / 2.0) * a.sin() - (y - h / 2.0) * a.cos();
        let d = (u - centre).abs();
        if d >= s {
            self.base
        } else if d >= s / 2.0 {
            crate::paint::color::mix(self.mid, self.base, (d - s / 2.0) / (s / 2.0))
        } else {
            crate::paint::color::mix(self.highlight, self.mid, d / (s / 2.0))
        }
    }
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
                    let runs = crate::fallback::cluster_runs(window.text_system(), line, &[run], &crate::fallback::default_chain());
                    let shaped = window.text_system().shape_line(SharedString::from(line.clone()), size, &runs, None);
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
                    let char_at = crate::measure::tracked_before(line);
                    let glyphs: Vec<(gpui::FontId, &gpui::ShapedGlyph)> = shaped.runs.iter().flat_map(|r| r.glyphs.iter().map(move |g| (r.font_id, g))).collect();
                    let mut end_x = x0;
                    let mut painted: Option<(f32, f32)> = None;
                    let mut inks = Vec::new();
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
                        let ink = match &t.shimmer {
                            Some(sh) => sh.color_at(x - f32::from(bounds.origin.x) + t.size * 0.3, f32::from(top - bounds.origin.y + lh / 2.0), f32::from(bounds.size.width), f32::from(bounds.size.height)),
                            None => t.ink,
                        };
                        inks.push(ink);
                        let _ = if g.is_emoji { window.paint_emoji(origin, *font_id, g.id, size) } else { window.paint_glyph(origin, *font_id, g.id, size, ink) };
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
                                inks: std::mem::take(&mut inks),
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
        Tracked { font: self.font.clone(), size: self.text_style.font_size, line_height: self.text_style.line_height, ink: self.ink, decoration: self.style.decoration, ls: self.style.letter_spacing, lines, align, ellipsize, shimmer: self.shimmer }
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
    // A shimmering text paints glyph by glyph too (its per-glyph colours).
    if cx.style.letter_spacing != 0.0 || cx.shimmer.is_some() {
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
    // EXP-1325: a loader may spin for a long while — stepped; #LOGS-4: a
    // glyph, so inside a host's cached region only the glyph repaints.
    crate::controls::stepped_glyph(id, gpui::size(px(size), px(size)), Animation::new(Duration::from_millis(800)).repeat(), SMOOTH_STEP, move |delta, _, _| {
        Icon::new(gpui_component::IconName::LoaderCircle)
            .size(px(size))
            .text_color(color)
            .transform(Transformation::rotate(gpui::percentage(delta)))
            .into_any_element()
    })
    .into_any_element()
}

/// Button / Toggle content: spinner or icon, then the label (mirrored in RTL).
///
/// EXP-1259 (feedback-states.json `pending`): a `loading` Button refuses
/// presses at once (the core) but LOOKS idle for `pendingDelayMs`, so a
/// fast action never flashes; then its icon slot is the spinner and its
/// label `pendingLabel`. Both faces are laid in the box the measurer sized
/// for the wider one; two one-shot animations swap them at the delay.
pub fn button(cx: &LeafCx, component: &str, window: &Window) -> AnyElement {
    let loading = cx.bool("loading");
    if !loading {
        return button_face(cx, component, window, false);
    }
    let delay = Duration::from_millis(eui_core::layout::PENDING_DELAY_MS as u64);
    let idle = div().absolute().size_full().child(button_face(cx, component, window, false)).with_animation(SharedString::from(format!("{}.idle", cx.node.id)), Animation::new(delay), |d, t| d.opacity(if t >= 1.0 { 0.0 } else { 1.0 }));
    let pending = div().absolute().size_full().child(button_face(cx, component, window, true)).with_animation(SharedString::from(format!("{}.pending", cx.node.id)), Animation::new(delay), |d, t| d.opacity(if t >= 1.0 { 1.0 } else { 0.0 }));
    div().absolute().size_full().child(idle).child(pending).into_any_element()
}

/// One face of a Button: idle (its icon + label) or pending (the spinner +
/// `pendingLabel`, else the label).
fn button_face(cx: &LeafCx, component: &str, window: &Window, loading: bool) -> AnyElement {
    let pending = cx.str("pendingLabel");
    let label = if loading && !pending.is_empty() { pending } else { cx.str("label") };
    let icon = cx.str("icon");
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

/// #EXP-1674: a `hostLinks` Text the host links part of, on one ellipsized
/// line: the links in the text's own colour, underlined. A press on a link
/// opens it through the host and never reaches a pressable Row behind it (the
/// rest of the text presses the Row as usual).
pub fn linked_text(cx: &LeafCx, segments: Vec<eui_core::host::TextLink>, host: Rc<dyn HostPlugin>) -> AnyElement {
    let pieces: Vec<(String, Option<String>)> = segments
        .into_iter()
        .map(|segment| (crate::text::transform(&segment.text, cx.style.text_transform.as_deref()).replace('\n', " "), segment.href))
        .collect();
    let joined: String = pieces.iter().map(|(text, _)| text.as_str()).collect();
    let text = crate::text::with_paragraph_direction(&joined, cx.rtl).into_owned();
    // The paragraph's direction mark, when it got one, belongs to the first run.
    let mut at = text.len() - joined.len();
    let base = TextRun { len: 0, font: cx.font.clone(), color: cx.ink, background_color: None, underline: None, strikethrough: None };
    let mut runs: Vec<TextRun> = Vec::new();
    let mut ranges = Vec::new();
    let mut hrefs = Vec::new();
    if at > 0 {
        runs.push(TextRun { len: at, ..base.clone() });
    }
    for (piece, href) in pieces {
        let len = piece.len();
        if len == 0 {
            continue;
        }
        let underline = href.as_ref().map(|_| gpui::UnderlineStyle { thickness: px(1.0), color: Some(cx.ink), wavy: false });
        runs.push(TextRun { len, underline, ..base.clone() });
        if let Some(href) = href {
            ranges.push(at..at + len);
            hrefs.push(href);
        }
        at += len;
    }
    let styled = gpui::StyledText::new(SharedString::from(text)).with_runs(runs);
    let layout = styled.layout().clone();
    let probe = ranges.clone();
    let links = gpui::InteractiveText::new(SharedString::from(format!("{}.links", cx.node.id)), styled).on_click(ranges, move |ix, _window, cx| {
        if let Some(href) = hrefs.get(ix) {
            host.open_url(href, cx);
        }
    });
    let (x, y, w, h) = cx.inner();
    aligned(cx.typed(div().absolute().left(px(x)).top(px(y)).w(px(w + 1.0)).h(px(h))), cx.align())
        .truncate()
        .on_mouse_down(gpui::MouseButton::Left, move |event: &gpui::MouseDownEvent, _window, cx| {
            if layout.index_for_position(event.position).is_ok_and(|ix| probe.iter().any(|range| range.contains(&ix))) {
                cx.stop_propagation();
            }
        })
        .child(links)
        .into_any_element()
}

/// A Link: underlined label (`letterSpacing` painted per glyph, as
/// measured).
/// A `Link`: underlined when the URL policy allows its href; a denied one
/// paints as plain text (the press opens nothing: `HostPlugin::open_url`).
pub fn link(cx: &LeafCx) -> AnyElement {
    let label = if cx.str("label").is_empty() { cx.str("href") } else { cx.str("label") };
    let shown = crate::text::transform(label, cx.style.text_transform.as_deref()).into_owned();
    let allowed = eui_core::host::safe_href(cx.host.url_policy().as_ref(), cx.str("href")).is_some();
    if !allowed {
        return cx.row(cx.content()).items_center().justify_center().whitespace_nowrap().child(SharedString::from(shown)).into_any_element();
    }
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
pub fn icon(cx: &LeafCx, app: &gpui::App) -> AnyElement {
    let (_, _, w, h) = cx.inner();
    let name = if cx.str("name").is_empty() { "ui-icon-placeholder" } else { cx.str("name") };
    // The glyph is the theme's Icon/root recipe size (`size: sm` →
    // `$control.iconSm` in the built-in themes; a remove button's box is 36,
    // its glyph 16), never larger than the box.
    let recipe = part_props(cx.theme, cx.mode, "Icon", "root", &cx.node.props, cx.states);
    // The parts whose glyph FILLS the part's box on the web (`>svg {width:
    // 100%}`): the FileUpload dropzone icon is `$control.iconLg`.
    let fills = matches!((cx.node.owner_component.as_deref(), cx.node.part.as_deref()), (Some("FileUpload"), Some("icon")));
    let size = if fills { w.min(h).max(1.0) } else { (px_prop(&recipe, "width").or_else(|| px_prop(&recipe, "height")).unwrap_or(f32::INFINITY) * cx.scale).min(w.min(h)).max(1.0) };
    if let Some(own) = cx.host.icon_element(name, size, cx.ink, app) {
        return cx.content().flex().items_center().justify_center().child(own).into_any_element();
    }
    let rotate = cx.style.transform.rotate;
    let glyph = if rotate.abs() > 0.01 { icons::concept_rotated(cx.host, name, size, cx.ink, rotate.to_radians()) } else { icons::concept_mirrored(cx.host, name, size, cx.ink, cx.rtl) };
    cx.content().flex().items_center().justify_center().child(glyph).into_any_element()
}

/// The Avatar's initials (`fixtures/avatar.json`: the IDE's `initials`).
pub fn initials(name: &str) -> String {
    eui_core::avatar::avatar_initials(name)
}

/// The seeded avatar's hue: `AVATAR_PALETTE[fnv1a32(seed) % 8]`.
pub fn seed_hue(seed: &str) -> Hsla {
    color_of(Some(eui_core::avatar::AVATAR_PALETTE[eui_core::avatar::avatar_hue_index(seed)])).unwrap_or_default()
}

/// The fallback's (fill, ink): a seed's hue at the fill alpha + the hue, no
/// seed = the `Avatar/fallback` recipe colours.
pub fn avatar_colors(seed: &str, recipe_bg: Option<Hsla>, recipe_ink: Hsla) -> (Option<Hsla>, Hsla) {
    if seed.is_empty() {
        return (recipe_bg, recipe_ink);
    }
    let hue = seed_hue(seed);
    (Some(hue.opacity(eui_core::avatar::AVATAR_FILL_ALPHA)), hue)
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

/// The bytes of a `data:image/<type>[;base64],…` URI (`None` = not an
/// image type gpui decodes). The media loader ([`crate::media`]) applies
/// the policy and the limits; nothing here loads a src by itself.
pub fn data_bytes(src: &str) -> Option<Vec<u8>> {
    let rest = src.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    let mime = meta.split(';').next().unwrap_or("").to_ascii_lowercase();
    if !matches!(mime.as_str(), "image/png" | "image/jpeg" | "image/jpg" | "image/webp" | "image/gif" | "image/svg+xml" | "image/bmp") {
        return None;
    }
    if meta.ends_with(";base64") {
        base64_decode(data)
    } else {
        Some(percent_decode(data))
    }
}

/// Standard base64 with padding (tests build data URIs with it).
pub fn base64_encode(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |acc, (i, b)| acc | (*b as u32) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() { T[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

pub(crate) fn percent_decode(s: &str) -> Vec<u8> {
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

/// `Avatar` (#EXP-1629, `fixtures/avatar.json`): the picture once it
/// decodes, wearing the `Avatar/image` ring; else the initials on the seed's
/// hue (no seed = the `Avatar/fallback` recipe colours), sized + weighted by
/// the fallback recipe.
pub fn avatar(cx: &LeafCx, window: &mut Window, app: &mut App) -> AnyElement {
    let fallback = cx.part("Avatar", "fallback", &[]);
    let fallback_props = cx.part_props("Avatar", "fallback", &[]);
    let (bg, fg) = avatar_colors(cx.str("seed"), fallback.bg, fallback.color.unwrap_or(cx.ink));
    let size = cx.w.min(cx.h);
    let fs = px_prop(&fallback_props, "fontSize").map_or((size / 2.0).round(), |f| f * cx.scale);
    let weight = fallback_props.get("fontWeight").and_then(Value::as_f64).unwrap_or(400.0) as f32;
    let out = div().size_full().rounded_full().overflow_hidden();
    let picture = crate::media::image_source(cx.host, cx.str("src")).and_then(|source| match render_image(&source, window, app) {
        Some(Ok(data)) => Some(data),
        _ => None,
    });
    if let Some(data) = picture {
        let ring = cx.part("Avatar", "image", &[]);
        let bw = ring.border.iter().copied().fold(0.0_f32, f32::max) * cx.scale;
        // The IDE's gpui-component Avatar: the border takes its width out of
        // the box and the picture sits inside it, so a translucent
        // `$color.border` reads as a hairline over the ground, not over the
        // picture's edge. #EXP-1629: the `image` fill sits under both, the
        // whole round box (its `secondary`: the ring blends over it).
        return out
            .when_some(ring.bg, |d, bg| d.bg(bg))
            .when(bw > 0.0, |d| d.border(px(bw)).border_color(ring.border_color.unwrap_or(gpui::transparent_black())))
            .child(img(ImageSource::Render(data)).size_full().rounded_full().object_fit(ObjectFit::Cover))
            .into_any_element();
    }
    out.flex()
        .items_center()
        .justify_center()
        .when_some(bg, |d, bg| d.bg(bg))
        .text_color(fg)
        .text_size(px(fs))
        .font_weight(gpui::FontWeight(weight))
        .child(SharedString::from(initials(cx.str("name"))))
        .into_any_element()
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
    /// EXP-1555: false under a Mosaic's "+N" scrim (nothing under the count).
    glyph: bool,
}

/// EXP-1555: what surrounds an Image's placeholder: an ancestor already
/// painting a background (`backdrop`), a sibling laid over it (`covered`,
/// the Mosaic's "+N").
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ImageSurround {
    pub backdrop: bool,
    pub covered: bool,
}

impl ImageFallback {
    pub fn of(cx: &LeafCx, around: ImageSurround) -> ImageFallback {
        let part = cx.part("Image", "fallback", &[]);
        let ink = part.color.or_else(|| cx.theme_color("mutedForeground")).unwrap_or(cx.ink);
        // EXP-1555: the muted fill only where nothing behind paints one (a
        // Mosaic tile, a card): never a second translucent layer.
        let bg = if around.backdrop { None } else { part.bg.or_else(|| cx.theme_color("muted")) };
        // A Carousel thumbnail's square shows it at `iconMd` (the web strip).
        let token = if cx.node.part.as_deref() == Some("thumbnailImage") { "iconMd" } else { "iconLg" };
        let size = cx.theme.and_then(|t| t.tokens.control.get(token).copied()).unwrap_or(if token == "iconMd" { 20.0 } else { 24.0 }) as f32;
        let name = match cx.str("fallback") {
            "" => eui_core::layout_tree::builtin_icon("Image.fallback").unwrap_or("ui-icon-placeholder").to_string(),
            f => f.to_string(),
        };
        let path = cx.host.icon(&name);
        ImageFallback { bg, ink, size, name, path, glyph: !around.covered }
    }

    pub fn element(&self) -> AnyElement {
        let glyph = self.glyph.then(|| match &self.path {
            Some(p) => gpui::svg().path(p.clone()).size(px(self.size)).flex_none().text_color(self.ink).into_any_element(),
            None => icons::glyph(Glyph::for_concept(&self.name).unwrap_or(Glyph::Image), self.size, self.ink),
        });
        div().size_full().flex().items_center().justify_center().when_some(self.bg, |d, bg| d.bg(bg)).children(glyph).into_any_element()
    }
}

pub fn image_placeholder(cx: &LeafCx, around: ImageSurround) -> AnyElement {
    ImageFallback::of(cx, around).element()
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
pub(crate) fn render_image(source: &ImageSource, window: &mut Window, cx: &mut App) -> Option<Result<Arc<RenderImage>, ()>> {
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
/// An Image's decode now against its last picture: a decoded one becomes
/// the held one, a failed one drops it, and while one loads the held one
/// is what to paint (`Some`).
pub(crate) fn hold(decoded: Option<Result<Arc<RenderImage>, ()>>, id: &gpui::SharedString, held: &mut std::collections::HashMap<gpui::SharedString, Arc<RenderImage>>) -> Option<Arc<RenderImage>> {
    match decoded {
        Some(Ok(data)) => {
            held.insert(id.clone(), data);
            None
        }
        Some(Err(())) => {
            held.remove(id);
            None
        }
        None => held.get(id).cloned(),
    }
}

/// `held` = the view's last decoded picture of this node (by painted id):
/// while a changed `src` loads, the previous picture stays.
pub fn image(cx: &LeafCx, around: ImageSurround, held: Option<(&gpui::SharedString, &std::cell::RefCell<std::collections::HashMap<gpui::SharedString, Arc<RenderImage>>>)>, window: &mut Window, app: &mut App) -> AnyElement {
    let src = cx.str("src");
    let Some(mut source) = crate::media::image_source(cx.host, src) else {
        return rounded(div().size_full().overflow_hidden(), cx.radii).child(image_placeholder(cx, around)).into_any_element();
    };
    if let Some((id, held)) = held {
        let decoded = render_image(&source, window, app);
        if let Some(previous) = hold(decoded, id, &mut held.borrow_mut()) {
            source = ImageSource::Render(previous);
        }
    }
    let fit_name = match cx.str("fit") {
        f @ ("contain" | "fill" | "none" | "scaleDown") => f,
        _ => "cover",
    };
    let focal = (cx.num("focalX").unwrap_or(0.5) as f32, cx.num("focalY").unwrap_or(0.5) as f32);
    // Every fit paints the decoded image at its focal bounds itself, the
    // leaf's rounded corners clipping it: gpui's `object_fit` always centres,
    // and its `img` takes the picture's aspect ratio into its own layout, so
    // a portrait picture in a wide box grew past the box and the box showed
    // only its top (#EDE-66: the taskbar previews' thumbnails).
    match render_image(&source, window, app) {
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
            _ => rounded(div().size_full().overflow_hidden(), cx.radii).child(image_placeholder(cx, around)).into_any_element(),
    }
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

/// A Video / AudioPlayer press (`media::play`: the policed src handed to
/// the system player); `None` = no src or a DENIED one: the control is
/// inert.
pub type MediaPlay = Option<Rc<dyn Fn(&mut App)>>;

/// The round play control: a pressable button with `play`, else inert.
fn play_control(id: String, size: f32, el: Div, play: MediaPlay) -> AnyElement {
    let el = el.size(px(size)).flex_none().rounded_full().flex().items_center().justify_center();
    match play {
        Some(play) => el.id(SharedString::from(id)).cursor_pointer().role(gpui::Role::Button).on_click(move |_, _, cx| play(cx)).into_any_element(),
        None => el.into_any_element(),
    }
}

/// `Video`: the poster (or a dark tint) with a play button and the
/// duration. gpui has no video pipeline: the press hands the policed src
/// to the system player (`media::play`); `autoplay` opens nothing.
pub fn video(cx: &LeafCx, play: MediaPlay) -> AnyElement {
    let poster = cx.str("poster");
    let mut out = rounded(div().size_full().relative().overflow_hidden().bg(gpui::black().opacity(0.85)), cx.radii);
    if let Some(source) = crate::media::image_source(cx.host, poster) {
        out = out.child(img(source).absolute().size_full().object_fit(ObjectFit::Cover).with_fallback(|| div().into_any_element()));
    }
    let white = gpui::white();
    let button = play_control(format!("{}.play", cx.node.id), 44.0, div().bg(white.opacity(0.18)).child(icons::glyph(Glyph::Play, 20.0, white)), play);
    out = out.child(div().absolute().size_full().flex().items_center().justify_center().child(button));
    if let Some(ms) = cx.num("durationMs") {
        let d = div().absolute().bottom(px(6.0)).px(px(6.0)).rounded(px(4.0)).bg(gpui::black().opacity(0.6)).text_color(white).text_size(px(12.0)).child(SharedString::from(fmt_duration(ms)));
        out = out.child(if cx.rtl { d.left(px(8.0)) } else { d.right(px(8.0)) });
    }
    out.into_any_element()
}

/// `AudioPlayer`: the title line over a controls bar. Like `Video`, the
/// play press hands the policed src to the system player (its controls
/// play, pause and seek); the bar stays `0:00 / durationMs`.
pub fn audio(cx: &LeafCx, play: MediaPlay) -> AnyElement {
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
            .child(play_control(format!("{}.play", cx.node.id), 28.0, div().bg(cx.ink).child(icons::glyph(Glyph::Play, 14.0, cx.theme_color("background").unwrap_or(gpui::white()))), play))
            .child(div().flex_1().h(px(4.0)).rounded_full().bg(muted_fg.opacity(0.35)))
            .child(div().text_size(px(12.0)).text_color(muted_fg).child(SharedString::from(format!("0:00 / {duration}")))),
    )
    .into_any_element()
}

/// #VAPP-258: the FileUpload `file` row's look for a Markdown file
/// attachment (`catalog/host.json` markdown): the row recipe's box (min
/// height, border, radius), `fileIcon` then `fileName` one line; no size, no
/// remove. A press opens `href` through the host (its URL policy and opener)
/// and never reaches what is behind.
pub fn file_row(cx: &LeafCx, name: &str, m: &crate::paint::markdown::AttachMetrics, href: Option<String>, host: Rc<dyn HostPlugin>) -> AnyElement {
    let look = cx.part("FileUpload", "file", &[]);
    let icon_color = cx.part("FileUpload", "fileIcon", &[]).color.unwrap_or(cx.ink);
    let name_color = cx.part("FileUpload", "fileName", &[]).color.unwrap_or(cx.ink);
    let icon = eui_core::layout_tree::builtin_icon("FileUpload.file").unwrap_or("ui-file");
    let label = div()
        .flex_1()
        .min_w_0()
        .truncate()
        .text_size(px(m.file_name.size))
        .line_height(px(m.file_name.line_height))
        .text_color(name_color)
        .child(SharedString::from(name.to_string()));
    let row = cx
        .row(styled_box(cx.typed(div()), &look, cx.w, cx.h).size_full())
        .items_center()
        .gap(px(m.file_gap))
        .px(px(m.file_pad))
        .overflow_hidden()
        .child(div().flex_none().child(icons::concept(cx.host, icon, m.file_icon, icon_color)))
        .child(label)
        .id(SharedString::from(cx.node.id.clone()));
    match href {
        Some(href) => row
            .cursor_pointer()
            .role(gpui::Role::Link)
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, _, cx| host.open_url(&href, cx))
            .into_any_element(),
        None => row.into_any_element(),
    }
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

/// One VoiceWave ring as painted (a test aid: [`record_voice_wave`]).
#[derive(Debug, Clone, PartialEq)]
pub struct PaintedRing {
    /// The node id.
    pub id: String,
    pub part: String,
    pub points: usize,
    pub phase: f64,
    pub fill: Hsla,
    pub stroke: Hsla,
}

thread_local! {
    static VOICE_RINGS: RefCell<Option<Vec<PaintedRing>>> = const { RefCell::new(None) };
}

/// Record every ring [`voice_wave`] paints on this thread (a test aid).
pub fn record_voice_wave(on: bool) {
    VOICE_RINGS.with(|t| *t.borrow_mut() = on.then(Vec::new));
    VOICE_BARS.with(|t| *t.borrow_mut() = on.then(Vec::new));
}

/// The rings painted since the last call (recording stays on).
pub fn take_voice_wave() -> Vec<PaintedRing> {
    VOICE_RINGS.with(|t| t.borrow_mut().as_mut().map(std::mem::take).unwrap_or_default())
}

/// The core's `activity` of a VoiceWave node's props.
pub fn voice_wave_activity(props: &Map<String, Value>) -> eui_core::voice_wave::Activity {
    eui_core::voice_wave::Activity::parse(props.get("activity").and_then(Value::as_str).unwrap_or(""))
}

/// One VoiceWave ring ready to paint: its closed path (px from the wave's
/// top-left), fill and stroke colours (alphas applied) and stroke width.
#[derive(Debug, Clone)]
pub struct RingPath {
    pub part: String,
    pub points: Vec<Point<Pixels>>,
    pub fill: Hsla,
    pub stroke: Hsla,
    pub width: f32,
}

/// The core's rings of one frame, each in `color(part)` at its alphas.
pub fn voice_wave_paths(wave: &eui_core::voice_wave::VoiceWave, color: impl Fn(&str) -> Hsla) -> Vec<RingPath> {
    wave.rings
        .iter()
        .map(|r| {
            let c = color(&r.part);
            RingPath {
                part: r.part.clone(),
                points: r.points.iter().map(|[x, y]| point(px(*x as f32), px(*y as f32))).collect(),
                fill: c.opacity(r.fill_alpha as f32),
                stroke: c.opacity(r.stroke_alpha as f32),
                width: r.stroke_width as f32,
            }
        })
        .collect()
}

/// VAPP-160: an invisible canvas laid over a moving native's box that asks
/// for the next frame from its PAINT, and only while that box is painted
/// inside the visible content (the window and every clipping ancestor): a
/// wave a scroller moved away, or one in a clipped-away container, stops
/// the surface's frame loop instead of redrawing it at the display's rate.
/// gpui notifies the whole surface view per frame request (its ancestors
/// are marked dirty either way), so the scope is the request itself: none
/// unless the wave is on screen.
pub fn frame_ticker() -> gpui::Canvas<()> {
    canvas(
        |_, _, _| {},
        |bounds, _, window, _| {
            if bounds.size.width > px(0.0) && bounds.size.height > px(0.0) && bounds.intersects(&window.content_mask().bounds) {
                window.request_animation_frame();
            }
        },
    )
}

/// The rings as a canvas (each filled, then stroked, outermost first),
/// drawn from the canvas's top-left; the caller sizes and places it.
pub fn voice_wave_canvas(rings: Vec<RingPath>) -> gpui::Canvas<()> {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let o = bounds.origin;
            for r in &rings {
                let Some((first, rest)) = r.points.split_first() else { continue };
                let mut f = PathBuilder::fill();
                f.move_to(o + *first);
                for p in rest {
                    f.line_to(o + *p);
                }
                f.close();
                if let Ok(path) = f.build() {
                    window.paint_path(path, r.fill);
                }
                let mut s = PathBuilder::stroke(px(r.width));
                s.move_to(o + *first);
                for p in rest {
                    s.line_to(o + *p);
                }
                s.close();
                if let Ok(path) = s.build() {
                    window.paint_path(path, r.stroke);
                }
            }
        },
    )
}

/// NEW-77 `VoiceWave` (catalog/style.json `voiceWave`): the core's three
/// rings for this frame (`eui_core::voice_wave`), each a closed path
/// filled with its part colour at `fillAlpha` and stroked at `strokeAlpha`
/// / `strokeWidth`. `elapsed` = seconds since the node started animating.
/// The root opacity is the node's own box; the child is placed by the view.
/// A box that is not square paints the wave of its shorter edge, centred.
#[allow(clippy::too_many_arguments)]
pub fn voice_wave(id: &str, props: &Map<String, Value>, theme: Option<&ResolvedTheme>, mode: Mode, w: f32, h: f32, elapsed: f64, reduced_motion: bool, ink: Hsla) -> AnyElement {
    // A non-square box: the square wave of its shorter edge, centred.
    let size = w.min(h).max(0.0);
    use eui_core::voice_wave::{voice_wave as rings_of, VoiceWaveInput};
    let num = |v: &Value| match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    };
    let input = VoiceWaveInput {
        size: size as f64,
        activity: voice_wave_activity(props),
        level: props.get("level").and_then(num),
        bands: props.get("bands").and_then(Value::as_array).map(|b| b.iter().map(|v| num(v).unwrap_or(0.0)).collect()),
        elapsed: Some(elapsed),
        reduced_motion: Some(reduced_motion),
        child: None,
    };
    let wave = rings_of(&input);
    let rings = voice_wave_paths(&wave, |part| color_of(part_props(theme, mode, "VoiceWave", part, props, &[]).get("color").and_then(Value::as_str)).unwrap_or(ink));
    VOICE_RINGS.with(|t| {
        if let Some(v) = t.borrow_mut().as_mut() {
            v.extend(rings.iter().map(|r| PaintedRing { id: id.to_string(), part: r.part.clone(), points: r.points.len(), phase: wave.phase, fill: r.fill, stroke: r.stroke }));
        }
    });
    voice_wave_canvas(rings).absolute().left(px((w - size) / 2.0)).top(px((h - size) / 2.0)).size(px(size)).into_any_element()
}

/// One VoiceWave `bars` meter as painted (a test aid: [`record_voice_wave`]).
#[derive(Debug, Clone, PartialEq)]
pub struct PaintedBars {
    pub id: String,
    pub phase: f64,
    /// (x, y, width, height, radius) px from the meter's top-left.
    pub bars: Vec<(f32, f32, f32, f32, f32)>,
    pub fill: Hsla,
}

thread_local! {
    static VOICE_BARS: RefCell<Option<Vec<PaintedBars>>> = const { RefCell::new(None) };
}

/// The bars meters painted since the last call (with [`record_voice_wave`] on).
pub fn take_voice_wave_bars() -> Vec<PaintedBars> {
    VOICE_BARS.with(|t| t.borrow_mut().as_mut().map(std::mem::take).unwrap_or_default())
}

/// Whether a VoiceWave node is the `bars` level meter.
pub fn voice_wave_is_bars(props: &Map<String, Value>) -> bool {
    props.get("variant").and_then(Value::as_str) == Some("bars")
}

/// VAPP-162 `VoiceWave variant: bars` (style.json `voiceWave.bars`): the
/// core's `count` rounded vertical bars for this frame
/// (`eui_core::voice_wave::voice_wave_bars`), each filled with the
/// `bar` part colour; the meter centred in the node's box (the root recipe
/// gives it the size class's exact box). No child is painted.
#[allow(clippy::too_many_arguments)]
pub fn voice_wave_bars(id: &str, props: &Map<String, Value>, theme: Option<&ResolvedTheme>, mode: Mode, w: f32, h: f32, elapsed: f64, reduced_motion: bool, ink: Hsla) -> AnyElement {
    use eui_core::voice_wave::{voice_wave_bars as bars_of, VoiceWaveBarsInput};
    let num = |v: &Value| match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    };
    let input = VoiceWaveBarsInput {
        size: props.get("size").and_then(Value::as_str).unwrap_or("md").to_string(),
        activity: voice_wave_activity(props),
        level: props.get("level").and_then(num),
        bands: props.get("bands").and_then(Value::as_array).map(|b| b.iter().map(|v| num(v).unwrap_or(0.0)).collect()),
        elapsed: Some(elapsed),
        reduced_motion: Some(reduced_motion),
    };
    let meter = bars_of(&input);
    let fill = color_of(part_props(theme, mode, "VoiceWave", "bar", props, &[]).get("color").and_then(Value::as_str)).unwrap_or(ink);
    let rects: Vec<(f32, f32, f32, f32, f32)> = meter.bars.iter().map(|b| (b.x as f32, b.y as f32, b.width as f32, b.height as f32, b.radius as f32)).collect();
    VOICE_BARS.with(|t| {
        if let Some(v) = t.borrow_mut().as_mut() {
            v.push(PaintedBars { id: id.to_string(), phase: meter.phase, bars: rects.clone(), fill });
        }
    });
    let (mw, mh) = (meter.width as f32, meter.height as f32);
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let o = bounds.origin;
            for (x, y, bw, bh, r) in &rects {
                window.paint_quad(gpui::fill(Bounds { origin: o + point(px(*x), px(*y)), size: gpui::Size { width: px(*bw), height: px(*bh) } }, fill).corner_radii(px(r.min(bh / 2.0))));
            }
        },
    )
    .absolute()
    .left(px((w - mw) / 2.0))
    .top(px((h - mh) / 2.0))
    .w(px(mw))
    .h(px(mh))
    .into_any_element()
}

/// `TreeGuides` (round 3,`layout.json` `treeGuideColumn` / `treeGuideRadius`
/// / `treeGuideBridge`): 14 px columns, the 1 px line of column i at
/// x = i·14 + 7. The elbow = a vertical from the row's top − bridge into a
/// 3 px ROUNDED corner at the row's centre, then a stub to the column's right
/// edge; `tee` carries the vertical on to the bottom; `passThrough` columns =
/// full-height verticals from top − bridge. The bridge overshoots the row's
/// top (paint only) so the line carries across a Section divider. Mirrored in
/// RTL. Stroke width / colour = the `TreeGuides/line` recipe.
pub fn tree_guides(cx: &LeafCx) -> AnyElement {
    let depth = cx.num("depth").unwrap_or(0.0).max(0.0) as usize;
    // The core fills `elbowAt`; a row it never saw (a lone root, a template item) draws no elbow.
    let elbow_at = cx.num("elbowAt").map(|v| v as i64).unwrap_or(-1);
    let tee = cx.bool("tee");
    let pass: Vec<i64> = cx.node.props.get("passThrough").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).map(|v| v as i64).collect()).unwrap_or_default();
    let line = cx.part_props("TreeGuides", "line", &[]);
    let color = color_of(line.get("color").and_then(Value::as_str)).unwrap_or(cx.ink.opacity(0.25));
    let lw = px_prop(&line, "width").unwrap_or(1.0);
    let rtl = cx.rtl;
    let col = eui_core::layout::TREE_GUIDE_COLUMN as f32;
    let radius = eui_core::layout::TREE_GUIDE_RADIUS as f32;
    let bridge = eui_core::layout::TREE_GUIDE_BRIDGE as f32;
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window, _| {
            let (left, width) = (f32::from(bounds.origin.x), f32::from(bounds.size.width));
            let (y0, h) = (f32::from(bounds.origin.y), f32::from(bounds.size.height));
            // The physical x of a span starting at gutter offset `x`.
            let at = |x: f32, span: f32| if rtl { left + width - x - span } else { left + x };
            let (top, bottom, mid) = (y0 - bridge, y0 + h, y0 + h / 2.0);
            let vertical = |window: &mut Window, x: f32, from: f32, to: f32| {
                if to > from {
                    window.paint_quad(gpui::fill(Bounds::new(point(px(at(x, lw)), px(from)), gpui::size(px(lw), px(to - from))), color));
                }
            };
            for i in 0..depth {
                let x = i as f32 * col + col / 2.0;
                if pass.contains(&(i as i64)) {
                    vertical(window, x, top, bottom);
                }
                if i as i64 == elbow_at {
                    vertical(window, x, top, if tee { bottom } else { mid - radius });
                    // The corner and the stub: ONE stroked path, so the
                    // quarter-turn is round rather than mitred.
                    let cxl = at(x, lw) + lw / 2.0;
                    let dir = if rtl { -1.0 } else { 1.0 };
                    let stub_end = at((i + 1) as f32 * col, 0.0);
                    let mut path = PathBuilder::stroke(px(lw));
                    path.move_to(point(px(cxl), px(mid - radius)));
                    path.curve_to(point(px(cxl + dir * radius), px(mid)), point(px(cxl), px(mid)));
                    path.line_to(point(px(stub_end), px(mid)));
                    if let Ok(path) = path.build() {
                        window.paint_path(path, color);
                    }
                }
            }
        },
    )
    .size_full()
    .into_any_element()
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

/// A Checkbox `box` (or a Table `checkbox`): the check glyph when checked;
/// round 8, a MIXED box (`indeterminate`, whatever `checked` says) the
/// built-in `Checkbox.check.indeterminate` dash (`ui-minus`) on the checked
/// look (the recipe's `indeterminate` when-prop paints the fill).
pub fn check_box(cx: &LeafCx, checked: bool, mixed: bool) -> AnyElement {
    if !checked && !mixed {
        return div().into_any_element();
    }
    let check = cx.part_props("Checkbox", "check", &[]);
    let size = px_prop(&check, "width").unwrap_or(12.0);
    let color = color_of(check.get("color").and_then(Value::as_str)).or(cx.style.color).unwrap_or(cx.ink);
    let glyph = if mixed { eui_core::layout_tree::builtin_icon("Checkbox.check.indeterminate").unwrap_or("ui-minus") } else { "ui-check" };
    div().size_full().flex().items_center().justify_center().child(icons::concept(cx.host, glyph, size, color)).into_any_element()
}

/// A markdown task item's box (`- [ ]` / `- [x]`): THE Checkbox `box` of
/// the theme, 16px, its check glyph when checked — the same square a
/// Checkbox component paints, so a checklist never draws a second look.
pub fn task_box_painter(cx: &LeafCx, host: Rc<dyn HostPlugin>) -> crate::paint::markdown::TaskBox {
    let style = |checked: bool| {
        let mut props = Map::new();
        props.insert("checked".into(), Value::Bool(checked));
        let states: Vec<String> = if checked { vec!["checked".into()] } else { Vec::new() };
        let p = part_props(cx.theme, cx.mode, "Checkbox", "box", &props, &states);
        PaintStyle::from_visual(&eui_core::style::visual(&p, eui_core::style::BoxKind::Leaf))
    };
    let (off, on) = (style(false), style(true));
    let check = cx.part_props("Checkbox", "check", &[]);
    let check_size = px_prop(&check, "width").unwrap_or(12.0);
    let check_color = color_of(check.get("color").and_then(Value::as_str)).or(on.color).unwrap_or(cx.ink);
    Rc::new(move |checked| {
        let s = if checked { &on } else { &off };
        styled_box(div().flex_shrink_0().size(px(TASK_BOX_PX)).flex().items_center().justify_center(), s, TASK_BOX_PX, TASK_BOX_PX)
            .when(checked, |d| d.child(icons::concept(host.as_ref(), "ui-check", check_size, check_color)))
            .into_any_element()
    })
}

/// The task box's side: the Checkbox `box` (`measure.rs`, web `size-4`).
pub const TASK_BOX_PX: f32 = 16.0;

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
    let thumb = PaintStyle::from_visual(&eui_core::style::visual(&tp, eui_core::style::BoxKind::Leaf));
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

/// #EXP-1629: a Select trigger from its parts: the `value` (else the
/// `placeholder`) and the `chevron`, sized, faded and spaced by their
/// recipes; a `row` trigger leads with its `label` and ends the value.
pub fn select_trigger(cx: &LeafCx, text: &str, is_placeholder: bool, icon: &str) -> AnyElement {
    let row = cx.owner_props.get("triggerVariant").and_then(Value::as_str) == Some("row");
    let value = cx.part("Select", "value", &[]);
    let ph = cx.part("Select", "placeholder", &[]);
    let color = if is_placeholder { ph.color.or_else(|| cx.theme_color("mutedForeground")).unwrap_or(cx.ink.opacity(0.6)) } else { value.color.unwrap_or(cx.ink) };
    let fade = if is_placeholder { ph.opacity } else { value.opacity };
    let chevron = cx.part_props("Select", "chevron", &[]);
    let size = px_prop(&chevron, "width").or_else(|| px_prop(&chevron, "height")).unwrap_or(16.0) * cx.scale;
    // The field's glyph keeps its 0.6 when no recipe fades it.
    let glyph_fade = chevron.get("opacity").and_then(Value::as_f64).map(|o| o as f32).unwrap_or(if row { 1.0 } else { 0.6 });
    let icon = if icon.is_empty() { "ui-chevron-down" } else { icon };
    let gap = if cx.style.gap > 0.0 { cx.style.gap } else { spacing(cx.theme, "sm") * cx.scale };
    let shown = crate::text::transform(text, cx.style.text_transform.as_deref()).into_owned();
    let end = if row { resolve_align(Some("end"), cx.rtl) } else { cx.align() };
    let text_el = aligned(div().min_w_0().flex_1().truncate(), end).text_color(color).when_some(fade, |d, o| d.opacity(o)).child(SharedString::from(shown));
    let mut out = cx.row(cx.content()).items_center().gap(px(gap));
    if row {
        let label = cx.str("label");
        if !label.is_empty() {
            let lp = cx.part_props("Select", "label", &[]);
            let ls = cx.part("Select", "label", &[]);
            let mut font = cx.font.clone();
            if let Some(w) = lp.get("fontWeight").and_then(Value::as_f64) {
                font.weight = gpui::FontWeight(w as f32);
            }
            let size_px = px_prop(&lp, "fontSize").map(|f| f * cx.scale).unwrap_or(cx.text_style.font_size);
            let lh = px_prop(&lp, "lineHeight").map(|f| f * cx.scale).unwrap_or(cx.text_style.line_height);
            out = out.child(div().flex_none().whitespace_nowrap().font(font).text_size(px(size_px)).line_height(px(lh)).text_color(ls.color.unwrap_or(cx.ink)).when_some(ls.opacity, |d, o| d.opacity(o)).child(SharedString::from(label.to_string())));
        }
    }
    out.child(text_el).child(div().flex_none().opacity(glyph_fade).child(icons::concept(cx.host, icon, size, cx.ink))).into_any_element()
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
    let track = PaintStyle::from_visual(&eui_core::style::visual(&track_props, eui_core::style::BoxKind::Leaf));
    let range = cx.part("Slider", "range", &[]);
    let thumb_states: Vec<String> = cx.states.iter().filter(|s| matches!(s.as_str(), "focus" | "focus-visible" | "hover" | "pressed")).cloned().collect();
    let thumb_props = cx.part_props("Slider", "thumb", &thumb_states);
    let thumb = PaintStyle::from_visual(&eui_core::style::visual(&thumb_props, eui_core::style::BoxKind::Leaf));
    // EXP-1555: the thumb keeps the recipe's size (`$control.slider`), centred
    // on the track and overflowing a thinner leaf (the web's Radix thumb),
    // never squeezed to the bar's height.
    let t = px_prop(&thumb_props, "width").unwrap_or_else(|| control(cx.theme, "slider", 16.0));
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

/// `Skeleton` (and a Skeleton row's `dot` / `bar`): the fill (the box paints
/// none of its own) pulses ONE slow opacity wave, `skeletonOpacityLow` →
/// `High` → `Low` over `skeletonPulseMs`, ease-in-out (EXP-1259,
/// feedback-states.json `placeholder`); static at `skeletonReducedOpacity`
/// under reduced motion.
pub fn skeleton(cx: &LeafCx) -> AnyElement {
    use eui_core::layout::{SKELETON_OPACITY_HIGH, SKELETON_OPACITY_LOW, SKELETON_PULSE_MS, SKELETON_REDUCED_OPACITY};
    let bg = cx.style.bg.unwrap_or(cx.ink.opacity(0.1));
    let block = rounded(div().absolute().size_full().bg(bg), cx.radii);
    if cx.reduced_motion {
        return block.opacity(SKELETON_REDUCED_OPACITY as f32).into_any_element();
    }
    let (low, high) = (SKELETON_OPACITY_LOW as f32, SKELETON_OPACITY_HIGH as f32);
    block
        .with_stepped_animation(SharedString::from(format!("{}.pulse", cx.node.id)), Animation::new(Duration::from_millis(SKELETON_PULSE_MS as u64)).repeat(), SMOOTH_STEP, move |d, t| {
            let phase = if t < 0.5 { t * 2.0 } else { (1.0 - t) * 2.0 };
            d.opacity(low + (high - low) * super::cubic_bezier([0.42, 0.0, 0.58, 1.0], phase))
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

    /// `fixtures/avatar.json` (#EXP-1629): palette, hash, initials, fill.
    #[test]
    fn avatar_replays_the_fixture() {
        let f: Value = serde_json::from_str(include_str!("../../../../../../packages/exponential-ui/fixtures/avatar.json")).expect("avatar.json");
        let palette: Vec<&str> = f["palette"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(palette, eui_core::avatar::AVATAR_PALETTE);
        assert_eq!(f["fillAlpha"].as_f64().unwrap() as f32, eui_core::avatar::AVATAR_FILL_ALPHA);
        for case in f["hues"].as_array().unwrap() {
            let (seed, idx) = (case[0].as_str().unwrap(), case[1].as_u64().unwrap() as usize);
            assert_eq!(eui_core::avatar::avatar_hue_index(seed), idx, "hue of {seed:?}");
            if !seed.is_empty() {
                let hue = color_of(Some(palette[idx])).unwrap();
                assert_eq!(avatar_colors(seed, None, gpui::black()), (Some(hue.opacity(0.2)), hue), "colours of {seed:?}");
            }
        }
        for case in f["initials"].as_array().unwrap() {
            assert_eq!(initials(case[0].as_str().unwrap()), case[1].as_str().unwrap(), "initials of {:?}", case[0]);
        }
        // No seed = the recipe's muted colours.
        let muted = color_of(Some("#27272a"));
        assert_eq!(avatar_colors("", muted, gpui::white()), (muted, gpui::white()));
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
        // #EDE-66: a centred portrait thumbnail (68×110) in the taskbar's
        // 192×110 box stays inside it, the whole window visible.
        assert_eq!(focal_bounds("contain", (0.0, 0.0, 192.0, 110.0), (68.0, 110.0), (0.5, 0.5)), (62.0, 0.0, 68.0, 110.0));
    }

    /// #EDE-65: a changed `src` keeps the last picture until its own lands.
    #[test]
    fn an_image_holds_its_last_picture_while_the_next_loads() {
        let picture = |shade: u8| Arc::new(RenderImage::new([image::Frame::new(image::RgbaImage::from_pixel(2, 2, image::Rgba([shade, 0, 0, 255])))]));
        let (id, mut held) = (gpui::SharedString::from("thumb"), std::collections::HashMap::new());
        assert!(hold(None, &id, &mut held).is_none(), "nothing yet: the placeholder");
        let first = picture(1);
        assert!(hold(Some(Ok(first.clone())), &id, &mut held).is_none(), "decoded: paint it");
        assert!(Arc::ptr_eq(&hold(None, &id, &mut held).unwrap(), &first), "the next one loading: the first stays");
        let second = picture(2);
        assert!(hold(Some(Ok(second.clone())), &id, &mut held).is_none());
        assert!(Arc::ptr_eq(&hold(None, &id, &mut held).unwrap(), &second));
        assert!(hold(Some(Err(())), &id, &mut held).is_none() && held.is_empty(), "a failed one drops it");
    }

    #[test]
    fn data_uris_decode() {
        assert_eq!(base64_decode("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64_decode("aGVsbG8").unwrap(), b"hello");
        assert!(base64_decode("@@").is_none());
        assert_eq!(data_bytes("data:image/png;base64,aGVsbG8=").unwrap(), b"hello");
        assert!(data_bytes("data:text/plain;base64,aGk=").is_none());
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
        assert_eq!(base64_decode(&base64_encode(&[0, 255, 7, 9])).unwrap(), [0, 255, 7, 9]);
    }
}
