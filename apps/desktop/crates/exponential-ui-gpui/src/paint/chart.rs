//! `Chart`: bar / line / area / pie over a grid with category labels and a
//! legend, the React renderer's geometry (a 320-wide viewBox stretched to the
//! frame, 8/8/8/20 padding) painted with gpui paths and quads.

use std::f32::consts::PI;

use gpui::{canvas, div, fill, point, prelude::*, px, size, AnyElement, Bounds, Hsla, PathBuilder, SharedString};
use serde_json::Value;

use super::color::color_of;
use super::natives::{chart_color, legend, LeafCx};
use super::parts::{px_prop, spacing};

fn num(v: &Value) -> f32 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0) as f32,
        Value::String(s) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

struct Series {
    values: Vec<f32>,
    color: Hsla,
}

/// Paint a chart leaf.
pub fn paint(cx: &LeafCx) -> AnyElement {
    let props = &cx.node.props;
    let kind = cx.str("kind").to_string();
    let kind = if kind.is_empty() { "bar".to_string() } else { kind };
    let categories: Vec<String> = props.get("categories").and_then(Value::as_array).map(|a| a.iter().map(|v| crate::measure::display_text(Some(v))).collect()).unwrap_or_default();
    let raw = props.get("series").and_then(Value::as_array).cloned().unwrap_or_default();
    let series: Vec<Series> = raw
        .iter()
        .enumerate()
        .map(|(i, s)| Series { values: s.get("values").and_then(Value::as_array).map(|a| a.iter().map(num).collect()).unwrap_or_default(), color: chart_color(cx, i, s.get("tone").and_then(Value::as_str)) })
        .collect();
    let pie_colors: Vec<Hsla> = (0..series.first().map(|s| s.values.len()).unwrap_or(0)).map(|i| chart_color(cx, i, None)).collect();
    let height = cx.num("height").unwrap_or(200.0) as f32;
    let title = cx.str("title").to_string();
    let gap = spacing(cx.theme, "xs");
    let axis = cx.part_props("Chart", "axis", &[]);
    let axis_color = color_of(axis.get("color").and_then(Value::as_str)).or_else(|| cx.theme_color("mutedForeground")).unwrap_or(cx.ink.opacity(0.6));
    let axis_size = px_prop(&axis, "fontSize").unwrap_or(10.0).min(12.0);
    let grid = cx.part_props("Chart", "grid", &[]);
    let grid_color = color_of(grid.get("color").and_then(Value::as_str)).or_else(|| cx.theme_color("border")).unwrap_or(cx.ink.opacity(0.15));
    let legend_props = cx.part_props("Chart", "legend", &[]);
    let legend_color = color_of(legend_props.get("color").and_then(Value::as_str)).unwrap_or(axis_color);
    let legend_lh = px_prop(&legend_props, "lineHeight").unwrap_or(16.0);
    let entries = legend(cx);

    let max = series.iter().flat_map(|s| s.values.iter().copied()).fold(1.0_f32, f32::max);
    let (pl, pr, pt, pb) = (8.0_f32, 8.0_f32, 8.0_f32, 20.0_f32);
    let width = cx.w;
    let inner_w = (width - pl - pr).max(1.0);
    let inner_h = (height - pt - pb).max(1.0);
    let n_cat = categories.len().max(1);
    let x_of = move |i: usize| pl + inner_w * (i as f32 + 0.5) / n_cat as f32;
    let y_of = move |v: f32| pt + inner_h - inner_h * v / max;

    let mut col = div().size_full().flex().flex_col().gap(px(gap));
    if !title.is_empty() {
        col = col.child(div().h(px(cx.text_style.line_height)).font_weight(gpui::FontWeight(500.0)).truncate().child(SharedString::from(title)));
    }
    let kind_c = kind.clone();
    let bars = canvas(
        |_, _, _| {},
        move |bounds: Bounds<gpui::Pixels>, _, window, _| {
            let o = bounds.origin;
            let at = |x: f32, y: f32| point(o.x + px(x), o.y + px(y));
            if kind_c != "pie" {
                for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
                    let y = y_of(max * t);
                    window.paint_quad(fill(Bounds::new(at(pl, y.floor()), size(px(inner_w), px(1.0))), grid_color));
                }
            }
            match kind_c.as_str() {
                "pie" => {
                    let values = series.first().map(|s| s.values.clone()).unwrap_or_default();
                    let total: f32 = values.iter().sum::<f32>().max(f32::EPSILON);
                    let (cx0, cy0) = (width / 2.0, height / 2.0);
                    let r = inner_w.min(inner_h) / 2.0;
                    let mut angle = -PI / 2.0;
                    for (i, v) in values.iter().enumerate() {
                        let sweep = 2.0 * PI * v / total;
                        let steps = ((sweep / (2.0 * PI)) * 64.0).ceil().max(2.0) as usize;
                        let mut b = PathBuilder::fill();
                        b.move_to(at(cx0, cy0));
                        for k in 0..=steps {
                            let a = angle + sweep * k as f32 / steps as f32;
                            b.line_to(at(cx0 + r * a.cos(), cy0 + r * a.sin()));
                        }
                        b.close();
                        if let Ok(path) = b.build() {
                            window.paint_path(path, pie_colors.get(i).copied().unwrap_or(grid_color));
                        }
                        angle += sweep;
                    }
                }
                "line" | "area" => {
                    for s in &series {
                        if s.values.is_empty() {
                            continue;
                        }
                        let pts: Vec<_> = s.values.iter().enumerate().map(|(i, v)| at(x_of(i), y_of(*v))).collect();
                        if kind_c == "area" {
                            let mut b = PathBuilder::fill();
                            b.move_to(pts[0]);
                            for p in &pts[1..] {
                                b.line_to(*p);
                            }
                            b.line_to(at(x_of(pts.len() - 1), y_of(0.0)));
                            b.line_to(at(x_of(0), y_of(0.0)));
                            b.close();
                            if let Ok(path) = b.build() {
                                window.paint_path(path, s.color.opacity(0.2));
                            }
                        }
                        if pts.len() > 1 {
                            let mut b = PathBuilder::stroke(px(2.0));
                            b.move_to(pts[0]);
                            for p in &pts[1..] {
                                b.line_to(*p);
                            }
                            if let Ok(path) = b.build() {
                                window.paint_path(path, s.color);
                            }
                        }
                    }
                }
                _ => {
                    let slot = inner_w / n_cat as f32;
                    let bw = slot * 0.7 / series.len().max(1) as f32;
                    for (si, s) in series.iter().enumerate() {
                        for (i, v) in s.values.iter().enumerate() {
                            let h = inner_h * v / max;
                            let x = pl + slot * i as f32 + slot * 0.15 + bw * si as f32;
                            window.paint_quad(fill(Bounds::new(at(x, pt + inner_h - h), size(px(bw), px(h))), s.color).corner_radii(px(2.0)));
                        }
                    }
                }
            }
        },
    )
    .absolute()
    .size_full();
    let mut plot = div().relative().w_full().h(px(height)).child(bars);
    if kind != "pie" {
        let slot = inner_w / n_cat as f32;
        for (i, c) in categories.iter().enumerate() {
            plot = plot.child(
                div()
                    .absolute()
                    .left(px(x_of(i) - slot / 2.0))
                    .top(px(height - pb + 4.0))
                    .w(px(slot))
                    .text_center()
                    .text_size(px(axis_size))
                    .line_height(px(14.0))
                    .text_color(axis_color)
                    .truncate()
                    .child(SharedString::from(c.clone())),
            );
        }
    }
    col = col.child(plot);
    if !entries.is_empty() {
        let mut row = div().flex().flex_row().flex_wrap().gap(px(spacing(cx.theme, "sm"))).h(px(legend_lh)).text_size(px(12.0)).line_height(px(legend_lh)).text_color(legend_color);
        for (name, color) in entries {
            row = row.child(div().flex().flex_row().items_center().gap(px(4.0)).child(div().size(px(8.0)).rounded(px(2.0)).bg(color)).child(SharedString::from(name)));
        }
        col = col.child(row);
    }
    col.into_any_element()
}
