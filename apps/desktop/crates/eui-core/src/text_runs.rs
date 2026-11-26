//! Round 4 group B (`fixtures/text-runs.json`): a Text's `runs` (inline
//! spans) and its `dropCap`, the rule the TS reference
//! packages/exponential-ui/src/text-runs.ts implements. `runs` set (an
//! array) REPLACES `text`: one paragraph of spans that wrap together. The
//! layout tree hands the natives the normal form ([`normal_runs`]) and
//! `text` = the joined text, so every measurer and a11y name sees it.

use serde_json::{Map, Value};

pub const RUN_WEIGHTS: [&str; 4] = ["regular", "medium", "semibold", "bold"];
pub const RUN_MARKS: [&str; 4] = ["highlight", "code", "strike", "underline"];
pub const RUN_TONES: [&str; 6] = ["neutral", "primary", "success", "warning", "danger", "info"];

/// The drop cap's fixed numbers (the fixture's `dropCap`).
pub const DROP_CAP_MIN: u32 = 2;
pub const DROP_CAP_MAX: u32 = 4;
pub const DROP_CAP_LINES: u32 = 3;
pub const DROP_CAP_RATIO: f32 = 1.375;
/// `$spacing.xs`: the gap after the cap.
pub const DROP_CAP_GAP_TOKEN: &str = "xs";

/// One normalised span (only the attributes it sets).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TextRun {
    pub text: String,
    pub weight: Option<String>,
    pub tone: Option<String>,
    pub mark: Option<String>,
    pub link: Option<String>,
}

impl TextRun {
    fn same(&self, o: &TextRun) -> bool {
        self.weight == o.weight && self.tone == o.tone && self.mark == o.mark && self.link == o.link
    }

    /// `$type.weight.<w>` as a number (unset = `None`).
    pub fn font_weight(&self) -> Option<u16> {
        self.weight.as_deref().map(|w| match w {
            "medium" => 500,
            "semibold" => 600,
            "bold" => 700,
            _ => 400,
        })
    }

    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("text".into(), Value::String(self.text.clone()));
        for (k, v) in [("weight", &self.weight), ("tone", &self.tone), ("mark", &self.mark), ("link", &self.link)] {
            if let Some(v) = v {
                m.insert(k.into(), Value::String(v.clone()));
            }
        }
        Value::Object(m)
    }

    /// A run of the normal form (as [`to_json`] wrote it).
    pub fn from_json(v: &Value) -> Option<TextRun> {
        let o = v.as_object()?;
        let s = |k: &str| o.get(k).and_then(Value::as_str).map(str::to_string);
        Some(TextRun { text: s("text")?, weight: s("weight"), tone: s("tone"), mark: s("mark"), link: s("link") })
    }
}

fn run_text(v: Option<&Value>) -> Option<String> {
    match v? {
        v @ (Value::String(_) | Value::Number(_) | Value::Bool(_)) => Some(crate::format::display_string(v)),
        _ => None,
    }
}

fn pick(o: &Map<String, Value>, key: &str, allowed: &[&str]) -> Option<String> {
    o.get(key).and_then(Value::as_str).filter(|v| allowed.contains(v)).map(str::to_string)
}

/// `runs` normalised (`None` = not an array: the Text shows `text`).
pub fn text_runs(value: Option<&Value>) -> Option<Vec<TextRun>> {
    let arr = value?.as_array()?;
    let mut out: Vec<TextRun> = Vec::new();
    for entry in arr {
        let run = match entry {
            Value::String(s) => Some(TextRun { text: s.clone(), ..TextRun::default() }),
            Value::Object(o) => run_text(o.get("text")).map(|text| TextRun {
                text,
                weight: pick(o, "weight", &RUN_WEIGHTS),
                tone: pick(o, "tone", &RUN_TONES),
                mark: pick(o, "mark", &RUN_MARKS),
                link: o.get("link").and_then(Value::as_str).filter(|l| !l.is_empty()).map(str::to_string),
            }),
            _ => None,
        };
        let Some(run) = run.filter(|r| !r.text.is_empty()) else { continue };
        match out.last_mut() {
            Some(last) if last.same(&run) => last.text.push_str(&run.text),
            _ => out.push(run),
        }
    }
    Some(out)
}

/// The runs' text joined.
pub fn runs_text(runs: &[TextRun]) -> String {
    runs.iter().map(|r| r.text.as_str()).collect()
}

/// The normal form as the JSON the natives read (`props.runs`).
pub fn normal_runs(runs: &[TextRun]) -> Value {
    Value::Array(runs.iter().map(TextRun::to_json).collect())
}

/// `dropCap` as lines (0 = none) for a Text's props.
pub fn drop_cap_lines(props: &Map<String, Value>) -> u32 {
    if props.get("shimmer").and_then(Value::as_bool) == Some(true) || matches!(props.get("truncate").and_then(Value::as_str), Some("end" | "middle")) {
        return 0;
    }
    if props.get("lines").and_then(Value::as_f64).is_some_and(|l| l > 0.0) {
        return 0;
    }
    match props.get("dropCap") {
        Some(Value::Bool(true)) => DROP_CAP_LINES,
        Some(Value::Number(n)) => match n.as_f64() {
            Some(f) if f.is_finite() => {
                let n = f.round();
                if n < DROP_CAP_MIN as f64 {
                    0
                } else {
                    (n as u32).min(DROP_CAP_MAX)
                }
            }
            _ => 0,
        },
        _ => 0,
    }
}

fn is_joiner(c: char) -> bool {
    matches!(c as u32, 0x300..=0x36f | 0x1ab0..=0x1aff | 0x1dc0..=0x1dff | 0x20d0..=0x20ff | 0xfe20..=0xfe2f | 0xfe0e | 0xfe0f)
}

/// The drop cap off the runs (the fixture's `splitCases`): leading ASCII
/// spaces, tabs and line breaks drop; the cap = the first scalar plus the
/// combining marks and variation selectors after it, in the weight and tone
/// of the run holding it; `None` = no letter.
pub fn drop_cap_split(runs: &[TextRun]) -> Option<(TextRun, Vec<TextRun>)> {
    let mut rest: Vec<TextRun> = runs.to_vec();
    while let Some(first) = rest.first_mut() {
        let trimmed = first.text.trim_start_matches([' ', '\t', '\n', '\r']).to_string();
        first.text = trimmed;
        if !first.text.is_empty() {
            break;
        }
        rest.remove(0);
    }
    let first = rest.first_mut()?;
    let mut end = 0;
    for (i, c) in first.text.char_indices() {
        if i > 0 && !is_joiner(c) {
            break;
        }
        end = i + c.len_utf8();
    }
    let cap = TextRun { text: first.text[..end].to_string(), weight: first.weight.clone(), tone: first.tone.clone(), mark: None, link: None };
    first.text = first.text[end..].to_string();
    if first.text.is_empty() {
        rest.remove(0);
    }
    Some((cap, rest))
}

/// The cap's font size: `fontSize + (lines - 1) × lineHeight × 1.375`,
/// rounded to a whole px.
pub fn drop_cap_size(font_size: f32, line_height: f32, lines: u32) -> f32 {
    (font_size + (lines.saturating_sub(1)) as f32 * line_height * DROP_CAP_RATIO).round()
}

/// The layout tree's step for a Text: `runs` in the normal form, `text` =
/// the joined text, `dropCap` = its lines (removed when none).
pub fn normalise_text_props(props: &mut Map<String, Value>) {
    if let Some(runs) = text_runs(props.get("runs")) {
        props.insert("text".into(), Value::String(runs_text(&runs)));
        props.insert("runs".into(), normal_runs(&runs));
    } else {
        props.remove("runs");
    }
    match drop_cap_lines(props) {
        0 => {
            props.remove("dropCap");
        }
        n => {
            props.insert("dropCap".into(), Value::from(n));
        }
    }
}
