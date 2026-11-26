//! #NEW-146 — THE elicitation card's renderer half ×4
//! (`packages/domain-contract/fixtures/elicitation-card.json`, TS
//! `src/elicitation-card.ts`): a THIRD-PARTY MCP server's
//! `elicitation/create` arrives as a `question` card with the sidecar
//! `elicitation` {server, mode, required, url}. The renderers replay
//! `missingCases` (Submit stays disabled while a required field is empty)
//! and `actionCases` (what an answered card carries back); the engine
//! (eagent `engine/elicitation_card.rs`) builds the card.

use serde::Serialize;
use serde_json::Value;

/// The card's copy (the fixture's `copy`; a test locks it).
pub mod copy {
    pub const SUBMIT: &str = "Submit";
    pub const DECLINE: &str = "Decline";
    pub const CANCEL: &str = "Cancel";
    pub const OPEN_LINK: &str = "Open link";
    pub const DONE: &str = "I'm done";
    pub const MISSING: &str = "Fill in every required field.";
    pub const UNKNOWN_SERVER: &str = "MCP server";
    pub const WARNING_PUNYCODE: &str = "This address uses look-alike characters.";
    pub const WARNING_IP: &str = "This link goes to an IP address, not a domain.";
    pub const WARNING_INSECURE: &str = "This link is not encrypted (no https).";
    /// The specimen's four captions.
    pub const SPECIMEN_FORM: &str = "Form";
    pub const SPECIMEN_URL: &str = "URL";
    pub const SPECIMEN_WARNED: &str = "URL with warnings";
    pub const SPECIMEN_ANSWERED: &str = "Answered";
}

/// The fixture's `icons` (concept names).
pub mod icons {
    /// Beside the asking server's name in the card's header.
    pub const SERVER: &str = "settings-mcp";
    pub const URL: &str = "globe";
}

/// The fixture's `testIds`.
pub mod test_ids {
    pub const CARD: &str = "elicitation-card";
    pub const SERVER: &str = "elicitation-server";
    pub const SUBMIT: &str = "elicitation-submit";
    pub const DECLINE: &str = "elicitation-decline";
    pub const CANCEL: &str = "elicitation-cancel";
    pub const OPEN_LINK: &str = "elicitation-open-link";
    pub const DONE: &str = "elicitation-done";
}

/// The surface's data model root: every field binds `/<FORM_KEY>/<name>`.
pub const FORM_KEY: &str = "form";

/// `form` (a genUI form) or `url` (open a link, then `I'm done`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ElicitationMode {
    Form,
    Url,
}

/// The question card's `elicitation` sidecar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ElicitationSidecar {
    /// The name the team configured the asking server under.
    pub server: String,
    pub mode: ElicitationMode,
    /// The required field names (form mode).
    pub required: Vec<String>,
    /// The url to open (url mode).
    pub url: Option<String>,
}

/// The sidecar (tolerant read, the TS `elicitationField`): an object with a
/// string `server` and a `form` / `url` mode, else `None`.
pub fn elicitation_sidecar(raw: Option<&Value>) -> Option<ElicitationSidecar> {
    let object = raw?.as_object()?;
    let server = object.get("server")?.as_str()?;
    let mode = match object.get("mode")?.as_str()? {
        "form" => ElicitationMode::Form,
        "url" => ElicitationMode::Url,
        _ => return None,
    };
    Some(ElicitationSidecar {
        server: server.to_string(),
        mode,
        required: object
            .get("required")
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default(),
        url: object.get("url").and_then(Value::as_str).map(str::to_string),
    })
}

/// A value counts as given: a non-blank string, a finite number, any
/// boolean, a non-empty list.
fn given(value: Option<&Value>) -> bool {
    match value {
        Some(Value::String(text)) => !text.trim().is_empty(),
        Some(Value::Number(number)) => number.as_f64().is_some_and(f64::is_finite),
        Some(Value::Bool(_)) => true,
        Some(Value::Array(list)) => !list.is_empty(),
        _ => false,
    }
}

/// The required fields still empty in `surface_data` (`/form/<name>`):
/// Submit is disabled while any is, `copy::MISSING` under the buttons.
pub fn elicitation_missing(required: &[String], surface_data: Option<&Value>) -> Vec<String> {
    let form = surface_data.and_then(|data| data.get(FORM_KEY)).and_then(Value::as_object);
    required
        .iter()
        .filter(|name| !given(form.and_then(|form| form.get(name.as_str()))))
        .cloned()
        .collect()
}

/// What an answered (or dismissed) elicitation card carries back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Accept,
    Decline,
    Cancel,
}

/// Submit / I'm done = accept, Decline = decline, anything else (Cancel,
/// the ×, a label never offered, nothing) = cancel. Never an accept by
/// default. `card` = the card's `{answers?, answer?, dismissed?}`.
pub fn elicitation_action(card: &Value) -> Action {
    if card.get("dismissed").and_then(Value::as_bool) == Some(true) {
        return Action::Cancel;
    }
    let pick = card
        .get("answers")
        .and_then(Value::as_array)
        .and_then(|answers| answers.first())
        .and_then(Value::as_str)
        .or_else(|| card.get("answer").and_then(Value::as_str))
        .unwrap_or_default()
        .trim();
    if pick == copy::SUBMIT || pick == copy::DONE {
        Action::Accept
    } else if pick == copy::DECLINE {
        Action::Decline
    } else {
        Action::Cancel
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/elicitation-card.json");

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE).expect("elicitation-card.json parses")
    }

    #[test]
    fn elicitation_missing_replays_the_fixture() {
        let fixture = fixture();
        let cases = fixture["missingCases"].as_array().expect("missingCases");
        assert!(!cases.is_empty());
        for case in cases {
            let required: Vec<String> = serde_json::from_value(case["required"].clone()).expect("required");
            let data = case.get("surfaceData").filter(|data| !data.is_null());
            let got = json!(elicitation_missing(&required, data));
            assert_eq!(got, case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn elicitation_action_replays_the_fixture() {
        let fixture = fixture();
        let cases = fixture["actionCases"].as_array().expect("actionCases");
        assert!(!cases.is_empty());
        for case in cases {
            let got = json!(elicitation_action(&case["card"]));
            assert_eq!(got, case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn elicitation_copy_icons_and_test_ids_are_the_fixture() {
        let fixture = fixture();
        let expected_copy = json!({
            "submit": copy::SUBMIT,
            "decline": copy::DECLINE,
            "cancel": copy::CANCEL,
            "openLink": copy::OPEN_LINK,
            "done": copy::DONE,
            "missing": copy::MISSING,
            "unknownServer": copy::UNKNOWN_SERVER,
            "warnings": {
                "punycode": copy::WARNING_PUNYCODE,
                "ip": copy::WARNING_IP,
                "insecure": copy::WARNING_INSECURE,
            },
            "specimen": {
                "form": copy::SPECIMEN_FORM,
                "url": copy::SPECIMEN_URL,
                "warned": copy::SPECIMEN_WARNED,
                "answered": copy::SPECIMEN_ANSWERED,
            },
        });
        assert_eq!(fixture["copy"], expected_copy);
        assert_eq!(fixture["icons"], json!({ "server": icons::SERVER, "url": icons::URL }));
        assert_eq!(
            fixture["testIds"],
            json!({
                "card": test_ids::CARD,
                "server": test_ids::SERVER,
                "submit": test_ids::SUBMIT,
                "decline": test_ids::DECLINE,
                "cancel": test_ids::CANCEL,
                "openLink": test_ids::OPEN_LINK,
                "done": test_ids::DONE,
            })
        );
        assert_eq!(fixture["formKey"], json!(FORM_KEY));
    }

    /// Every card case's sidecar reads back as itself (the tolerant read).
    #[test]
    fn elicitation_sidecar_reads_the_card_cases() {
        let fixture = fixture();
        for case in fixture["cardCases"].as_array().expect("cardCases") {
            let expected = &case["expected"];
            if expected.is_null() {
                continue;
            }
            let sidecar = elicitation_sidecar(expected.get("elicitation")).expect("a sidecar");
            assert_eq!(json!(sidecar), expected["elicitation"], "{}", case["name"]);
        }
        assert_eq!(elicitation_sidecar(Some(&json!({ "server": "x", "mode": "other" }))), None);
        assert_eq!(elicitation_sidecar(Some(&json!("form"))), None);
        assert_eq!(elicitation_sidecar(None), None);
    }
}
