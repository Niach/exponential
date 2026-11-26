//! #EXP-2161 (`packages/domain-contract/fixtures/member-space.json`, TS
//! reference `src/member-space.ts`): member spaces, a member's private
//! Assistant on a device shared with their team. The pure rules the
//! desktop's screens read (Settings › Assistant › Runs on's team sections,
//! Device settings › Sharing's opt-in row, the lists that never name a
//! space), replayed below against the fixture's case lists.

use std::sync::LazyLock;

use serde::Deserialize;

use crate::rows::DeviceRow;

const FIXTURE_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/member-space.json");

/// `copy`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberSpaceCopy {
    pub opt_in: String,
    pub opt_in_title: String,
    pub opt_in_body: String,
    pub opt_in_confirm: String,
    pub pick_title: String,
    pub pick_body: String,
    pub pick_confirm: String,
    pub members: String,
    pub remove: String,
    pub waiting: String,
    pub no_device: String,
    pub ended: String,
}

/// The fixture's constants the clients read.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberSpaceSpec {
    pub doctor_key: String,
    pub device_id_prefix: String,
    pub copy: MemberSpaceCopy,
}

pub static MEMBER_SPACE: LazyLock<MemberSpaceSpec> =
    LazyLock::new(|| serde_json::from_str(FIXTURE_JSON).expect("member-space.json parses"));

/// `optInCases`: who may move one team in or out of a device's opt-in.
/// `caller` = `deviceOwner` | `teamOwner` | `member`.
pub fn may_opt_in(caller: &str, allowed: bool, kind: &str, shared: bool, doctor: Option<&str>) -> bool {
    if !allowed {
        return caller == "deviceOwner" || caller == "teamOwner";
    }
    caller == "deviceOwner" && kind == "server" && shared && doctor == Some("ok")
}

/// `teamCases`: the teams a space works in, opted in, the host's owner and
/// the member both in them, in the member's order.
pub fn space_teams(assistant_team_ids: &[String], host_owner_team_ids: &[String], member_team_ids: &[String]) -> Vec<String> {
    member_team_ids
        .iter()
        .filter(|id| assistant_team_ids.contains(id) && host_owner_team_ids.contains(id))
        .cloned()
        .collect()
}

/// `endCases`: whether a space ends when its teams narrow to `teams`.
pub fn space_ends(space_team_id: &str, teams: &[String]) -> bool {
    !teams.iter().any(|team| team == space_team_id)
}

/// `startCases`: a space runs only the trees of its Assistant conversation.
pub fn may_space_run(tree: &str, root_device_is_space: bool) -> bool {
    tree == "assistant" && root_device_is_space
}

/// `deviceCases`: what a request speaking through a space may run on.
pub fn space_may_use_device(device: &str, own_tree: bool) -> bool {
    if device == "space" {
        own_tree
    } else {
        device == "otherShared"
    }
}

/// THE selector of every device list and picker (`listed`): a member space
/// (`space_host_id`) is never a device. Only what RESOLVES a machine id a
/// session or the Assistant names reads the raw rows.
pub fn is_listed(row: &DeviceRow) -> bool {
    row.space_host_id.as_deref().is_none_or(str::is_empty)
}

/// One device as [`space_sections`] reads it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceDevice {
    pub row_id: String,
    pub device_id: String,
    pub user_id: String,
    pub label: String,
    pub online: bool,
    pub assistant_team_ids: Vec<String>,
    pub space_host_id: Option<String>,
}

/// One row of a team section.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceSectionRow {
    /// The host's `device_id`.
    pub id: String,
    pub label: String,
    pub disabled: bool,
    pub selected: bool,
}

/// One team's section, headed by the team's name.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceSection {
    pub team_id: String,
    pub title: String,
    pub rows: Vec<SpaceSectionRow>,
}

/// `sectionCases`: Runs on's team sections, other people's opted-in hosts
/// per team (`teams` = (id, name) in the member's order), label order
/// inside; the bound space's host reads selected.
pub fn space_sections(devices: &[SpaceDevice], teams: &[(String, String)], bound_id: Option<&str>, me: &str) -> Vec<SpaceSection> {
    let bound_host = devices
        .iter()
        .find(|device| Some(device.device_id.as_str()) == bound_id)
        .and_then(|device| device.space_host_id.clone());
    teams
        .iter()
        .filter_map(|(team_id, name)| {
            let mut hosts: Vec<&SpaceDevice> = devices
                .iter()
                .filter(|device| device.user_id != me && device.space_host_id.is_none() && device.assistant_team_ids.contains(team_id))
                .collect();
            hosts.sort_by(|a, b| a.label.cmp(&b.label));
            let rows: Vec<SpaceSectionRow> = hosts
                .into_iter()
                .map(|device| {
                    let selected = bound_host.as_deref() == Some(device.row_id.as_str());
                    SpaceSectionRow { id: device.device_id.clone(), label: device.label.clone(), disabled: !device.online && !selected, selected }
                })
                .collect();
            (!rows.is_empty()).then(|| SpaceSection { team_id: team_id.clone(), title: name.clone(), rows })
        })
        .collect()
}

/// `copy.pickTitle` / `copy.pickBody` filled (web `memberSpacePickPrompt`).
pub fn pick_prompt(device: &str, owner: &str) -> (String, String) {
    let copy = &MEMBER_SPACE.copy;
    (crate::prompts::fill(&copy.pick_title, &[("device", device)]), crate::prompts::fill(&copy.pick_body, &[("owner", owner)]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE_JSON).unwrap()
    }

    fn strings(value: &Value) -> Vec<String> {
        value.as_array().unwrap().iter().map(|item| item.as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn the_copy_and_constants_are_the_fixture_s() {
        let fx = fixture();
        assert_eq!(MEMBER_SPACE.doctor_key, fx["doctorKey"].as_str().unwrap());
        assert_eq!(MEMBER_SPACE.device_id_prefix, "space-");
        assert_eq!(MEMBER_SPACE.copy.opt_in, "Members' Assistants");
        assert_eq!(MEMBER_SPACE.copy.no_device, fx["copy"]["noDevice"].as_str().unwrap());
        let (title, body) = pick_prompt("unraid", "Mira");
        assert_eq!(title, "Run your Assistant on unraid?");
        assert!(body.starts_with("Mira and admins of this machine"), "{body}");
    }

    #[test]
    fn opt_in_cases() {
        for case in fixture()["optInCases"].as_array().unwrap() {
            let got = may_opt_in(
                case["caller"].as_str().unwrap(),
                case["allowed"].as_bool().unwrap(),
                case["kind"].as_str().unwrap(),
                case["shared"].as_bool().unwrap(),
                case["doctor"].as_str(),
            );
            assert_eq!(got, case["expected"].as_bool().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn team_cases() {
        for case in fixture()["teamCases"].as_array().unwrap() {
            let got = space_teams(&strings(&case["assistantTeamIds"]), &strings(&case["hostOwnerTeamIds"]), &strings(&case["memberTeamIds"]));
            assert_eq!(got, strings(&case["expected"]), "{}", case["name"]);
        }
    }

    #[test]
    fn end_cases() {
        for case in fixture()["endCases"].as_array().unwrap() {
            let got = space_ends(case["spaceTeamId"].as_str().unwrap(), &strings(&case["spaceTeams"]));
            assert_eq!(got, case["ends"].as_bool().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn start_and_device_cases() {
        let fx = fixture();
        for case in fx["startCases"].as_array().unwrap() {
            let got = may_space_run(case["tree"].as_str().unwrap(), case["rootDeviceIsSpace"].as_bool().unwrap());
            assert_eq!(got, case["expected"].as_bool().unwrap(), "{}", case["name"]);
        }
        for case in fx["deviceCases"].as_array().unwrap() {
            let got = space_may_use_device(case["device"].as_str().unwrap(), case["ownTree"].as_bool().unwrap());
            assert_eq!(got, case["expected"].as_bool().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn section_cases() {
        for case in fixture()["sectionCases"].as_array().unwrap() {
            let devices: Vec<SpaceDevice> = serde_json::from_value(case["devices"].clone()).unwrap();
            let teams: Vec<(String, String)> = case["teams"]
                .as_array()
                .unwrap()
                .iter()
                .map(|team| (team["id"].as_str().unwrap().to_string(), team["name"].as_str().unwrap().to_string()))
                .collect();
            let expected: Vec<SpaceSection> = serde_json::from_value(case["expected"].clone()).unwrap();
            let got = space_sections(&devices, &teams, case["boundId"].as_str(), case["me"].as_str().unwrap());
            assert_eq!(got, expected, "{}", case["name"]);
        }
    }

    #[test]
    fn a_space_row_is_never_listed() {
        let row = |host: Option<&str>| -> DeviceRow { serde_json::from_value(serde_json::json!({"id": "r1", "space_host_id": host})).unwrap() };
        assert!(is_listed(&row(None)));
        assert!(!is_listed(&row(Some("r0"))));
    }
}
