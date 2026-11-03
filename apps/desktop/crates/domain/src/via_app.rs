//! #VAPP-229 (`packages/domain-contract/fixtures/via-app.json`): the
//! `via <app>` caption of every row an app caused (issues, sessions,
//! records; comments read `reporter_reply`'s `captions.viaApp`, the same
//! string). Mirrors TS `packages/domain-contract/src/via-app.ts`; the app
//! name is resolved by the caller against the synced, non-deleted apps (an
//! unknown or deleted app = `None` = no caption).

/// `copy.viaApp`.
const VIA_APP: &str = "via {app}";
/// `copy.separator`.
pub const SEPARATOR: &str = " · ";

/// `via {app}`, or `None` without a (resolved, non-empty) app name.
pub fn via_app_caption(app_name: Option<&str>) -> Option<String> {
    app_name.filter(|name| !name.is_empty()).map(|name| VIA_APP.replace("{app}", name))
}

/// `text` with the caption appended after [`SEPARATOR`]; the text alone
/// without an app.
pub fn with_via_app(text: &str, app_name: Option<&str>) -> String {
    match via_app_caption(app_name) {
        Some(caption) => format!("{text}{SEPARATOR}{caption}"),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/via-app.json");

    fn fixture() -> serde_json::Value {
        serde_json::from_str(FIXTURE).unwrap()
    }

    #[test]
    fn copy_matches_the_fixture() {
        let fixture = fixture();
        assert_eq!(fixture["copy"]["viaApp"].as_str(), Some(VIA_APP));
        assert_eq!(fixture["copy"]["separator"].as_str(), Some(SEPARATOR));
        assert_eq!(fixture["copy"].as_object().unwrap().len(), 2, "a new string needs a test");
    }

    #[test]
    fn replays_the_caption_cases() {
        let fixture = fixture();
        let cases = fixture["captionCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            assert_eq!(
                via_app_caption(case["viaAppName"].as_str()).as_deref(),
                case["text"].as_str(),
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn replays_the_append_cases() {
        let fixture = fixture();
        let cases = fixture["appendCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            assert_eq!(
                with_via_app(case["text"].as_str().unwrap(), case["viaAppName"].as_str()),
                case["expected"].as_str().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
}
