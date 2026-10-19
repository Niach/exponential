//! EXP-1280 custom plan mode: THE plan / question card's behaviour ×4
//! (`packages/domain-contract/src/plan-card.ts`'s twin, iOS ExpCore
//! `PlanCard.swift`, Android `domain/PlanCard.kt`). Byte-locked by
//! `fixtures/plan-card.json` (its `$comment` = the render rules); pure.
//!
//! Public API (the device pickup + the desktop UI):
//! * [`answer_text`]: the text the agent reads as its next turn once a card
//!   is answered (the device delivers it);
//! * [`answer_issue`]: why an answer cannot be sent yet ([`AnswerIssue`],
//!   its words = [`AnswerIssue::copy`]);
//! * [`resolve_upload`]: the card once a picture upload landed;
//! * [`picture_for`]: the attachment a picture label resolved to;
//! * [`option_rows`]: text-only options = the option list, any picture =
//!   tiles ([`OptionRows`]);
//! * [`stable_json`]: compact JSON with sorted keys at every depth;
//! * [`PLAN_CARD`]: the copy, test ids, tile geometry and answer-text
//!   templates ([`PlanCardSpec`]).
//!
//! Cards are read as `serde_json::Value` (the stored `messages.card` jsonb),
//! tolerant of missing/null fields exactly like the TS reference.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::Deserialize;
use serde_json::{Map, Value};

const PLAN_CARD_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/plan-card.json");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AnswerIssueCopy {
    pub decision: String,
    pub missing: String,
    pub multi: String,
    pub unknown: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanCardCopy {
    pub answers: String,
    pub feedback: String,
    pub choose_one: String,
    pub choose_any: String,
    pub custom: String,
    pub surface: String,
    pub answer: String,
    /// A closed card's caption.
    pub answered: String,
    /// The in-flight caption while the answer posts.
    pub answering: String,
    /// The answered card's ONE feedback line (`{feedback}`).
    pub feedback_line: String,
    pub specimen: PlanCardSpecimenCopy,
    pub answer_issues: AnswerIssueCopy,
    pub plan_mode_gate: String,
    pub propose_next: String,
}

impl PlanCardCopy {
    /// `feedback_line` with `{feedback}` filled in.
    pub fn feedback_line(&self, feedback: &str) -> String {
        self.feedback_line.replace("{feedback}", feedback)
    }
}

/// The styleguide entry's three captions ×4.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PlanCardSpecimenCopy {
    pub pending: String,
    pub answered: String,
    pub approved: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PlanCardTestIds {
    pub card: String,
    pub question: String,
    pub option: String,
    pub tile: String,
    pub custom: String,
    pub surface: String,
    pub feedback: String,
    pub issue: String,
}

/// The picture tiles' geometry (px, `picture_aspect` = w / h).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanCardTiles {
    pub columns: u32,
    pub phone_columns: u32,
    pub gap: f32,
    pub picture_aspect: f32,
    pub radius: f32,
    pub selected_ring: f32,
    pub padding: f32,
    pub picture_radius: f32,
    /// The tile's border width.
    pub stroke: f32,
    /// The `editor-image` placeholder glyph size (drawn at 0.6).
    pub placeholder_glyph: f32,
    /// Picture → label and label → description.
    pub label_gap: f32,
    /// A closed card's unpicked tiles.
    pub inert_opacity: f32,
}

/// The rest of the card's geometry (px; `heading_scale` × the body size).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanCardCard {
    pub picture_radius: f32,
    pub picture_stroke: f32,
    pub surface_radius: f32,
    pub surface_padding: f32,
    pub surface_label_gap: f32,
    pub block_gap: f32,
    pub block_inner_gap: f32,
    pub options_gap: f32,
    pub row_min_height: f32,
    pub row_radius: f32,
    pub field_min_height: f32,
    pub inert_opacity: f32,
    pub list_indent: f32,
    pub heading_scale: f32,
}

/// The answer text's templates (`{plan}`, `{prompt}`, `{answer}`, …).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanCardAnswerText {
    pub approved: String,
    pub approved_fresh: String,
    pub rejected: String,
    pub answers: String,
    pub answer_line: String,
    pub feedback: String,
    pub surface_data: String,
    pub question: String,
    pub joiner: String,
}

/// `fixtures/plan-card.json` minus its cases.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanCardSpec {
    pub copy: PlanCardCopy,
    pub test_ids: PlanCardTestIds,
    pub tiles: PlanCardTiles,
    pub card: PlanCardCard,
    pub upload_scheme: String,
    pub answer_text: PlanCardAnswerText,
}

/// The card's copy, test ids, tile geometry and answer-text templates.
pub static PLAN_CARD: LazyLock<PlanCardSpec> =
    LazyLock::new(|| serde_json::from_str(PLAN_CARD_JSON).expect("plan-card.json parses"));

/// The card's copy (`PLAN_CARD.copy`).
pub fn copy() -> &'static PlanCardCopy {
    &PLAN_CARD.copy
}

/// The tile geometry (`PLAN_CARD.tiles`).
pub fn tiles() -> &'static PlanCardTiles {
    &PLAN_CARD.tiles
}

/// The card geometry (`PLAN_CARD.card`).
pub fn card() -> &'static PlanCardCard {
    &PLAN_CARD.card
}

/// The test ids (`PLAN_CARD.test_ids`).
pub fn test_ids() -> &'static PlanCardTestIds {
    &PLAN_CARD.test_ids
}

/// Why an answer cannot be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerIssue {
    /// A plan needs a decision.
    Decision,
    /// Approving needs every question answered (a question card: an answer).
    Missing,
    /// A single-choice question takes one answer.
    Multi,
    /// Without `custom`, every answer must be an option (or the question is unknown).
    Unknown,
}

impl AnswerIssue {
    /// The fixture key (`decision` | `missing` | `multi` | `unknown`).
    pub fn key(self) -> &'static str {
        match self {
            AnswerIssue::Decision => "decision",
            AnswerIssue::Missing => "missing",
            AnswerIssue::Multi => "multi",
            AnswerIssue::Unknown => "unknown",
        }
    }

    /// The words under the button (`copy.answerIssues`).
    pub fn copy(self) -> &'static str {
        let issues = &PLAN_CARD.copy.answer_issues;
        match self {
            AnswerIssue::Decision => &issues.decision,
            AnswerIssue::Missing => &issues.missing,
            AnswerIssue::Multi => &issues.multi,
            AnswerIssue::Unknown => &issues.unknown,
        }
    }
}

/// A person's answer in progress (`sessions.answer`'s input).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CardAnswerInput {
    /// A question card's free text (used when `answers` is not a list).
    pub answer: Option<String>,
    /// A plan: `{questionId: labels}`; a question card: labels.
    pub answers: Option<CardAnswers>,
    /// `approved` | `rejected` (plans).
    pub decision: Option<String>,
    pub fresh: Option<bool>,
    pub feedback: Option<String>,
    pub surface_data: Option<Value>,
}

/// The two shapes of `answers`.
#[derive(Debug, Clone, PartialEq)]
pub enum CardAnswers {
    /// A plan card's picks, by question id.
    ByQuestion(BTreeMap<String, Vec<String>>),
    /// A question card's picked labels.
    Labels(Vec<String>),
}

impl CardAnswerInput {
    /// Reads the fixture/wire shape (`{answer?, answers?, decision?, …}`).
    pub fn from_json(value: &Value) -> Self {
        let answers = match value.get("answers") {
            Some(Value::Array(items)) => Some(CardAnswers::Labels(strings(items))),
            Some(Value::Object(map)) => Some(CardAnswers::ByQuestion(
                map.iter()
                    .map(|(key, inner)| (key.clone(), inner.as_array().map(|a| strings(a)).unwrap_or_default()))
                    .collect(),
            )),
            _ => None,
        };
        CardAnswerInput {
            answer: value.get("answer").and_then(Value::as_str).map(str::to_owned),
            answers,
            decision: value.get("decision").and_then(Value::as_str).map(str::to_owned),
            fresh: value.get("fresh").and_then(Value::as_bool),
            feedback: value.get("feedback").and_then(Value::as_str).map(str::to_owned),
            surface_data: value.get("surfaceData").filter(|v| !v.is_null()).cloned(),
        }
    }
}

fn strings(items: &[Value]) -> Vec<String> {
    items.iter().filter_map(|item| item.as_str().map(str::to_owned)).collect()
}

fn str_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn array_field<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value.get(key).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

fn trimmed_picks(labels: &[String]) -> Vec<String> {
    labels.iter().map(|label| label.trim().to_owned()).filter(|label| !label.is_empty()).collect()
}

/// Why an answer cannot be sent (`None` = fine). A plan needs a decision;
/// approving needs every question answered; a single-choice question takes
/// one answer; without `custom` every answer must be an option's label. A
/// question card needs one answer (`answers`, else `answer`).
pub fn answer_issue(card: &Value, input: &CardAnswerInput) -> Option<AnswerIssue> {
    if str_field(card, "kind") == Some("plan") {
        let decision = input.decision.as_deref();
        if decision != Some("approved") && decision != Some("rejected") {
            return Some(AnswerIssue::Decision);
        }
        let empty = BTreeMap::new();
        let answers = match &input.answers {
            Some(CardAnswers::ByQuestion(map)) => map,
            _ => &empty,
        };
        let questions = array_field(card, "questions");
        for id in answers.keys() {
            if !questions.iter().any(|question| str_field(question, "id") == Some(id.as_str())) {
                return Some(AnswerIssue::Unknown);
            }
        }
        for question in questions {
            let id = str_field(question, "id").unwrap_or_default();
            let picked = trimmed_picks(answers.get(id).map(Vec::as_slice).unwrap_or(&[]));
            if picked.is_empty() {
                if decision == Some("approved") {
                    return Some(AnswerIssue::Missing);
                }
                continue;
            }
            let labels: Vec<String> = array_field(question, "options")
                .iter()
                .map(|option| str_field(option, "label").unwrap_or_default().to_owned())
                .collect();
            let multi = question.get("multi").and_then(Value::as_bool) == Some(true);
            let custom = question.get("custom").and_then(Value::as_bool) != Some(false);
            if let Some(issue) = option_issue(&labels, multi, custom, &picked) {
                return Some(issue);
            }
        }
        return None;
    }
    let picked = match &input.answers {
        Some(CardAnswers::Labels(labels)) => trimmed_picks(labels),
        _ => match input.answer.as_deref().map(str::trim) {
            Some(answer) if !answer.is_empty() => vec![answer.to_owned()],
            _ => Vec::new(),
        },
    };
    if picked.is_empty() {
        return Some(AnswerIssue::Missing);
    }
    let labels: Vec<String> = strings(array_field(card, "options"));
    let multi = card.get("multi").and_then(Value::as_bool) == Some(true);
    let custom = card.get("custom").and_then(Value::as_bool) != Some(false);
    option_issue(&labels, multi, custom, &picked)
}

fn option_issue(labels: &[String], multi: bool, custom: bool, picked: &[String]) -> Option<AnswerIssue> {
    if !multi && picked.len() > 1 {
        return Some(AnswerIssue::Multi);
    }
    if !custom && picked.iter().any(|label| !labels.iter().any(|option| option.trim() == label)) {
        return Some(AnswerIssue::Unknown);
    }
    None
}

/// The text the agent reads as its next message once the card is answered:
/// the decision line (a fresh approval carries the plan), the answers block
/// (answered questions only, card order), the surface's data model as
/// compact JSON with sorted keys, the feedback. Paragraphs joined by a blank
/// line.
pub fn answer_text(card: &Value) -> String {
    let t = &PLAN_CARD.answer_text;
    let mut parts: Vec<String> = Vec::new();
    let is_plan = str_field(card, "kind") == Some("plan");
    if is_plan {
        if str_field(card, "decision") == Some("approved") {
            if card.get("fresh").and_then(Value::as_bool) == Some(true) {
                parts.push(t.approved_fresh.replace("{plan}", str_field(card, "plan").unwrap_or_default()));
            } else {
                parts.push(t.approved.clone());
            }
        } else {
            parts.push(t.rejected.clone());
        }
        let answers = card.get("answers").and_then(Value::as_object);
        let lines: Vec<String> = array_field(card, "questions")
            .iter()
            .filter_map(|question| {
                let id = str_field(question, "id")?;
                let picked = strings(answers?.get(id)?.as_array()?);
                if picked.is_empty() {
                    return None;
                }
                Some(
                    t.answer_line
                        .replace("{prompt}", str_field(question, "prompt").unwrap_or_default())
                        .replace("{answer}", &picked.join(&t.joiner)),
                )
            })
            .collect();
        if !lines.is_empty() {
            let mut block = vec![t.answers.clone()];
            block.extend(lines);
            parts.push(block.join("\n"));
        }
    } else {
        let answers = strings(array_field(card, "answers"));
        let picked =
            if !answers.is_empty() { answers.join(&t.joiner) } else { str_field(card, "answer").unwrap_or_default().to_owned() };
        parts.push(
            t.question.replace("{question}", str_field(card, "question").unwrap_or_default()).replace("{answer}", &picked),
        );
    }
    if let Some(data) = card.get("surfaceData").filter(|v| v.is_object() || v.is_array()) {
        parts.push(t.surface_data.replace("{json}", &stable_json(data)));
    }
    if is_plan {
        if let Some(feedback) = str_field(card, "feedback").map(str::trim).filter(|f| !f.is_empty()) {
            parts.push(t.feedback.replace("{feedback}", feedback));
        }
    }
    parts.join("\n\n")
}

/// JSON with object keys sorted at every depth, compact (one byte sequence
/// ×4; integral floats print like JS, `2.0` → `2`).
pub fn stable_json(value: &Value) -> String {
    match value {
        Value::Array(items) => format!("[{}]", items.iter().map(stable_json).collect::<Vec<_>>().join(",")),
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
            let body = entries
                .into_iter()
                .map(|(key, inner)| format!("{}:{}", Value::String(key.clone()), stable_json(inner)))
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{body}}}")
        }
        Value::Number(number) => match number.as_f64() {
            // JS `String(n)`: an integral float below 1e21 prints its digits
            // (`2.0` → `2`, `1e20` → `100000000000000000000`, `-0` → `0`) —
            // through the float's own text, never an `i64` (which saturates
            // past ~9.22e18).
            Some(f) if number.is_f64() && f.fract() == 0.0 && f.abs() < 1e21 => {
                if f == 0.0 {
                    "0".to_string()
                } else {
                    format!("{f}")
                }
            }
            _ => number.to_string(),
        },
        other => other.to_string(),
    }
}

/// A landed picture upload.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedUpload {
    pub label: String,
    pub attachment_id: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// The card once `upload.label`'s upload landed: its `pictures` entry filled
/// and, on a plan, every `](upload:<label>)` embed pointed at the
/// attachment. An unknown label changes nothing.
pub fn resolve_upload(card: &Value, upload: &ResolvedUpload) -> Value {
    let pictures = array_field(card, "pictures");
    if !pictures.iter().any(|picture| str_field(picture, "label") == Some(upload.label.as_str())) {
        return card.clone();
    }
    let mut next = card.clone();
    let filled: Vec<Value> = pictures
        .iter()
        .map(|picture| {
            if str_field(picture, "label") != Some(upload.label.as_str()) {
                return picture.clone();
            }
            let mut entry = picture.as_object().cloned().unwrap_or_else(Map::new);
            entry.insert("attachmentId".into(), Value::String(upload.attachment_id.clone()));
            entry.insert("width".into(), upload.width.map(Value::from).unwrap_or(Value::Null));
            entry.insert("height".into(), upload.height.map(Value::from).unwrap_or(Value::Null));
            Value::Object(entry)
        })
        .collect();
    if let Some(object) = next.as_object_mut() {
        object.insert("pictures".into(), Value::Array(filled));
        if object.get("kind").and_then(Value::as_str) == Some("plan") {
            if let Some(plan) = object.get("plan").and_then(Value::as_str) {
                let from = format!("]({}{})", PLAN_CARD.upload_scheme, upload.label);
                let to = format!("](/api/attachments/{})", upload.attachment_id);
                let rewritten = plan.replace(&from, &to);
                object.insert("plan".into(), Value::String(rewritten));
            }
        }
    }
    next
}

/// The attachment a picture label resolved to (`None` = no label, no
/// pictures, unknown label or not uploaded yet). `pictures` = the card's
/// `pictures` value (an array, or null/absent).
pub fn picture_for(pictures: Option<&Value>, label: Option<&str>) -> Option<String> {
    let label = label?;
    pictures?
        .as_array()?
        .iter()
        .find(|picture| str_field(picture, "label") == Some(label))
        .and_then(|picture| str_field(picture, "attachmentId"))
        .map(str::to_owned)
}

/// How a question's options render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionRows {
    /// THE option rows (label + description, a check when picked).
    List,
    /// Picture tiles (`PLAN_CARD.tiles`).
    Tiles,
}

impl OptionRows {
    pub fn key(self) -> &'static str {
        match self {
            OptionRows::List => "list",
            OptionRows::Tiles => "tiles",
        }
    }
}

/// Text-only options = THE option list; any option with a picture = tiles.
/// Each item = an option's resolved attachment id (`None` = no picture),
/// e.g. `picture_for(card.get("pictures"), option.picture)`.
pub fn option_rows(attachment_ids: &[Option<String>]) -> OptionRows {
    if attachment_ids.iter().any(|id| id.as_deref().is_some_and(|id| !id.is_empty())) {
        OptionRows::Tiles
    } else {
        OptionRows::List
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(PLAN_CARD_JSON).unwrap()
    }

    fn cases(name: &str) -> Vec<Value> {
        let cases = fixture()[name].as_array().cloned().unwrap();
        assert!(!cases.is_empty(), "{name} empty");
        cases
    }

    #[test]
    fn spec_parses() {
        assert_eq!(PLAN_CARD.upload_scheme, "upload:");
        assert_eq!(copy().answer_issues.missing, "Answer every question to approve");
        assert_eq!(tiles().columns, 2);
        assert_eq!(test_ids().card, "plan-card");
    }

    #[test]
    fn spec_carries_the_look_and_captions() {
        let c = copy();
        for s in [&c.answered, &c.answering, &c.specimen.pending, &c.specimen.answered, &c.specimen.approved] {
            assert!(!s.is_empty());
        }
        assert_eq!(c.feedback_line("Ship it"), "Feedback: Ship it");
        let t = tiles();
        for v in [t.picture_radius, t.stroke, t.placeholder_glyph, t.label_gap, t.inert_opacity] {
            assert!(v > 0.0);
        }
        let k = card();
        for v in [
            k.picture_radius,
            k.picture_stroke,
            k.surface_radius,
            k.surface_padding,
            k.surface_label_gap,
            k.block_gap,
            k.block_inner_gap,
            k.options_gap,
            k.row_min_height,
            k.row_radius,
            k.field_min_height,
            k.inert_opacity,
            k.list_indent,
            k.heading_scale,
        ] {
            assert!(v > 0.0);
        }
        assert!(k.inert_opacity < 1.0 && k.heading_scale > 1.0);
    }

    #[test]
    fn answer_text_cases() {
        for case in cases("answerTextCases") {
            assert_eq!(answer_text(&case["card"]), case["expected"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn answer_issue_cases() {
        for case in cases("answerIssueCases") {
            let got = answer_issue(&case["card"], &CardAnswerInput::from_json(&case["input"])).map(AnswerIssue::key);
            assert_eq!(got, case["expected"].as_str(), "{}", case["name"]);
        }
    }

    #[test]
    fn issue_copy_resolves() {
        assert_eq!(AnswerIssue::Decision.copy(), "Pick a decision");
        assert_eq!(AnswerIssue::Multi.copy(), "This question takes one answer");
    }

    #[test]
    fn resolve_upload_cases() {
        for case in cases("resolveUploadCases") {
            let upload = &case["upload"];
            let resolved = resolve_upload(
                &case["card"],
                &ResolvedUpload {
                    label: upload["label"].as_str().unwrap().into(),
                    attachment_id: upload["attachmentId"].as_str().unwrap().into(),
                    width: upload["width"].as_u64().map(|w| w as u32),
                    height: upload["height"].as_u64().map(|h| h as u32),
                },
            );
            assert_eq!(resolved, case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn picture_for_cases() {
        for case in cases("pictureForCases") {
            let pictures = Some(&case["pictures"]).filter(|v| !v.is_null());
            let got = picture_for(pictures, case["label"].as_str());
            assert_eq!(got.as_deref(), case["expected"].as_str(), "{}", case["name"]);
        }
    }

    #[test]
    fn option_rows_cases() {
        for case in cases("optionRowsCases") {
            let ids: Vec<Option<String>> = case["options"]
                .as_array()
                .unwrap()
                .iter()
                .map(|option| option["attachmentId"].as_str().map(str::to_owned))
                .collect();
            assert_eq!(option_rows(&ids).key(), case["expected"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn stable_json_sorts_every_depth() {
        let value: Value = serde_json::from_str(r#"{"b":[{"z":1,"a":2.0}],"a":"x\"y","c":null}"#).unwrap();
        assert_eq!(stable_json(&value), r#"{"a":"x\"y","b":[{"a":2,"z":1}],"c":null}"#);
    }

    /// An integral float prints like JS `String(n)` up to 1e21 — including
    /// the whole numbers past `i64::MAX` (~9.22e18) that a cast saturated.
    #[test]
    fn stable_json_prints_integral_floats_like_js() {
        let value: Value = serde_json::from_str("[2.0, -3.0, 9300000000000000000.0, 1e20, -1e20, -0.0, 1.5, 7]").unwrap();
        assert_eq!(stable_json(&value), "[2,-3,9300000000000000000,100000000000000000000,-100000000000000000000,0,1.5,7]");
    }
}
