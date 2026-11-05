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

/// VAPP-231 (fixture `claim`): the MCP header a host's claim rides on.
pub const VIA_APP_HEADER: &str = "X-Exp-Via-App";

/// VAPP-231: a host's claim that a write comes from its app (`viaApp` on a
/// records write); the server checks it, and it only adds the caption.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViaAppClaim {
    pub app_id: String,
    pub version: i64,
}

impl ViaAppClaim {
    /// The MCP header's value, `{appId}@{version}`.
    pub fn header_value(&self) -> String {
        format!("{}@{}", self.app_id, self.version)
    }
}

/// The claim of a host over `app_id`; `None` without an app or a published
/// version.
pub fn via_app_claim(app_id: Option<&str>, published_version: Option<i64>) -> Option<ViaAppClaim> {
    let app_id = app_id.filter(|id| !id.is_empty())?;
    let version = published_version.filter(|version| *version > 0)?;
    Some(ViaAppClaim { app_id: app_id.to_string(), version })
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
    fn replays_the_claim_cases() {
        let fixture = fixture();
        assert_eq!(fixture["claim"]["header"].as_str(), Some(VIA_APP_HEADER));
        let cases = fixture["claim"]["claimCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let claim = via_app_claim(case["appId"].as_str(), case["publishedVersion"].as_i64());
            assert_eq!(serde_json::to_value(&claim).unwrap(), case["claim"], "{}", case["name"]);
            assert_eq!(claim.map(|c| c.header_value()).as_deref(), case["header"].as_str(), "{}", case["name"]);
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
