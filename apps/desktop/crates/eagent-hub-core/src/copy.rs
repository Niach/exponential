//! The hub's thresholds and its own lines, rendered from
//! `packages/eagent/fixtures/rules.json` (compiled in), like `copy.ts`.

use std::sync::OnceLock;

use serde::Deserialize;
use serde_json::Value;

use crate::js;
use crate::model::Card;

const RULES_JSON: &str = include_str!("../../../../../packages/eagent/fixtures/rules.json");

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Caps {
    pub tree_max_depth: i64,
    pub tree_max_sessions: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reuse {
    pub max_ended_hours: i64,
    pub max_idle_hours: i64,
    pub max_children: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Watchdog {
    pub unclaimed_minutes: i64,
    pub nudge_minutes: i64,
    pub resume_minutes: i64,
    pub stuck_minutes: i64,
    pub background_stuck_minutes: i64,
    pub online_window_seconds: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerRules {
    pub caught_up_after_minutes: i64,
}

#[derive(Debug, Deserialize)]
pub struct ReportRules {
    pub max: usize,
    pub fallback: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskRules {
    pub plan_max: usize,
}

/// `rules.json`: the thresholds, the copy (kept as JSON, read by key) and the fixture defaults.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rules {
    pub caps: Caps,
    pub reuse: Reuse,
    pub watchdog: Watchdog,
    pub trigger: TriggerRules,
    pub report: ReportRules,
    pub ask: AskRules,
    pub assistant_silent_events: Vec<String>,
    pub copy: Value,
    pub defaults: Value,
}

pub fn rules() -> &'static Rules {
    static RULES: OnceLock<Rules> = OnceLock::new();
    RULES.get_or_init(|| serde_json::from_str(RULES_JSON).expect("packages/eagent/fixtures/rules.json parses"))
}

/// A copy line by its path under `copy`.
pub fn text(path: &[&str]) -> &'static str {
    let mut at = &rules().copy;
    for key in path {
        at = &at[*key];
    }
    at.as_str().unwrap_or_else(|| panic!("rules.json copy.{} is a string", path.join(".")))
}

/// `{name}` placeholders filled; an unknown name stays as written.
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let word = after.len() - after.trim_start_matches(|c: char| c.is_ascii_alphanumeric() || c == '_').len();
        if word > 0 && after[word..].starts_with('}') {
            let key = &after[..word];
            match values.iter().find(|(k, _)| *k == key) {
                Some((_, value)) => out.push_str(value),
                None => out.push_str(&rest[open..open + word + 2]),
            }
            rest = &after[word + 1..];
        } else {
            out.push('{');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

pub fn blocked_text(window: Option<&str>, resets_at: Option<&str>) -> String {
    let window = window.filter(|w| !w.is_empty());
    let resets_at = resets_at.filter(|r| !r.is_empty());
    let window = window.map_or(String::new(), |w| fill(text(&["blockedDetail", "window"]), &[("window", w)]));
    let resets = match resets_at {
        Some(r) => format!(
            "{}{}",
            if window.is_empty() { " (" } else { ", " },
            fill(text(&["blockedDetail", "resets"]), &[("resetsAt", r)])
        ),
        None if !window.is_empty() => ")".into(),
        None => String::new(),
    };
    fill(text(&["blocked"]), &[("detail", &format!("{window}{resets}"))])
}

pub fn unclaimed_text() -> String {
    fill(text(&["unclaimed"]), &[("minutes", &rules().watchdog.unclaimed_minutes.to_string())])
}

pub fn stuck_text(minutes: i64) -> String {
    fill(text(&["stuck"]), &[("minutes", &minutes.to_string())])
}

pub fn unresumed_text() -> String {
    fill(text(&["unresumed"]), &[("minutes", &rules().watchdog.resume_minutes.to_string())])
}

pub fn pr_opened_text(number: &str, url: &str) -> String {
    fill(text(&["prOpened"]), &[("number", number), ("url", url)])
}

pub fn private_text(id: &str, lead: &str) -> String {
    fill(text(&["private"]), &[("lead", lead), ("id", id)])
}

/// Whole minutes since `alive` (None = NaN), never fewer than the stuck threshold (a skewed clock).
pub fn silent_minutes(alive: Option<i64>, now: i64) -> i64 {
    let minutes = alive.map_or(0, |alive| (now - alive).div_euclid(60_000));
    rules().watchdog.stuck_minutes.max(minutes)
}

fn clip(text: &str, max: usize) -> String {
    if js::len(text) > max {
        format!("{}…", js::trim_end(&js::slice(text, max - 1)))
    } else {
        text.to_string()
    }
}

/// What a listener's agent reads for a child's open question or plan
/// (`card` is a question; anything else renders as a plan).
pub fn ask_text(id: &str, card: &Card) -> String {
    let ask = |key: &str| text(&["askQuestion", key]);
    if let Card::Question { question, options, multi, custom } = card {
        let options: Vec<&str> = options.iter().flatten().filter_map(Value::as_str).collect();
        let flags: Vec<&str> = [
            (*multi == Some(true)).then(|| ask("several")),
            (*custom != Some(false)).then(|| ask("freeText")),
        ]
        .into_iter()
        .flatten()
        .collect();
        let flags = if flags.is_empty() { String::new() } else { format!(" ({})", flags.join(", ")) };
        let lines = [
            format!("{}{question}", text(&["askQuestionLead"])),
            if options.is_empty() { String::new() } else { fill(ask("options"), &[("options", &options.join(" | ")), ("flags", &flags)]) },
            fill(ask("answer"), &[("id", id)]),
        ];
        return lines.into_iter().filter(|line| !line.is_empty()).collect::<Vec<_>>().join("\n");
    }
    let (plan, questions) = match card {
        Card::Plan { plan, questions } => (plan.as_str(), questions.as_deref().unwrap_or_default()),
        _ => ("", &[][..]),
    };
    let questions: Vec<String> = questions
        .iter()
        .map(|q| {
            let labels: Vec<&str> = q.options.iter().map(|o| o.label.as_str()).collect();
            format!("- {}: {} ({})", q.id, q.prompt, labels.join(" | "))
        })
        .collect();
    let blocks = [
        text(&["askPlanLead"]).to_string(),
        clip(plan, rules().ask.plan_max),
        if questions.is_empty() { String::new() } else { format!("{}\n{}", ask("questions"), questions.join("\n")) },
        fill(ask("decide"), &[("id", id)]),
    ];
    blocks.into_iter().filter(|block| !block.is_empty()).collect::<Vec<_>>().join("\n\n")
}

/// The neutral ask line for a listener whose readers may not see the child.
pub fn ask_hidden_text(id: &str, card: &Card) -> String {
    let lead = if matches!(card, Card::Plan { .. }) { "askPrivatePlanLead" } else { "askPrivateQuestionLead" };
    fill(text(&["askHidden"]), &[("lead", text(&[lead])), ("id", id)])
}

pub fn schedule_text(cron: &str, tz: Option<&str>, caught_up: bool) -> String {
    let tz = tz.filter(|tz| !tz.is_empty()).map_or(String::new(), |tz| fill(text(&["scheduleTz"]), &[("tz", tz)]));
    fill(
        text(&["schedule"]),
        &[("cron", cron), ("tz", &tz), ("caughtUp", if caught_up { text(&["scheduleCaughtUp"]) } else { "" })],
    )
}

pub fn event_text(name: &str, emitted_by: &str, payload: &Value) -> String {
    let from = rules().copy["eventFrom"][emitted_by].as_str().unwrap_or_else(|| text(&["eventFrom", "server"]));
    fill(text(&["event"]), &[("name", name), ("from", from), ("payload", &js::stringify_pretty(payload))])
}
