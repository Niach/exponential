//! The form controls (a themed surface expands each into its recipe parts):
//! Input/Textarea, Select (the combobox, its popup a LAYER), DatePicker and
//! DateRangePicker (one calendar layer), TimePicker (a list layer),
//! NumberField (steppers), ChipInput (chips + suggestions layer), FileUpload
//! (drop zone + files), Checkbox/Switch, Radio, Slider and the Form that
//! collects them. A field's failed checks (after a refused submit) show
//! under it as `<field id>.error` and put its parts in the `invalid` state.

use serde_json::{json, Map, Value};

use super::{bool_prop, js, str_prop, Builder, LNode, LayerPlacement, NodeKind};
use crate::overlay::{OverlayAlign, OverlaySide};
use crate::theme::RecipeQuery;
use crate::types::UiNode;

/// Calendar arithmetic for the pickers (proleptic Gregorian, no clock).
pub mod dates {
    pub fn is_leap(y: i32) -> bool {
        (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
    }

    pub fn days_in_month(y: i32, m: u32) -> u32 {
        match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            _ => {
                if is_leap(y) {
                    29
                } else {
                    28
                }
            }
        }
    }

    /// 0 = Sunday … 6 = Saturday (Sakamoto).
    pub fn weekday(y: i32, m: u32, d: u32) -> u32 {
        const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
        let y = if m < 3 { y - 1 } else { y };
        ((y + y / 4 - y / 100 + y / 400 + T[(m - 1) as usize] + d as i32).rem_euclid(7)) as u32
    }

    /// `yyyy-mm-dd` → (y, m, d).
    pub fn parse(s: &str) -> Option<(i32, u32, u32)> {
        let b = s.get(0..10)?;
        let mut it = b.split('-');
        let y = it.next()?.parse().ok()?;
        let m: u32 = it.next()?.parse().ok()?;
        let d: u32 = it.next()?.parse().ok()?;
        ((1..=12).contains(&m) && d >= 1 && d <= days_in_month(y, m)).then_some((y, m, d))
    }

    pub fn format(y: i32, m: u32, d: u32) -> String {
        format!("{y:04}-{m:02}-{d:02}")
    }

    /// The month after/before.
    pub fn shift_month(y: i32, m: u32, delta: i32) -> (i32, u32) {
        let total = y * 12 + (m as i32 - 1) + delta;
        (total.div_euclid(12), (total.rem_euclid(12) + 1) as u32)
    }

    /// The day `n` days after (y, m, d).
    pub fn add_days(y: i32, m: u32, d: u32, n: i32) -> (i32, u32, u32) {
        let (mut y, mut m, mut d) = (y, m, d as i32 + n);
        while d < 1 {
            let (py, pm) = shift_month(y, m, -1);
            y = py;
            m = pm;
            d += days_in_month(y, m) as i32;
        }
        while d > days_in_month(y, m) as i32 {
            d -= days_in_month(y, m) as i32;
            let (ny, nm) = shift_month(y, m, 1);
            y = ny;
            m = nm;
        }
        (y, m, d as u32)
    }

    pub const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    pub const WEEKDAYS_SHORT: [&str; 7] = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];

    /// `Oct 7, 2026` (the `en` medium date; natives format in the surface
    /// locale from the ISO value).
    pub fn label(s: &str) -> String {
        match parse(s) {
            Some((y, m, d)) => format!("{} {d}, {y}", &MONTHS[(m - 1) as usize][..3]),
            None => s.to_string(),
        }
    }
}

/// `HH:mm` → minutes since midnight.
pub fn parse_time(s: &str) -> Option<u32> {
    let (h, m) = s.split_once(':')?;
    let h: u32 = h.parse().ok()?;
    let m: u32 = m.get(0..2)?.parse().ok()?;
    (h < 24 && m < 60).then_some(h * 60 + m)
}

pub fn format_time(minutes: u32) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// Round a plain non-negative decimal string (`123.4567`, no exponent) to
/// `p` fraction digits, ties AWAY from zero (a next digit ≥ 5 rounds up).
fn round_decimal(s: &str, p: usize) -> String {
    let (int, frac) = s.split_once('.').unwrap_or((s, ""));
    let mut digits: Vec<u8> = int.bytes().chain(frac.bytes().chain(std::iter::repeat(b'0')).take(p)).collect();
    if frac.as_bytes().get(p).copied().unwrap_or(b'0') >= b'5' {
        let mut k = digits.len();
        loop {
            if k == 0 {
                digits.insert(0, b'1');
                break;
            }
            k -= 1;
            if digits[k] == b'9' {
                digits[k] = b'0';
            } else {
                digits[k] += 1;
                break;
            }
        }
    }
    let split = digits.len() - p;
    let mut out = String::from_utf8_lossy(&digits[..split]).into_owned();
    if out.is_empty() {
        out.push('0');
    }
    if p > 0 {
        out.push('.');
        out.push_str(&String::from_utf8_lossy(&digits[split..]));
    }
    out
}

/// JavaScript's `String(n)` (shortest round trip; exponent form below 1e-6
/// and from 1e21, as JS writes it).
pub fn js_number_string(n: f64) -> String {
    if !n.is_finite() {
        return if n.is_nan() { "NaN".into() } else if n > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if n != 0.0 && (n.abs() < 1e-6 || n.abs() >= 1e21) {
        let e = format!("{n:e}");
        return match e.split_once('e') {
            Some((m, x)) if !x.starts_with('-') => format!("{m}e+{x}"),
            _ => e,
        };
    }
    crate::json::number_to_string(n)
}

/// The reference's `decimalsOf(step)`: the digits after the `.` of
/// `String(step)`.
pub fn decimals_of(n: f64) -> u32 {
    let s = js_number_string(n);
    s.find('.').map(|i| (s.len() - i - 1) as u32).unwrap_or(0)
}

/// JavaScript's `n.toFixed(p)`: the EXACT binary value rounded half away
/// from zero (`(2.5).toFixed(0)` = `3`, `(1.005).toFixed(2)` = `1.00`).
pub fn to_fixed(n: f64, p: u32) -> String {
    if !n.is_finite() || n.abs() >= 1e21 {
        return js_number_string(n);
    }
    let p = p.min(100) as usize;
    // Every f64 has at most 1074 fraction digits: this is the exact value.
    let exact = format!("{:.1100}", n.abs());
    let body = round_decimal(&exact, p);
    // `(-0.04).toFixed(1)` = "-0.0"; `(-0).toFixed(1)` = "0.0".
    if n < 0.0 {
        format!("-{body}")
    } else {
        body
    }
}

/// A NumberField's value as it shows: `Intl.NumberFormat("en", {minimum +
/// maximumFractionDigits: precision})`: the SHORTEST round-trip decimal
/// rounded half away from zero (`1.005` → `1.01`, `2.5` → `3`), grouped.
pub fn format_number_value(n: f64, precision: u32) -> String {
    if !n.is_finite() {
        return js_number_string(n);
    }
    let shortest = format!("{}", n.abs());
    let rounded = round_decimal(&shortest, precision.min(100) as usize);
    let (int, frac) = rounded.split_once('.').unwrap_or((&rounded, ""));
    let mut out = String::new();
    if n.is_sign_negative() {
        out.push('-');
    }
    let len = int.len();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if !frac.is_empty() {
        out.push('.');
        out.push_str(frac);
    }
    out
}

/// A NumberField's step (`step || 1`) and precision (`precision`, else the
/// step's decimals), as the reference reads them.
pub fn number_step_precision(props: &crate::types::Props) -> (f64, u32) {
    let step = props.get("step").map(crate::json::to_number).filter(|s| *s != 0.0 && s.is_finite()).unwrap_or(1.0);
    let precision = match props.get("precision") {
        Some(p) => crate::json::to_number(p).max(0.0).floor().min(100.0) as u32,
        None => decimals_of(step),
    };
    (step, precision)
}

/// A numeric NumberField prop the reference's way (`num`): numbers and
/// numeric strings; absent, null or `""` = none.
pub fn number_prop(props: &crate::types::Props, key: &str) -> Option<f64> {
    match props.get(key)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) if !s.trim().is_empty() => s.trim().parse::<f64>().ok().filter(|v| v.is_finite()).or(Some(0.0)),
        Value::Null => None,
        Value::String(_) => None,
        _ => Some(0.0),
    }
}

/// The reference's `clamp`: into `[min, max]`, then `Number(v.toFixed(p))`.
pub fn clamp_number_value(v: f64, min: Option<f64>, max: Option<f64>, precision: u32) -> f64 {
    let mut v = v;
    if let Some(m) = min {
        v = v.max(m);
    }
    if let Some(m) = max {
        v = v.min(m);
    }
    to_fixed(v, precision).parse().unwrap_or(v)
}

/// A byte count as `48 KB` / `1.2 MB`.
pub fn format_bytes(n: f64) -> String {
    if n < 1024.0 {
        format!("{} B", n as u64)
    } else if n < 1024.0 * 1024.0 {
        format!("{} KB", (n / 1024.0).round() as u64)
    } else {
        format!("{:.1} MB", n / 1024.0 / 1024.0)
    }
}

impl Builder<'_, '_> {
    /// The column frame every labelled field shares: label first.
    fn field_open(&mut self, index: u32) -> LNode {
        self.style_default(index, &[("display", json!("flex")), ("flexDirection", json!("column")), ("gap", json!("$spacing.xs"))]);
        self.nodes[index as usize].kind = NodeKind::Container;
        let owner = self.nodes[index as usize].clone();
        if let Some(label) = str_prop(&owner.props, "label") {
            self.text_part(index, &owner, "label", label, "label", "");
        }
        owner
    }

    /// The failed checks of a field (after a refused submit) plus an
    /// authored `error`.
    fn field_errors(&self, owner: &LNode) -> Vec<String> {
        let mut out: Vec<String> = self.ctx.local.errors.get(&owner.id).map(|e| e.messages.clone()).unwrap_or_default();
        if let Some(e) = str_prop(&owner.props, "error") {
            out.push(e.to_string());
        }
        out
    }

    /// Description and error lines under the control; `control` (the part
    /// that IS the control) gets the `invalid` state.
    fn field_close(&mut self, owner: &LNode, control: Option<u32>) {
        if let Some(d) = str_prop(&owner.props, "description") {
            self.text_part(owner.index, owner, "description", d, "muted", "");
        }
        let errors = self.field_errors(owner);
        if !errors.is_empty() {
            let e = self.text_part(owner.index, owner, "error", &errors.join("\n"), "caption", "");
            self.add_state(e, "invalid");
            if let Some(c) = control {
                self.add_state(c, "invalid");
            }
        }
    }

    /// Props for a field's control part: the owner's minus the frame's.
    fn control_props(owner: &LNode) -> Value {
        let mut props = owner.props.clone();
        for k in ["label", "description", "style", "checks", "error"] {
            props.shift_remove(k);
        }
        Value::Object(props)
    }

    /// Input / Textarea: label, the host-measured field, description, error.
    pub(crate) fn field(&mut self, index: u32) {
        let owner = self.field_open(index);
        let f = self.part(&owner, "field", &owner.component, NodeKind::Leaf, json!({"alignSelf": "stretch"}), Self::control_props(&owner));
        self.nodes[f as usize].pressable = !bool_prop(&owner.props, "disabled");
        self.field_close(&owner, Some(f));
    }

    /// A trigger part that opens the owner's popup layer.
    fn trigger(&mut self, owner: &LNode, text: &str, placeholder: bool, glyph: &str) -> u32 {
        let mut props = Self::control_props(owner);
        props["text"] = json!(text);
        props["placeholder"] = json!(placeholder);
        props["icon"] = json!(super::builtin_icon(glyph).unwrap_or("ui-selector"));
        let t = self.part(owner, "trigger", &owner.component, NodeKind::Leaf, json!({"alignSelf": "stretch"}), props);
        let disabled = bool_prop(&owner.props, "disabled");
        self.nodes[t as usize].pressable = !disabled;
        self.nodes[t as usize].trigger_for = Some(owner.id.clone());
        if self.is_open(&owner.id, &Map::new(), false) {
            self.add_state(t, "open");
        }
        t
    }

    /// The anchored popup of a field: its layer root (`part`), placed under
    /// the trigger, at least as wide.
    fn popup(&mut self, owner: &LNode, part: &str, anchor: u32, style: Value) -> (u32, u32) {
        let (layer, root) = self.layer_root(owner, part, style);
        let _ = anchor;
        (layer, root)
    }

    /// Select = the combobox: trigger; open → a layer with an optional
    /// search field, the (filtered) options and `empty`.
    pub(crate) fn select(&mut self, index: u32) {
        let owner = self.field_open(index);
        let options = owner.props.get("options").and_then(Value::as_array).cloned().unwrap_or_default();
        let multiple = bool_prop(&owner.props, "multiple");
        let value = owner.props.get("value").map(js).unwrap_or_default();
        let chosen: Vec<String> = if multiple { value.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect() } else { vec![value.clone()].into_iter().filter(|v| !v.is_empty()).collect() };
        let label_of = |v: &str| options.iter().find(|o| o.get("value").map(js).as_deref() == Some(v)).and_then(|o| o.get("label")).map(js);
        let shown: Vec<String> = chosen.iter().map(|v| label_of(v).unwrap_or_else(|| v.clone())).collect();
        let placeholder = str_prop(&owner.props, "placeholder").map(str::to_string).unwrap_or_else(|| self.string("choose"));
        let (text, is_placeholder) = if shown.is_empty() { (placeholder, true) } else { (shown.join(", "), false) };
        let trigger = self.trigger(&owner, &text, is_placeholder, "Select.trigger");
        self.field_close(&owner, Some(trigger));
        if !self.is_open(&owner.id, &Map::new(), false) {
            return;
        }
        let (layer, root) = self.popup(&owner, "content", trigger, json!({"display": "flex", "flexDirection": "column", "gap": "$spacing.xxs", "padding": "$spacing.xs", "borderRadius": "$radius.md", "maxHeight": 320}));
        let query = self.ctx.local.query.get(&owner.id).cloned().unwrap_or_default();
        if bool_prop(&owner.props, "searchable") {
            let search = self.part_in(root, &owner, "search", "Select", NodeKind::Leaf, json!({"alignSelf": "stretch", "flexShrink": 0}), json!({"placeholder": self.string("search"), "value": query, "icon": super::builtin_icon("Select.search")}), "");
            self.nodes[search as usize].pressable = true;
        }
        let list = self.part_in(root, &owner, "list", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "column", "flexShrink": 1, "minHeight": 0, "overflowY": "scroll"}), json!({}), "");
        self.nodes[list as usize].part_query = None;
        let needle = query.to_lowercase();
        let mut shown_any = false;
        for (i, option) in options.iter().enumerate() {
            let label = option.get("label").map(js).unwrap_or_default();
            if !needle.is_empty() && !label.to_lowercase().contains(&needle) {
                continue;
            }
            shown_any = true;
            let v = option.get("value").map(js).unwrap_or_default();
            let selected = chosen.contains(&v);
            let disabled = option.get("disabled").and_then(Value::as_bool).unwrap_or(false);
            let mut props = json!({"text": label, "value": v, "lines": 1, "selected": selected, "disabled": disabled});
            if let Some(icon) = option.get("icon") {
                props["icon"] = icon.clone();
            }
            if selected {
                props["check"] = json!(super::builtin_icon("Select.check"));
            }
            let item = self.part_in(list, &owner, "item", "Text", NodeKind::Leaf, json!({"display": "flex", "flexDirection": "row", "alignItems": "center"}), props, &format!(".{i}"));
            self.nodes[item as usize].pressable = !disabled;
            if selected {
                self.add_state(item, "selected");
            }
            if disabled {
                self.add_state(item, "disabled");
            }
        }
        if !shown_any {
            let empty = owner.props.get("emptyText").map(js).unwrap_or_else(|| self.string("noResults"));
            self.text_part(list, &owner, "empty", &empty, "muted", "");
        }
        self.push_layer(layer, root, &owner, LayerPlacement::Anchored { anchor: trigger, side: OverlaySide::Bottom, align: OverlayAlign::Start }, false, true, true, Some(list));
    }

    /// DatePicker / DateRangePicker: trigger; open → one calendar layer:
    /// header (previous, the month title, next), the weekday row, six weeks
    /// of `day` cells from the week start (`firstDayOfWeek`, else the
    /// locale's).
    pub(crate) fn date_picker(&mut self, index: u32) {
        let owner = self.field_open(index);
        let range = owner.component == "DateRangePicker";
        let (start, end) = if range {
            (owner.props.get("start").and_then(Value::as_str).unwrap_or("").to_string(), owner.props.get("end").and_then(Value::as_str).unwrap_or("").to_string())
        } else {
            (owner.props.get("value").and_then(Value::as_str).unwrap_or("").to_string(), String::new())
        };
        let placeholder = str_prop(&owner.props, "placeholder").map(str::to_string).unwrap_or_else(|| self.string("choose"));
        let text = match (range, start.is_empty()) {
            (_, true) => placeholder.clone(),
            (false, false) => dates::label(&start),
            (true, false) => format!("{} – {}", dates::label(&start), if end.is_empty() { "…".to_string() } else { dates::label(&end) }),
        };
        let glyph = format!("{}.trigger", owner.component);
        let trigger = self.trigger(&owner, &text, start.is_empty(), &glyph);
        self.field_close(&owner, Some(trigger));
        if !self.is_open(&owner.id, &Map::new(), false) {
            return;
        }
        let today = self.today();
        let (y, m) = self.ctx.local.month.get(&owner.id).copied().or_else(|| dates::parse(&start).map(|(y, m, _)| (y, m))).or_else(|| today.as_deref().and_then(dates::parse).map(|(y, m, _)| (y, m))).or_else(|| owner.props.get("min").and_then(Value::as_str).and_then(dates::parse).map(|(y, m, _)| (y, m))).unwrap_or((2026, 1));
        let first_dow = owner.props.get("firstDayOfWeek").and_then(Value::as_u64).map(|d| (d % 7) as u32).unwrap_or_else(|| crate::locale::week_start(self.ctx.locale) as u32);
        let (layer, root) = self.popup(&owner, "calendar", trigger, json!({"display": "flex", "flexDirection": "column", "gap": "$spacing.xs", "padding": "$spacing.sm", "borderRadius": "$radius.md"}));
        let header = self.part_in(root, &owner, "header", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "space-between", "gap": "$spacing.xs"}), json!({}), "");
        self.nodes[header as usize].part_query = None;
        let button = |b: &mut Self, parent: u32, part: &str, glyph: &str, label_id: &str| {
            let label = b.string(label_id);
            let n = b.part_in(parent, &owner, part, "Button", NodeKind::Leaf, json!({"flexShrink": 0}), json!({"icon": super::builtin_icon(glyph), "label": label, "variant": "ghost", "size": "icon"}), "");
            b.nodes[n as usize].pressable = true;
            b.nodes[n as usize].own_query = Some(RecipeQuery::new("Button", "root", b.ctx.recipes.native_recipe_props("Button", &b.nodes[n as usize].props.clone()), Vec::new()));
            n
        };
        button(self, header, "previous", &format!("{}.previousMonth", "DatePicker"), "previousMonth");
        let title = format!("{} {y}", dates::MONTHS[(m - 1) as usize]);
        let t = self.part_in(header, &owner, "title", "Text", NodeKind::Leaf, json!({"flexGrow": 1, "textAlign": "center"}), json!({"text": title, "variant": "label", "month": m, "year": y}), "");
        self.nodes[t as usize].live = Some("polite".into());
        button(self, header, "next", "DatePicker.nextMonth", "nextMonth");
        let week_style = json!({"display": "flex", "flexDirection": "row", "gap": "$spacing.xxs"});
        let cell_style = json!({"flexGrow": 1, "flexBasis": 0, "minWidth": 32, "height": 32, "textAlign": "center"});
        let weekdays = self.part_in(root, &owner, "weekdays", "Box", NodeKind::Container, week_style.clone(), json!({}), "");
        self.nodes[weekdays as usize].part_query = None;
        for c in 0..7u32 {
            let dow = (first_dow + c) % 7;
            let w = self.part_in(weekdays, &owner, "weekday", "Text", NodeKind::Leaf, cell_style.clone(), json!({"text": dates::WEEKDAYS_SHORT[dow as usize], "variant": "caption", "weekday": dow}), &format!(".{c}"));
            self.nodes[w as usize].part_query = None;
        }
        let lead = (dates::weekday(y, m, 1) + 7 - first_dow) % 7;
        let (sy, sm, sd) = dates::add_days(y, m, 1, -(lead as i32));
        let min = owner.props.get("min").and_then(Value::as_str).unwrap_or("").to_string();
        let max = owner.props.get("max").and_then(Value::as_str).unwrap_or("").to_string();
        let anchor = self.ctx.local.range_anchor.get(&owner.id).cloned();
        let (lo, hi) = match (&anchor, range) {
            (Some(a), true) => (a.clone(), String::new()),
            _ => (start.clone(), end.clone()),
        };
        for r in 0..6 {
            let week = self.part_in(root, &owner, "week", "Box", NodeKind::Container, week_style.clone(), json!({}), &format!(".{r}"));
            self.nodes[week as usize].part_query = None;
            for c in 0..7 {
                let (dy, dm, dd) = dates::add_days(sy, sm, sd, r * 7 + c);
                let date = dates::format(dy, dm, dd);
                let outside = dm != m;
                let disabled = (!min.is_empty() && date < min) || (!max.is_empty() && date > max);
                let is_start = !lo.is_empty() && date == lo;
                let is_end = range && !hi.is_empty() && date == hi;
                let in_range = range && !lo.is_empty() && !hi.is_empty() && date > lo && date < hi;
                let selected = is_start || is_end;
                let props = json!({"text": dd.to_string(), "date": date, "outside": outside, "disabled": disabled, "selected": selected, "inRange": in_range, "rangeStart": range && is_start, "rangeEnd": is_end, "today": today.as_deref() == Some(date.as_str())});
                let day = self.part_in(week, &owner, "day", "Text", NodeKind::Leaf, cell_style.clone(), props, &format!(".{r}.{c}"));
                self.nodes[day as usize].pressable = !disabled;
                if selected {
                    self.add_state(day, "selected");
                }
                if disabled {
                    self.add_state(day, "disabled");
                }
                if in_range {
                    self.query_prop(day, "inRange", Value::Bool(true));
                }
            }
        }
        self.push_layer(layer, root, &owner, LayerPlacement::Anchored { anchor: trigger, side: OverlaySide::Bottom, align: OverlayAlign::Start }, false, true, false, None);
    }

    /// The host's notion of today (`SurfaceSettings.today`), if set.
    fn today(&self) -> Option<String> {
        self.ctx.today.clone()
    }

    /// TimePicker: trigger; open → a scrolling list layer every `step`
    /// minutes from `min` to `max`.
    pub(crate) fn time_picker(&mut self, index: u32) {
        let owner = self.field_open(index);
        let value = owner.props.get("value").and_then(Value::as_str).unwrap_or("").to_string();
        let placeholder = str_prop(&owner.props, "placeholder").map(str::to_string).unwrap_or_else(|| "--:--".to_string());
        let trigger = self.trigger(&owner, if value.is_empty() { &placeholder } else { &value }, value.is_empty(), "TimePicker.trigger");
        self.field_close(&owner, Some(trigger));
        if !self.is_open(&owner.id, &Map::new(), false) {
            return;
        }
        let step = owner.props.get("step").and_then(Value::as_f64).unwrap_or(15.0).max(1.0) as u32;
        let min = owner.props.get("min").and_then(Value::as_str).and_then(parse_time).unwrap_or(0);
        let max = owner.props.get("max").and_then(Value::as_str).and_then(parse_time).unwrap_or(23 * 60 + 59);
        let (layer, root) = self.popup(&owner, "list", trigger, json!({"display": "flex", "flexDirection": "column", "padding": "$spacing.xs", "borderRadius": "$radius.md", "maxHeight": 240, "overflowY": "scroll"}));
        let mut t = min;
        let mut i = 0;
        while t <= max && i < 24 * 60 {
            let label = format_time(t);
            let selected = label == value;
            let item = self.part_in(root, &owner, "item", "Text", NodeKind::Leaf, json!({}), json!({"text": label, "value": label, "lines": 1, "selected": selected}), &format!(".{i}"));
            self.nodes[item as usize].pressable = true;
            if selected {
                self.add_state(item, "selected");
            }
            t += step;
            i += 1;
        }
        self.push_layer(layer, root, &owner, LayerPlacement::Anchored { anchor: trigger, side: OverlaySide::Bottom, align: OverlayAlign::Start }, false, true, true, Some(root));
    }

    /// NumberField: the bordered `field` box holds decrement, the host text
    /// `input`, the unit and increment.
    pub(crate) fn number_field(&mut self, index: u32) {
        let owner = self.field_open(index);
        let value = number_prop(&owner.props, "value");
        let min = number_prop(&owner.props, "min");
        let max = number_prop(&owner.props, "max");
        let (_, precision) = number_step_precision(&owner.props);
        let disabled = bool_prop(&owner.props, "disabled");
        let field = self.part(&owner, "field", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row", "alignItems": "center", "alignSelf": "stretch", "gap": "$spacing.xs"}), json!({}));
        let stepper = |b: &mut Self, part: &str, at_limit: bool| {
            let label = b.string(part);
            let glyph = format!("NumberField.{part}");
            let n = b.part_in(field, &owner, part, "Button", NodeKind::Leaf, json!({"flexShrink": 0}), json!({"icon": super::builtin_icon(&glyph), "label": label, "variant": "ghost", "size": "icon", "disabled": disabled || at_limit}), "");
            b.nodes[n as usize].pressable = !(disabled || at_limit);
            if disabled || at_limit {
                b.add_state(n, "disabled");
            }
            n
        };
        stepper(self, "decrement", value.zip(min).is_some_and(|(v, m)| v <= m));
        let text = value.map(|v| format_number_value(v, precision)).unwrap_or_default();
        let mut props = Self::control_props(&owner);
        props["text"] = json!(text);
        let input = self.part_in(field, &owner, "input", "NumberField", NodeKind::Leaf, json!({"flexGrow": 1, "flexBasis": 0, "minWidth": 40}), props, "");
        self.nodes[input as usize].pressable = !disabled;
        if let Some(unit) = str_prop(&owner.props, "unit") {
            self.text_part(field, &owner, "unit", unit, "muted", "");
        }
        stepper(self, "increment", value.zip(max).is_some_and(|(v, m)| v >= m));
        self.field_close(&owner, Some(field));
    }

    /// ChipInput: the `field` box wraps the chips (label + remove) and the
    /// host text `input`; typed text offers matching suggestions in a layer.
    pub(crate) fn chip_input(&mut self, index: u32) {
        let owner = self.field_open(index);
        let values: Vec<String> = owner.props.get("values").and_then(Value::as_array).map(|a| a.iter().map(js).collect()).unwrap_or_default();
        let disabled = bool_prop(&owner.props, "disabled");
        let field = self.part(&owner, "field", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row", "flexWrap": "wrap", "alignItems": "center", "alignSelf": "stretch", "gap": "$spacing.xs"}), json!({}));
        for (i, v) in values.iter().enumerate() {
            let suffix = format!(".{i}");
            let chip = self.part_in(field, &owner, "chip", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": "$spacing.xxs", "flexShrink": 0}), json!({"value": v}), &suffix);
            let _ = chip;
            self.part_in(chip, &owner, "chipLabel", "Text", NodeKind::Leaf, json!({}), json!({"text": v, "lines": 1}), &suffix);
            if !disabled {
                let label = self.string_with("removeItem", &[("name", v)]);
                let r = self.part_in(chip, &owner, "remove", "Icon", NodeKind::Leaf, json!({"flexShrink": 0}), json!({"name": super::builtin_icon("ChipInput.remove"), "size": "sm", "label": label, "value": v}), &suffix);
                self.nodes[r as usize].pressable = true;
            }
        }
        let query = self.ctx.local.query.get(&owner.id).cloned().unwrap_or_default();
        let at_max = owner.props.get("max").and_then(Value::as_f64).is_some_and(|m| values.len() as f64 >= m);
        let placeholder = if values.is_empty() { str_prop(&owner.props, "placeholder").unwrap_or("").to_string() } else { String::new() };
        let input = self.part_in(field, &owner, "input", "ChipInput", NodeKind::Leaf, json!({"flexGrow": 1, "flexBasis": 80, "minWidth": 80}), json!({"text": query, "placeholder": placeholder, "disabled": disabled || at_max}), "");
        self.nodes[input as usize].pressable = !disabled;
        self.field_close(&owner, Some(field));
        let needle = query.trim().to_lowercase();
        if needle.is_empty() || disabled || at_max {
            return;
        }
        let suggestions: Vec<Value> = owner
            .props
            .get("suggestions")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter(|o| o.get("label").map(js).is_some_and(|l| l.to_lowercase().contains(&needle)) && !values.contains(&o.get("value").map(js).unwrap_or_default())).cloned().collect())
            .unwrap_or_default();
        if suggestions.is_empty() {
            return;
        }
        let (layer, root) = self.popup(&owner, "suggestions", field, json!({"display": "flex", "flexDirection": "column", "padding": "$spacing.xs", "borderRadius": "$radius.md", "maxHeight": 240, "overflowY": "scroll"}));
        for (i, o) in suggestions.iter().enumerate() {
            let item = self.part_in(root, &owner, "suggestion", "Text", NodeKind::Leaf, json!({}), json!({"text": o.get("label").map(js).unwrap_or_default(), "value": o.get("value").map(js).unwrap_or_default(), "lines": 1}), &format!(".{i}"));
            self.nodes[item as usize].pressable = true;
        }
        self.push_layer(layer, root, &owner, LayerPlacement::Anchored { anchor: field, side: OverlaySide::Bottom, align: OverlayAlign::Start }, false, true, true, Some(root));
    }

    /// FileUpload: the dashed `dropzone` (icon, hint, browse) and one row
    /// per attached file (name, size, remove).
    pub(crate) fn file_upload(&mut self, index: u32) {
        let owner = self.field_open(index);
        let disabled = bool_prop(&owner.props, "disabled");
        let zone = self.part(&owner, "dropzone", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "column", "alignItems": "center", "gap": "$spacing.xs", "padding": "$spacing.md", "alignSelf": "stretch"}), json!({"accept": owner.props.get("accept").cloned().unwrap_or(Value::Null), "multiple": bool_prop(&owner.props, "multiple")}));
        self.nodes[zone as usize].pressable = !disabled;
        self.icon_part(zone, &owner, "icon", "FileUpload.icon", "");
        let title = self.string("dropFiles");
        self.text_part(zone, &owner, "title", &title, "label", "");
        if let Some(hint) = str_prop(&owner.props, "hint") {
            self.text_part(zone, &owner, "hint", hint, "muted", "");
        }
        let browse_label = self.string("browse");
        let browse = self.part_in(zone, &owner, "browse", "Button", NodeKind::Leaf, json!({"flexShrink": 0}), json!({"label": browse_label, "variant": "outline", "size": "sm", "disabled": disabled}), "");
        self.nodes[browse as usize].pressable = !disabled;
        self.nodes[browse as usize].own_query = Some(RecipeQuery::new("Button", "root", self.ctx.recipes.native_recipe_props("Button", &self.nodes[browse as usize].props.clone()), Vec::new()));
        let files = owner.props.get("files").and_then(Value::as_array).cloned().unwrap_or_default();
        for (i, f) in files.iter().enumerate() {
            let suffix = format!(".{i}");
            let name = f.get("name").map(js).unwrap_or_default();
            let row = self.part_in(owner.index, &owner, "file", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": "$spacing.sm"}), json!({"name": name}), &suffix);
            self.icon_part(row, &owner, "fileIcon", "FileUpload.file", &suffix);
            let n = self.part_in(row, &owner, "fileName", "Text", NodeKind::Leaf, json!({"flexGrow": 1, "flexShrink": 1, "minWidth": 0}), json!({"text": name, "lines": 1}), &suffix);
            let _ = n;
            if let Some(size) = f.get("size").and_then(Value::as_f64) {
                self.text_part(row, &owner, "fileMeta", &format_bytes(size), "muted", &suffix);
            }
            if !disabled {
                let label = self.string_with("removeItem", &[("name", &name)]);
                let r = self.part_in(row, &owner, "remove", "Icon", NodeKind::Leaf, json!({"flexShrink": 0}), json!({"name": super::builtin_icon("FileUpload.remove"), "size": "sm", "label": label, "file": name}), &suffix);
                self.nodes[r as usize].pressable = true;
            }
        }
        self.field_close(&owner, Some(zone));
    }

    /// Checkbox / Switch: the control box beside label + description.
    pub(crate) fn check(&mut self, index: u32) {
        let owner = self.nodes[index as usize].clone();
        self.style_default(index, &[("display", json!("flex")), ("flexDirection", json!("row")), ("alignItems", json!("center")), ("gap", json!("$spacing.sm"))]);
        self.nodes[index as usize].kind = NodeKind::Container;
        self.nodes[index as usize].pressable = !bool_prop(&owner.props, "disabled");
        let control_part = if owner.component == "Switch" { "track" } else { "box" };
        let checked = bool_prop(&owner.props, "checked");
        let invalid = !self.field_errors(&owner).is_empty();
        let control = |b: &mut Self, parent: u32| {
            let c = b.part_in(parent, &owner, control_part, &owner.component, NodeKind::Leaf, json!({"flexShrink": 0}), json!({"checked": checked, "disabled": bool_prop(&owner.props, "disabled")}), "");
            if checked {
                b.add_state(c, "checked");
            }
            if invalid {
                b.add_state(c, "invalid");
            }
            c
        };
        let text = |b: &mut Self, parent: u32| {
            let has_desc = str_prop(&owner.props, "description").is_some();
            if has_desc {
                let col = b.part_in(parent, &owner, "body", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "column", "gap": "$spacing.xxs", "flexGrow": 1, "minWidth": 0}), json!({}), "");
                if let Some(label) = str_prop(&owner.props, "label") {
                    b.text_part(col, &owner, "label", label, "body", "");
                }
                if let Some(d) = str_prop(&owner.props, "description") {
                    b.text_part(col, &owner, "description", d, "muted", "");
                }
            } else if let Some(label) = str_prop(&owner.props, "label") {
                let l = b.text_part(parent, &owner, "label", label, "body", "");
                b.nodes[l as usize].base_style.insert("flexGrow".into(), json!(1));
                b.nodes[l as usize].base_style.insert("minWidth".into(), json!(0));
            }
        };
        if owner.component == "Switch" {
            text(self, index);
            control(self, index);
        } else {
            control(self, index);
            text(self, index);
        }
        let errors = self.field_errors(&owner);
        if !errors.is_empty() {
            let wrap_note = errors.join("\n");
            let e = self.text_part(index, &owner, "error", &wrap_note, "caption", "");
            self.nodes[e as usize].base_style.insert("flexBasis".into(), json!("100%"));
            self.add_state(e, "invalid");
            self.style_default(index, &[("flexWrap", json!("wrap"))]);
        }
    }

    pub(crate) fn radio(&mut self, index: u32) {
        let owner = self.nodes[index as usize].clone();
        let horizontal = str_prop(&owner.props, "orientation") == Some("horizontal");
        self.style_default(index, &[("display", json!("flex")), ("flexDirection", json!("column")), ("gap", json!("$spacing.xs"))]);
        self.nodes[index as usize].kind = NodeKind::Container;
        if let Some(label) = str_prop(&owner.props, "label") {
            self.text_part(index, &owner, "label", label, "label", "");
        }
        let items = self.part(&owner, "items", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": if horizontal { "row" } else { "column" }, "gap": "$spacing.sm", "flexWrap": "wrap"}), json!({}));
        let value = owner.props.get("value").map(js);
        let options = owner.props.get("options").and_then(Value::as_array).cloned().unwrap_or_default();
        for (i, option) in options.iter().enumerate() {
            let v = option.get("value").map(js).unwrap_or_default();
            let checked = value.as_deref() == Some(v.as_str());
            // VAPP-90: the ROW is a plain option row (no recipe — the
            // `Radio/item` recipe is the 16 px circle); the `.dot` LEAF wears
            // the `item` circle recipe and the painter draws `dot` inside it.
            let row = self.part_in(items, &owner, "item", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": "$spacing.sm"}), json!({"value": v, "checked": checked}), &format!(".{i}"));
            let disabled = bool_prop(&owner.props, "disabled") || option.get("disabled").and_then(Value::as_bool).unwrap_or(false);
            self.nodes[row as usize].pressable = !disabled;
            self.nodes[row as usize].part_query = None;
            let dot = self.part_in(row, &owner, "dot", "Radio", NodeKind::Leaf, json!({"flexShrink": 0}), json!({"checked": checked}), &format!(".{i}"));
            self.nodes[dot as usize].pressable = !disabled;
            let mut circle = RecipeQuery::new("Radio", "item", self.ctx.recipes.native_recipe_props("Radio", &owner.props), Vec::new());
            if checked {
                circle.states.push("checked".into());
            }
            self.nodes[dot as usize].part_query = Some(circle);
            let label = option.get("label").map(js).unwrap_or_default();
            self.text_part(row, &owner, "label", &label, "body", &format!(".{i}"));
        }
        let errors = self.field_errors(&owner);
        if !errors.is_empty() {
            let e = self.text_part(index, &owner, "error", &errors.join("\n"), "caption", "");
            self.add_state(e, "invalid");
        }
    }

    pub(crate) fn slider(&mut self, index: u32) {
        let owner = self.nodes[index as usize].clone();
        self.style_default(index, &[("display", json!("flex")), ("flexDirection", json!("column")), ("gap", json!("$spacing.xs"))]);
        self.nodes[index as usize].kind = NodeKind::Container;
        if owner.props.get("label").is_some() {
            let head = self.part(&owner, "header", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "space-between"}), json!({}));
            if let Some(label) = str_prop(&owner.props, "label") {
                self.text_part(head, &owner, "label", label, "label", "");
            }
            if let Some(v) = owner.props.get("value") {
                let text = js(v);
                self.text_part(head, &owner, "value", &text, "muted", "");
            }
        }
        let mut props = owner.props.clone();
        props.shift_remove("label");
        let track = self.part_in(index, &owner, "track", "Slider", NodeKind::Leaf, json!({"alignSelf": "stretch"}), Value::Object(props), "");
        self.nodes[track as usize].pressable = !bool_prop(&owner.props, "disabled");
    }

    /// Form: a column of its children; `summary` lists every failed check
    /// above them after a refused submit (a live region).
    pub(crate) fn form(&mut self, index: u32, node: &UiNode, scope: &str) {
        self.style_default(index, &[("display", json!("flex")), ("flexDirection", json!("column")), ("gap", json!("$spacing.md"))]);
        let owner = self.nodes[index as usize].clone();
        self.forms.insert(owner.id.clone(), (bool_prop(&owner.props, "disabled"), bool_prop(&owner.props, "busy")));
        if bool_prop(&owner.props, "summary") {
            let mut messages: Vec<String> = Vec::new();
            for field in self.form_fields_of(node) {
                if let Some(e) = self.ctx.local.errors.get(&field) {
                    messages.extend(e.messages.iter().cloned());
                }
            }
            if !messages.is_empty() {
                let s = self.text_part(index, &owner, "summary", &messages.join("\n"), "body", "");
                self.nodes[s as usize].live = Some("assertive".into());
                self.add_state(s, "invalid");
            }
        }
        self.children_of(index, node, scope, false);
    }

    /// The ids of the named fields a Form collects (not inside a nested Form).
    pub(crate) fn form_fields_of(&self, node: &UiNode) -> Vec<String> {
        fn walk(n: &UiNode, out: &mut Vec<String>, top: bool) {
            if !top && n.component == "Form" {
                return;
            }
            if super::FORM_FIELDS.contains(&n.component.as_str()) && n.props.contains_key("name") {
                out.push(n.id.clone());
            }
            for s in n.slots.iter().flat_map(|s| s.values()) {
                walk(s, out, false);
            }
            for c in &n.children {
                walk(c, out, false);
            }
        }
        let mut out = Vec::new();
        walk(node, &mut out, true);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::dates::*;
    use super::*;

    #[test]
    fn numbers_round_like_to_fixed_and_intl() {
        // toFixed: the exact binary value, ties away from zero.
        assert_eq!(to_fixed(2.5, 0), "3");
        assert_eq!(to_fixed(-2.5, 0), "-3");
        assert_eq!(to_fixed(0.125, 2), "0.13");
        assert_eq!(to_fixed(1.005, 2), "1.00", "1.005 is 1.00499999…");
        assert_eq!(to_fixed(0.1 + 0.2, 1), "0.3");
        assert_eq!(to_fixed(9.95, 1), "9.9", "9.95 is 9.9499999…");
        assert_eq!(to_fixed(99.5, 0), "100");
        assert_eq!(to_fixed(-0.04, 1), "-0.0");
        assert_eq!(to_fixed(1234.5678, 3), "1234.568");
        // Intl: the shortest decimal, ties away from zero, grouped.
        assert_eq!(format_number_value(1.005, 2), "1.01");
        assert_eq!(format_number_value(2.5, 0), "3");
        assert_eq!(format_number_value(1234.55, 1), "1,234.6");
        assert_eq!(format_number_value(0.30000000000000004, 1), "0.3");
        assert_eq!(format_number_value(999_999.5, 0), "1,000,000");
        assert_eq!(format_number_value(-1234.5, 0), "-1,235");
        // decimalsOf(String(step)).
        assert_eq!(decimals_of(0.1), 1);
        assert_eq!(decimals_of(0.25), 2);
        assert_eq!(decimals_of(1.0), 0);
        assert_eq!(decimals_of(1e-7), 0, "String(1e-7) = \"1e-7\"");
        assert_eq!(decimals_of(1.5e-7), 4, "String(1.5e-7) = \"1.5e-7\"");
        assert_eq!(clamp_number_value(0.30000000000000004, None, None, 1), 0.3);
        assert_eq!(clamp_number_value(2.5, Some(0.0), Some(10.0), 0), 3.0);
        assert_eq!(clamp_number_value(42.0, Some(0.0), Some(10.0), 0), 10.0);
        let props = |v: serde_json::Value| v.as_object().cloned().unwrap();
        assert_eq!(number_step_precision(&props(json!({"step": 0.1}))), (0.1, 1));
        assert_eq!(number_step_precision(&props(json!({"step": 0}))), (1.0, 0), "step || 1");
        assert_eq!(number_step_precision(&props(json!({"step": 0.5, "precision": 3}))), (0.5, 3));
    }

    #[test]
    fn calendar_arithmetic() {
        assert_eq!(weekday(2026, 10, 7), 3, "a Wednesday");
        assert_eq!(weekday(2000, 1, 1), 6);
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2100, 2), 28);
        assert_eq!(add_days(2026, 1, 1, -1), (2025, 12, 31));
        assert_eq!(add_days(2026, 2, 27, 3), (2026, 3, 2));
        assert_eq!(shift_month(2026, 12, 1), (2027, 1));
        assert_eq!(parse("2026-02-30"), None);
        assert_eq!(label("2026-10-07"), "Oct 7, 2026");
        assert_eq!(parse_time("09:30"), Some(570));
        assert_eq!(format_time(570), "09:30");
        assert_eq!(format_number_value(2.5, 2), "2.50");
        assert_eq!(format_bytes(48213.0), "47 KB");
    }
}
