//! VAPP-241 (`packages/domain-contract/fixtures/app-share.json`): an app's
//! PUBLIC LINKS. Mirrors TS `packages/domain-contract/src/app-share.ts`: a
//! link's state, the submission caps, the Guest byline and the Settings
//! row's subtitle. The guest page itself is web-only, so `shareFields` has
//! no twin here.

use std::sync::LazyLock;

use serde::Deserialize;

use crate::instant::parse_instant;
use crate::via_app::SEPARATOR;

const SHARE_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/app-share.json");

/// The fixture's `copy` (the strings a native screen shows).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareCopy {
    pub title: String,
    pub submit: String,
    pub view: String,
    pub off: String,
    pub on: String,
    pub expired: String,
    pub orphaned: String,
    pub expires_on: String,
    pub copy_link: String,
    pub new_link: String,
    pub guest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareCaps {
    pub per_ip_per_hour: u32,
    pub per_link_per_hour: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareTestIds {
    pub share_links: String,
    pub share_row: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppShareSpec {
    pub copy: ShareCopy,
    pub caps: ShareCaps,
    pub test_ids: ShareTestIds,
}

pub static APP_SHARE: LazyLock<AppShareSpec> =
    LazyLock::new(|| serde_json::from_str(SHARE_JSON).expect("app-share.json parses"));

/// `ShareLinkState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareLinkState {
    Off,
    On,
    Expired,
    Orphaned,
}

impl ShareLinkState {
    pub fn parse(state: &str) -> Option<Self> {
        match state {
            "off" => Some(Self::Off),
            "on" => Some(Self::On),
            "expired" => Some(Self::Expired),
            "orphaned" => Some(Self::Orphaned),
            _ => None,
        }
    }
}

/// `ShareMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareMode {
    Submit,
    View,
}

impl ShareMode {
    pub fn parse(mode: &str) -> Option<Self> {
        match mode {
            "submit" => Some(Self::Submit),
            "view" => Some(Self::View),
            _ => None,
        }
    }
}

/// The `app_share_links` columns a state reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShareLink<'a> {
    pub revoked_at: Option<&'a str>,
    pub expires_at: Option<&'a str>,
}

/// A link's state at `now` (`stateCases`); `declared` = the published
/// package still declares the share.
pub fn link_state(link: Option<ShareLink<'_>>, declared: bool, now: &str) -> ShareLinkState {
    let Some(link) = link.filter(|link| link.revoked_at.is_none()) else {
        return ShareLinkState::Off;
    };
    if !declared {
        return ShareLinkState::Orphaned;
    }
    match (link.expires_at.and_then(parse_instant), parse_instant(now)) {
        (Some(expires), Some(now)) if expires <= now => ShareLinkState::Expired,
        _ => ShareLinkState::On,
    }
}

/// May one more submission land (`capCases`)?
pub fn under_share_caps(ip_count: u32, link_count: u32) -> bool {
    let caps = &APP_SHARE.caps;
    ip_count < caps.per_ip_per_hour && link_count < caps.per_link_per_hour
}

/// The By/author label (`bylineCases`): a shared record reads Guest.
pub fn share_byline(share_name: Option<&str>, byline: &str) -> String {
    match share_name {
        Some(name) if !name.is_empty() => APP_SHARE.copy.guest.clone(),
        _ => byline.to_string(),
    }
}

/// Does a history event's `via` name a public link (`historyCases`)?
pub fn is_share_via(via: Option<&str>) -> bool {
    via.is_some_and(|via| via.starts_with("share:"))
}

/// A Settings row's Switch (`switchCases`): an orphaned link still reads on,
/// so its owner can revoke it.
pub fn share_switch_on(state: ShareLinkState) -> bool {
    matches!(state, ShareLinkState::On | ShareLinkState::Orphaned)
}

/// A Settings row's subtitle (`rowCases`).
pub fn share_row_subtitle(mode: ShareMode, state: ShareLinkState, expires_at: Option<&str>) -> String {
    let c = &APP_SHARE.copy;
    let mode_label = match mode {
        ShareMode::Submit => &c.submit,
        ShareMode::View => &c.view,
    };
    let state_label = match state {
        ShareLinkState::Off => &c.off,
        ShareLinkState::On => &c.on,
        ShareLinkState::Expired => &c.expired,
        ShareLinkState::Orphaned => &c.orphaned,
    };
    let mut parts = vec![mode_label.clone(), state_label.clone()];
    if let (ShareLinkState::On, Some(date)) = (state, expires_at.filter(|date| !date.is_empty())) {
        parts.push(c.expires_on.replace("{date}", date));
    }
    parts.join(SEPARATOR)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_records::{history_rows, RecordEvent};
    use serde_json::{json, Value};

    fn cases(key: &str) -> Vec<Value> {
        let fixture: Value = serde_json::from_str(SHARE_JSON).unwrap();
        let cases = fixture[key].as_array().unwrap().clone();
        assert!(!cases.is_empty(), "{key}");
        cases
    }

    #[test]
    fn replays_the_state_cases() {
        for case in cases("stateCases") {
            let link = case["link"].as_object().map(|link| ShareLink {
                revoked_at: link["revokedAt"].as_str(),
                expires_at: link["expiresAt"].as_str(),
            });
            let got = link_state(link, case["declared"].as_bool().unwrap(), case["now"].as_str().unwrap());
            assert_eq!(Some(got), ShareLinkState::parse(case["state"].as_str().unwrap()), "{}", case["name"]);
        }
    }

    #[test]
    fn replays_the_cap_cases() {
        for case in cases("capCases") {
            let got = under_share_caps(case["ipCount"].as_u64().unwrap() as u32, case["linkCount"].as_u64().unwrap() as u32);
            assert_eq!(got, case["allowed"].as_bool().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn replays_the_byline_cases() {
        for case in cases("bylineCases") {
            assert_eq!(
                share_byline(case["shareName"].as_str(), case["byline"].as_str().unwrap()),
                case["text"].as_str().unwrap(),
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn replays_the_switch_cases() {
        for case in cases("switchCases") {
            let state = ShareLinkState::parse(case["state"].as_str().unwrap()).unwrap();
            assert_eq!(share_switch_on(state), case["on"].as_bool().unwrap(), "{}", case["state"]);
        }
    }

    #[test]
    fn replays_the_row_cases() {
        for case in cases("rowCases") {
            let mode = ShareMode::parse(case["mode"].as_str().unwrap()).unwrap();
            let state = ShareLinkState::parse(case["state"].as_str().unwrap()).unwrap();
            assert_eq!(
                share_row_subtitle(mode, state, case["expiresAt"].as_str()),
                case["text"].as_str().unwrap(),
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn replays_the_history_cases() {
        for case in cases("historyCases") {
            let event = RecordEvent::from_value(&json!({
                "id": "e1",
                "op": "create",
                "before": null,
                "patch": {},
                "actor": case["actor"],
                "via": case["via"],
                "createdAt": "2026-11-04T10:00:00Z",
            }))
            .unwrap();
            let names = case["names"].as_object().unwrap();
            let rows = history_rows(&[event], |id| names.get(id).and_then(Value::as_str).map(str::to_string), "?");
            assert_eq!(rows[0].who, case["who"].as_str().unwrap(), "{}", case["name"]);
        }
    }
}
