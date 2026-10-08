//! Content painters of the measured leaves that need no interaction state of
//! the view (the interactive ones — fields, sliders, toggle groups, carousel
//! dots, the composer — live in `view::leaves`). Each returns the element
//! drawn INSIDE the leaf's frame div.

use std::f32::consts::PI;
use std::time::Duration;

use exponential_ui::measure::TextStyle;
use exponential_ui::surface::PlacedNode;
use exponential_ui::theme::{Mode, ResolvedTheme};
use gpui::{
    canvas, div, img, point, prelude::*, px, Animation, AnimationExt as _, AnyElement, Div, Font, Hsla, ObjectFit, PathBuilder, Pixels, Point, SharedString,
    Transformation,
};
use gpui_component::Icon;
use serde_json::{Map, Value};

use super::color::{color_of, hsl};
use super::icons::{self, Glyph};
use super::parts::{control, part_props, part_visual, px_prop, spacing, theme_color};
use super::PaintStyle;
use crate::host::HostPlugin;
use crate::measure::{chart_legend, display_text, unknown_label};

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

    /// The content box inside padding + border: (x, y, w, h).
    pub fn inner(&self) -> (f32, f32, f32, f32) {
        let (ix, iy) = self.style.inset();
        (ix, iy, (self.w - 2.0 * ix).max(0.0), (self.h - 2.0 * iy).max(0.0))
    }

    /// An absolutely placed div over the content box with the leaf's text style.
    pub fn content(&self) -> Div {
        let (x, y, w, h) = self.inner();
        self.typed(div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h)))
    }

    pub fn typed(&self, d: Div) -> Div {
        d.font(self.font.clone()).text_size(px(self.text_style.font_size)).line_height(px(self.text_style.line_height)).text_color(self.ink)
    }

    pub fn owner_component(&self) -> &str {
        self.node.owner_component.as_deref().unwrap_or(&self.node.component)
    }

    pub fn part(&self, component: &str, part: &str, states: &[String]) -> super::PaintStyle {
        PaintStyle::from_visual(&part_visual(self.theme, self.mode, component, part, self.owner_props, states))
    }

    pub fn part_props(&self, component: &str, part: &str, states: &[String]) -> Map<String, Value> {
        part_props(self.theme, self.mode, component, part, self.owner_props, states)
    }

    pub fn theme_color(&self, name: &str) -> Option<Hsla> {
        theme_color(self.theme, self.mode, name)
    }
}

/// A box painted from a part style (bg, border, radius, shadow, opacity).
pub fn styled_box<E: Styled + gpui::prelude::FluentBuilder>(d: E, s: &PaintStyle, w: f32, h: f32) -> E {
    let r = s.radius.min(w.min(h) / 2.0);
    d.when_some(s.bg, |d, bg| d.bg(bg))
        .when(s.border_width > 0.0, |d| d.border(px(s.border_width)).border_color(s.border_color.unwrap_or(gpui::transparent_black())))
        .rounded(px(r))
        .when(!s.shadows.is_empty(), |d| d.shadow(s.shadows.clone()))
        .when_some(s.opacity, |d, o| d.opacity(o))
}

fn align(d: Div, a: Option<&str>) -> Div {
    match a {
        Some("center") => d.text_center(),
        Some("right") | Some("end") => d.text_right(),
        _ => d,
    }
}

/// Plain text (`Text` and the text parts).
pub fn text(cx: &LeafCx, text: &str, lines: Option<u32>) -> AnyElement {
    let a = cx.style.text_align.as_deref().or_else(|| cx.node.props.get("align").and_then(Value::as_str));
    let d = align(cx.content(), a);
    let d = match lines {
        Some(1) => d.truncate(),
        Some(n) if n > 1 => d.line_clamp(n as usize).text_ellipsis().overflow_hidden(),
        _ => d,
    };
    d.child(SharedString::from(text.to_string())).into_any_element()
}

/// A Tabs `tab`: icon + label + count, centred; the indicator under it.
pub fn tab(cx: &LeafCx, selected: bool) -> AnyElement {
    let mut row = cx.content().flex().flex_row().items_center().justify_center().gap(px(4.0)).whitespace_nowrap();
    if let Some(icon) = cx.node.props.get("icon").and_then(Value::as_str) {
        row = row.child(icons::concept(cx.host, icon, 16.0, cx.ink));
    }
    row = row.child(div().min_w_0().truncate().child(SharedString::from(cx.str("text").to_string())));
    if let Some(count) = cx.node.props.get("count") {
        let muted = cx.theme_color("mutedForeground").unwrap_or(cx.ink);
        row = row.child(div().px(px(6.0)).rounded_full().text_size(px(12.0)).text_color(muted).child(SharedString::from(display_text(Some(count)))));
    }
    let states: Vec<String> = if selected { vec!["selected".into()] } else { vec![] };
    let ind_props = cx.part_props("Tabs", "indicator", &states);
    let ind = PaintStyle::from_visual(&exponential_ui::style::visual(&ind_props, exponential_ui::style::BoxKind::Leaf));
    let ind_h = px_prop(&ind_props, "height").unwrap_or(0.0);
    let mut out = div().size_full().child(row);
    if selected && ind_h > 0.0 {
        if let Some(bg) = ind.bg {
            out = out.child(div().absolute().left_0().bottom_0().w_full().h(px(ind_h)).bg(bg).rounded(px(ind.radius)));
        }
    }
    out.into_any_element()
}

/// An Accordion `trigger`: title (+ count) and a chevron that turns open.
pub fn accordion_trigger(cx: &LeafCx, open: bool) -> AnyElement {
    let mut title = cx.str("text").to_string();
    if let Some(count) = cx.node.props.get("count") {
        title.push_str(&format!(" · {}", display_text(Some(count))));
    }
    let chevron = Icon::new(gpui_component::IconName::ChevronDown).size(px(16.0)).text_color(cx.ink).when(open, |i| i.rotate(gpui::Radians(PI)));
    cx.content()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(cx.style.gap.max(8.0)))
        .child(div().min_w_0().truncate().child(SharedString::from(title)))
        .child(chevron)
        .into_any_element()
}

/// A DropdownMenu `item`: icon + label (destructive tinted).
pub fn menu_item(cx: &LeafCx) -> AnyElement {
    let ink = if cx.bool("destructive") { cx.theme_color("destructive").unwrap_or(cx.ink) } else { cx.ink };
    let mut row = cx.content().text_color(ink).flex().flex_row().items_center().gap(px(cx.style.gap.max(8.0))).whitespace_nowrap();
    if let Some(icon) = cx.node.props.get("icon").and_then(Value::as_str) {
        row = row.child(icons::concept(cx.host, icon, 16.0, ink));
    }
    row.child(div().min_w_0().truncate().child(SharedString::from(cx.str("text").to_string()))).into_any_element()
}

/// The rotating loader glyph.
pub fn spinner_glyph(id: impl Into<gpui::ElementId>, size: f32, color: Hsla) -> AnyElement {
    Icon::new(gpui_component::IconName::LoaderCircle)
        .size(px(size))
        .text_color(color)
        .with_animation(id, Animation::new(Duration::from_millis(800)).repeat(), |icon, delta| icon.transform(Transformation::rotate(gpui::percentage(delta))))
        .into_any_element()
}

/// Button / Toggle content: spinner or icon, then the label.
pub fn button(cx: &LeafCx, component: &str) -> AnyElement {
    let label = cx.str("label");
    let icon = cx.str("icon");
    let loading = cx.bool("loading");
    let icon_only = cx.str("size") == "icon";
    let icon_size = px_prop(&cx.part_props(component, "icon", &[]), "width").unwrap_or_else(|| control(cx.theme, "iconSm", 16.0));
    let mut row = cx.content().flex().flex_row().items_center().justify_center().gap(px(cx.style.gap)).whitespace_nowrap();
    if loading {
        row = row.child(spinner_glyph(SharedString::from(format!("{}.spinner", cx.node.id)), icon_size, cx.ink));
    } else if !icon.is_empty() {
        row = row.child(icons::concept(cx.host, icon, icon_size, cx.ink));
    } else if icon_only {
        row = row.child(icons::placeholder(icon_size, cx.ink));
    }
    if !icon_only && !label.is_empty() {
        row = row.child(SharedString::from(label.to_string()));
    }
    row.into_any_element()
}

/// A Link: underlined label.
pub fn link(cx: &LeafCx) -> AnyElement {
    let label = if cx.str("label").is_empty() { cx.str("href") } else { cx.str("label") };
    // Like the web's `inline-flex; justify-content: center` Link: centred
    // when the leaf is wider than its label (a stretched column item).
    cx.content().flex().flex_row().items_center().justify_center().whitespace_nowrap().underline().text_decoration_color(cx.ink).child(SharedString::from(label.to_string())).into_any_element()
}

/// An `Icon` leaf.
pub fn icon(cx: &LeafCx) -> AnyElement {
    let (_, _, w, h) = cx.inner();
    let name = if cx.str("name").is_empty() { "ui-icon-placeholder" } else { cx.str("name") };
    cx.content().flex().items_center().justify_center().child(icons::concept(cx.host, name, w.min(h).max(1.0), cx.ink)).into_any_element()
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

fn is_remote(src: &str) -> bool {
    src.starts_with("http://") || src.starts_with("https://") || src.starts_with("file://") || src.starts_with('/')
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
    if !src.is_empty() && is_remote(src) {
        let url = cx.host.resolve_url(src);
        let fb = initials_el.clone();
        out = out.child(img(SharedString::from(url)).size_full().object_fit(ObjectFit::Cover).with_fallback(fb).with_loading(initials_el));
    } else {
        out = out.child(initials_el());
    }
    out.into_any_element()
}

/// The tinted placeholder an `Image` paints without a loadable source.
pub fn image_placeholder(ink: Hsla, label: &str) -> AnyElement {
    let label = SharedString::from(label.to_string());
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(4.0))
        .bg(ink.opacity(0.08))
        .text_color(ink.opacity(0.6))
        .text_size(px(12.0))
        .child(icons::glyph(Glyph::Image, 20.0, ink.opacity(0.6)))
        .child(label)
        .into_any_element()
}

/// `Image`: `img()` for http(s)/file sources (placeholder while loading and
/// on failure), else the placeholder.
pub fn image(cx: &LeafCx) -> AnyElement {
    let src = cx.str("src");
    let alt = if cx.str("alt").is_empty() { "image".to_string() } else { cx.str("alt").to_string() };
    let muted = cx.theme_color("mutedForeground").unwrap_or(cx.ink);
    if src.is_empty() || !is_remote(src) {
        return image_placeholder(muted, &alt);
    }
    let fit = match cx.str("fit") {
        "contain" => ObjectFit::Contain,
        "fill" => ObjectFit::Fill,
        "none" => ObjectFit::None,
        "scaleDown" => ObjectFit::ScaleDown,
        _ => ObjectFit::Cover,
    };
    let url = cx.host.resolve_url(src);
    let (a, b) = (alt.clone(), alt);
    div()
        .size_full()
        .overflow_hidden()
        .child(img(SharedString::from(url)).size_full().object_fit(fit).with_fallback(move || image_placeholder(muted, &a)).with_loading(move || image_placeholder(muted, &b)))
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
    let mut out = div().size_full().relative().overflow_hidden().bg(gpui::black().opacity(0.85));
    if !poster.is_empty() && is_remote(poster) {
        out = out.child(img(SharedString::from(cx.host.resolve_url(poster))).absolute().size_full().object_fit(ObjectFit::Cover).with_fallback(|| div().into_any_element()));
    }
    let white = gpui::white();
    out = out.child(div().absolute().size_full().flex().items_center().justify_center().child(div().size(px(44.0)).rounded_full().bg(white.opacity(0.18)).flex().items_center().justify_center().child(icons::glyph(Glyph::Play, 20.0, white))));
    if let Some(ms) = cx.num("durationMs") {
        out = out.child(div().absolute().right(px(8.0)).bottom(px(6.0)).px(px(6.0)).rounded(px(4.0)).bg(gpui::black().opacity(0.6)).text_color(white).text_size(px(12.0)).child(SharedString::from(fmt_duration(ms))));
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
        col = col.child(div().truncate().text_color(track.color.unwrap_or(cx.ink)).child(SharedString::from(title.to_string())));
    }
    col.child(
        div()
            .h(px(40.0))
            .w_full()
            .rounded(px(20.0))
            .bg(muted)
            .flex()
            .flex_row()
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

fn stroke_arc(window: &mut gpui::Window, center: Point<Pixels>, r: f32, from: f32, sweep: f32, width: f32, color: Hsla) {
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

/// `Ring`: track circle + value arc from 12 o'clock, label centred.
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
    let mut out = div().size_full().relative().child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let c = bounds.center();
                let r = size / 2.0 - stroke.max(fill_stroke) / 2.0;
                stroke_arc(window, c, r, 0.0, 2.0 * PI, stroke, track_color);
                stroke_arc(window, c, r, -PI / 2.0, 2.0 * PI * value, fill_stroke, fill_color);
            },
        )
        .absolute()
        .size_full(),
    );
    // The label sits centred over the ring like the React `.xui-Ring-label`
    // (it may overflow a small ring, as it does there).
    if !label.is_empty() {
        let fs = px_prop(&cx.part_props("Ring", "label", &[]), "fontSize").unwrap_or(12.0);
        out = out.child(div().absolute().size_full().flex().items_center().justify_center().whitespace_nowrap().text_size(px(fs)).text_color(label_style.color.unwrap_or(cx.ink)).child(SharedString::from(label)));
    }
    out.into_any_element()
}

/// The chart palette colour of series/slice `i` (tone wins).
pub fn chart_color(cx: &LeafCx, i: usize, tone: Option<&str>) -> Hsla {
    let named = match tone {
        Some("primary") => Some("primary"),
        Some("success") => Some("success"),
        Some("warning") => Some("warning"),
        Some("danger") => Some("destructive"),
        Some("info") => Some("info"),
        Some("neutral") => Some("mutedForeground"),
        _ => None,
    };
    named.and_then(|n| cx.theme_color(n)).or_else(|| cx.theme_color(&format!("chart{}", i % 5 + 1))).unwrap_or_else(|| hsl(seed_hue(&i.to_string()) * 7.0, 0.6, 0.55, 1.0))
}

/// `Chart`.
pub fn chart(cx: &LeafCx) -> AnyElement {
    super::chart::paint(cx)
}

/// `TreeGuides`: 16 px columns, 1 px lines at x = 7, the elbow at mid-height.
pub fn tree_guides(cx: &LeafCx) -> AnyElement {
    let depth = cx.num("depth").unwrap_or(0.0).max(0.0) as usize;
    let elbow_at = cx.num("elbowAt").map(|v| v as i64).unwrap_or(depth as i64 - 1);
    let tee = cx.bool("tee");
    let pass: Vec<i64> = cx.node.props.get("passThrough").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).map(|v| v as i64).collect()).unwrap_or_default();
    let line = cx.part_props("TreeGuides", "line", &[]);
    let color = color_of(line.get("color").and_then(Value::as_str)).unwrap_or(cx.ink.opacity(0.25));
    let lw = px_prop(&line, "width").unwrap_or(1.0);
    let h = cx.h;
    let mut out = div().size_full().relative();
    for i in 0..depth {
        let x = i as f32 * 16.0 + 7.0;
        if pass.contains(&(i as i64)) {
            out = out.child(div().absolute().left(px(x)).top_0().w(px(lw)).h(px(h)).bg(color));
        }
        if i as i64 == elbow_at {
            out = out.child(div().absolute().left(px(x)).top_0().w(px(lw)).h(px(if tee { h } else { h / 2.0 })).bg(color));
            out = out.child(div().absolute().left(px(x)).top(px((h / 2.0).floor())).w(px(16.0 - 7.0)).h(px(lw)).bg(color));
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
    cx.content()
        .text_color(color)
        .text_size(px(fs))
        .line_height(px(lh))
        .when_some(family, |d, f| d.font_family(f))
        .child(SharedString::from(unknown_label(&cx.node.props, &cx.node.component)))
        .into_any_element()
}

/// A Checkbox `box`: the check glyph when checked.
pub fn check_box(cx: &LeafCx, checked: bool) -> AnyElement {
    if !checked {
        return div().into_any_element();
    }
    let check = cx.part_props("Checkbox", "check", &[]);
    let size = px_prop(&check, "width").unwrap_or(12.0);
    let color = color_of(check.get("color").and_then(Value::as_str)).unwrap_or(cx.ink);
    div().size_full().flex().items_center().justify_center().child(icons::glyph(Glyph::Check, size, color)).into_any_element()
}

/// A Switch `track`: the thumb, travelled right when checked.
pub fn switch_thumb(cx: &LeafCx, checked: bool) -> AnyElement {
    let states: Vec<String> = if checked { vec!["checked".into()] } else { vec![] };
    let mut props = cx.owner_props.clone();
    props.insert("checked".into(), Value::Bool(checked));
    let tp = part_props(cx.theme, cx.mode, "Switch", "thumb", &props, &states);
    let thumb = PaintStyle::from_visual(&exponential_ui::style::visual(&tp, exponential_ui::style::BoxKind::Leaf));
    let size = px_prop(&tp, "width").unwrap_or((cx.h - 4.0).max(4.0));
    let inset = ((cx.h - size) / 2.0).max(0.0);
    let x = if checked { cx.w - size - inset } else { inset };
    let fallback = cx.theme_color("background").unwrap_or(gpui::white());
    let style = PaintStyle { bg: thumb.bg.or(Some(fallback)), radius: size / 2.0, ..thumb };
    div().size_full().relative().child(styled_box(div().absolute().left(px(x)).top(px(inset)).size(px(size)), &style, size, size)).into_any_element()
}

/// A Select / DatePicker trigger's content: the value (or placeholder) and
/// the chevron / calendar glyph.
pub fn trigger_content(cx: &LeafCx, label: &str, is_placeholder: bool, glyph: Glyph) -> AnyElement {
    let ph = cx.part(cx.owner_component(), "placeholder", &[]);
    let muted = ph.color.or_else(|| cx.theme_color("mutedForeground")).unwrap_or(cx.ink.opacity(0.6));
    let color = if is_placeholder { muted } else { cx.ink };
    cx.content()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(spacing(cx.theme, "sm")))
        .child(div().min_w_0().flex_1().truncate().text_color(color).child(SharedString::from(label.to_string())))
        .child(div().flex_none().opacity(0.6).child(icons::glyph(glyph, 16.0, cx.ink)))
        .into_any_element()
}

/// The value bar of a Slider `track` (range + thumb at `fraction`).
pub fn slider(cx: &LeafCx, fraction: f32) -> AnyElement {
    let track_props = cx.part_props("Slider", "track", &[]);
    let bar_h = px_prop(&track_props, "height").unwrap_or(6.0).min(cx.h.max(1.0));
    let track = PaintStyle::from_visual(&exponential_ui::style::visual(&track_props, exponential_ui::style::BoxKind::Leaf));
    let range = cx.part("Slider", "range", &[]);
    let thumb_states: Vec<String> = cx.states.iter().filter(|s| *s == "focus" || *s == "hover").cloned().collect();
    let thumb_props = cx.part_props("Slider", "thumb", &thumb_states);
    let thumb = PaintStyle::from_visual(&exponential_ui::style::visual(&thumb_props, exponential_ui::style::BoxKind::Leaf));
    let t = px_prop(&thumb_props, "width").unwrap_or(16.0);
    let f = fraction.clamp(0.0, 1.0);
    let y = (cx.h - bar_h) / 2.0;
    let primary = cx.theme_color("primary").unwrap_or(cx.ink);
    let muted = cx.theme_color("muted").unwrap_or(cx.ink.opacity(0.15));
    let radius = (bar_h / 2.0).min(track.radius.max(bar_h / 2.0));
    let thumb_style = PaintStyle { bg: thumb.bg.or(Some(gpui::white())), radius: t / 2.0, ..thumb };
    div()
        .size_full()
        .relative()
        .child(div().absolute().left_0().top(px(y)).w(px(cx.w)).h(px(bar_h)).rounded(px(radius)).bg(track.bg.unwrap_or(muted)))
        .child(div().absolute().left_0().top(px(y)).w(px(cx.w * f)).h(px(bar_h)).rounded(px(radius)).bg(range.bg.unwrap_or(primary)))
        .child(styled_box(div().absolute().left(px((cx.w - t) * f)).top(px((cx.h - t) / 2.0)).size(px(t)), &thumb_style, t, t))
        .into_any_element()
}

/// `Skeleton`: the box (background + radius) is the whole paint.
pub fn skeleton(_cx: &LeafCx) -> AnyElement {
    div().into_any_element()
}

/// The legend row under a chart.
pub fn legend(cx: &LeafCx) -> Vec<(String, Hsla)> {
    let series = cx.node.props.get("series").and_then(Value::as_array).cloned().unwrap_or_default();
    let pie = cx.str("kind") == "pie";
    chart_legend(&cx.node.props)
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            let tone = if pie { None } else { series.get(i).and_then(|s| s.get("tone")).and_then(Value::as_str) };
            (name, chart_color(cx, i, tone))
        })
        .collect()
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
}
