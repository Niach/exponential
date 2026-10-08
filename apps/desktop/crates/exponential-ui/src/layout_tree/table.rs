//! Table (round 1, a native): typed columns over object rows, sorting
//! (bound `sort` = the host orders `rows`; unbound = a local sorted copy),
//! row selection (`multiple` = a checkbox column with a select-all header),
//! striped rows, slot cells with the ROW as their data scope, windowing past
//! `chart::WINDOW_THRESHOLD` (50) rows. Columns share the width (a fixed
//! `width` keeps its px), so header and rows line up as flex rows.

use std::cmp::Ordering;

use serde_json::{json, Value};

use super::{bool_prop, js, str_prop, Builder, LNode, NodeKind};
use crate::list::VisibleRange;
use crate::types::UiNode;

/// The rows past which a Table windows its body.
pub const TABLE_WINDOW_THRESHOLD: usize = crate::chart::WINDOW_THRESHOLD;

/// The order the renderer sorts an unbound table in (the reference's
/// `sortRows`): missing values (absent, null, `""`) last in both
/// directions; two numbers numerically; a `number` column coerces (`num`),
/// a `date` column compares by time; booleans as 0/1; the rest through the
/// collator stand-in ([`collate`]). Ties keep the row order (a stable sort).
pub fn compare_cells(a: Option<&Value>, b: Option<&Value>, column_type: &str, descending: bool) -> Ordering {
    let missing = |v: Option<&Value>| v.is_none_or(|v| v.is_null() || v.as_str() == Some(""));
    match (missing(a), missing(b)) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    let (a, b) = (a.expect("present"), b.expect("present"));
    let ord = match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64().partial_cmp(&y.as_f64()).unwrap_or(Ordering::Equal),
        _ if column_type == "number" => num(a).partial_cmp(&num(b)).unwrap_or(Ordering::Equal),
        _ if column_type == "date" => match (date_time(a), date_time(b)) {
            (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
            // `NaN - x` is NaN: the reference's comparator calls it a tie.
            _ => Ordering::Equal,
        },
        (Value::Bool(_), _) | (_, Value::Bool(_)) => (crate::expr::truthy(a) as u8).cmp(&(crate::expr::truthy(b) as u8)),
        _ => collate(&js(a), &js(b)),
    };
    if descending {
        ord.reverse()
    } else {
        ord
    }
}

/// The reference's `num`: a finite number, a numeric string, else 0.
fn num(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().filter(|f| f.is_finite()).unwrap_or(0.0),
        Value::String(s) if !s.trim().is_empty() => s.trim().parse::<f64>().ok().filter(|f| f.is_finite()).unwrap_or(0.0),
        _ => 0.0,
    }
}

/// `new Date(String(v)).getTime()` for the ISO forms a date column holds
/// (`yyyy-mm-dd`, `yyyy-mm-ddThh:mm[:ss[.sss]][Z|±hh:mm]`), in ms. A date
/// alone is UTC midnight; a local date-time is read as UTC (the host's zone
/// does not move one row past another in the same column).
fn date_time(v: &Value) -> Option<f64> {
    let s = match v {
        Value::Number(n) => return n.as_f64(),
        Value::String(s) => s.trim(),
        _ => return None,
    };
    let (y, m, d) = super::fields::dates::parse(s.get(0..10)?)?;
    let days = days_from_civil(y as i64, m as i64, d as i64) as f64;
    let mut ms = days * 86_400_000.0;
    if let Some(t) = s.get(10..).filter(|t| !t.is_empty()) {
        let t = t.strip_prefix(['T', ' '])?;
        let (clock, zone) = match t.find(['Z', '+', '-']) {
            Some(i) => (&t[..i], &t[i..]),
            None => (t, ""),
        };
        let mut parts = clock.split(':');
        let h: f64 = parts.next()?.parse().ok()?;
        let mi: f64 = parts.next()?.parse().ok()?;
        let sec: f64 = parts.next().map(|x| x.parse().ok()).unwrap_or(Some(0.0))?;
        ms += ((h * 60.0 + mi) * 60.0 + sec) * 1000.0;
        if zone.len() > 1 {
            let sign = if zone.starts_with('-') { 1.0 } else { -1.0 };
            let (zh, zm) = zone[1..].split_once(':').unwrap_or((zone.get(1..3).unwrap_or("0"), zone.get(3..5).unwrap_or("0")));
            ms += sign * (zh.parse::<f64>().ok()? * 60.0 + zm.parse::<f64>().unwrap_or(0.0)) * 60_000.0;
        }
    }
    Some(ms)
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The stand-in for `Intl.Collator(locale, {numeric: true, sensitivity:
/// "base"})`: digit runs compare as numbers (`File 2` < `File 10`), letters
/// by their BASE letter (case and accents ignored: `Émile` = `emile`),
/// whitespace < punctuation < digits < letters like the root collation.
pub fn collate(a: &str, b: &str) -> Ordering {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        let (ca, cb) = (a[i], b[j]);
        if ca.is_ascii_digit() && cb.is_ascii_digit() {
            let si = i;
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            let sj = j;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let na: String = a[si..i].iter().collect::<String>().trim_start_matches('0').to_string();
            let nb: String = b[sj..j].iter().collect::<String>().trim_start_matches('0').to_string();
            let ord = na.len().cmp(&nb.len()).then_with(|| na.cmp(&nb));
            if ord != Ordering::Equal {
                return ord;
            }
            continue;
        }
        let (ka, kb) = (collation_key(ca), collation_key(cb));
        if ka != kb {
            return ka.cmp(&kb);
        }
        i += 1;
        j += 1;
    }
    (a.len() - i.min(a.len())).cmp(&(b.len() - j.min(b.len())))
}

/// (class, base) of one character: 0 whitespace, 1 punctuation/symbols,
/// 2 digits, 3 letters (lower-cased, accents folded), 4 anything else.
fn collation_key(c: char) -> (u8, u32) {
    if c.is_whitespace() {
        return (0, c as u32);
    }
    if c.is_ascii_digit() {
        return (2, c as u32);
    }
    if c.is_alphabetic() {
        let base = fold_accent(c.to_lowercase().next().unwrap_or(c));
        return (3, base as u32);
    }
    if c.is_ascii_punctuation() || matches!(c, '\u{a1}'..='\u{bf}' | '\u{2010}'..='\u{2027}' | '\u{2030}'..='\u{205e}') {
        return (1, c as u32);
    }
    (4, c as u32)
}

/// The base letter of a Latin-1 / Latin Extended-A letter (lower case).
fn fold_accent(c: char) -> char {
    match c {
        'à'..='å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
        'ď' | 'đ' => 'd',
        'è'..='ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => 'g',
        'ĥ' | 'ħ' => 'h',
        'ì'..='ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => 'i',
        'ĵ' => 'j',
        'ķ' => 'k',
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => 'l',
        'ñ' | 'ń' | 'ņ' | 'ň' => 'n',
        'ò'..='ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => 'o',
        'ŕ' | 'ŗ' | 'ř' => 'r',
        'ś' | 'ŝ' | 'ş' | 'š' => 's',
        'ţ' | 'ť' | 'ŧ' => 't',
        'ù'..='ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'ŵ' => 'w',
        'ý' | 'ÿ' | 'ŷ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        other => other,
    }
}

/// A cell's text by column type (`en` formatting; natives format numbers
/// and dates in the surface locale from `value`).
pub fn cell_text(kind: &str, v: Option<&Value>) -> String {
    let Some(v) = v.filter(|v| !v.is_null()) else { return String::new() };
    match kind {
        "number" => v.as_f64().map(|n| crate::data::format_number(n, 0, 2)).unwrap_or_else(|| js(v)),
        "date" => v.as_str().map(super::fields::dates::label).unwrap_or_else(|| js(v)),
        "boolean" => {
            if crate::expr::truthy(v) && v != &Value::Bool(false) {
                "✓".into()
            } else {
                String::new()
            }
        }
        _ => js(v),
    }
}

impl Builder<'_, '_> {
    fn column_style(col: &Value) -> Value {
        match col.get("width").and_then(Value::as_f64) {
            Some(w) => json!({"flexGrow": 0, "flexShrink": 0, "flexBasis": w, "width": w, "minWidth": 0}),
            None => json!({"flexGrow": 1, "flexShrink": 1, "flexBasis": 0, "minWidth": 0}),
        }
    }

    pub(crate) fn table(&mut self, index: u32, node: &UiNode, scope: &str) {
        self.style_default(index, &[("display", json!("flex")), ("flexDirection", json!("column"))]);
        let owner = self.nodes[index as usize].clone();
        let columns = owner.props.get("columns").and_then(Value::as_array).cloned().unwrap_or_default();
        let rows = owner.props.get("rows").and_then(Value::as_array).cloned().unwrap_or_default();
        let row_key = str_prop(&owner.props, "rowKey").unwrap_or("id").to_string();
        let selectable = str_prop(&owner.props, "selectable").unwrap_or("none").to_string();
        let selected: Vec<String> = owner.props.get("selected").and_then(Value::as_array).map(|a| a.iter().map(js).collect()).unwrap_or_default();
        let sort = owner.props.get("sort").filter(|s| s.is_object()).cloned();
        let sort_bound = node.props.get("sort").is_some_and(crate::expr::is_dynamic) && !self.ctx.local.sort.contains_key(&owner.id);
        // Where each row's data lives: a bound `rows` = the model's array, a
        // literal one = a row scope the builder resolves itself.
        let rows_path = node.props.get("rows").filter(|r| crate::expr::is_binding(r)).and_then(|r| r["path"].as_str()).map(|p| crate::data::absolute_path(p, scope));
        let mut order: Vec<usize> = (0..rows.len()).collect();
        if let (Some(s), false) = (&sort, sort_bound) {
            let key = s.get("key").map(js).unwrap_or_default();
            let desc = s.get("direction").and_then(Value::as_str) == Some("desc");
            // The reference sorts only by a column it knows.
            if let Some(col) = columns.iter().find(|c| c.get("key").map(js).as_deref() == Some(key.as_str())) {
                let kind = col.get("type").and_then(Value::as_str).unwrap_or("text").to_string();
                order.sort_by(|&a, &b| compare_cells(rows[a].get(&key), rows[b].get(&key), &kind, desc));
            }
        }
        let key_of = |i: usize| rows[i].get(&row_key).filter(|v| !v.is_null()).map(js).unwrap_or_else(|| i.to_string());
        let multiple = selectable == "multiple";
        let striped = bool_prop(&owner.props, "striped");

        let header = self.part(&owner, "header", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row", "alignItems": "center", "flexShrink": 0}), json!({}));
        if multiple {
            let all = !rows.is_empty() && order.iter().all(|&i| selected.contains(&key_of(i)));
            let label = self.string("selectAll");
            let c = self.part_in(header, &owner, "checkbox", "Checkbox", NodeKind::Leaf, json!({"flexShrink": 0}), json!({"checked": all, "label": label, "header": true}), ".header");
            self.nodes[c as usize].pressable = true;
            if all {
                self.add_state(c, "checked");
            }
        }
        let sort_key = sort.as_ref().and_then(|s| s.get("key")).map(js);
        let sort_dir = sort.as_ref().and_then(|s| s.get("direction")).and_then(Value::as_str).unwrap_or("asc").to_string();
        for (c, col) in columns.iter().enumerate() {
            let key = col.get("key").map(js).unwrap_or_default();
            let label = col.get("label").map(js).unwrap_or_default();
            let sortable = col.get("sortable").and_then(Value::as_bool).unwrap_or(false);
            let align = col.get("align").and_then(Value::as_str).unwrap_or("start").to_string();
            let sorted = sort_key.as_deref() == Some(key.as_str());
            let mut props = json!({"text": label, "key": key, "sortable": sortable, "align": align, "lines": 1});
            if sorted {
                props["sort"] = json!(sort_dir);
                props["sortIcon"] = json!(super::builtin_icon(&format!("Table.sortIcon.{sort_dir}")));
            }
            let mut style = Self::column_style(col);
            style["textAlign"] = json!(align);
            let h = self.part_in(header, &owner, "headerCell", "Text", NodeKind::Leaf, style, props, &format!(".{c}"));
            self.query_prop(h, "align", json!(align));
            self.nodes[h as usize].pressable = sortable;
            if sorted {
                self.add_state(h, "selected");
            }
        }

        let body = self.part(&owner, "body", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "column", "flexShrink": 1, "minHeight": 0}), json!({}));
        self.nodes[body as usize].part_query = None;
        let keys: std::sync::Arc<Vec<String>> = std::sync::Arc::new(order.iter().map(|&i| key_of(i)).collect());
        let windowed = rows.len() > TABLE_WINDOW_THRESHOLD;
        let range = if rows.is_empty() { VisibleRange::default() } else { self.window(&owner.id, &keys, 0.0, windowed, body) };
        for (pos, &i) in order.iter().enumerate().take(range.end).skip(range.start) {
            let key = &keys[pos];
            let is_selected = selected.contains(key);
            let row_scope = match &rows_path {
                Some(p) => format!("{p}/{i}"),
                None => {
                    let s = format!("/$row/{}/{i}", owner.id);
                    self.row_scopes.insert(s.clone(), rows[i].clone());
                    s
                }
            };
            let row = self.part_in(body, &owner, "row", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row", "alignItems": "center"}), json!({"key": key, "index": i}), &format!(".{key}"));
            self.nodes[row as usize].pressable = true;
            self.nodes[row as usize].scope = row_scope.clone();
            self.query_prop(row, "striped", Value::Bool(striped && pos % 2 == 1));
            if is_selected {
                self.add_state(row, "selected");
            }
            if multiple {
                let label = self.string("selectRow");
                let c = self.part_in(row, &owner, "checkbox", "Checkbox", NodeKind::Leaf, json!({"flexShrink": 0}), json!({"checked": is_selected, "label": label, "key": key}), &format!(".{key}"));
                self.nodes[c as usize].pressable = true;
                if is_selected {
                    self.add_state(c, "checked");
                }
            }
            for (c, col) in columns.iter().enumerate() {
                self.cell(row, &owner, node, col, &rows[i], key, c, &row_scope);
            }
        }
        self.window_end(body);
        if rows.is_empty() {
            let empty = owner.props.get("emptyText").map(js).unwrap_or_else(|| self.string("noResults"));
            self.text_part(body, &owner, "empty", &empty, "muted", "");
        }
        if let Some(caption) = str_prop(&owner.props, "caption") {
            let caption = caption.to_string();
            self.text_part(index, &owner, "caption", &caption, "muted", "");
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn cell(&mut self, row: u32, owner: &LNode, node: &UiNode, col: &Value, data: &Value, key: &str, c: usize, row_scope: &str) {
        let kind = col.get("type").and_then(Value::as_str).unwrap_or("text").to_string();
        let col_key = col.get("key").map(js).unwrap_or_default();
        let align = col.get("align").and_then(Value::as_str).unwrap_or("start").to_string();
        let suffix = format!(".{key}.{c}");
        let mut style = Self::column_style(col);
        if kind == "slot" {
            style["display"] = json!("flex");
            style["flexDirection"] = json!("row");
            style["alignItems"] = json!("center");
            style["justifyContent"] = json!(match align.as_str() {
                "center" => "center",
                "end" => "flex-end",
                _ => "flex-start",
            });
            let cell = self.part_in(row, owner, "cell", "Box", NodeKind::Container, style, json!({"key": col_key, "cellType": kind, "align": align}), &suffix);
            self.nodes[cell as usize].scope = row_scope.to_string();
            self.query_prop(cell, "align", json!(align));
            let slot = col.get("slot").and_then(Value::as_str).and_then(|s| node.slots.as_ref().and_then(|slots| slots.get(s))).cloned();
            if let Some(slot) = slot {
                let item = self.instance(&slot, &format!(".{key}"), None);
                let layer = self.nodes[row as usize].layer;
                self.add(&item, Some(cell), layer, row_scope, false);
            }
            return;
        }
        style["textAlign"] = json!(align);
        let value = data.get(&col_key);
        let text = cell_text(&kind, value);
        let cell = self.part_in(row, owner, "cell", "Text", NodeKind::Leaf, style, json!({"text": text, "cellType": kind, "align": align, "value": value.cloned().unwrap_or(Value::Null), "lines": 1}), &suffix);
        self.query_prop(cell, "align", json!(align));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn local_sort_numbers_strings_missing_last() {
        let rows = [json!({"n": 3, "s": "b"}), json!({"n": 12, "s": "A"}), json!({"s": "c"}), json!({"n": 1})];
        let mut idx: Vec<usize> = (0..4).collect();
        idx.sort_by(|&a, &b| compare_cells(rows[a].get("n"), rows[b].get("n"), "number", false));
        assert_eq!(idx, [3, 0, 1, 2]);
        idx.sort_by(|&a, &b| compare_cells(rows[a].get("n"), rows[b].get("n"), "number", true));
        assert_eq!(idx, [1, 0, 3, 2]);
        idx.sort_by(|&a, &b| compare_cells(rows[a].get("s"), rows[b].get("s"), "text", false));
        assert_eq!(idx, [1, 0, 2, 3]);
        assert_eq!(collate("File 2", "File 10"), Ordering::Less);
        assert_eq!(collate("Émile", "emile"), Ordering::Equal);
        assert_eq!(collate("Émile", "Ezra"), Ordering::Less);
        assert_eq!(collate("a01", "a1"), Ordering::Equal);
        assert_eq!(date_time(&json!("1970-01-02")), Some(86_400_000.0));
        assert_eq!(date_time(&json!("2026-10-07T10:00:00Z")), date_time(&json!("2026-10-07T12:00:00+02:00")));
        assert_eq!(cell_text("number", Some(&json!(1234.5))), "1,234.5");
        assert_eq!(cell_text("boolean", Some(&json!(true))), "✓");
        assert_eq!(cell_text("date", Some(&json!("2026-10-07"))), "Oct 7, 2026");
    }
}
