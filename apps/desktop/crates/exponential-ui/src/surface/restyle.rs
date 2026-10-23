//! Per-node RESTYLE: theme recipes + the node's style → conditions flattened
//! for this pass → logical keys made physical → the engine style, the
//! painted visual, the text style and control box a measurer needs.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use serde_json::{json, Map, Value};
use taffy::style::Direction;

use super::{RadiusPercent, Surface};
use crate::conditions::{parse_media_condition, ConditionContext};
use crate::layout_tree::{LNode, NodeKind};
use crate::measure::{ControlBox, TextStyle};
use crate::style::{self, BoxKind, Visual};
use crate::theme::{resolve_recipe, resolve_style_values, RecipeQuery};

impl Surface {
    /// The theme-resolved style of a node (tokens → values, conditions nested).
    pub(super) fn themed_style(&self, n: &LNode, states: &[String]) -> Map<String, Value> {
        let Some(theme) = &self.effective else {
            return n.base_style.clone();
        };
        let mut out = Map::new();
        // Builder defaults < the native's own root recipe < the author's
        // style (round 2: a recipe `gap` on Form/Radio roots applies).
        let (defaults, authored): (Map<String, Value>, Map<String, Value>) = n.base_style.iter().map(|(k, v)| (k.clone(), v.clone())).partition(|(k, _)| n.default_keys.contains(k));
        for (k, v) in resolve_style_values(theme, &defaults, self.mode) {
            out.insert(k, v);
        }
        if let Some(q) = &n.own_query {
            let mut q = q.clone();
            q.states = states.to_vec();
            for (k, v) in resolve_recipe(theme, &q, self.mode) {
                out.insert(k, v);
            }
        }
        for (k, v) in resolve_style_values(theme, &authored, self.mode) {
            out.insert(k, v);
        }
        if let Some(q) = &n.part_query {
            let mut q = q.clone();
            for s in states {
                if !q.states.contains(s) {
                    q.states.push(s.clone());
                }
            }
            for (k, v) in resolve_recipe(theme, &q, self.mode) {
                out.insert(k, v);
            }
        }
        fade_to_gradient(n, &mut out);
        out
    }

    fn condition_context(&self) -> ConditionContext {
        let (w, h) = self.viewport;
        ConditionContext {
            width: w,
            height: if h > 0.0 { Some(h) } else { None },
            hover: self.settings.hover,
            reduced_motion: self.settings.reduced_motion,
            states: Vec::new(),
            breakpoints: self.breakpoints(),
        }
    }

    /// The surface direction: the root's `direction`, else the locale's.
    fn surface_direction(&self) -> Direction {
        let from_root = self.main_root.and_then(|r| {
            let n = &self.nodes[r as usize];
            let themed = self.themed_style(n, &[]);
            style::direction_of(&crate::conditions::resolve_conditions(&themed, &self.condition_context()))
        });
        from_root.unwrap_or(if crate::locale::text_direction(&self.settings.locale) == "rtl" { Direction::Rtl } else { Direction::Ltr })
    }

    /// Restyle what is dirty (everything after a theme/mode/settings or a
    /// direction change).
    pub(super) fn restyle(&mut self) {
        crate::roomy(|| self.restyle_now())
    }

    fn restyle_now(&mut self) {
        let direction = self.surface_direction();
        if direction != self.direction {
            self.direction = direction;
            self.restyle_all = true;
        }
        let mut targets: Vec<u32> = if self.restyle_all {
            (0..self.nodes.len() as u32).filter(|i| self.live[*i as usize]).collect()
        } else {
            std::mem::take(&mut self.style_dirty).into_iter().filter(|i| self.live.get(*i as usize).copied().unwrap_or(false)).collect()
        };
        self.restyle_all = false;
        self.style_dirty.clear();
        // Parents first: a `%` font size resolves against the parent's.
        targets.sort_by_key(|i| self.nodes[*i as usize].depth);
        let ctx = self.condition_context();
        let mut memo = StyleMemo::default();
        loop {
            self.restyled += targets.len() as u32;
            let round: std::collections::HashSet<u32> = targets.iter().copied().collect();
            for slot in targets {
                self.restyle_node(slot, &ctx, &mut memo);
            }
            // A restyle that moved a node's type size or direction dirtied
            // children that follow it (`%` font sizes, inherited direction):
            // they restyle in THIS pass (those already in this round ran
            // after their parent: depth order).
            let next: Vec<u32> = std::mem::take(&mut self.style_dirty).into_iter().filter(|i| !round.contains(i) && self.live.get(*i as usize).copied().unwrap_or(false)).collect();
            if next.is_empty() {
                break;
            }
            targets = next;
            targets.sort_by_key(|i| self.nodes[*i as usize].depth);
        }
    }

    /// The node-independent half of a restyle: the themed style with its
    /// conditions flattened for this pass, made physical, typed and scaled.
    fn styled_flat(&self, n: &LNode, states: &[String], base: &ConditionContext, parent_dir: Direction, parent_font: f32) -> Styled {
        let themed = self.themed_style(n, states);
        let conditional = themed.keys().any(|k| parse_media_condition(k).is_some());
        let ctx = ConditionContext { states: states.to_vec(), ..base.clone() };
        let mut flat = crate::conditions::resolve_conditions(&themed, &ctx);
        let direction = style::direction_of(&flat).unwrap_or(parent_dir);
        style::resolve_logical(&mut flat, direction);
        // Round 2 §7: the Slider's track row is as tall as the thumb, the
        // rail centred in it (the thumb is a paint part).
        if n.owner_component.as_deref() == Some("Slider") && n.part.as_deref() == Some("track") {
            if let (Some(theme), Some(q)) = (&self.effective, &n.part_query) {
                let thumb = resolve_recipe(theme, &RecipeQuery::new("Slider", "thumb", q.props.clone(), Vec::new()), self.mode).get("height").and_then(style::px);
                let rail = flat.get("height").and_then(style::px);
                if let (Some(t), Some(r)) = (thumb, rail) {
                    let m = ((t - r) / 2.0).max(0.0);
                    for k in ["marginTop", "marginBottom"] {
                        flat.entry(k.to_string()).or_insert(json!(m));
                    }
                }
            }
        }
        // Round 2: a CodeBlock line number sits in the `CodeBlock/gutter`
        // padding (its margins: the dumped number is the text alone).
        if n.owner_component.as_deref() == Some("CodeBlock") && n.part.as_deref() == Some("lineNumber") {
            if let (Some(theme), Some(q)) = (&self.effective, &n.part_query) {
                let gutter = resolve_recipe(theme, &RecipeQuery::new("CodeBlock", "gutter", q.props.clone(), Vec::new()), self.mode);
                let pad = |k: &str| gutter.get(k).and_then(style::px);
                let (l, r) = (pad("paddingLeft").or(pad("paddingHorizontal")).or(pad("padding")), pad("paddingRight").or(pad("paddingHorizontal")).or(pad("padding")));
                for (k, v) in [("marginLeft", l), ("marginRight", r)] {
                    if let Some(v) = v {
                        flat.entry(k.to_string()).or_insert(json!(v));
                    }
                }
            }
        }
        if n.hidden {
            flat.insert("display".into(), json!("none"));
        }
        // Type: `%` sizes against the parent's (font) or the own size (line
        // height), then the host's font scale.
        let scale = self.settings.font_scale;
        let mut font_size = flat.get("fontSize").and_then(|v| style::px(v).or_else(|| style::percent_value(v).map(|p| p * parent_font / scale.max(0.1))));
        if let Some(f) = font_size.as_mut() {
            *f *= scale;
            flat.insert("fontSize".into(), json!(*f));
        }
        let own_font = font_size.unwrap_or(14.0 * scale);
        if let Some(lh) = flat.get("lineHeight").and_then(|v| style::px(v).map(|p| p * scale).or_else(|| style::percent_value(v).map(|p| p * own_font))) {
            flat.insert("lineHeight".into(), json!(lh));
        }
        if let Some(ls) = flat.get("letterSpacing").and_then(style::px) {
            flat.insert("letterSpacing".into(), json!(ls * scale));
        }
        // Motion: none under reduced motion.
        if self.settings.reduced_motion && flat.contains_key("transition") {
            flat.insert("transition".into(), json!(0));
        }
        let radius_pct = {
            let all = flat.get("borderRadius").and_then(style::percent_value);
            let corners = ["borderTopLeftRadius", "borderTopRightRadius", "borderBottomRightRadius", "borderBottomLeftRadius"].map(|k| flat.get(k).and_then(style::percent_value));
            (all.is_some() || corners.iter().any(Option::is_some)).then_some(RadiusPercent { all, corners })
        };
        Styled { flat, conditional, direction, radius_pct, built: None }
    }

    /// The engine style and the painted visual of a flat style (the node's
    /// own text alignment, chart colours and direction come after).
    fn styled_box(&self, n: &LNode, flat: &Map<String, Value>, direction: Direction, kind: BoxKind) -> (Result<taffy::prelude::Style, String>, Visual) {
        let taffy_style = style::to_taffy(flat, direction, kind).map(|mut taffy_style| {
            // A one-line (ellipsized) leaf clips on x, like the web's `overflow:
            // clip; min-width: 0`. In taffy it is a clipping box on x (layout
            // only, never painted): min-width auto = 0 as on the web, and its
            // max-content contribution is not floored at the PARENT's padding
            // (taffy 0.12 bug: a "3" Badge came out 2 × its padding wide).
            if kind == BoxKind::Leaf && n.lines == Some(1) && taffy_style.overflow.x == taffy::style::Overflow::Visible {
                taffy_style.overflow.x = taffy::style::Overflow::Hidden;
            }
            // A Table cell LEAF keeps its padding inside the measure, where taffy
            // cannot see it: its flex basis floors at that padding like a slot
            // cell's (a box), so `flexBasis: 0` columns share the row equally.
            if kind == BoxKind::Leaf && n.owner_component.as_deref() == Some("Table") && matches!(n.part.as_deref(), Some("cell" | "headerCell")) {
                let v = style::visual(flat, kind);
                let p = v.padding.map(|p| p[1] + p[3]).unwrap_or(2.0 * v.padding_horizontal.unwrap_or(0.0)) + 2.0 * v.border_width.unwrap_or(0.0);
                if taffy_style.flex_basis.tag() == taffy::style::CompactLength::LENGTH_TAG && taffy_style.flex_basis.value() < p {
                    taffy_style.flex_basis = taffy::style::Dimension::length(p);
                }
            }
            taffy_style
        });
        let mut visual = style::visual(flat, kind);
        if let (Some(name), Some(theme)) = (flat.get("animation").and_then(Value::as_str), &self.effective) {
            let duration = flat.get("animationDuration").and_then(Value::as_f64);
            visual.animation = crate::animation::animation_timing_ms(name, theme, duration).map(|timing| crate::style::VisualAnimation { name: name.to_string(), timing, reduced: self.settings.reduced_motion });
        }
        (taffy_style, visual)
    }

    fn restyle_node(&mut self, slot: u32, base: &ConditionContext, memo: &mut StyleMemo) {
        let i = slot as usize;
        let n = &self.nodes[i];
        let states = self.states_of(n);
        // `direction` inherits like CSS: the node's own, else its parent's,
        // else the surface's (a layer root takes the surface's).
        let parent_dir = n.parent.and_then(|p| self.node_state.get(p as usize)).filter(|s| s.styled).map(|s| s.direction).unwrap_or(self.direction);
        let parent_font = n.parent.and_then(|p| self.node_state.get(p as usize)).map(|s| s.text.font_size).unwrap_or(14.0);
        let kind = if n.kind == NodeKind::Leaf { BoxKind::Leaf } else { BoxKind::Container };
        let extra = StyleExtra { parent_dir, parent_font: parent_font.to_bits(), hidden: n.hidden, leaf: kind == BoxKind::Leaf, lines: n.lines };
        // VAPP-132: template instances share their style inputs, so 10,000
        // rows resolve their recipes, conditions and boxes once.
        let bucket = memo.by_key.entry(style_key(n, &states, &extra)).or_default();
        let at = match bucket.iter().position(|e| e.extra == extra && e.states == states && same_style_inputs(n, &self.nodes[e.slot as usize])) {
            Some(at) => at,
            None => {
                let styled = self.styled_flat(n, &states, base, parent_dir, parent_font);
                bucket.push(StyleEntry { slot, states, extra, styled });
                bucket.len() - 1
            }
        };
        let styled = &mut bucket[at].styled;
        let (direction, radius_pct) = (styled.direction, styled.radius_pct);
        let state = &self.node_state[i];
        let unchanged = state.styled && state.flat == styled.flat && state.direction == direction;
        let direction_changed = state.styled && state.direction != direction;
        self.node_state[i].conditional = styled.conditional;
        if unchanged {
            return;
        }
        if styled.built.is_none() {
            styled.built = Some(self.styled_box(n, &styled.flat, direction, kind));
        }
        let flat = styled.flat.clone();
        let (taffy_style, mut visual) = styled.built.clone().expect("built above");
        let taffy_style = match taffy_style {
            Ok(s) => s,
            Err(e) => {
                let issue = crate::types::ReduceIssue { id: n.id.clone(), message: format!("style: {e}") };
                if !self.issues.contains(&issue) {
                    self.issues.push(issue.clone());
                }
                if !self.style_issues.contains(&issue) {
                    self.style_issues.push(issue);
                }
                taffy::prelude::Style { box_sizing: taffy::style::BoxSizing::BorderBox, direction, ..taffy::prelude::Style::default() }
            }
        };
        self.engine.set_style(slot, taffy_style);
        if n.component == "Chart" {
            visual = self.chart_colors(n, visual);
        }
        // Round 2: a leaf's text shapes with the NODE's direction and its
        // resolved physical alignment (style `textAlign`, a Text's `align`,
        // else start); the animation with its timing.
        if kind == BoxKind::Leaf {
            let dir = if direction == Direction::Rtl { crate::direction::TextDirection::Rtl } else { crate::direction::TextDirection::Ltr };
            visual.direction = Some(dir.as_str().to_string());
            visual.text_align = Some(crate::direction::text_align_of(&flat, &n.props, dir).as_str().to_string());
        }
        let scale = self.settings.font_scale;
        // Type INHERITS like CSS (round 2 parity): an unset size, weight,
        // family, style, case or tracking is the parent's; an unset line
        // height is the parent's px value when one was set (or inherited),
        // else `normal` for this node's own size. A layer root starts over.
        let inherited = n.parent.filter(|p| self.nodes[*p as usize].layer == n.layer).and_then(|p| self.node_state.get(p as usize)).filter(|s| s.styled).map(|s| (s.text.clone(), s.line_height_set));
        // A root inherits the SURFACE's type (`$type.size.sm` on
        // `$type.lineHeight.sm`, the web's `.xui-surface`).
        let inherited = inherited.or_else(|| {
            let theme = self.effective.as_ref()?;
            let token = |t: &str| crate::theme::resolve_token(theme, &json!(t), self.mode).and_then(|v| style::px(&v));
            let (size, lh) = (token("$type.size.sm")?, token("$type.lineHeight.sm")?);
            Some((TextStyle { font_size: size * scale, line_height: lh * scale, font_weight: 400, ..TextStyle::default() }, true))
        });
        let (pt, parent_lh_set) = match &inherited {
            Some((t, set)) => (Some(t), *set),
            None => (None, false),
        };
        let font_size = visual.font_size.or(pt.map(|t| t.font_size)).unwrap_or(14.0 * scale);
        let line_height_set = visual.line_height.is_some() || parent_lh_set;
        let text = TextStyle {
            font_size,
            font_weight: visual.font_weight.or(pt.map(|t| t.font_weight)).unwrap_or(400),
            line_height: visual.line_height.or(pt.filter(|_| parent_lh_set).map(|t| t.line_height)).unwrap_or_else(|| (font_size * 1.43).round()),
            font_family: visual.font_family.clone().or_else(|| pt.and_then(|t| t.font_family.clone())),
            font_style: visual.font_style.clone().or_else(|| pt.and_then(|t| t.font_style.clone())).filter(|s| s != "normal"),
            text_transform: visual.text_transform.clone().or_else(|| pt.and_then(|t| t.text_transform.clone())).filter(|t| t != "none"),
            letter_spacing: visual.letter_spacing.or(pt.and_then(|t| t.letter_spacing)).filter(|l| *l != 0.0),
        };
        let pads = visual.padding.unwrap_or([0.0; 4]);
        let control = ControlBox {
            padding_horizontal: visual.padding_horizontal.unwrap_or(0.0),
            padding_vertical: visual.padding_vertical.unwrap_or(0.0),
            padding: pads,
            border_width: visual.border_width.unwrap_or(0.0),
            gap: visual.gap.unwrap_or(0.0),
            min_width: flat.get("minWidth").and_then(style::px),
            min_height: flat.get("minHeight").and_then(style::px),
            width: flat.get("width").and_then(style::px),
            height: flat.get("height").and_then(style::px),
        };
        let state = &mut self.node_state[i];
        if state.visual != visual {
            self.visuals_dirty.insert(slot);
        }
        let first = !state.styled;
        let text_changed = state.text != text;
        if !first && (text_changed || state.control != control) {
            // The measurer's inputs changed: its answers are stale.
            self.memo.forget(slot);
            self.memo_versions.remove(&slot);
            self.engine.mark_dirty(slot);
        }
        state.flat = flat;
        state.visual = visual;
        state.text = text;
        state.line_height_set = line_height_set;
        state.control = control;
        state.radius_pct = radius_pct;
        state.styled = true;
        state.direction = direction;
        if (text_changed || direction_changed) && !first {
            // `%` font sizes below follow; so does an inherited direction.
            let children = self.nodes[i].children.clone();
            for c in children {
                // Type inherits: every child follows a text change.
                self.style_dirty.insert(c);
            }
        }
    }

    /// A Chart's series colour tokens resolved for the mode.
    fn chart_colors(&self, n: &LNode, mut visual: crate::style::Visual) -> crate::style::Visual {
        let tokens: Vec<String> = n.props.get("chart").and_then(|c| c.get("colors")).and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
        if let Some(theme) = &self.effective {
            let colors: Vec<String> = tokens.iter().map(|t| crate::theme::resolve_token(theme, &Value::String(t.clone()), self.mode).and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| t.clone())).collect();
            visual.series_colors = Some(colors);
        } else if !tokens.is_empty() {
            visual.series_colors = Some(tokens);
        }
        visual
    }
}

/// VAPP-132: the node-independent restyle results of ONE pass (theme,
/// mode, settings and the nodes stay put during it), by a hash of the
/// inputs; a hit compares them in full (source order included: media
/// blocks apply in it) against the slot that computed the entry.
#[derive(Default)]
pub(super) struct StyleMemo {
    by_key: HashMap<u64, Vec<StyleEntry>>,
}

struct StyleEntry {
    slot: u32,
    states: Vec<String>,
    extra: StyleExtra,
    styled: Styled,
}

/// What a restyle reads besides the style inputs ([`same_style_inputs`]).
#[derive(Clone, Copy, PartialEq)]
struct StyleExtra {
    parent_dir: Direction,
    parent_font: u32,
    hidden: bool,
    leaf: bool,
    lines: Option<u32>,
}

struct Styled {
    flat: Map<String, Value>,
    conditional: bool,
    direction: Direction,
    radius_pct: Option<RadiusPercent>,
    /// [`Surface::styled_box`], computed the first time a node needs it.
    built: Option<(Result<taffy::prelude::Style, String>, Visual)>,
}

fn style_key(n: &LNode, states: &[String], extra: &StyleExtra) -> u64 {
    let mut h = rustc_hash::FxHasher::default();
    h.write_u64(themed_key(n, states));
    (extra.parent_dir == Direction::Rtl).hash(&mut h);
    extra.parent_font.hash(&mut h);
    extra.hidden.hash(&mut h);
    extra.leaf.hash(&mut h);
    extra.lines.hash(&mut h);
    h.finish()
}

/// Everything [`Surface::themed_style`] reads of a node, and its states.
fn themed_key(n: &LNode, states: &[String]) -> u64 {
    let mut h = rustc_hash::FxHasher::default();
    hash_props(&n.base_style, &mut h);
    n.default_keys.hash(&mut h);
    for q in [&n.own_query, &n.part_query] {
        match q {
            Some(q) => {
                1u8.hash(&mut h);
                q.component.hash(&mut h);
                q.part.hash(&mut h);
                hash_props(&q.props, &mut h);
                q.states.hash(&mut h);
            }
            None => 0u8.hash(&mut h),
        }
    }
    n.part.hash(&mut h);
    n.owner_component.hash(&mut h);
    states.hash(&mut h);
    h.finish()
}

fn same_style_inputs(a: &LNode, b: &LNode) -> bool {
    let same_query = |x: &Option<RecipeQuery>, y: &Option<RecipeQuery>| match (x, y) {
        (None, None) => true,
        (Some(x), Some(y)) => x.component == y.component && x.part == y.part && x.states == y.states && same_props(&x.props, &y.props),
        _ => false,
    };
    a.part == b.part && a.owner_component == b.owner_component && a.default_keys == b.default_keys && same_props(&a.base_style, &b.base_style) && same_query(&a.own_query, &b.own_query) && same_query(&a.part_query, &b.part_query)
}

/// Source order counts (serde_json's map equality ignores it).
fn same_props(a: &Map<String, Value>, b: &Map<String, Value>) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|((ka, va), (kb, vb))| ka == kb && same_value(va, vb))
}

fn same_value(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => same_props(a, b),
        (Value::Array(a), Value::Array(b)) => a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same_value(x, y)),
        _ => a == b,
    }
}

fn hash_props(m: &Map<String, Value>, h: &mut impl Hasher) {
    m.len().hash(h);
    for (k, v) in m {
        k.hash(h);
        hash_value(v, h);
    }
}

fn hash_value(v: &Value, h: &mut impl Hasher) {
    match v {
        Value::Object(m) => {
            5u8.hash(h);
            hash_props(m, h);
        }
        Value::Array(a) => {
            4u8.hash(h);
            a.len().hash(h);
            for x in a {
                hash_value(x, h);
            }
        }
        other => other.hash(h),
    }
}

/// EXP-1259: a search list's bottom `fade` paints its ground colour as a
/// gradient from transparent (top) to opaque (bottom), so rows fade out
/// under it: the resolved `backgroundColor` (the recipe's, the list's
/// ground) becomes `backgroundGradient`, which every painter draws.
pub(crate) fn fade_to_gradient(n: &LNode, out: &mut Map<String, Value>) {
    if n.part.as_deref() != Some("fade") || !matches!(n.owner_component.as_deref(), Some("SearchList" | "Menu")) {
        return;
    }
    let Some(color) = out.get("backgroundColor").and_then(Value::as_str).filter(|c| c.starts_with('#') && (c.len() == 7 || c.len() == 9)).map(|c| c[..7].to_string()) else { return };
    out.remove("backgroundColor");
    out.insert("backgroundGradient".into(), json!({"angle": 180, "stops": [{"color": format!("{color}00"), "offset": 0}, {"color": color, "offset": 1}]}));
}
