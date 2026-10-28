//! NEW-59: THE work screen's picker, the twin of
//! `packages/domain-contract/src/work-screen.ts` (`copy` + `options`), locked
//! by `fixtures/work-screen.json` (`optionCases` replay below). The gate and
//! the overlay rule live in the desktop `computer` crate.

use std::sync::LazyLock;

use serde::Deserialize;

const WORK_SCREEN_JSON: &str =
    include_str!("../../../../../packages/domain-contract/fixtures/work-screen.json");

/// The fixture's `copy` (the refusals stay with the computer hub).
#[derive(Debug, Clone, Deserialize)]
pub struct WorkScreenCopy {
    pub row: String,
    pub auto: String,
    pub display: String,
    pub primary: String,
    #[serde(rename = "notConnected")]
    pub not_connected: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkScreenTestIds {
    pub row: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkScreenSpec {
    pub copy: WorkScreenCopy,
    #[serde(rename = "testIds")]
    pub test_ids: WorkScreenTestIds,
}

/// `fixtures/work-screen.json` minus its cases.
pub static WORK_SCREEN: LazyLock<WorkScreenSpec> =
    LazyLock::new(|| serde_json::from_str(WORK_SCREEN_JSON).expect("work-screen.json parses"));

/// One display as the device reports it in `doctor.displays`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct WorkScreenDisplay {
    pub number: u32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub primary: bool,
}

/// One picker option: `value` = the stored `launch_defaults.computerUseDisplay`
/// (`None` = Auto).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkScreenOption {
    pub value: Option<u32>,
    pub label: String,
}

const SEP: &str = " · ";

fn display_label(number: u32) -> String {
    WORK_SCREEN.copy.display.replace("{number}", &number.to_string())
}

/// The Work screen picker's options: Auto, the reported displays, then a
/// stored display the device no longer reports.
pub fn work_screen_options(displays: &[WorkScreenDisplay], stored: Option<u32>) -> Vec<WorkScreenOption> {
    let copy = &WORK_SCREEN.copy;
    let mut out = vec![WorkScreenOption { value: None, label: copy.auto.clone() }];
    for display in displays {
        let mut label = format!("{}{SEP}{} × {}", display_label(display.number), display.width, display.height);
        if display.primary {
            label.push_str(SEP);
            label.push_str(&copy.primary);
        }
        out.push(WorkScreenOption { value: Some(display.number), label });
    }
    if let Some(stored) = stored {
        if !out.iter().any(|option| option.value == Some(stored)) {
            out.push(WorkScreenOption {
                value: Some(stored),
                label: format!("{}{SEP}{}", display_label(stored), copy.not_connected),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn options_match_the_fixture() {
        let fixture: Value = serde_json::from_str(WORK_SCREEN_JSON).unwrap();
        for case in fixture["optionCases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let displays: Vec<WorkScreenDisplay> =
                serde_json::from_value(case["displays"].clone()).unwrap_or_default();
            let stored = case["stored"].as_u64().map(|number| number as u32);
            let expect: Vec<WorkScreenOption> = case["expect"]
                .as_array()
                .unwrap()
                .iter()
                .map(|option| WorkScreenOption {
                    value: option["value"].as_u64().map(|number| number as u32),
                    label: option["label"].as_str().unwrap().to_string(),
                })
                .collect();
            assert_eq!(work_screen_options(&displays, stored), expect, "{name}");
        }
    }

    #[test]
    fn copy_reads_the_fixture() {
        assert_eq!(WORK_SCREEN.copy.row, "Work screen");
        assert_eq!(WORK_SCREEN.test_ids.row, "device-settings-work-screen");
    }
}
