//! `Chart`: bar / stackedBar / line / area / pie / donut / sparkline drawn
//! with gpui paths and quads from the numbers the core put in
//! `props.chart` (extent, nice ticks, the series colours resolved into the
//! visual, the donut hole), so every painter draws the same axes: grid
//! lines and y labels at the ticks, category labels, optional value labels,
//! the axis titles, a legend (2+ series or slices) and a tooltip for the
//! category (or slice) under the pointer. The plot fills the leaf's
//! frame (whose height the core fixed) minus the title and legend rows.

use std::cell::Cell;
use std::f32::consts::PI;
use std::rc::Rc;

use gpui::{canvas, div, fill, point, prelude::*, px, size, AnyElement, App, Bounds, Hsla, PathBuilder, Pixels, SharedString, Window};
use serde_json::Value;

use super::color::color_of;
use super::natives::{styled_box, LeafCx};
use super::parts::{px_prop, spacing, theme_color};
use super::PaintStyle;
use crate::measure::{chart_legend, display_text};

fn num(v: &Value) -> f32 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0) as f32,
        Value::String(s) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// A tick / value label: integers plain, else up to 2 decimals.
pub fn fmt_value(v: f32) -> String {
    if (v - v.round()).abs() < 1e-4 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{v:.2}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// The category under pointer x in a plot `width` wide with `n` slots.
pub fn category_at(x: f32, width: f32, n: usize) -> Option<usize> {
    if n == 0 || width <= 0.0 || x < 0.0 || x > width {
        return None;
    }
    Some(((x / width) * n as f32).floor().min(n as f32 - 1.0) as usize)
}

/// The slice under a point (relative to the centre) of a pie whose values
/// are `values` (drawn clockwise from 12 o'clock).
pub fn slice_at(dx: f32, dy: f32, r: f32, hole: f32, values: &[f32]) -> Option<usize> {
    let d = (dx * dx + dy * dy).sqrt();
    if d > r || d < hole * r {
        return None;
    }
    let total: f32 = values.iter().sum();
    if total <= 0.0 {
        return None;
    }
    let mut a = dy.atan2(dx) + PI / 2.0;
    if a < 0.0 {
        a += 2.0 * PI;
    }
    let mut acc = 0.0;
    for (i, v) in values.iter().enumerate() {
        acc += 2.0 * PI * v / total;
        if a <= acc {
            return Some(i);
        }
    }
    values.len().checked_sub(1)
}

struct Series {
    name: String,
    values: Vec<f32>,
    color: Hsla,
}

/// The fallback palette colour of series `i` (the theme's chart1..8).
fn palette(cx: &LeafCx, i: usize) -> Hsla {
    cx.style.series.get(i).copied().or_else(|| theme_color(cx.theme, cx.mode, &format!("chart{}", i % 8 + 1))).unwrap_or_else(|| super::color::hsl((i as f32 * 67.0) % 360.0, 0.6, 0.55, 1.0))
}

/// How many categories (slices for pie / donut) the keyboard steps over.
pub fn point_count(props: &serde_json::Map<String, Value>) -> usize {
    let series = props.get("series").and_then(Value::as_array).cloned().unwrap_or_default();
    let longest = series.iter().map(|s| s.get("values").and_then(Value::as_array).map_or(0, Vec::len)).max().unwrap_or(0);
    match props.get("kind").and_then(Value::as_str) {
        Some("pie" | "donut") => series.first().and_then(|s| s.get("values")).and_then(Value::as_array).map_or(0, Vec::len),
        _ => props.get("categories").and_then(Value::as_array).map_or(0, Vec::len).max(longest),
    }
}

/// The tooltip index after a key (`ArrowLeft/Right` mirrored by the caller,
/// `Home`/`End`), wrapping like the React renderer; `None` = not a chart key.
pub fn step_point(current: Option<usize>, n: usize, delta: Option<i64>, key: &str) -> Option<Option<usize>> {
    if n == 0 {
        return None;
    }
    match key {
        "home" => Some(Some(0)),
        "end" => Some(Some(n - 1)),
        _ => {
            let d = delta?;
            Some(Some(match current {
                None if d > 0 => 0,
                None => n - 1,
                Some(a) => ((a as i64 + d).rem_euclid(n as i64)) as usize,
            }))
        }
    }
}

/// Paint a chart leaf; `hovered` = the category / slice under the pointer.
pub fn paint(cx: &LeafCx, hovered: Option<usize>, on_hover: impl Fn(&Option<usize>, &mut Window, &mut App) + 'static) -> AnyElement {
    let props = &cx.node.props;
    let kind = match cx.str("kind") {
        "" => "bar".to_string(),
        k => k.to_string(),
    };
    let chart = props.get("chart").cloned().unwrap_or(Value::Null);
    let categories: Vec<String> = props.get("categories").and_then(Value::as_array).map(|a| a.iter().map(|v| display_text(Some(v))).collect()).unwrap_or_default();
    let raw = props.get("series").and_then(Value::as_array).cloned().unwrap_or_default();
    let slices = matches!(kind.as_str(), "pie" | "donut");
    let series: Vec<Series> = raw
        .iter()
        .enumerate()
        .map(|(i, s)| Series {
            name: display_text(s.get("name")),
            values: s.get("values").and_then(Value::as_array).map(|a| a.iter().map(num).collect()).unwrap_or_default(),
            color: palette(cx, i),
        })
        .collect();
    let slice_values: Vec<f32> = series.first().map(|s| s.values.iter().map(|v| v.max(0.0)).collect()).unwrap_or_default();
    let slice_colors: Vec<Hsla> = (0..slice_values.len()).map(|i| palette(cx, i)).collect();
    let n_cat = categories.len().max(series.iter().map(|s| s.values.len()).max().unwrap_or(0)).max(1);
    let min = chart.get("min").map(num).unwrap_or(0.0);
    let max = chart.get("max").map(num).unwrap_or_else(|| series.iter().flat_map(|s| s.values.iter().copied()).fold(1.0, f32::max));
    let ticks: Vec<f32> = chart.get("ticks").and_then(Value::as_array).map(|a| a.iter().map(num).collect()).unwrap_or_else(|| vec![min, max]);
    let hole = chart.get("hole").map(num).unwrap_or(if kind == "donut" { 0.6 } else { 0.0 });
    let spark = kind == "sparkline";
    let show_axes = !spark && !slices && props.get("showAxes").and_then(Value::as_bool) != Some(false);
    let show_grid = !spark && !slices && props.get("showGrid").and_then(Value::as_bool) != Some(false);
    let show_values = props.get("showValues").and_then(Value::as_bool) == Some(true);
    let title = cx.str("title").to_string();
    let x_label = cx.str("xLabel").to_string();
    let y_label = cx.str("yLabel").to_string();
    let gap = spacing(cx.theme, "xs");
    let axis = cx.part_props("Chart", "axis", &[]);
    let axis_color = color_of(axis.get("color").and_then(Value::as_str)).or_else(|| cx.theme_color("mutedForeground")).unwrap_or(cx.ink.opacity(0.6));
    let axis_size = px_prop(&axis, "fontSize").unwrap_or(12.0).min(12.0);
    let grid = cx.part_props("Chart", "grid", &[]);
    let grid_color = color_of(grid.get("color").and_then(Value::as_str)).or_else(|| cx.theme_color("border")).unwrap_or(cx.ink.opacity(0.15));
    let legend_props = cx.part_props("Chart", "legend", &[]);
    let legend_color = color_of(legend_props.get("color").and_then(Value::as_str)).unwrap_or(axis_color);
    let legend_lh = px_prop(&legend_props, "lineHeight").unwrap_or(16.0);
    let title_props = cx.part_props("Chart", "title", &[]);
    let value_color = color_of(cx.part_props("Chart", "valueLabel", &[]).get("color").and_then(Value::as_str)).unwrap_or(cx.ink);
    let entries: Vec<(String, Hsla)> = chart_legend(props).into_iter().enumerate().map(|(i, name)| (name, if slices { slice_colors.get(i).copied().unwrap_or(grid_color) } else { series.get(i).map(|s| s.color).unwrap_or(grid_color) })).collect();

    let title_h = if title.is_empty() { 0.0 } else { cx.text_style.line_height + gap };
    let legend_h = if entries.is_empty() { 0.0 } else { legend_lh + gap };
    let plot_h = (cx.h - title_h - legend_h).max(1.0);
    let width = cx.w;
    // Gutters for the axes.
    let y_label_w = if show_axes { ticks.iter().map(|t| fmt_value(*t).chars().count()).max().unwrap_or(1) as f32 * axis_size * 0.62 + 8.0 } else { 0.0 };
    let (pl, pr, pt) = if spark { (1.0, 1.0, 2.0) } else { (y_label_w.max(4.0), 8.0, if show_values || !y_label.is_empty() { 16.0 } else { 8.0 }) };
    let pb = if spark || slices { 2.0 } else if show_axes { 20.0 + if x_label.is_empty() { 0.0 } else { 14.0 } } else { 8.0 };
    let inner_w = (width - pl - pr).max(1.0);
    let inner_h = (plot_h - pt - pb).max(1.0);
    let span = (max - min).max(f32::EPSILON);
    let x_of = move |i: usize| pl + inner_w * (i as f32 + 0.5) / n_cat as f32;
    let y_of = move |v: f32| pt + inner_h - inner_h * (v - min) / span;

    let mut col = div().size_full().flex().flex_col().gap(px(gap));
    if !title.is_empty() {
        let weight = title_props.get("fontWeight").and_then(Value::as_u64).unwrap_or(600) as f32;
        col = col.child(div().h(px(cx.text_style.line_height)).font_weight(gpui::FontWeight(weight)).truncate().child(SharedString::from(title)));
    }
    let kind_c = kind.clone();
    let series_c: Vec<(Vec<f32>, Hsla)> = series.iter().map(|s| (s.values.clone(), s.color)).collect();
    let ticks_c = ticks.clone();
    let slice_c = slice_values.clone();
    let slice_colors_c = slice_colors.clone();
    let hover_band = cx.ink.opacity(0.06);
    let bounds_cell: Rc<Cell<Option<Bounds<Pixels>>>> = Rc::new(Cell::new(None));
    let probe = bounds_cell.clone();
    let plot_canvas = canvas(
        move |b, _, _| probe.set(Some(b)),
        move |bounds: Bounds<Pixels>, _, window, _| {
            let o = bounds.origin;
            let at = |x: f32, y: f32| point(o.x + px(x), o.y + px(y));
            if show_grid {
                for t in &ticks_c {
                    let y = y_of(*t).floor();
                    window.paint_quad(fill(Bounds::new(at(pl, y), size(px(inner_w), px(1.0))), grid_color));
                }
            }
            if let (Some(h), false) = (hovered, matches!(kind_c.as_str(), "pie" | "donut" | "sparkline")) {
                let slot = inner_w / n_cat as f32;
                window.paint_quad(fill(Bounds::new(at(pl + slot * h as f32, pt), size(px(slot), px(inner_h))), hover_band));
            }
            match kind_c.as_str() {
                "pie" | "donut" => {
                    let total: f32 = slice_c.iter().sum::<f32>().max(f32::EPSILON);
                    let (cx0, cy0) = (width / 2.0, plot_h / 2.0);
                    let r = inner_w.min(inner_h) / 2.0;
                    let mut angle = -PI / 2.0;
                    for (i, v) in slice_c.iter().enumerate() {
                        let sweep = 2.0 * PI * v / total;
                        let steps = ((sweep / (2.0 * PI)) * 96.0).ceil().max(2.0) as usize;
                        let grow = if hovered == Some(i) { 3.0 } else { 0.0 };
                        let mut b = PathBuilder::fill();
                        let inner_r = hole * r;
                        if inner_r > 0.0 {
                            b.move_to(at(cx0 + inner_r * angle.cos(), cy0 + inner_r * angle.sin()));
                        } else {
                            b.move_to(at(cx0, cy0));
                        }
                        for k in 0..=steps {
                            let a = angle + sweep * k as f32 / steps as f32;
                            b.line_to(at(cx0 + (r + grow) * a.cos(), cy0 + (r + grow) * a.sin()));
                        }
                        if inner_r > 0.0 {
                            for k in (0..=steps).rev() {
                                let a = angle + sweep * k as f32 / steps as f32;
                                b.line_to(at(cx0 + inner_r * a.cos(), cy0 + inner_r * a.sin()));
                            }
                        }
                        b.close();
                        if let Ok(path) = b.build() {
                            window.paint_path(path, slice_colors_c.get(i).copied().unwrap_or(grid_color));
                        }
                        angle += sweep;
                    }
                }
                "line" | "area" | "sparkline" => {
                    for (values, color) in &series_c {
                        if values.is_empty() {
                            continue;
                        }
                        let pts: Vec<_> = values.iter().enumerate().map(|(i, v)| at(x_of(i), y_of(*v))).collect();
                        if kind_c == "area" {
                            let base = y_of(min.max(0.0).min(max));
                            let mut b = PathBuilder::fill();
                            b.move_to(at(x_of(0), base));
                            for p in &pts {
                                b.line_to(*p);
                            }
                            b.line_to(at(x_of(pts.len() - 1), base));
                            b.close();
                            if let Ok(path) = b.build() {
                                window.paint_path(path, color.opacity(0.2));
                            }
                        }
                        if pts.len() > 1 {
                            let mut b = PathBuilder::stroke(px(if kind_c == "sparkline" { 1.5 } else { 2.0 }));
                            b.move_to(pts[0]);
                            for p in &pts[1..] {
                                b.line_to(*p);
                            }
                            if let Ok(path) = b.build() {
                                window.paint_path(path, *color);
                            }
                        } else {
                            window.paint_quad(fill(Bounds::new(point(pts[0].x - px(2.0), pts[0].y - px(2.0)), size(px(4.0), px(4.0))), *color).corner_radii(px(2.0)));
                        }
                        if let Some(h) = hovered.filter(|_| kind_c != "sparkline") {
                            if let Some(p) = pts.get(h) {
                                window.paint_quad(fill(Bounds::new(point(p.x - px(3.5), p.y - px(3.5)), size(px(7.0), px(7.0))), *color).corner_radii(px(3.5)));
                            }
                        }
                    }
                }
                "stackedBar" => {
                    let slot = inner_w / n_cat as f32;
                    let bw = slot * 0.6;
                    for i in 0..n_cat {
                        let mut acc = 0.0f32;
                        for (values, color) in &series_c {
                            let v = values.get(i).copied().unwrap_or(0.0).max(0.0);
                            let (y0, y1) = (y_of(acc), y_of(acc + v));
                            acc += v;
                            window.paint_quad(fill(Bounds::new(at(pl + slot * i as f32 + slot * 0.2, y1), size(px(bw), px((y0 - y1).max(0.0)))), *color));
                        }
                    }
                }
                _ => {
                    let slot = inner_w / n_cat as f32;
                    let bw = slot * 0.7 / series_c.len().max(1) as f32;
                    let zero = y_of(0.0f32.clamp(min, max));
                    for (si, (values, color)) in series_c.iter().enumerate() {
                        for (i, v) in values.iter().enumerate() {
                            let y = y_of(*v);
                            let (top, h) = if y <= zero { (y, zero - y) } else { (zero, y - zero) };
                            let x = pl + slot * i as f32 + slot * 0.15 + bw * si as f32;
                            window.paint_quad(fill(Bounds::new(at(x, top), size(px(bw), px(h.max(0.0)))), *color).corner_radii(px(2.0)));
                        }
                    }
                }
            }
        },
    )
    .absolute()
    .size_full();
    let mut plot = div().id(SharedString::from(format!("{}.plot", cx.node.id))).relative().w_full().h(px(plot_h)).flex_none().child(plot_canvas);
    // Hover → the category (or slice) under the pointer.
    let on_hover = Rc::new(on_hover);
    let (hover_move, hover_leave) = (on_hover.clone(), on_hover);
    let probe = bounds_cell;
    let kind_h = kind.clone();
    let slice_h = slice_values.clone();
    plot = plot
        .on_mouse_move(move |ev, window, cx| {
            let Some(b) = probe.get() else { return };
            let (x, y) = (f32::from(ev.position.x - b.origin.x), f32::from(ev.position.y - b.origin.y));
            let hit = match kind_h.as_str() {
                "pie" | "donut" => slice_at(x - width / 2.0, y - plot_h / 2.0, inner_w.min(inner_h) / 2.0, hole, &slice_h),
                "sparkline" => None,
                _ => category_at(x - pl, inner_w, n_cat),
            };
            if hit != hovered {
                hover_move(&hit, window, cx);
            }
        })
        .on_hover(move |inside: &bool, window, cx| {
            if !*inside {
                hover_leave(&None, window, cx);
            }
        });
    if show_axes {
        for t in &ticks {
            let y = y_of(*t);
            plot = plot.child(
                div().absolute().left_0().top(px(y - 7.0)).w(px(pl - 6.0)).h(px(14.0)).text_right().text_size(px(axis_size)).line_height(px(14.0)).text_color(axis_color).whitespace_nowrap().child(SharedString::from(fmt_value(*t))),
            );
        }
        let slot = inner_w / n_cat as f32;
        for (i, c) in categories.iter().enumerate() {
            plot = plot.child(div().absolute().left(px(x_of(i) - slot / 2.0)).top(px(pt + inner_h + 4.0)).w(px(slot)).text_center().text_size(px(axis_size)).line_height(px(14.0)).text_color(axis_color).truncate().child(SharedString::from(c.clone())));
        }
        if !x_label.is_empty() {
            plot = plot.child(div().absolute().left(px(pl)).top(px(pt + inner_h + 20.0)).w(px(inner_w)).text_center().text_size(px(axis_size)).line_height(px(14.0)).text_color(axis_color).truncate().child(SharedString::from(x_label)));
        }
        if !y_label.is_empty() {
            plot = plot.child(div().absolute().left_0().top_0().text_size(px(axis_size)).line_height(px(14.0)).text_color(axis_color).whitespace_nowrap().child(SharedString::from(y_label)));
        }
    }
    if show_values && !slices && !spark {
        let slot = inner_w / n_cat as f32;
        for (si, s) in series.iter().enumerate() {
            let bw = slot * 0.7 / series.len().max(1) as f32;
            for (i, v) in s.values.iter().enumerate() {
                let x = if kind == "bar" { pl + slot * i as f32 + slot * 0.15 + bw * si as f32 + bw / 2.0 } else { x_of(i) };
                plot = plot.child(div().absolute().left(px(x - 20.0)).top(px(y_of(*v) - 15.0)).w(px(40.0)).text_center().text_size(px(axis_size)).line_height(px(14.0)).text_color(value_color).child(SharedString::from(fmt_value(*v))));
            }
        }
    }
    // The tooltip for the hovered category / slice.
    if let Some(h) = hovered {
        let tip_props = cx.part_props("Chart", "tooltip", &[]);
        let tip = PaintStyle::from_visual(&exponential_ui::style::visual(&tip_props, exponential_ui::style::BoxKind::Leaf));
        let tip_fg = tip.color.unwrap_or(cx.ink);
        let fs = px_prop(&tip_props, "fontSize").unwrap_or(12.0);
        let pad = px_prop(&tip_props, "padding").unwrap_or(8.0);
        let bg = tip.bg.or_else(|| cx.theme_color("popover")).unwrap_or(gpui::black());
        let tip = PaintStyle { bg: Some(bg), ..tip };
        let mut body = div().flex().flex_col().gap(px(2.0)).text_size(px(fs)).line_height(px(fs + 4.0)).text_color(tip_fg).whitespace_nowrap();
        let (anchor_x, lines): (f32, Vec<(String, Hsla, String)>) = if slices {
            let label = categories.get(h).cloned().unwrap_or_default();
            (width / 2.0, vec![(label, slice_colors.get(h).copied().unwrap_or(grid_color), slice_values.get(h).map(|v| fmt_value(*v)).unwrap_or_default())])
        } else {
            let label = categories.get(h).cloned().unwrap_or_default();
            body = body.child(div().font_weight(gpui::FontWeight(600.0)).child(SharedString::from(label)));
            (x_of(h), series.iter().map(|s| (s.name.clone(), s.color, s.values.get(h).map(|v| fmt_value(*v)).unwrap_or_default())).collect())
        };
        for (name, color, value) in lines {
            body = body.child(div().flex().flex_row().items_center().gap(px(6.0)).child(div().size(px(8.0)).rounded(px(2.0)).bg(color)).child(SharedString::from(if name.is_empty() { value } else { format!("{name}: {value}") })));
        }
        let left = (anchor_x + 8.0).min((width - 120.0).max(0.0));
        plot = plot.child(styled_box(div().absolute().left(px(left)).top(px(pt)).p(px(pad)), &tip, 120.0, 40.0).child(body));
    }
    col = col.child(plot);
    if !entries.is_empty() {
        let mut row = div().flex().flex_row().flex_wrap().gap(px(spacing(cx.theme, "sm"))).h(px(legend_lh)).text_size(px(axis_size)).line_height(px(legend_lh)).text_color(legend_color);
        for (name, color) in entries {
            row = row.child(div().flex().flex_row().items_center().gap(px(4.0)).child(div().size(px(8.0)).rounded(px(2.0)).bg(color)).child(SharedString::from(name)));
        }
        col = col.child(row);
    }
    col.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keyboard_steps_through_the_points() {
        let props = serde_json::json!({"kind": "bar", "categories": ["a", "b", "c"], "series": [{"values": [1, 2, 3]}]});
        let n = point_count(props.as_object().unwrap());
        assert_eq!(n, 3);
        assert_eq!(step_point(None, n, Some(1), "right"), Some(Some(0)));
        assert_eq!(step_point(None, n, Some(-1), "left"), Some(Some(2)));
        assert_eq!(step_point(Some(2), n, Some(1), "right"), Some(Some(0)));
        assert_eq!(step_point(Some(1), n, None, "home"), Some(Some(0)));
        assert_eq!(step_point(Some(1), n, None, "end"), Some(Some(2)));
        assert_eq!(step_point(Some(1), n, None, "up"), None);
        let pie = serde_json::json!({"kind": "pie", "categories": ["a", "b"], "series": [{"values": [1, 2, 3, 4]}]});
        assert_eq!(point_count(pie.as_object().unwrap()), 4);
    }

    #[test]
    fn values_and_hits() {
        assert_eq!(fmt_value(3.0), "3");
        assert_eq!(fmt_value(2.5), "2.5");
        assert_eq!(fmt_value(0.126), "0.13");
        assert_eq!(category_at(0.0, 100.0, 4), Some(0));
        assert_eq!(category_at(99.0, 100.0, 4), Some(3));
        assert_eq!(category_at(-1.0, 100.0, 4), None);
        // A pie of two equal halves: right half = slice 0 (clockwise from 12).
        assert_eq!(slice_at(10.0, 0.0, 50.0, 0.0, &[1.0, 1.0]), Some(0));
        assert_eq!(slice_at(-10.0, 0.0, 50.0, 0.0, &[1.0, 1.0]), Some(1));
        assert_eq!(slice_at(10.0, 0.0, 50.0, 0.6, &[1.0, 1.0]), None, "inside the donut hole");
        assert_eq!(slice_at(100.0, 0.0, 50.0, 0.0, &[1.0, 1.0]), None);
    }
}
