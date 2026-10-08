//! Per-node RESTYLE: theme recipes + the node's style → conditions flattened
//! for this pass → logical keys made physical → the engine style, the
//! painted visual, the text style and control box a measurer needs.

use serde_json::{json, Map, Value};
use taffy::style::Direction;

use super::{RadiusPercent, Surface};
use crate::conditions::{parse_media_condition, ConditionContext};
use crate::layout_tree::{LNode, NodeKind};
use crate::measure::{ControlBox, TextStyle};
use crate::style::{self, BoxKind};
use crate::theme::{resolve_recipe, resolve_style_values};

impl Surface {
    /// The theme-resolved style of a node (tokens → values, conditions nested).
    pub(super) fn themed_style(&self, n: &LNode, states: &[String]) -> Map<String, Value> {
        let Some(theme) = &self.effective else {
            return n.base_style.clone();
        };
        let mut out = Map::new();
        if let Some(q) = &n.own_query {
            let mut q = q.clone();
            q.states = states.to_vec();
            for (k, v) in resolve_recipe(theme, &q, self.mode) {
                out.insert(k, v);
            }
        }
        for (k, v) in resolve_style_values(theme, &n.base_style, self.mode) {
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
        loop {
            self.restyled += targets.len() as u32;
            let round: std::collections::HashSet<u32> = targets.iter().copied().collect();
            for slot in targets {
                self.restyle_node(slot, &ctx);
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

    fn restyle_node(&mut self, slot: u32, base: &ConditionContext) {
        let i = slot as usize;
        let n = &self.nodes[i];
        let states = self.states_of(n);
        let themed = self.themed_style(n, &states);
        let conditional = themed.keys().any(|k| parse_media_condition(k).is_some());
        let ctx = ConditionContext { states: states.clone(), ..base.clone() };
        let mut flat = crate::conditions::resolve_conditions(&themed, &ctx);
        // `direction` inherits like CSS: the node's own, else its parent's,
        // else the surface's (a layer root takes the surface's).
        let parent_dir = n.parent.and_then(|p| self.node_state.get(p as usize)).filter(|s| s.styled).map(|s| s.direction).unwrap_or(self.direction);
        let direction = style::direction_of(&flat).unwrap_or(parent_dir);
        style::resolve_logical(&mut flat, direction);
        if n.hidden {
            flat.insert("display".into(), json!("none"));
        }
        let kind = if n.kind == NodeKind::Leaf { BoxKind::Leaf } else { BoxKind::Container };
        // Type: `%` sizes against the parent's (font) or the own size (line
        // height), then the host's font scale.
        let parent_font = n.parent.and_then(|p| self.node_state.get(p as usize)).map(|s| s.text.font_size).unwrap_or(14.0);
        let scale = self.settings.font_scale;
        let font_relative = flat.get("fontSize").is_some_and(|v| style::percent_value(v).is_some());
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
        let state = &self.node_state[i];
        let unchanged = state.styled && state.flat == flat && state.direction == direction;
        let direction_changed = state.styled && state.direction != direction;
        self.node_state[i].conditional = conditional;
        self.node_state[i].font_relative = font_relative;
        if unchanged {
            return;
        }
        let taffy_style = match style::to_taffy(&flat, direction, kind) {
            Ok(s) => s,
            Err(e) => {
                self.issues.push(crate::types::ReduceIssue { id: n.id.clone(), message: format!("style: {e}") });
                taffy::prelude::Style { box_sizing: taffy::style::BoxSizing::BorderBox, direction, ..taffy::prelude::Style::default() }
            }
        };
        self.engine.set_style(slot, taffy_style);
        let mut visual = style::visual(&flat, kind);
        if n.component == "Chart" {
            visual = self.chart_colors(n, visual);
        }
        let text = TextStyle {
            font_size: visual.font_size.unwrap_or(14.0 * scale),
            font_weight: visual.font_weight.unwrap_or(400),
            line_height: visual.line_height.unwrap_or_else(|| (visual.font_size.unwrap_or(14.0 * scale) * 1.43).round()),
            font_family: visual.font_family.clone(),
            font_style: visual.font_style.clone().filter(|s| s != "normal"),
            text_transform: visual.text_transform.clone().filter(|t| t != "none"),
            letter_spacing: visual.letter_spacing.filter(|l| *l != 0.0),
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
        state.control = control;
        state.radius_pct = radius_pct;
        state.styled = true;
        state.direction = direction;
        if (text_changed || direction_changed) && !first {
            // `%` font sizes below follow; so does an inherited direction.
            let children = self.nodes[i].children.clone();
            for c in children {
                // (`flat` holds the RESOLVED px: the flag remembers the `%`.)
                if direction_changed || self.node_state[c as usize].font_relative {
                    self.style_dirty.insert(c);
                }
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
