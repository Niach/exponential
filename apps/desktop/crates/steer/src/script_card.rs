//! EXP-1279 (EXP-1259 D13) — a code-mode call (`mcp__codemode__exec`,
//! contract toolKind `script`) renders as ONE card, "Ran a script · 59 calls ·
//! 3.0 s", with the calls it made grouped by tool and the script itself
//! behind a "Script" disclosure.
//!
//! Pure model, the Rust mirror of `packages/domain-contract/src/script-card.ts`,
//! byte-locked by `packages/domain-contract/fixtures/script-card.json` (the
//! same file web, iOS and Android replay).

use serde::{Deserialize, Serialize};

use crate::exp_tool::exp_tool_index;
use crate::work_card::work_duration_short;

/// Space, MIDDLE DOT, space — the TS `SCRIPT_CARD_SEPARATOR`.
pub const SCRIPT_CARD_SEPARATOR: &str = " \u{b7} ";

/// A tool call whose name ends in this is a code-mode script.
pub const SCRIPT_TOOL_NAME_SUFFIX: &str = "codemode__exec";

/// The fixture's `copy`.
pub mod copy {
    pub const RUNNING: &str = "Running a script";
    pub const DONE: &str = "Ran a script";
    pub const SCRIPT: &str = "Script";
    pub const CALL: &str = "call";
    pub const CALLS: &str = "calls";
    pub const FAILED: &str = "failed";
    pub const MORE_CALL: &str = "more call";
    pub const MORE_CALLS: &str = "more calls";
}

/// Whether a tool NAME is a code-mode script (`mcp__codemode__exec`).
pub fn is_script_tool(name: &str) -> bool {
    name.trim().ends_with(SCRIPT_TOOL_NAME_SUFFIX)
}

/// One entry of codemode's call log (`_meta["exponential/calls"]`).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct ScriptCall {
    /// `<server>.<tool>` as the script called it.
    pub tool: String,
    #[serde(default)]
    pub subject: Option<String>,
    pub ok: bool,
    #[serde(default)]
    pub ms: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScriptCardInput {
    pub running: bool,
    /// Total calls the script made (the log is capped).
    #[serde(default)]
    pub count: Option<u64>,
    #[serde(default)]
    pub calls: Option<Vec<ScriptCall>>,
    #[serde(default)]
    pub elapsed_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ScriptCardGroup {
    pub tool: String,
    /// An Exponential tool (draw the brand mark), else the generic tool glyph.
    pub exponential: bool,
    pub label: String,
    pub subject: Option<String>,
    /// `+54`, or `None`.
    pub more: Option<String>,
    /// `×55`, or `None`; replaced by `failed` when calls failed.
    pub count: Option<String>,
    /// `1 failed` (rose), or `None`.
    pub failed: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ScriptCard {
    pub title: String,
    pub meta: Option<String>,
    pub groups: Vec<ScriptCardGroup>,
    /// `12 more calls`: the log's cap cut these off.
    pub overflow: Option<String>,
}

/// `3.0 s` under a minute (integer tenths, half up), the Work card's coarse
/// format above.
pub fn script_duration(ms: u64) -> String {
    if ms >= 60_000 {
        return work_duration_short(ms);
    }
    // Integer tenths, half up: `{:.1}` rounds .x5 to even (1250 ms = "1.2 s"),
    // the fixture pins "1.3 s" on every platform.
    let tenths = (ms + 50) / 100;
    format!("{}.{} s", tenths / 10, tenths % 10)
}

fn plural(count: u64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// The TS `scriptCard(input)`.
pub fn script_card(input: &ScriptCardInput) -> ScriptCard {
    if input.running {
        return ScriptCard {
            title: copy::RUNNING.to_string(),
            meta: None,
            groups: Vec::new(),
            overflow: None,
        };
    }
    let calls = input.calls.as_deref().unwrap_or(&[]);
    let logged = calls.len() as u64;
    let count = input.count.unwrap_or(logged).max(logged);
    let mut meta = Vec::new();
    if count > 0 {
        meta.push(plural(count, copy::CALL, copy::CALLS));
    }
    if let Some(ms) = input.elapsed_ms {
        meta.push(script_duration(ms));
    }

    let mut order: Vec<(&str, Vec<&ScriptCall>)> = Vec::new();
    for call in calls {
        let key = call.tool.trim();
        match order.iter_mut().find(|(tool, _)| *tool == key) {
            Some((_, members)) => members.push(call),
            None => order.push((key, vec![call])),
        }
    }
    let groups = order
        .into_iter()
        .map(|(tool, members)| {
            let name = tool.rsplit('.').next().unwrap_or(tool);
            let index = exp_tool_index(name);
            let subject = members
                .iter()
                .map(|call| call.subject.as_deref().unwrap_or("").trim())
                .find(|subject| !subject.is_empty())
                .map(str::to_string);
            let failed = members.iter().filter(|call| !call.ok).count();
            ScriptCardGroup {
                tool: tool.to_string(),
                exponential: index.is_some(),
                label: match index {
                    Some(index) => domain::contract::EXP_TOOL_DONE[index].to_string(),
                    None => tool.to_string(),
                },
                more: (subject.is_some() && members.len() > 1)
                    .then(|| format!("+{}", members.len() - 1)),
                subject,
                count: (failed == 0 && members.len() > 1).then(|| format!("\u{d7}{}", members.len())),
                failed: (failed > 0).then(|| format!("{failed} {}", copy::FAILED)),
            }
        })
        .collect();
    let cut = count - logged;
    ScriptCard {
        title: copy::DONE.to_string(),
        meta: (!meta.is_empty()).then(|| meta.join(SCRIPT_CARD_SEPARATOR)),
        groups,
        overflow: (cut > 0).then(|| {
            format!("{cut} {}", if cut == 1 { copy::MORE_CALL } else { copy::MORE_CALLS })
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/script-card.json");

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        copy: serde_json::Value,
        tool_name_suffix: String,
        cases: Vec<Case>,
        durations: Vec<DurationCase>,
    }

    #[derive(Deserialize)]
    struct DurationCase {
        ms: u64,
        expected: String,
    }

    #[derive(Deserialize)]
    struct Case {
        name: String,
        input: ScriptCardInput,
        expected: ScriptCard,
    }

    #[test]
    fn every_fixture_case_matches_the_whole_card() {
        let fixture: Fixture = serde_json::from_str(FIXTURE).expect("the fixture parses");
        assert!(fixture.cases.len() >= 6);
        for case in fixture.cases {
            assert_eq!(script_card(&case.input), case.expected, "{}", case.name);
        }
    }

    #[test]
    fn every_fixture_duration_matches() {
        let fixture: Fixture = serde_json::from_str(FIXTURE).expect("the fixture parses");
        assert!(fixture.durations.len() >= 6);
        for case in fixture.durations {
            assert_eq!(script_duration(case.ms), case.expected, "{} ms", case.ms);
        }
    }

    #[test]
    fn the_constants_match_the_fixture() {
        let fixture: Fixture = serde_json::from_str(FIXTURE).expect("the fixture parses");
        assert_eq!(fixture.tool_name_suffix, SCRIPT_TOOL_NAME_SUFFIX);
        for (key, value) in [
            ("running", copy::RUNNING),
            ("done", copy::DONE),
            ("script", copy::SCRIPT),
            ("call", copy::CALL),
            ("calls", copy::CALLS),
            ("failed", copy::FAILED),
            ("moreCall", copy::MORE_CALL),
            ("moreCalls", copy::MORE_CALLS),
        ] {
            assert_eq!(fixture.copy[key], value, "copy.{key}");
        }
        assert!(is_script_tool("mcp__codemode__exec"));
        assert!(!is_script_tool("mcp__codemode__describe"));
    }
}
