//! CodeBlock (header with title + copy, one row per line: gutter number and
//! the line's tokens from the core tokenizer) and Chart (one measured leaf
//! carrying the numbers every painter shares: extent, nice ticks, the series
//! colour tokens).

use serde_json::{json, Value};

use super::{bool_prop, js, str_prop, Builder, NodeKind};
use crate::chart::{chart_extent, nice_ticks, series_color, ChartSeries, DONUT_HOLE};
use crate::code::tokenize_code;
use crate::theme::RecipeQuery;

/// The px height a CodeBlock line takes in the fixed geometry (the theme's
/// `lineHeight` decides on a real surface; `maxLines` uses this as a floor).
pub const CODE_LINE_HEIGHT: f32 = 20.0;

impl Builder<'_, '_> {
    pub(crate) fn code_block(&mut self, index: u32) {
        let owner = self.nodes[index as usize].clone();
        self.style_default(index, &[("display", json!("flex")), ("flexDirection", json!("column"))]);
        self.nodes[index as usize].kind = NodeKind::Container;
        let code = str_prop(&owner.props, "code").unwrap_or("").to_string();
        let language = str_prop(&owner.props, "language").unwrap_or("plain").to_string();
        let numbers = bool_prop(&owner.props, "lineNumbers");
        let wrap = bool_prop(&owner.props, "wrap");
        let copyable = owner.props.get("copyable").and_then(Value::as_bool).unwrap_or(true);
        let highlight: Vec<u64> = owner.props.get("highlight").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default();
        let title = str_prop(&owner.props, "title").map(str::to_string);
        if title.is_some() || copyable {
            let header = self.part(&owner, "header", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "space-between", "gap": "$spacing.sm"}), json!({}));
            if let Some(t) = &title {
                let n = self.text_part(header, &owner, "title", t, "caption", "");
                self.nodes[n as usize].base_style.insert("flexShrink".into(), json!(1));
                self.nodes[n as usize].base_style.insert("minWidth".into(), json!(0));
            } else {
                let spacer = self.part_in(header, &owner, "spacer", "Box", NodeKind::Container, json!({"flexGrow": 1}), json!({}), "");
                self.nodes[spacer as usize].part_query = None;
            }
            if copyable {
                let copied = self.ctx.local.copied.get(&owner.id).copied().unwrap_or(false);
                let label = self.string(if copied { "copied" } else { "copy" });
                let glyph = if copied { "CodeBlock.copied" } else { "CodeBlock.copy" };
                let c = self.part_in(header, &owner, "copy", "Button", NodeKind::Leaf, json!({"flexShrink": 0}), json!({"icon": super::builtin_icon(glyph), "label": label, "variant": "ghost", "size": "icon", "copied": copied}), "");
                self.nodes[c as usize].pressable = true;
                self.nodes[c as usize].live = copied.then(|| "polite".to_string());
            }
        }
        let mut body_style = json!({"display": "flex", "flexDirection": "column", "alignItems": "flex-start"});
        if !wrap {
            body_style["overflowX"] = json!("auto");
        }
        if let Some(max) = owner.props.get("maxLines").and_then(Value::as_f64) {
            body_style["maxHeight"] = json!(max as f32 * CODE_LINE_HEIGHT);
            body_style["overflowY"] = json!("auto");
        }
        let body = self.part(&owner, "body", "Box", NodeKind::Container, body_style, json!({}));
        self.nodes[body as usize].part_query = None;
        let lines = tokenize_code(&code, &language);
        let digits = lines.len().to_string().len();
        for (i, tokens) in lines.iter().enumerate() {
            let suffix = format!(".{i}");
            let text: String = tokens.iter().map(|t| t.text.as_str()).collect();
            let row_style = if wrap { json!({"display": "flex", "flexDirection": "row", "alignSelf": "stretch"}) } else { json!({"display": "flex", "flexDirection": "row", "flexShrink": 0}) };
            let row = self.part_in(body, &owner, "line", "Box", NodeKind::Container, row_style, json!({"line": i + 1}), &suffix);
            if highlight.contains(&((i + 1) as u64)) {
                self.add_state(row, "selected");
                self.query_prop(row, "highlighted", Value::Bool(true));
            }
            if numbers {
                let n = self.part_in(row, &owner, "lineNumber", "Text", NodeKind::Leaf, json!({"flexShrink": 0, "minWidth": digits as f32 * 8.0 + 8.0, "textAlign": "end"}), json!({"text": (i + 1).to_string(), "lines": 1}), &suffix);
                let _ = n;
            }
            let tokens = serde_json::to_value(tokens).unwrap_or(Value::Null);
            let mut props = json!({"text": text, "tokens": tokens, "language": language});
            if !wrap {
                props["lines"] = json!(1);
            }
            let style = if wrap { json!({"flexGrow": 1, "flexShrink": 1, "minWidth": 0}) } else { json!({"flexShrink": 0}) };
            let code_leaf = self.part_in(row, &owner, "code", "Text", NodeKind::Leaf, style, props, &suffix);
            // The line wears the `token` recipe's base (mono type); the
            // painter colours each token with `CodeBlock/token {kind}`.
            self.nodes[code_leaf as usize].part_query = Some(RecipeQuery::new("CodeBlock", "token", self.ctx.recipes.native_recipe_props("CodeBlock", &owner.props), Vec::new()));
        }
    }

    /// Chart: one leaf; `props.chart` carries the shared numbers.
    pub(crate) fn chart(&mut self, index: u32) {
        let props = self.nodes[index as usize].props.clone();
        let kind = str_prop(&props, "kind").unwrap_or("bar").to_string();
        let series: Vec<ChartSeries> = props.get("series").and_then(|s| serde_json::from_value(s.clone()).ok()).unwrap_or_default();
        let extent = chart_extent(&kind, &series, props.get("min").and_then(Value::as_f64), props.get("max").and_then(Value::as_f64));
        let ticks = nice_ticks(extent.min, extent.max, 5);
        let slices = matches!(kind.as_str(), "pie" | "donut");
        let count = if slices { series.first().map(|s| s.values.len()).unwrap_or(0) } else { series.len() };
        let colors: Vec<String> = (0..count).map(|i| series_color(i, if slices { None } else { series[i].tone.as_deref() })).collect();
        let legend = props.get("showLegend").and_then(Value::as_bool).unwrap_or(true) && count >= 2 && kind != "sparkline";
        let n = crate::json::number;
        let chart = json!({
            "min": n(extent.min), "max": n(extent.max),
            "ticks": ticks.ticks.iter().map(|t| n(*t)).collect::<Vec<_>>(), "step": n(ticks.step),
            "colors": colors,
            "hole": if kind == "donut" { DONUT_HOLE } else { 0.0 },
            "legend": legend,
        });
        self.nodes[index as usize].props.insert("chart".into(), chart);
        let h = props.get("height").and_then(Value::as_f64).unwrap_or(200.0);
        self.nodes[index as usize].base_style.entry("height".to_string()).or_insert(json!(h));
        let _ = js;
    }
}
