//! CodeBlock (header with title + copy, one row per line: gutter number and
//! the line's tokens from the core tokenizer; #EXP-1804 `follow` marks the
//! body a bottom-following scroll container) and Chart (one measured leaf
//! carrying the numbers every painter shares: extent, nice ticks, the series
//! colour tokens).

use serde_json::{json, Value};

use super::{bool_prop, js, str_prop, Builder, NodeKind};
use crate::chart::{chart_data, chart_extent, format_chart_value, nice_ticks, series_color, ValueFormat, DONUT_HOLE};
use crate::code::tokenize_code;
use crate::theme::RecipeQuery;

/// The px height a CodeBlock line takes in the fixed geometry (the theme's
/// `lineHeight` decides on a real surface; `maxLines` uses this as a floor):
/// shadcn's code-block `leading-relaxed` over text-sm (EXP-1324).
pub const CODE_LINE_HEIGHT: f32 = 22.75;
/// The same at `size: sm` (text-xs, trace.json `geometry.codeBlock`).
pub const CODE_LINE_HEIGHT_SM: f32 = 19.5;

impl Builder<'_, '_> {
    pub(crate) fn code_block(&mut self, index: u32) {
        let owner = self.nodes[index as usize].clone();
        self.style_default(index, &[("display", json!("flex")), ("flexDirection", json!("column"))]);
        self.nodes[index as usize].kind = NodeKind::Container;
        let code = str_prop(&owner.props, "code").unwrap_or("").to_string();
        let language = str_prop(&owner.props, "language").unwrap_or("plain").to_string();
        // #EXP-2204: `anchors` turns the numbers on and makes them pressable.
        let anchors = bool_prop(&owner.props, "anchors");
        let numbers = bool_prop(&owner.props, "lineNumbers") || anchors;
        let selection = crate::ci_log::parse_line_anchor_value(owner.props.get("selection"));
        let blame = (str_prop(&owner.props, "gutter") == Some("blame")).then(|| owner.props.get("blame").cloned().unwrap_or(Value::Null));
        let wrap = bool_prop(&owner.props, "wrap");
        let copyable = owner.props.get("copyable").and_then(Value::as_bool).unwrap_or(true);
        let highlight: Vec<u64> = owner.props.get("highlight").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default();
        let title = str_prop(&owner.props, "title").map(str::to_string);
        // Round 2: the region's name — its title, else `$string.codeBlock`.
        let label = title.clone().unwrap_or_else(|| self.string("codeBlock"));
        self.default_a11y(index, "label", json!(label));
        // Round 2: an untitled block names its language in the header (`ts`;
        // none for `plain`), so a header never stands empty beside the copy
        // button. The region's name stays `$string.codeBlock`.
        let heading = title.clone().or_else(|| (language != "plain" && !language.is_empty()).then(|| language.clone()));
        if heading.is_some() || copyable {
            let header = self.part(&owner, "header", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "space-between", "gap": "$spacing.sm"}), json!({}));
            if let Some(t) = &heading {
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
                let c = self.part_in(header, &owner, "copy", "Button", NodeKind::Leaf, json!({"flexShrink": 0}), json!({"icon": super::builtin_icon(glyph), "label": label, "variant": "ghost", "size": "icon", "copied": copied, "text": crate::code::code_text(&code, &language)}), "");
                self.nodes[c as usize].pressable = true;
                self.nodes[c as usize].live = copied.then(|| "polite".to_string());
            }
        }
        let mut body_style = json!({"display": "flex", "flexDirection": "column", "alignItems": "flex-start"});
        if !wrap {
            body_style["overflowX"] = json!("auto");
        }
        let line_height = if str_prop(&owner.props, "size") == Some("sm") { CODE_LINE_HEIGHT_SM } else { CODE_LINE_HEIGHT };
        if let Some(max) = owner.props.get("maxLines").and_then(Value::as_f64) {
            body_style["maxHeight"] = json!(max as f32 * line_height);
            body_style["overflowY"] = json!("auto");
        }
        // #EXP-1804: `follow` = the scrolling body sticks to its bottom as
        // the code grows (the pass), until the person scrolls up (`scroll_to`).
        let body_props = if bool_prop(&owner.props, "follow") { json!({"follow": true}) } else { json!({}) };
        let body = self.part(&owner, "body", "Box", NodeKind::Container, body_style, body_props);
        self.nodes[body as usize].part_query = None;
        let lines = tokenize_code(&code, &language);
        let digits = lines.len().to_string().len();
        let blocks = blame.as_ref().map(|b| crate::ci_log::blame_blocks(Some(b), lines.len()));
        let blame_width = crate::ci_log::CODE_BLAME_WIDTH;
        for (i, tokens) in lines.iter().enumerate() {
            let suffix = format!(".{i}");
            let text: String = tokens.iter().map(|t| t.text.as_str()).collect();
            // A line spans the body (its highlight too); a long unwrapped
            // line overflows it (the body scrolls). Every line is at least
            // one line box tall (the web's `min-h-lh`): an EMPTY line (no
            // tokens, its Text measures 0×0) keeps its height, and its code
            // leaf stretches to it so a painter still places it.
            let mut row_style = if wrap { json!({"display": "flex", "flexDirection": "row", "alignSelf": "stretch"}) } else { json!({"display": "flex", "flexDirection": "row", "flexShrink": 0, "alignSelf": "stretch"}) };
            row_style["minHeight"] = json!(line_height);
            let row = self.part_in(body, &owner, "line", "Box", NodeKind::Container, row_style, json!({"line": i + 1}), &suffix);
            let selected = selection.is_some_and(|r| r.contains((i + 1) as u64));
            if highlight.contains(&((i + 1) as u64)) || selected {
                self.add_state(row, "selected");
                self.query_prop(row, "highlighted", Value::Bool(true));
            }
            if selected {
                self.nodes[row as usize].props.insert("selected".into(), Value::Bool(true));
            }
            // #EXP-2204 `gutter: blame`: the author column before the numbers;
            // a block's first line names its author and age, every block after
            // the first draws a hairline across the line (restyle: the
            // `CodeBlock/blame` borderColor).
            if let Some(blocks) = &blocks {
                let at = blocks[i];
                let cell = self.part_in(row, &owner, "blame", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row", "alignItems": "center", "width": blame_width, "flexShrink": 0, "minWidth": 0}), json!({"block": at.block, "first": at.first}), &suffix);
                if at.first && at.block >= 0 {
                    let entry = blame.as_ref().and_then(|b| b.get(at.block as usize)).cloned().unwrap_or(Value::Null);
                    let author = entry.get("author").and_then(Value::as_str).unwrap_or("").to_string();
                    let age = entry.get("age").and_then(Value::as_str).unwrap_or("").to_string();
                    let a = self.part_in(cell, &owner, "blameAuthor", "Text", NodeKind::Leaf, json!({"flexGrow": 1, "flexShrink": 1, "minWidth": 0}), json!({"text": author, "lines": 1, "truncate": "end"}), &suffix);
                    let _ = a;
                    self.part_in(cell, &owner, "blameAge", "Text", NodeKind::Leaf, json!({"flexShrink": 0, "textAlign": "end"}), json!({"text": age, "lines": 1}), &suffix);
                }
                if at.border {
                    self.nodes[row as usize].props.insert("blameBorder".into(), Value::Bool(true));
                    // In the query too: the style memo keys on it.
                    self.query_prop(row, "blameBorder", Value::Bool(true));
                }
            }
            if numbers {
                // Round 2: the gutter's padding comes from `CodeBlock/gutter`
                // (margins of the number, resolved at restyle); `digits`
                // lets a painter size every number like the widest one.
                let n = self.part_in(row, &owner, "lineNumber", "Text", NodeKind::Leaf, json!({"flexShrink": 0, "textAlign": "end"}), json!({"text": (i + 1).to_string(), "lines": 1, "digits": digits, "line": i + 1}), &suffix);
                // #EXP-2204: an anchored number is a link (`Line {line}`)
                // pressing selects its line (Shift extends: `select`).
                if anchors {
                    self.nodes[n as usize].pressable = true;
                    let label = self.string_with("lineAnchor", &[("line", &(i + 1).to_string())]);
                    self.default_a11y(n, "role", json!("link"));
                    self.default_a11y(n, "label", json!(label));
                }
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

    /// #EXP-2204: whether the node's style decides its height (a `height`,
    /// or a flex that fills).
    fn sized_by_style(&self, index: u32) -> bool {
        let st = &self.nodes[index as usize].base_style;
        st.contains_key("height") || st.contains_key("flexBasis") || st.get("flexGrow").and_then(Value::as_f64).is_some_and(|g| g > 0.0)
    }

    /// #EXP-2204 LogLines: ONE leaf the platform paints (toolbar + the
    /// windowed rows of `ci_log::log_rows`). The state reaches the painter
    /// resolved: `open` (default `log_open_default`), `query` (""),
    /// `follow` (true), `timestamps` (false), `maxLines` (24). Its height =
    /// the toolbar (`$control.input`) + `$spacing.sm` + maxLines × the code
    /// line height, unless the node style sets a height or flex.
    pub(crate) fn log_lines(&mut self, index: u32) {
        let props = self.nodes[index as usize].props.clone();
        let model = crate::ci_log::log_model(props.get("lines"), props.get("sections"));
        let open: Vec<usize> = match props.get("open") {
            Some(v @ Value::Array(_)) => crate::ci_log::open_set(Some(v), &model),
            _ => crate::ci_log::log_open_default(&model),
        };
        let max_lines = props.get("maxLines").and_then(Value::as_f64).filter(|m| m.is_finite() && *m >= 1.0).map_or(crate::ci_log::LOG_MAX_LINES, |m| m.floor() as f32);
        {
            let p = &mut self.nodes[index as usize].props;
            p.insert("open".into(), json!(open));
            p.insert("query".into(), json!(props.get("query").and_then(Value::as_str).unwrap_or("")));
            p.insert("follow".into(), json!(props.get("follow").and_then(Value::as_bool).unwrap_or(true)));
            p.insert("timestamps".into(), json!(props.get("timestamps").and_then(Value::as_bool).unwrap_or(false)));
            p.insert("maxLines".into(), json!(max_lines));
            p.insert("lineCount".into(), json!(model.lines.len()));
        }
        let label = self.string("logView");
        self.default_a11y(index, "label", json!(label));
        let mut defaults = vec![("minWidth", json!(0))];
        if !self.sized_by_style(index) {
            let sm = (self.ctx.gap_px)("sm");
            defaults.push(("height", json!(log_lines_height(self.ctx.control_input, sm, max_lines, sm + self.ctx.hairline))));
        }
        self.style_default(index, &defaults);
    }

    /// #EXP-2204 JobGraph: ONE leaf the platform paints from
    /// `job_graph::job_graph_layout`; `compact` (the surface below `md`,
    /// a breakpoint change rebuilds) = the status list. Its height = the
    /// layout's (list rows style.json `jobGraph.rowHeight` tall, rows ≥ 40), unless the node style sets
    /// a height or flex.
    pub(crate) fn job_graph(&mut self, index: u32) {
        self.responsive = true;
        let compact = !self.ctx.wide;
        let props = self.nodes[index as usize].props.clone();
        let geometry = crate::job_graph::JOB_GRAPH;
        let layout = crate::job_graph::job_graph_layout(props.get("jobs"), compact, &geometry);
        self.nodes[index as usize].props.insert("compact".into(), json!(compact));
        let label = self.string("jobGraph");
        self.default_a11y(index, "label", json!(label));
        let mut defaults = vec![("minWidth", json!(0))];
        if !self.sized_by_style(index) {
            defaults.push(("height", json!(layout.height)));
        }
        self.style_default(index, &defaults);
    }

    /// Chart: one leaf; `props.chart` carries the shared numbers.
    pub(crate) fn chart(&mut self, index: u32) {
        let props = self.nodes[index as usize].props.clone();
        let kind = str_prop(&props, "kind").unwrap_or("bar").to_string();
        // FEED-205: `rows` + `categoryField` + series `field`s resolve here
        // into plain `categories` / `series[].values`, which painters read.
        let (categories, series) = chart_data(&props);
        if props.get("rows").is_some_and(Value::is_array) {
            let node = &mut self.nodes[index as usize].props;
            node.insert("categories".into(), json!(categories));
            let n = crate::json::number;
            node.insert("series".into(), Value::Array(series.iter().map(|s| json!({"name": s.name, "values": s.values.iter().map(|v| if v.is_finite() { n(*v) } else { Value::Null }).collect::<Vec<_>>(), "tone": s.tone})).collect()));
        }
        let extent = chart_extent(&kind, &series, props.get("min").and_then(Value::as_f64), props.get("max").and_then(Value::as_f64));
        let ticks = nice_ticks(extent.min, extent.max, 5);
        let slices = matches!(kind.as_str(), "pie" | "donut");
        let count = if slices { series.first().map(|s| s.values.len()).unwrap_or(0) } else { series.len() };
        let colors: Vec<String> = (0..count).map(|i| series_color(i, if slices { None } else { series[i].tone.as_deref() })).collect();
        let legend = props.get("showLegend").and_then(Value::as_bool).unwrap_or(true) && count >= 2 && kind != "sparkline";
        let n = crate::json::number;
        // Round 2 §3: tick and value labels through the surface formatter
        // in the chart's `valueFormat` (FEED-205; default the formatter's
        // number, 0..3 digits), like the reference's `formatChartValue`.
        // `valueLabels[s][i]` = series s, value i (the tooltip's too).
        let value_format = ValueFormat::of(props.get("valueFormat"));
        let fmt = |v: f64| if v.is_finite() { format_chart_value(self.ctx.formatter, v, &value_format) } else { String::new() };
        let labels: Vec<String> = ticks.ticks.iter().map(|t| fmt(*t)).collect();
        let value_labels: Vec<Vec<String>> = series.iter().map(|s| s.values.iter().map(|v| fmt(*v)).collect()).collect();
        let chart = json!({
            "min": n(extent.min), "max": n(extent.max),
            "ticks": ticks.ticks.iter().map(|t| n(*t)).collect::<Vec<_>>(), "step": n(ticks.step),
            "tickLabels": labels,
            "valueLabels": value_labels,
            "colors": colors,
            "hole": if kind == "donut" { DONUT_HOLE } else { 0.0 },
            "legend": legend,
        });
        self.nodes[index as usize].props.insert("chart".into(), chart);
        let h = props.get("height").and_then(Value::as_f64).unwrap_or(200.0);
        self.nodes[index as usize].base_style.entry("height".to_string()).or_insert(json!(h));
        // Round 2 §7: a Chart's min-content width is 0 — it shrinks in a
        // crowded row even with a `width` (taffy takes a leaf's own width as
        // its min-content, CSS does not).
        self.nodes[index as usize].base_style.entry("minWidth".to_string()).or_insert(json!(0));
        let _ = js;
    }
}

/// #EXP-2204: a LogLines leaf's default height: the toolbar, the gap, then
/// the body: `max_lines` code lines inside its `body_inset` (the `body`
/// recipe's paddingVertical `$spacing.sm` + its hairline border, per edge),
/// as the web's body is maxLines lines plus its padding.
pub fn log_lines_height(toolbar: f32, gap: f32, max_lines: f32, body_inset: f32) -> f32 {
    toolbar + gap + max_lines * CODE_LINE_HEIGHT + 2.0 * body_inset
}
