//! EXP-1324 — the Trace patterns, ONE model ×4.
//!
//! The transcript's tool rows and groups, the reasoning line, the subagent
//! line and the failed-tests line are all Trace lines (shadcn's Trace; the
//! desktop paints it in `ui::trace`). Mirror of
//! `packages/domain-contract/src/trace.ts`; the tests replay
//! `fixtures/trace.json` and compare WHOLE objects.

use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::work_card::work_duration_long;

/// A line's tint: `running` shimmers the title, `error` paints the line
/// destructive, `pending` dims it.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TraceStatus {
    Running,
    Error,
    Pending,
}

/// One painted Trace line.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TraceLine {
    /// Concept icon name (`trace-terminal`).
    pub icon: String,
    pub title: String,
    /// Muted trailing text (tabular figures); a subagent line paints it as
    /// the outline Badge.
    pub description: Option<String>,
    pub status: Option<TraceStatus>,
}

/// The fixture's `icons`.
pub mod icons {
    pub const TOOL: &str = "trace-tool";
    pub const REASONING: &str = "trace-reasoning";
    pub const SUBAGENT: &str = "trace-subagent";
    pub const FAILED: &str = "trace-failed";
    pub const CHEVRON: &str = "trace-chevron";
}

/// The fixture's `copy` and line templates.
pub mod copy {
    pub const OUTPUT: &str = "Output";
    pub const COPY: &str = "Copy";
    pub const COPIED: &str = "Copied";
    pub const UNNAMED_TOOL: &str = "Tool";
    pub const REASONING_RUNNING: &str = "Thinking";
    pub const REASONING_SETTLED: &str = "Thought for {duration}";
    pub const REASONING_BRIEF: &str = "Thought for a moment";
    pub const SUBAGENT_NAMED_RUNNING: &str = "Running the {label} agent";
    pub const SUBAGENT_NAMED_SETTLED: &str = "Ran the {label} agent";
    pub const SUBAGENT_NAMED_FAILED: &str = "The {label} agent failed";
    pub const SUBAGENT_UNNAMED_RUNNING: &str = "Running a subagent";
    pub const SUBAGENT_UNNAMED_SETTLED: &str = "Ran a subagent";
    pub const SUBAGENT_UNNAMED_FAILED: &str = "A subagent failed";
    pub const TESTS_TITLE: &str = "Failed: {failed} of {total} {tests}";
    pub const TESTS_TITLE_IN: &str = "Failed: {failed} of {total} {tests} in {suite}";
    pub const TEST: &str = "test";
    pub const TESTS: &str = "tests";
}

/// `fixtures/trace.json` `geometry` (px). `SM` = the transcript + activity
/// scale, `DEFAULT` = the styleguide's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceGeometry {
    pub font_size: f32,
    pub line_height: f32,
    pub icon: f32,
    pub gap: f32,
    pub line_gap: f32,
    pub chevron_gap: f32,
    pub content_top: f32,
    pub content_bottom: f32,
    pub rail_inset: f32,
    pub rail_width: f32,
    pub content_inset: f32,
    pub content_trailing: f32,
}

pub mod geometry {
    use super::TraceGeometry;

    pub const DEFAULT: TraceGeometry = TraceGeometry {
        font_size: 14.0,
        line_height: 20.0,
        icon: 16.0,
        gap: 8.0,
        line_gap: 8.0,
        chevron_gap: 4.0,
        content_top: 8.0,
        content_bottom: 4.0,
        rail_inset: 7.0,
        rail_width: 1.0,
        content_inset: 24.0,
        content_trailing: 4.0,
    };
    pub const SM: TraceGeometry = TraceGeometry {
        font_size: 12.0,
        line_height: 18.0,
        icon: 14.0,
        gap: 6.0,
        line_gap: 6.0,
        chevron_gap: 4.0,
        content_top: 6.0,
        content_bottom: 4.0,
        rail_inset: 6.0,
        rail_width: 1.0,
        content_inset: 20.0,
        content_trailing: 4.0,
    };
    pub const DESCRIPTION_ALPHA: f32 = 0.7;
    pub const PENDING_OPACITY: f32 = 0.6;

    /// `geometry.codeBlock`: THE CodeBlock at size sm.
    pub mod code_block {
        pub const BORDER: f32 = 1.0;
        pub const FILL_ALPHA: f32 = 0.3;
        pub const PADDING: f32 = 10.0;
        pub const CONTENT_TOP_WITH_HEADER: f32 = 6.0;
        pub const TITLE_SIZE: f32 = 12.0;
        pub const FONT_SIZE: f32 = 12.0;
        pub const LINE_HEIGHT: f32 = 19.5;
        pub const COPY_BUTTON: f32 = 24.0;
        pub const COPY_ICON: f32 = 12.0;
        pub const HEADER_ACTIONS_OUTSET: f32 = 4.0;
        pub const MAX_LINES: usize = 16;
        pub const DESTRUCTIVE_BORDER_ALPHA: f32 = 0.4;
    }
}

fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (key, value) in values {
        out = out.replace(&format!("{{{key}}}"), value);
    }
    out
}

fn clean(value: Option<&str>) -> String {
    value.unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" ")
}

// ── Tool line ────────────────────────────────────────────────────────────────

/// `tool.verbs[kind]` = (running, settled, blank, icon).
fn verbs(tool_kind: &str) -> Option<(&'static str, &'static str, &'static str, &'static str)> {
    Some(match tool_kind {
        "execute" => ("Running", "Ran", "a command", "trace-terminal"),
        "read" => ("Reading", "Read", "a file", "trace-read"),
        "edit" => ("Editing", "Edited", "a file", "trace-edit"),
        "delete" => ("Deleting", "Deleted", "a file", "trace-edit"),
        "move" => ("Moving", "Moved", "a file", "trace-edit"),
        "search" => ("Searching", "Searched", "the code", "trace-search"),
        "fetch" => ("Fetching", "Fetched", "a page", "trace-fetch"),
        "think" => ("Thinking", "Thought", "it through", "trace-reasoning"),
        "wait" => ("Waiting on", "Waited on", "a task", "trace-background"),
        _ => return None,
    })
}

/// A tool call as its Trace line: verb kinds read `{verb} {detail}` ("Ran
/// bun test"), a blank detail takes the kind's object ("Ran a command");
/// `other`/unknown kinds keep the tool's own name, the detail = description.
pub fn trace_tool(
    tool_kind: &str,
    name: &str,
    detail: Option<&str>,
    settled: bool,
    failed: bool,
) -> TraceLine {
    let status = if failed {
        Some(TraceStatus::Error)
    } else if settled {
        None
    } else {
        Some(TraceStatus::Running)
    };
    let detail = clean(detail);
    let Some((running, past, blank, icon)) = verbs(tool_kind) else {
        let name = clean(Some(name));
        return TraceLine {
            icon: icons::TOOL.to_string(),
            title: if name.is_empty() { copy::UNNAMED_TOOL.to_string() } else { name },
            description: (!detail.is_empty()).then_some(detail),
            status,
        };
    };
    let verb = if settled || failed { past } else { running };
    TraceLine {
        icon: icon.to_string(),
        title: format!("{verb} {}", if detail.is_empty() { blank } else { &detail }),
        description: None,
        status,
    }
}

// ── Reasoning line ───────────────────────────────────────────────────────────

/// "Thinking" (shimmering) while the thought streams; "Thought for 5s" once
/// closed; "Thought for a moment" under a second or without a duration.
pub fn trace_reasoning(running: bool, duration_ms: Option<u64>) -> TraceLine {
    if running {
        return TraceLine {
            icon: icons::REASONING.to_string(),
            title: copy::REASONING_RUNNING.to_string(),
            description: None,
            status: Some(TraceStatus::Running),
        };
    }
    let title = match duration_ms {
        Some(ms) if ms >= 1000 => {
            fill(copy::REASONING_SETTLED, &[("duration", &work_duration_long(ms))])
        }
        _ => copy::REASONING_BRIEF.to_string(),
    };
    TraceLine {
        icon: icons::REASONING.to_string(),
        title,
        description: None,
        status: None,
    }
}

// ── Subagent line ────────────────────────────────────────────────────────────

/// "Running the Explore agent" → "Ran the Explore agent" + the duration
/// (painted as the outline Badge) → "The Explore agent failed".
pub fn trace_subagent(
    label: &str,
    running: bool,
    failed: bool,
    duration_ms: Option<i64>,
) -> TraceLine {
    let label = clean(Some(label));
    let template = match (label.is_empty(), failed, running) {
        (false, true, _) => copy::SUBAGENT_NAMED_FAILED,
        (false, false, true) => copy::SUBAGENT_NAMED_RUNNING,
        (false, false, false) => copy::SUBAGENT_NAMED_SETTLED,
        (true, true, _) => copy::SUBAGENT_UNNAMED_FAILED,
        (true, false, true) => copy::SUBAGENT_UNNAMED_RUNNING,
        (true, false, false) => copy::SUBAGENT_UNNAMED_SETTLED,
    };
    TraceLine {
        icon: icons::SUBAGENT.to_string(),
        title: fill(template, &[("label", &label)]),
        description: duration_ms
            .filter(|ms| *ms >= 0)
            .map(|ms| work_duration_long(ms as u64)),
        status: if failed {
            Some(TraceStatus::Error)
        } else if running {
            Some(TraceStatus::Running)
        } else {
            None
        },
    }
}

// ── Failed tests line ────────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TestResults {
    pub passed: u64,
    pub failed: u64,
    pub total: u64,
    /// The ONE suite every failure names; `None` when none or several.
    pub suite: Option<String>,
}

struct Patterns {
    ansi: Regex,
    jest: Regex,
    vitest: Regex,
    cargo: Regex,
    xcode: Regex,
    gradle: Regex,
    pytest: Regex,
    bun_pass: Regex,
    bun_fail: Regex,
    fail_bun: Regex,
    fail_jest: Regex,
    fail_gradle: Regex,
    fail_pytest: Regex,
    fail_xcode: Regex,
    fail_cargo: Regex,
}

fn patterns() -> &'static Patterns {
    static PATTERNS: OnceLock<Patterns> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let re = |pattern: &str| Regex::new(pattern).expect("a valid pattern");
        Patterns {
            ansi: re(r"\x1b\[[0-?]*[ -/]*[@-~]"),
            jest: re(
                r"(?m)^Tests:\s+(?:(\d+) failed, )?(?:\d+ skipped, )?(?:\d+ todo, )?(?:(\d+) passed, )?(\d+) total",
            ),
            vitest: re(r"(?m)^\s*Tests\s+(?:(\d+) failed)?(?:\s*\|\s*)?(?:(\d+) passed)?[^(\n]*\((\d+)\)"),
            cargo: re(r"(?m)^test result: \w+\. (\d+) passed; (\d+) failed;"),
            xcode: re(r"(?m)^\s*Executed (\d+) tests?, with (\d+) failures?"),
            gradle: re(r"(?m)^(\d+) tests? completed, (\d+) failed"),
            pytest: re(
                r"(?m)^=+ (?:(\d+) failed)?(?:, )?(?:(\d+) passed)?[^=\n]* in [\d.]+s(?: \([^)]*\))? =+$",
            ),
            bun_pass: re(r"(?m)^\s*(\d+) pass$"),
            bun_fail: re(r"(?m)^\s*(\d+) fail$"),
            fail_bun: re(r"^\(fail\) (.+?) > "),
            fail_jest: re(r"^\s*FAIL\s+(\S+?)(?:\s+>\s+(.+?)\s+>)?(?:\s|$)"),
            fail_gradle: re(r"^(\S+) > .* FAILED$"),
            fail_pytest: re(r"^FAILED (\S+?)::"),
            fail_xcode: re(r"^Test Case '-\[(?:\w+\.)?(\w+) \w+\]' failed"),
            fail_cargo: re(r"^---- (?:(\S+)::)?\S+ stdout ----$"),
        }
    })
}

fn int(capture: Option<regex::Match<'_>>) -> u64 {
    capture.and_then(|m| m.as_str().parse().ok()).unwrap_or(0)
}

/// Parses a command's output as a test run's summary (jest, vitest, cargo,
/// xcodebuild, gradle, pytest, bun; the FIRST that matches wins). `None` =
/// not test output.
pub fn parse_test_results(output: Option<&str>) -> Option<TestResults> {
    let output = output.filter(|output| !output.is_empty())?;
    let p = patterns();
    let text = p.ansi.replace_all(output, "").replace("\r\n", "\n").replace('\r', "\n");
    let mut counts: Option<(u64, u64, u64)> = None; // passed, failed, total
    if let Some(m) = p.jest.captures(&text) {
        counts = Some((int(m.get(2)), int(m.get(1)), int(m.get(3))));
    }
    if counts.is_none() {
        if let Some(m) = p.vitest.captures(&text) {
            if m.get(1).is_some() || m.get(2).is_some() {
                counts = Some((int(m.get(2)), int(m.get(1)), int(m.get(3))));
            }
        }
    }
    if counts.is_none() {
        let mut seen = false;
        let (mut passed, mut failed) = (0, 0);
        for hit in p.cargo.captures_iter(&text) {
            seen = true;
            passed += int(hit.get(1));
            failed += int(hit.get(2));
        }
        if seen {
            counts = Some((passed, failed, passed + failed));
        }
    }
    if counts.is_none() {
        if let Some(last) = p.xcode.captures_iter(&text).last() {
            let total = int(last.get(1));
            let failed = int(last.get(2));
            counts = Some((total.saturating_sub(failed), failed, total));
        }
    }
    if counts.is_none() {
        if let Some(m) = p.gradle.captures(&text) {
            let total = int(m.get(1));
            let failed = int(m.get(2));
            counts = Some((total.saturating_sub(failed), failed, total));
        }
    }
    if counts.is_none() {
        if let Some(m) = p.pytest.captures(&text) {
            if m.get(1).is_some() || m.get(2).is_some() {
                let (failed, passed) = (int(m.get(1)), int(m.get(2)));
                counts = Some((passed, failed, passed + failed));
            }
        }
    }
    if counts.is_none() {
        let pass = p.bun_pass.captures(&text);
        let fail = p.bun_fail.captures(&text);
        if pass.is_some() || fail.is_some() {
            let passed = int(pass.as_ref().and_then(|m| m.get(1)));
            let failed = int(fail.as_ref().and_then(|m| m.get(1)));
            counts = Some((passed, failed, passed + failed));
        }
    }
    let (passed, failed, total) = counts?;
    Some(TestResults {
        passed,
        failed,
        total,
        suite: failed_suite(&text),
    })
}

/// `src/bench/StartupBenchmark.test.ts` → `StartupBenchmark`.
fn file_stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or("");
    name.split('.').next().unwrap_or("")
}

fn failed_suite(text: &str) -> Option<String> {
    let p = patterns();
    let mut suites: Vec<String> = Vec::new();
    let mut add = |value: Option<&str>| {
        let name = clean(value);
        if !name.is_empty() && !suites.contains(&name) {
            suites.push(name);
        }
    };
    for line in text.split('\n') {
        if let Some(m) = p.fail_bun.captures(line) {
            add(m.get(1).map(|m| m.as_str()));
        } else if let Some(m) = p.fail_jest.captures(line) {
            match m.get(2) {
                Some(suite) => add(Some(suite.as_str())),
                None => add(m.get(1).map(|m| file_stem(m.as_str()))),
            }
        } else if let Some(m) = p.fail_gradle.captures(line) {
            add(m.get(1).and_then(|m| m.as_str().rsplit('.').next()));
        } else if let Some(m) = p.fail_pytest.captures(line) {
            add(m.get(1).map(|m| file_stem(m.as_str())));
        } else if let Some(m) = p.fail_xcode.captures(line) {
            add(m.get(1).map(|m| m.as_str()));
        } else if let Some(m) = p.fail_cargo.captures(line) {
            add(m.get(1).map(|m| m.as_str()));
        }
    }
    (suites.len() == 1).then(|| suites.remove(0))
}

/// The failed-tests line for a SETTLED command whose output parsed with at
/// least one failure; `None` otherwise.
pub fn trace_failed_tests(results: Option<&TestResults>) -> Option<TraceLine> {
    let results = results.filter(|results| results.failed > 0)?;
    let tests = if results.total == 1 { copy::TEST } else { copy::TESTS };
    let failed = results.failed.to_string();
    let total = results.total.to_string();
    let title = match &results.suite {
        Some(suite) => fill(
            copy::TESTS_TITLE_IN,
            &[("failed", &failed), ("total", &total), ("tests", tests), ("suite", suite)],
        ),
        None => fill(
            copy::TESTS_TITLE,
            &[("failed", &failed), ("total", &total), ("tests", tests)],
        ),
    };
    Some(TraceLine {
        icon: icons::FAILED.to_string(),
        title,
        description: None,
        status: Some(TraceStatus::Error),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/trace.json");

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE).expect("the fixture parses")
    }

    fn line(value: &Value) -> Option<TraceLine> {
        serde_json::from_value(value.clone()).expect("a trace line")
    }

    fn str_of<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
        value.get(key).and_then(Value::as_str)
    }

    #[test]
    fn tool_cases() {
        let fixture = fixture();
        let cases = fixture["cases"]["tool"].as_array().unwrap();
        assert!(cases.len() >= 10);
        for case in cases {
            let input = &case["input"];
            let actual = trace_tool(
                str_of(input, "toolKind").unwrap(),
                str_of(input, "name").unwrap(),
                str_of(input, "detail"),
                input["settled"].as_bool().unwrap(),
                input["failed"].as_bool().unwrap(),
            );
            assert_eq!(Some(actual), line(&case["expected"]), "{}", case["name"]);
        }
    }

    #[test]
    fn reasoning_cases() {
        let fixture = fixture();
        for case in fixture["cases"]["reasoning"].as_array().unwrap() {
            let input = &case["input"];
            let actual = trace_reasoning(
                input["running"].as_bool().unwrap(),
                input["durationMs"].as_u64(),
            );
            assert_eq!(Some(actual), line(&case["expected"]), "{}", case["name"]);
        }
    }

    #[test]
    fn subagent_cases() {
        let fixture = fixture();
        for case in fixture["cases"]["subagent"].as_array().unwrap() {
            let input = &case["input"];
            let actual = trace_subagent(
                str_of(input, "label").unwrap(),
                input["running"].as_bool().unwrap(),
                input["failed"].as_bool().unwrap(),
                input["durationMs"].as_i64(),
            );
            assert_eq!(Some(actual), line(&case["expected"]), "{}", case["name"]);
        }
    }

    #[test]
    fn test_cases() {
        let fixture = fixture();
        for case in fixture["cases"]["tests"].as_array().unwrap() {
            let results = parse_test_results(str_of(case, "output"));
            let expected: Option<TestResults> =
                serde_json::from_value(case["expected"]["results"].clone()).unwrap();
            assert_eq!(results, expected, "{}", case["name"]);
            assert_eq!(
                trace_failed_tests(results.as_ref()),
                line(&case["expected"]["line"]),
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn the_constants_match_the_fixture() {
        let fixture = fixture();
        let verbs_json = fixture["tool"]["verbs"].as_object().unwrap();
        for (kind, value) in verbs_json {
            let (running, settled, blank, icon) = verbs(kind).expect(kind);
            assert_eq!(value["running"], running, "{kind}");
            assert_eq!(value["settled"], settled, "{kind}");
            assert_eq!(value["blank"], blank, "{kind}");
            assert_eq!(value["icon"], icon, "{kind}");
        }
        assert_eq!(fixture["tool"]["unnamed"], copy::UNNAMED_TOOL);
        assert_eq!(fixture["copy"]["output"], copy::OUTPUT);
        assert_eq!(fixture["copy"]["copy"], copy::COPY);
        assert_eq!(fixture["copy"]["copied"], copy::COPIED);
        assert_eq!(fixture["reasoning"]["running"], copy::REASONING_RUNNING);
        assert_eq!(fixture["reasoning"]["settled"], copy::REASONING_SETTLED);
        assert_eq!(fixture["reasoning"]["brief"], copy::REASONING_BRIEF);
        assert_eq!(fixture["tests"]["title"], copy::TESTS_TITLE);
        assert_eq!(fixture["tests"]["titleIn"], copy::TESTS_TITLE_IN);
        assert_eq!(fixture["icons"]["tool"], icons::TOOL);
        assert_eq!(fixture["icons"]["reasoning"], icons::REASONING);
        assert_eq!(fixture["icons"]["subagent"], icons::SUBAGENT);
        assert_eq!(fixture["icons"]["failed"], icons::FAILED);
        assert_eq!(fixture["icons"]["chevron"], icons::CHEVRON);
        for (key, geometry) in [("default", geometry::DEFAULT), ("sm", geometry::SM)] {
            let json = &fixture["geometry"][key];
            for (field, value) in [
                ("fontSize", geometry.font_size),
                ("lineHeight", geometry.line_height),
                ("icon", geometry.icon),
                ("gap", geometry.gap),
                ("lineGap", geometry.line_gap),
                ("chevronGap", geometry.chevron_gap),
                ("contentTop", geometry.content_top),
                ("contentBottom", geometry.content_bottom),
                ("railInset", geometry.rail_inset),
                ("railWidth", geometry.rail_width),
                ("contentInset", geometry.content_inset),
                ("contentTrailing", geometry.content_trailing),
            ] {
                assert_eq!(json[field].as_f64(), Some(value as f64), "{key}.{field}");
            }
        }
        let code = &fixture["geometry"]["codeBlock"];
        use geometry::code_block as cb;
        for (field, value) in [
            ("border", cb::BORDER),
            ("fillAlpha", cb::FILL_ALPHA),
            ("padding", cb::PADDING),
            ("contentTopWithHeader", cb::CONTENT_TOP_WITH_HEADER),
            ("titleSize", cb::TITLE_SIZE),
            ("fontSize", cb::FONT_SIZE),
            ("lineHeight", cb::LINE_HEIGHT),
            ("copyButton", cb::COPY_BUTTON),
            ("copyIcon", cb::COPY_ICON),
            ("headerActionsOutset", cb::HEADER_ACTIONS_OUTSET),
            ("destructiveBorderAlpha", cb::DESTRUCTIVE_BORDER_ALPHA),
        ] {
            assert!((code[field].as_f64().unwrap() - value as f64).abs() < 1e-6, "codeBlock.{field}");
        }
        assert_eq!(code["maxLines"].as_u64(), Some(cb::MAX_LINES as u64));
        assert!(
            (fixture["geometry"]["descriptionAlpha"].as_f64().unwrap()
                - geometry::DESCRIPTION_ALPHA as f64)
                .abs()
                < 1e-6
        );
        assert!(
            (fixture["geometry"]["pendingOpacity"].as_f64().unwrap()
                - geometry::PENDING_OPACITY as f64)
                .abs()
                < 1e-6
        );
    }
}
