//! Round 1 (docs/round-1-contract.md §3, Chart): the numbers every painter
//! must agree on — the value extent per kind and the "nice" axis ticks
//! (Heckbert's nice numbers), the series colour order and the donut hole.
//! Pure; geometry (bars, arcs) stays each painter's. Mirrors `src/chart.ts`.

use crate::format::{fixed_digits, Formatter, NumberOptions};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One series as the painter reads it off the Chart props.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ChartSeries {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub values: Vec<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Extent {
    pub min: f64,
    pub max: f64,
}

/// The value range the y axis spans: `min` defaults to 0 (lower when a value
/// is negative), `max` to the largest value — the largest category SUM for
/// `stackedBar`; `sparkline` spans its own min..max.
pub fn chart_extent(kind: &str, series: &[ChartSeries], min: Option<f64>, max: Option<f64>) -> Extent {
    let values: Vec<f64> = series.iter().flat_map(|s| s.values.iter().copied().filter(|v| v.is_finite())).collect();
    let mut lo = if kind == "sparkline" { values.iter().copied().fold(f64::INFINITY, f64::min) } else { values.iter().copied().fold(0.0, f64::min) };
    let mut hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if kind == "stackedBar" {
        let n = series.iter().map(|s| s.values.len()).max().unwrap_or(0);
        hi = f64::NEG_INFINITY;
        for i in 0..n {
            let sum: f64 = series.iter().map(|s| s.values.get(i).copied().filter(|v| v.is_finite()).map(|v| v.max(0.0)).unwrap_or(0.0)).sum();
            hi = hi.max(sum);
        }
    }
    if !lo.is_finite() {
        lo = 0.0;
    }
    if !hi.is_finite() {
        hi = 0.0;
    }
    let lo = min.unwrap_or(lo);
    let mut hi = max.unwrap_or(hi);
    if hi <= lo {
        hi = lo + 1.0;
    }
    Extent { min: lo, max: hi }
}

fn nice_num(x: f64, round: bool) -> f64 {
    let exp = x.log10().floor();
    let f = x / 10f64.powf(exp);
    let nf = if round {
        if f < 1.5 {
            1.0
        } else if f < 3.0 {
            2.0
        } else if f < 7.0 {
            5.0
        } else {
            10.0
        }
    } else if f <= 1.0 {
        1.0
    } else if f <= 2.0 {
        2.0
    } else if f <= 5.0 {
        5.0
    } else {
        10.0
    };
    nf * 10f64.powf(exp)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ticks {
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub ticks: Vec<f64>,
}

/// JS `Number(v.toFixed(decimals))`.
fn fix(v: f64, decimals: i32) -> f64 {
    let s = format!("{:.*}", decimals.max(0) as usize, v);
    s.parse().unwrap_or(v)
}

/// Axis ticks over [min, max]: a nice step (1, 2 or 5 × 10^n) giving about
/// `target` ticks, the range widened to whole steps, every tick rounded to
/// the step's decimals.
pub fn nice_ticks(min: f64, max: f64, target: usize) -> Ticks {
    let lo = min.min(max);
    let hi = if max == min { min + 1.0 } else { min.max(max) };
    let range = nice_num(hi - lo, false);
    let step = nice_num(range / (target.max(2) - 1) as f64, true);
    let decimals = (-step.log10().floor()).max(0.0) as i32;
    let nice_min = fix((lo / step).floor() * step, decimals);
    let nice_max = fix((hi / step).ceil() * step, decimals);
    let mut ticks = Vec::new();
    let mut i = 0;
    let mut v = nice_min;
    while v <= nice_max + step / 2.0 && i < 100 {
        ticks.push(fix(v, decimals));
        i += 1;
        v = nice_min + i as f64 * step;
    }
    Ticks { min: nice_min, max: nice_max, step, ticks }
}

/// The colour token of series (or slice) `index`: its tone's colour when
/// set, else `$color.chart1..8` in order, wrapping.
pub fn series_color(index: usize, tone: Option<&str>) -> String {
    match tone {
        Some("danger") => "$color.destructive".into(),
        Some("neutral") => "$color.mutedForeground".into(),
        Some(t) => format!("$color.{t}"),
        None => format!("$color.chart{}", index % 8 + 1),
    }
}

/// FEED-205: `Chart.valueFormat` (`src/chart.ts` `ChartValueFormat`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ValueFormat {
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decimals: Option<f64>,
}

impl ValueFormat {
    /// Read off the prop; anything that is not an object = the default.
    pub fn of(v: Option<&Value>) -> ValueFormat {
        v.and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default()
    }
}

/// A chart value through the surface formatter, as a Table cell of the
/// same type (`src/chart.ts` `formatChartValue`): `number` (default),
/// `currency` (`currency`, default USD; not 3 letters = a number),
/// `percent` (a ratio), `compact` ([`Formatter::compact`]; with a valid
/// `currency` a compact currency); `decimals` fixes the fraction digits.
pub fn format_chart_value(formatter: &dyn Formatter, value: f64, format: &ValueFormat) -> String {
    let decimals = format.decimals.and_then(fixed_digits);
    let code = match &format.currency {
        None | Some(Value::Null) => Some("USD".to_string()),
        Some(Value::String(c)) if c.len() == 3 && c.bytes().all(|b| b.is_ascii_alphabetic()) => Some(c.to_uppercase()),
        Some(_) => None,
    };
    let options = NumberOptions { decimals, grouping: true };
    match format.kind.as_deref() {
        Some("currency") => match &code {
            Some(code) => formatter.currency(value, code, options),
            None => formatter.number(value, options),
        },
        Some("percent") => formatter.percent(value, decimals),
        Some("compact") => formatter.compact(value, if matches!(format.currency, Some(Value::String(_))) { code.as_deref() } else { None }, decimals),
        _ => formatter.number(value, options),
    }
}

/// FEED-205: a Chart's categories and series over `rows` (`src/chart.ts`
/// `chartData`): with `rows`, the categories are each row's
/// `categoryField` and a series with a `field` takes that field of every
/// row (a numeric string counts, anything else is NaN = no point); without,
/// the props as written.
pub fn chart_data(props: &serde_json::Map<String, Value>) -> (Vec<String>, Vec<ChartSeries>) {
    let rows = props.get("rows").and_then(Value::as_array);
    let field = props.get("categoryField").and_then(Value::as_str).unwrap_or("");
    let show = |v: Option<&Value>| match v {
        Some(Value::String(s)) => s.clone(),
        Some(v @ (Value::Number(_) | Value::Bool(_))) => crate::format::display_string(v),
        _ => String::new(),
    };
    let num = crate::format::to_num;
    let categories = match rows {
        Some(rows) if !field.is_empty() => rows.iter().map(|r| show(r.get(field))).collect(),
        _ => props.get("categories").and_then(Value::as_array).map(|a| a.iter().map(|c| show(Some(c))).collect()).unwrap_or_default(),
    };
    let series = props
        .get("series")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|s| {
                    let values = match (rows, s.get("field").and_then(Value::as_str)) {
                        (Some(rows), Some(f)) => rows.iter().map(|r| num(r.get(f))).collect(),
                        _ => s.get("values").and_then(Value::as_array).map(|v| v.iter().map(|v| num(Some(v))).collect()).unwrap_or_default(),
                    };
                    ChartSeries { name: show(s.get("name")), values, tone: s.get("tone").and_then(Value::as_str).map(str::to_string) }
                })
                .collect()
        })
        .unwrap_or_default();
    (categories, series)
}

/// The donut's hole as a share of its radius (pie = 0).
pub const DONUT_HOLE: f64 = 0.6;
/// Rows past which Table and List window their children (`layout.json`).
pub const WINDOW_THRESHOLD: usize = crate::list::WINDOW_THRESHOLD;

#[cfg(test)]
mod tests {
    use super::*;

    fn s(values: &[f64]) -> ChartSeries {
        ChartSeries { name: "s".into(), values: values.to_vec(), tone: None }
    }

    #[test]
    fn extents_per_kind() {
        assert_eq!(chart_extent("bar", &[s(&[3.0, 8.0])], None, None), Extent { min: 0.0, max: 8.0 });
        assert_eq!(chart_extent("bar", &[s(&[-2.0, 8.0])], None, None), Extent { min: -2.0, max: 8.0 });
        assert_eq!(chart_extent("stackedBar", &[s(&[3.0, 8.0]), s(&[4.0, 1.0])], None, None), Extent { min: 0.0, max: 9.0 });
        assert_eq!(chart_extent("sparkline", &[s(&[3.0, 8.0])], None, None), Extent { min: 3.0, max: 8.0 });
        assert_eq!(chart_extent("line", &[], None, None), Extent { min: 0.0, max: 1.0 });
        assert_eq!(chart_extent("line", &[s(&[3.0])], Some(10.0), None), Extent { min: 10.0, max: 11.0 });
    }

    #[test]
    fn nice_ticks_are_1_2_5_steps() {
        let t = nice_ticks(0.0, 8.0, 5);
        assert_eq!((t.min, t.max, t.step), (0.0, 8.0, 2.0));
        assert_eq!(t.ticks, vec![0.0, 2.0, 4.0, 6.0, 8.0]);
        let t = nice_ticks(0.0, 1.0, 5);
        assert_eq!(t.step, 0.2);
        assert_eq!(t.ticks, vec![0.0, 0.2, 0.4, 0.6, 0.8, 1.0]);
        let t = nice_ticks(-3.0, 97.0, 5);
        assert_eq!(t.ticks.first(), Some(&-20.0));
        assert_eq!(series_color(9, None), "$color.chart2");
        assert_eq!(series_color(0, Some("danger")), "$color.destructive");
    }

    #[test]
    fn rows_feed_categories_and_series() {
        let props = serde_json::json!({
            "rows": [{"month": "Jan", "income": 4200, "expenses": "3100"}, {"month": "Feb", "income": 3900, "expenses": null}],
            "categoryField": "month",
            "series": [{"name": "Income", "field": "income", "tone": "success"}, {"name": "Expenses", "field": "expenses"}]
        });
        let (categories, series) = chart_data(props.as_object().unwrap());
        assert_eq!(categories, vec!["Jan", "Feb"]);
        assert_eq!(series[0].values, vec![4200.0, 3900.0]);
        assert_eq!(series[0].tone.as_deref(), Some("success"));
        assert_eq!(series[1].values[0], 3100.0);
        assert!(series[1].values[1].is_nan());
        let plain = serde_json::json!({"categories": ["a"], "series": [{"name": "s", "values": [1]}]});
        assert_eq!(chart_data(plain.as_object().unwrap()), (vec!["a".to_string()], vec![ChartSeries { name: "s".into(), values: vec![1.0], tone: None }]));
    }

    #[test]
    fn value_formats() {
        let en = crate::format::EnglishFormatter;
        let vf = |v: serde_json::Value| ValueFormat::of(Some(&v));
        assert_eq!(format_chart_value(&en, 12000.0, &ValueFormat::default()), "12,000");
        assert_eq!(format_chart_value(&en, 12000.0, &vf(serde_json::json!({"type": "currency", "currency": "EUR", "decimals": 0}))), "€12,000");
        assert_eq!(format_chart_value(&en, 1234567.0, &vf(serde_json::json!({"type": "compact"}))), "1.2M");
        assert_eq!(format_chart_value(&en, 12000.0, &vf(serde_json::json!({"type": "compact", "currency": "eur"}))), "€12K");
        assert_eq!(format_chart_value(&en, 1.0, &vf(serde_json::json!("nonsense"))), "1");
    }
}
