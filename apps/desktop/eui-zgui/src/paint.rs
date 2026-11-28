//! What each placed node paints, as CSS for its zgui element plus the
//! leaf's content pieces (text runs, vector glyphs, small boxes). The
//! reference is eui-gpui's painter (`crates/eui-gpui/src/view/paint.rs`
//! `paint_node` / `paint_leaf`, `src/paint/natives.rs`); geometry is the
//! core's frame, never zgui's own layout.

use std::fmt::Write as _;

use eui_core::measure::TextStyle;
use eui_core::style::{BoxKind, Visual};
use eui_core::surface::PlacedNode;
use eui_core::theme::{Mode, RecipeQuery, ResolvedTheme};
use serde_json::{Map, Value};

use crate::measure::{Wrap, ZMeasure};

/// One piece of a leaf's content, positioned in the leaf's PADDING box.
#[derive(Clone, Debug, PartialEq, Hash)]
pub struct Piece {
    pub css: String,
    pub kind: PieceKind,
}

#[derive(Clone, Debug, PartialEq, Hash)]
pub enum PieceKind {
    /// A text element: `(span css, text)` runs (one run = plain text).
    Text(Vec<(Option<String>, String)>),
    /// A vector element over a whole SVG document.
    Svg(String),
    /// A plain box (a thumb, a bullet).
    Box,
}

pub fn str_prop<'a>(props: &'a Map<String, Value>, key: &str) -> &'a str {
    props.get(key).and_then(Value::as_str).unwrap_or("")
}

pub fn display_text(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(v @ (Value::Number(_) | Value::Bool(_))) => eui_core::format::display_string(v),
        _ => String::new(),
    }
}

pub fn part_props(theme: Option<&ResolvedTheme>, mode: Mode, owner: &str, part: &str, owner_props: &Map<String, Value>, states: &[String]) -> Map<String, Value> {
    let Some(theme) = theme else { return Map::new() };
    let query = RecipeQuery::new(owner, part, eui_core::recipes::native_recipe_props(owner, owner_props), states.to_vec());
    eui_core::theme::resolve_recipe(theme, &query, mode)
}

pub fn part_visual(theme: Option<&ResolvedTheme>, mode: Mode, owner: &str, part: &str, owner_props: &Map<String, Value>, states: &[String]) -> Visual {
    eui_core::style::visual(&part_props(theme, mode, owner, part, owner_props, states), BoxKind::Leaf)
}

pub fn px_prop(props: &Map<String, Value>, key: &str) -> Option<f32> {
    props.get(key).and_then(eui_core::style::px)
}

pub fn theme_color(theme: Option<&ResolvedTheme>, mode: Mode, name: &str) -> Option<String> {
    theme.and_then(|t| t.modes.get(mode).color.get(name)).cloned()
}

fn f(v: f32) -> String {
    let r = (v * 100.0).round() / 100.0;
    if r == r.trunc() { format!("{}", r as i64) } else { format!("{r}") }
}

/// The border widths `[t, r, b, l]` a visual paints.
pub fn borders(v: &Visual) -> [f32; 4] {
    v.border_widths.unwrap_or([v.border_width.unwrap_or(0.0); 4])
}

fn has_border(v: &Visual) -> bool {
    v.border_color.is_some() && borders(v).iter().any(|b| *b > 0.0)
}

/// The BORDER that offsets a node's children in CSS (abs children sit in
/// the padding box; eui frames are border boxes in surface coordinates).
pub fn border_offset(v: &Visual) -> (f32, f32) {
    if has_border(v) {
        let b = borders(v);
        (b[3], b[0])
    } else {
        (0.0, 0.0)
    }
}

pub fn clips(v: &Visual) -> bool {
    v.overflow_hidden || v.overflow_scroll || v.display_none || matches!(v.overflow_x.as_deref(), Some("hidden" | "clip" | "scroll" | "auto")) || matches!(v.overflow_y.as_deref(), Some("hidden" | "clip" | "scroll" | "auto"))
}

/// A node's own element: its box at its frame (relative to the parent's
/// padding box), fill, border, corners, shadow, opacity, clip and ink.
pub fn node_css(v: &Visual, (x, y, w, h): (f32, f32, f32, f32), leaf: bool) -> String {
    let mut s = String::with_capacity(160);
    let _ = write!(s, "position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;box-sizing:border-box;", f(x), f(y), f(w.max(0.0)), f(h.max(0.0)));
    if let Some(bg) = v.background_color.as_deref().filter(|c| !is_clear(c)) {
        let _ = write!(s, "background-color:{bg};");
    }
    if let Some(g) = &v.background_gradient {
        if g.stops.len() >= 2 {
            let stops: Vec<String> = g.stops.iter().map(|st| format!("{} {}%", st.color, f(st.offset * 100.0))).collect();
            let _ = write!(s, "background-image:linear-gradient({}deg, {});", f(g.angle), stops.join(", "));
        }
    }
    if has_border(v) {
        let b = borders(v);
        let style = v.border_style.as_deref().unwrap_or("solid");
        let _ = write!(s, "border-style:{style};border-width:{}px {}px {}px {}px;border-color:{};", f(b[0]), f(b[1]), f(b[2]), f(b[3]), v.border_color.as_deref().unwrap_or("transparent"));
    }
    let r = radii(v, w, h);
    if r.iter().any(|r| *r > 0.0) {
        let _ = write!(s, "border-radius:{}px {}px {}px {}px;", f(r[0]), f(r[1]), f(r[2]), f(r[3]));
    }
    if let Some(sh) = v.box_shadow.as_ref().filter(|s| !s.is_empty()) {
        let layers: Vec<String> = sh.iter().map(|l| format!("{}px {}px {}px {}px {}", f(l.x), f(l.y), f(l.blur), f(l.spread), l.color)).collect();
        let _ = write!(s, "box-shadow:{};", layers.join(", "));
    }
    if let Some(o) = v.opacity.filter(|o| *o < 1.0) {
        let _ = write!(s, "opacity:{};", f(o));
    }
    if clips(v) || leaf {
        s.push_str("overflow:hidden;");
    }
    if let Some(c) = &v.color {
        let _ = write!(s, "color:{c};");
    }
    if v.visibility_hidden {
        s.push_str("visibility:hidden;");
    }
    s
}

fn is_clear(c: &str) -> bool {
    c.len() == 9 && c.ends_with("00")
}

pub fn radii(v: &Visual, w: f32, h: f32) -> [f32; 4] {
    let r = v.corner_radii.unwrap_or([v.border_radius.unwrap_or(0.0); 4]);
    let max = (w.min(h) / 2.0).max(0.0);
    r.map(|r| r.min(max).max(0.0))
}

/// The CSS of a text run in a typography.
pub fn text_css(ts: &TextStyle, wrap: Wrap) -> String {
    let mut s = String::with_capacity(160);
    let _ = write!(s, "font-family:{};font-size:{}px;font-weight:{};line-height:{}px;", crate::fonts::stack(ts.font_family.as_deref()), f(ts.font_size), ts.font_weight, f(ts.line_height));
    if ts.font_style.as_deref() == Some("italic") {
        s.push_str("font-style:italic;");
    }
    if let Some(l) = ts.letter_spacing.filter(|l| *l != 0.0) {
        let _ = write!(s, "letter-spacing:{}px;", f(l));
    }
    s.push_str(if wrap == Wrap::NoWrap { "white-space:pre;" } else { "white-space:pre-wrap;" });
    s
}

/// What a leaf's content is drawn with.
pub struct LeafCx<'a> {
    pub node: &'a PlacedNode,
    pub vis: &'a Visual,
    pub ts: &'a TextStyle,
    pub theme: Option<&'a ResolvedTheme>,
    pub mode: Mode,
    pub owner_props: &'a Map<String, Value>,
    pub w: f32,
    pub h: f32,
}

impl LeafCx<'_> {
    /// The padding box's content rect (x, y, w, h), relative to the padding box.
    fn content(&self) -> (f32, f32, f32, f32) {
        let p = self.vis.padding.unwrap_or([self.vis.padding_vertical.unwrap_or(0.0), self.vis.padding_horizontal.unwrap_or(0.0), self.vis.padding_vertical.unwrap_or(0.0), self.vis.padding_horizontal.unwrap_or(0.0)]);
        let b = if has_border(self.vis) { borders(self.vis) } else { [0.0; 4] };
        let w = (self.w - p[1] - p[3] - b[1] - b[3]).max(0.0);
        let h = (self.h - p[0] - p[2] - b[0] - b[2]).max(0.0);
        (p[3], p[0], w, h)
    }

    fn str(&self, key: &str) -> &str {
        str_prop(&self.node.props, key)
    }

    fn checked(&self) -> bool {
        self.node.props.get("checked").and_then(Value::as_bool).unwrap_or(false) || self.node.states.iter().any(|s| s == "checked")
    }
}

fn text_piece(css_box: String, ts: &TextStyle, text: String, wrap: Wrap, extra: &str) -> Piece {
    Piece { css: format!("position:absolute;{css_box}{}{extra}", text_css(ts, wrap)), kind: PieceKind::Text(vec![(None, text)]) }
}

fn svg_piece(name: &str, x: f32, y: f32, size: f32, color: Option<&str>, extra: &str) -> Option<Piece> {
    let doc = crate::icons::svg(name)?;
    let color = color.map(|c| format!("color:{c};")).unwrap_or_default();
    Some(Piece { css: format!("position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;{color}{extra}", f(x), f(y), f(size), f(size)), kind: PieceKind::Svg(doc) })
}

/// The content pieces of one leaf.
pub fn leaf_pieces(cx: &LeafCx, m: &mut ZMeasure) -> Vec<Piece> {
    let n = cx.node;
    let (cx0, cy0, cw, ch) = cx.content();
    let ts = cx.ts;
    let lines = n.lines.or_else(|| n.props.get("lines").and_then(Value::as_u64).map(|l| l as u32));
    let single = lines == Some(1);
    let wrap = if single { Wrap::NoWrap } else { Wrap::PreWrap };
    let centred_v = |lh: f32| (cy0 + (ch - lh) / 2.0).max(0.0);
    let mut out = Vec::new();
    match (n.component.as_str(), n.part.as_deref()) {
        ("Text", part) => {
            let text = crate::measure::transform(&display_text(n.props.get("text")), ts.text_transform.as_deref());
            if text.is_empty() {
                return out;
            }
            let control = matches!(part, Some("tab" | "trigger" | "item"));
            let top = if control { centred_v(ts.line_height) } else { cy0 };
            let align = cx.vis.text_align.as_deref().unwrap_or("left");
            let mut extra = format!("text-align:{align};");
            if single {
                extra.push_str("overflow:hidden;text-overflow:ellipsis;");
            } else if let Some(l) = lines.filter(|l| *l > 1) {
                let _ = write!(extra, "overflow:hidden;max-height:{}px;", f(l as f32 * ts.line_height));
            }
            out.push(text_piece(format!("left:{}px;top:{}px;width:{}px;", f(cx0), f(top), f(cw.max(1.0))), ts, text, wrap, &extra));
        }
        ("Button" | "Toggle", _) => {
            let label = cx.str("label").to_string();
            let icon = cx.str("icon").to_string();
            let icon_only = matches!(cx.str("size"), "icon" | "icon-xs");
            let label_w = if icon_only || label.is_empty() { 0.0 } else { m.line(&label, ts) };
            let has_icon = !icon.is_empty();
            let gap = if has_icon && label_w > 0.0 { cx.vis.gap.unwrap_or(8.0) } else { 0.0 };
            let total = if has_icon { 16.0 } else { 0.0 } + gap + label_w;
            let mut x = cx0 + ((cw - total) / 2.0).max(0.0);
            if has_icon {
                out.extend(svg_piece(&icon, x, centred_v(16.0), 16.0, None, ""));
                x += 16.0 + gap;
            }
            if label_w > 0.0 {
                out.push(text_piece(format!("left:{}px;top:{}px;width:{}px;", f(x), f(centred_v(ts.line_height)), f(label_w + 1.0)), ts, label, Wrap::NoWrap, ""));
            }
        }
        ("Icon", _) => {
            let name = if cx.str("name").is_empty() { "ui-icon-placeholder" } else { cx.str("name") };
            let size = cw.min(ch).max(1.0);
            out.extend(svg_piece(name, cx0 + (cw - size) / 2.0, cy0 + (ch - size) / 2.0, size, None, ""));
        }
        ("Checkbox", Some("box" | "checkbox")) => {
            let mixed = n.props.get("indeterminate").and_then(Value::as_bool) == Some(true);
            if cx.checked() || mixed {
                let check = part_props(cx.theme, cx.mode, "Checkbox", "check", cx.owner_props, &[]);
                let size = px_prop(&check, "width").unwrap_or(12.0);
                let color = check.get("color").and_then(Value::as_str).map(str::to_string).or_else(|| cx.vis.color.clone());
                let glyph = if mixed { "ui-minus" } else { "ui-check" };
                out.extend(svg_piece(glyph, cx0 + (cw - size) / 2.0, cy0 + (ch - size) / 2.0, size, color.as_deref(), ""));
            }
        }
        ("Switch", Some("track")) => {
            let checked = cx.checked();
            let states: Vec<String> = if checked { vec!["checked".into()] } else { vec![] };
            let mut props = cx.owner_props.clone();
            props.insert("checked".into(), Value::Bool(checked));
            let tp = part_props(cx.theme, cx.mode, "Switch", "thumb", &props, &states);
            let tv = eui_core::style::visual(&tp, BoxKind::Leaf);
            let size = px_prop(&tp, "width").unwrap_or((cx.h - 4.0).max(4.0));
            let inset = ((cx.h - size) / 2.0).max(0.0);
            let x = if checked { cx.w - size - inset } else { inset };
            let bg = tv.background_color.clone().or_else(|| theme_color(cx.theme, cx.mode, "background")).unwrap_or("#ffffff".into());
            let mut css = format!("position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;border-radius:{}px;background-color:{bg};", f(x - cx0.min(0.0)), f(inset), f(size), f(size), f(size / 2.0));
            if let Some(sh) = tv.box_shadow.as_ref().filter(|s| !s.is_empty()) {
                let layers: Vec<String> = sh.iter().map(|l| format!("{}px {}px {}px {}px {}", f(l.x), f(l.y), f(l.blur), f(l.spread), l.color)).collect();
                let _ = write!(css, "box-shadow:{};", layers.join(", "));
            }
            out.push(Piece { css, kind: PieceKind::Box });
        }
        ("Select" | "DatePicker" | "DateRangePicker" | "TimePicker", Some("trigger")) => {
            let text = cx.str("text").to_string();
            let ph = match n.props.get("placeholder") {
                Some(Value::String(s)) => s.clone(),
                _ => String::new(),
            };
            let is_ph = text.is_empty() || n.props.get("placeholder").and_then(Value::as_bool) == Some(true);
            let shown = if text.is_empty() { ph } else { text };
            let color = if is_ph { part_visual(cx.theme, cx.mode, &n.component, "placeholder", cx.owner_props, &[]).color.or_else(|| theme_color(cx.theme, cx.mode, "mutedForeground")) } else { None };
            let glyph = 16.0;
            let gap = cx.vis.gap.filter(|g| *g > 0.0).unwrap_or(8.0);
            let tw = (cw - glyph - gap).max(1.0);
            let c = color.map(|c| format!("color:{c};")).unwrap_or_default();
            out.push(text_piece(format!("left:{}px;top:{}px;width:{}px;", f(cx0), f(centred_v(ts.line_height)), f(tw)), ts, shown, Wrap::NoWrap, &format!("overflow:hidden;text-overflow:ellipsis;{c}")));
            let icon = if cx.str("icon").is_empty() { "ui-chevron-down" } else { cx.str("icon") };
            let variant = cx.owner_props.get("triggerVariant").and_then(Value::as_str);
            let fade = if variant.is_some_and(|v| v != "field") { 1.0 } else { 0.6 };
            out.extend(svg_piece(icon, cx0 + cw - glyph, centred_v(glyph), glyph, None, &format!("opacity:{};", f(fade))));
        }
        ("Input" | "Textarea" | "NumberField", Some("field" | "input")) => {
            let value = display_text(n.props.get("value"));
            let is_ph = value.is_empty();
            let shown = if is_ph { cx.str("placeholder").to_string() } else { value };
            if !shown.is_empty() {
                let color = if is_ph { part_visual(cx.theme, cx.mode, &n.component, "placeholder", cx.owner_props, &[]).color.or_else(|| theme_color(cx.theme, cx.mode, "mutedForeground")) } else { None };
                let c = color.map(|c| format!("color:{c};")).unwrap_or_default();
                let multi = n.component == "Textarea";
                let top = if multi { cy0 + ((ch - ts.line_height).min(0.0)) } else { centred_v(ts.line_height) };
                let top = if multi { cy0.max(top) } else { top };
                out.push(text_piece(format!("left:{}px;top:{}px;width:{}px;", f(cx0), f(top), f(cw.max(1.0))), ts, shown, if multi { Wrap::PreWrap } else { Wrap::NoWrap }, &format!("overflow:hidden;{c}")));
            }
        }
        ("Markdown", _) => {
            let blocks = markdown_blocks(str_prop(&n.props, "text"));
            let gap = cx.vis.gap.unwrap_or(8.0);
            let mut y = cy0;
            for b in blocks {
                let (bts, indent, bullet) = block_style(ts, &b);
                let plain: String = b.runs.iter().map(|r| r.1.as_str()).collect();
                let s = m.shape(&plain, &bts, Some((cw - indent).max(1.0)), Wrap::PreWrap, None);
                if let Some(bullet) = bullet {
                    out.push(text_piece(format!("left:{}px;top:{}px;width:{}px;", f(cx0 + indent * 0.35), f(y), f(indent)), &bts, bullet.to_string(), Wrap::NoWrap, ""));
                }
                let runs: Vec<(Option<String>, String)> = b.runs.iter().map(|(k, t)| (span_css(*k, cx), t.clone())).collect();
                out.push(Piece { css: format!("position:absolute;left:{}px;top:{}px;width:{}px;{}", f(cx0 + indent), f(y), f((cw - indent).max(1.0)), text_css(&bts, Wrap::PreWrap)), kind: PieceKind::Text(runs) });
                y += s.height.max(bts.line_height) + gap;
            }
        }
        ("Link", _) => {
            out.push(text_piece(format!("left:{}px;top:{}px;width:{}px;", f(cx0), f(cy0), f(cw.max(1.0))), ts, cx.str("label").to_string(), Wrap::NoWrap, "text-decoration:underline;"));
        }
        ("CountBadge", _) => {
            out.push(text_piece(format!("left:{}px;top:{}px;width:{}px;", f(cx0), f(centred_v(ts.line_height)), f(cw.max(1.0))), ts, display_text(n.props.get("text")), Wrap::NoWrap, "text-align:center;"));
        }
        _ => {}
    }
    out
}

// ------------------------------------------------------------ markdown

/// A deliberately small Markdown subset (paragraphs, `-`/`*`/`1.` items,
/// `#` headings, `**bold**`, `*em*`, `` `code` ``): enough for the trial's
/// specimen; eui-gpui's full GFM painter is ~4k lines (paint/markdown.rs).
pub struct Block {
    pub kind: BlockKind,
    pub runs: Vec<(Span, String)>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BlockKind {
    Para,
    Item(Option<u32>),
    Heading(u8),
}

#[derive(Clone, Copy, PartialEq, Hash, Debug)]
pub enum Span {
    Plain,
    Strong,
    Em,
    Code,
}

pub fn markdown_blocks(src: &str) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    let mut para: Vec<String> = Vec::new();
    let flush = |para: &mut Vec<String>, out: &mut Vec<Block>| {
        if !para.is_empty() {
            out.push(Block { kind: BlockKind::Para, runs: inline(&para.join(" ")) });
            para.clear();
        }
    };
    for line in src.lines() {
        let t = line.trim();
        if t.is_empty() {
            flush(&mut para, &mut out);
        } else if let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            flush(&mut para, &mut out);
            out.push(Block { kind: BlockKind::Item(None), runs: inline(rest) });
        } else if let Some((num, rest)) = t.split_once(". ").filter(|(n, _)| n.parse::<u32>().is_ok()) {
            flush(&mut para, &mut out);
            out.push(Block { kind: BlockKind::Item(num.parse().ok()), runs: inline(rest) });
        } else if t.starts_with('#') {
            flush(&mut para, &mut out);
            let level = t.chars().take_while(|c| *c == '#').count().min(6) as u8;
            out.push(Block { kind: BlockKind::Heading(level), runs: inline(t[level as usize..].trim()) });
        } else {
            para.push(t.to_string());
        }
    }
    flush(&mut para, &mut out);
    out
}

fn inline(s: &str) -> Vec<(Span, String)> {
    let mut out: Vec<(Span, String)> = Vec::new();
    let mut rest = s;
    while !rest.is_empty() {
        let next = [("**", Span::Strong), ("`", Span::Code), ("*", Span::Em)].iter().filter_map(|(m, k)| rest.find(m).map(|i| (i, *m, *k))).min_by_key(|(i, m, _)| (*i, usize::MAX - m.len()));
        match next {
            Some((i, m, k)) if rest[i + m.len()..].contains(m) => {
                if i > 0 {
                    out.push((Span::Plain, rest[..i].to_string()));
                }
                let body = &rest[i + m.len()..];
                let end = body.find(m).unwrap();
                out.push((k, body[..end].to_string()));
                rest = &body[end + m.len()..];
            }
            _ => {
                out.push((Span::Plain, rest.to_string()));
                rest = "";
            }
        }
    }
    out
}

fn block_style(ts: &TextStyle, b: &Block) -> (TextStyle, f32, Option<String>) {
    match b.kind {
        BlockKind::Para => (ts.clone(), 0.0, None),
        BlockKind::Item(n) => (ts.clone(), ts.font_size * 1.5, Some(n.map(|n| format!("{n}.")).unwrap_or("•".into()))),
        BlockKind::Heading(l) => {
            let size = match l {
                1 => 24.0,
                2 => 20.0,
                3 => 18.0,
                _ => ts.font_size,
            };
            (TextStyle { font_size: size, line_height: (size * 1.4).round(), font_weight: 600, ..ts.clone() }, 0.0, None)
        }
    }
}

fn span_css(k: Span, cx: &LeafCx) -> Option<String> {
    match k {
        Span::Plain => None,
        Span::Strong => Some("font-weight:600;".into()),
        Span::Em => Some("font-style:italic;".into()),
        Span::Code => {
            let bg = theme_color(cx.theme, cx.mode, "muted").map(|c| format!("background-color:{c};")).unwrap_or_default();
            Some(format!("font-family:{};font-size:{}px;{bg}", crate::fonts::MONO_STACK, f((cx.ts.font_size - 1.0).max(10.0))))
        }
    }
}

/// The size a Markdown leaf's blocks take at a content width.
pub fn markdown_size(m: &mut ZMeasure, props: &Map<String, Value>, ts: &TextStyle, gap: f32, wrap: Option<f32>) -> (f32, f32) {
    let blocks = markdown_blocks(str_prop(props, "text"));
    let gap = if gap > 0.0 { gap } else { 8.0 };
    let mut w = 0.0_f32;
    let mut h = 0.0_f32;
    for (i, b) in blocks.iter().enumerate() {
        let (bts, indent, _) = block_style(ts, b);
        let plain: String = b.runs.iter().map(|r| r.1.as_str()).collect();
        let s = m.shape(&plain, &bts, wrap.map(|x| if x <= 0.0 { 0.0 } else { (x - indent).max(1.0) }), Wrap::PreWrap, None);
        w = w.max(s.width + indent);
        h += s.height.max(bts.line_height) + if i > 0 { gap } else { 0.0 };
    }
    (w, h)
}
