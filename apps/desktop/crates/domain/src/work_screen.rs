//! NEW-59: THE work screen's picker, the twin of
//! `packages/domain-contract/src/work-screen.ts` (`copy` + `options` + the
//! picker keys), locked by `fixtures/work-screen.json` (`optionCases`,
//! `pickerKeyCases`, #EXP-1514 `spaceOptionCases` and `spaceKeyCases` replay
//! below). The gate and the overlay rule live in the desktop `epilot` crate.

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

/// #FEED-164: a string-keyed picker's keys (`pickerKeys`).
#[derive(Debug, Clone, Deserialize)]
pub struct WorkScreenPickerKeys {
    pub auto: String,
    pub display: String,
    /// #EXP-1514: the agent space's key.
    pub space: String,
}

/// #EXP-1514: the reserved pick `value` (agents work in an agent space only)
/// and the picker's words for it (`space`, apart from `copy`).
#[derive(Debug, Clone, Deserialize)]
pub struct WorkScreenSpace {
    pub value: String,
    pub label: String,
    pub only: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkScreenSpec {
    pub copy: WorkScreenCopy,
    #[serde(rename = "testIds")]
    pub test_ids: WorkScreenTestIds,
    #[serde(rename = "pickerKeys")]
    pub picker_keys: WorkScreenPickerKeys,
    pub space: WorkScreenSpace,
}

/// `fixtures/work-screen.json` minus its cases.
pub static WORK_SCREEN: LazyLock<WorkScreenSpec> =
    LazyLock::new(|| serde_json::from_str(WORK_SCREEN_JSON).expect("work-screen.json parses"));

/// One display as the device reports it in `doctor.displays`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WorkScreenDisplay {
    /// #FEED-161: stable across reconnects and reorders (the stored value).
    pub id: String,
    /// 1-based position, for the label only.
    pub number: u32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub primary: bool,
}

/// One picker option: `value` = the stored `launch_defaults.computerUseDisplayId`
/// (`None` = Auto).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkScreenOption {
    pub value: Option<String>,
    pub label: String,
}

const SEP: &str = " · ";

fn display_label(number: u32) -> String {
    WORK_SCREEN.copy.display.replace("{number}", &number.to_string())
}

/// The Work screen picker's options: Auto, the agent space, the reported
/// displays, then a stored display id the device no longer reports (labelled
/// Not connected). #EXP-1514: while the device offers a space
/// (`space_offered`) Auto says it lands there and Agent space only follows
/// it; a stored space stays listed in that place, never Not connected.
pub fn work_screen_options(
    displays: &[WorkScreenDisplay],
    stored: Option<&str>,
    space_offered: bool,
) -> Vec<WorkScreenOption> {
    let copy = &WORK_SCREEN.copy;
    let space = &WORK_SCREEN.space;
    let auto = if space_offered { format!("{}{SEP}{}", copy.auto, space.label) } else { copy.auto.clone() };
    let mut out = vec![WorkScreenOption { value: None, label: auto }];
    if space_offered || stored == Some(space.value.as_str()) {
        out.push(WorkScreenOption { value: Some(space.value.clone()), label: space.only.clone() });
    }
    for display in displays {
        let mut label = format!("{}{SEP}{} × {}", display_label(display.number), display.width, display.height);
        if display.primary {
            label.push_str(SEP);
            label.push_str(&copy.primary);
        }
        out.push(WorkScreenOption { value: Some(display.id.clone()), label });
    }
    if let Some(stored) = stored {
        if !out.iter().any(|option| option.value.as_deref() == Some(stored)) {
            out.push(WorkScreenOption { value: Some(stored.to_string()), label: copy.not_connected.clone() });
        }
    }
    out
}

/// #FEED-164: the picker key of an option value (`None` = Auto): Auto's
/// key, #EXP-1514 the agent space's key, else the display prefix + the id,
/// so no id can read as Auto or as the space.
pub fn work_screen_picker_key(value: Option<&str>) -> String {
    let keys = &WORK_SCREEN.picker_keys;
    match value {
        None => keys.auto.clone(),
        Some(value) if value == WORK_SCREEN.space.value => keys.space.clone(),
        Some(id) => format!("{}{id}", keys.display),
    }
}

/// #FEED-164: the stored value (the bare display id, #EXP-1514 the space's
/// value, `None` = Auto) of a picker key.
pub fn work_screen_picked_id(key: &str) -> Option<String> {
    if key == WORK_SCREEN.picker_keys.space {
        return Some(WORK_SCREEN.space.value.clone());
    }
    key.strip_prefix(WORK_SCREEN.picker_keys.display.as_str()).map(str::to_string)
}

/// device-doctor.json's agent space row (#EXP-1514).
pub const AGENT_SPACE_ITEM: &str = "agent_space";

/// #EXP-1514: whether the device offers an agent space, read by the picker:
/// its doctor carries the `agent_space` item in state `ok`. `items` = each
/// doctor item's key and whether its state is `ok`.
pub fn space_offered<'a>(items: impl IntoIterator<Item = (&'a str, bool)>) -> bool {
    items.into_iter().any(|(key, ok)| key == AGENT_SPACE_ITEM && ok)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn replay_options(cases: &str) {
        let fixture: Value = serde_json::from_str(WORK_SCREEN_JSON).unwrap();
        let cases = fixture[cases].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let displays: Vec<WorkScreenDisplay> =
                serde_json::from_value(case["displays"].clone()).unwrap_or_default();
            let stored = case["stored"].as_str();
            // #EXP-1514: `spaceOptionCases` carry the offer; absent = false.
            let offered = case["spaceOffered"].as_bool().unwrap_or(false);
            let expect: Vec<WorkScreenOption> = case["expect"]
                .as_array()
                .unwrap()
                .iter()
                .map(|option| WorkScreenOption {
                    value: option["value"].as_str().map(str::to_string),
                    label: option["label"].as_str().unwrap().to_string(),
                })
                .collect();
            assert_eq!(work_screen_options(&displays, stored, offered), expect, "{name}");
        }
    }

    #[test]
    fn options_match_the_fixture() {
        replay_options("optionCases");
    }

    #[test]
    fn space_options_match_the_fixture() {
        replay_options("spaceOptionCases");
    }

    fn replay_keys(cases: &str) {
        let fixture: Value = serde_json::from_str(WORK_SCREEN_JSON).unwrap();
        let cases = fixture[cases].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let value = case["value"].as_str();
            let key = case["key"].as_str().unwrap();
            assert_eq!(work_screen_picker_key(value), key);
            assert_eq!(work_screen_picked_id(key).as_deref(), value);
        }
    }

    #[test]
    fn picker_keys_match_the_fixture() {
        replay_keys("pickerKeyCases");
    }

    #[test]
    fn space_keys_match_the_fixture() {
        replay_keys("spaceKeyCases");
        assert_eq!(WORK_SCREEN.picker_keys.space, WORK_SCREEN.space.value);
    }

    /// #EXP-1514 (device-doctor.json): only an `agent_space` item in state
    /// `ok` offers a space.
    #[test]
    fn space_offered_reads_the_agent_space_row() {
        assert!(space_offered([("git", true), ("computer_use", true), (AGENT_SPACE_ITEM, true)]));
        assert!(!space_offered([("computer_use", true), (AGENT_SPACE_ITEM, false)]));
        assert!(!space_offered([("computer_use", true)]));
        assert!(!space_offered([]));
    }

    #[test]
    fn copy_reads_the_fixture() {
        assert_eq!(WORK_SCREEN.copy.row, "Work screen");
        assert_eq!(WORK_SCREEN.test_ids.row, "device-settings-work-screen");
        assert_eq!(WORK_SCREEN.space.only, "Agent space only");
    }
}
