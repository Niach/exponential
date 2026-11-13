//! VAPP-201 (fixtures/team-zone.json): the TEAM TIME ZONE every app host
//! reads dates in. Rust twin of `packages/domain-contract/src/team-zone.ts`;
//! iOS ExpCore `TeamZone.swift` and Android `domain/TeamZone.kt` replay the
//! same `zoneCases` and `resolveCases`.

use std::sync::LazyLock;

use serde::Deserialize;

const TEAM_ZONE_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/team-zone.json");

#[derive(Debug, Clone, Deserialize)]
pub struct TeamZoneCopy {
    /// The Settings › General row's label.
    pub row: String,
    /// The picker's first item (= no team zone, the device's).
    pub device: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TeamZoneTestIds {
    pub row: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamZoneSpec {
    pub copy: TeamZoneCopy,
    pub test_ids: TeamZoneTestIds,
}

pub static TEAM_ZONE: LazyLock<TeamZoneSpec> =
    LazyLock::new(|| serde_json::from_str(TEAM_ZONE_JSON).expect("team-zone.json parses"));

/// The zone a team's `time_zone` names when the tz database knows it; `None`
/// = the device's zone (no team zone, blank, or an unknown name).
///
/// The system database comes first (jiff reads `/usr/share/zoneinfo` when any
/// crate in the build turns on `tzdb-zoneinfo`), the bundled copy second:
/// distros now split the legacy link names (`Asia/Calcutta`, `Asia/Katmandu`,
/// the names browsers still report) into an optional `tzdata-legacy`
/// package, and a team zone must not fall back to the device's just because
/// this machine lacks it.
pub fn resolve_zone(team_zone: Option<&str>) -> Option<jiff::tz::TimeZone> {
    let name = team_zone.map(str::trim).filter(|name| !name.is_empty())?;
    jiff::tz::TimeZone::get(name).ok().or_else(|| jiff::tz::TimeZoneDatabase::bundled().get(name).ok())
}

/// `resolve_zone`'s name (the fixture's `zone`).
pub fn resolve_zone_name(team_zone: Option<&str>) -> Option<String> {
    resolve_zone(team_zone).map(|_| team_zone.unwrap_or_default().trim().to_string())
}

/// A zone's offset from UTC at an instant (epoch ms), in minutes east.
pub fn zone_offset(zone: &jiff::tz::TimeZone, ms: f64) -> i32 {
    let Ok(at) = jiff::Timestamp::from_millisecond(ms as i64) else { return 0 };
    zone.to_offset(at).seconds() / 60
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture() -> Value {
        serde_json::from_str(TEAM_ZONE_JSON).unwrap()
    }

    #[test]
    fn the_fixture_parses() {
        assert_eq!(TEAM_ZONE.copy.row, "Time zone");
        assert_eq!(TEAM_ZONE.test_ids.row, "team-time-zone");
    }

    #[test]
    fn zone_cases_agree_with_the_tz_database() {
        for case in fixture()["zoneCases"].as_array().unwrap() {
            let name = case["zone"].as_str().unwrap();
            let zone = resolve_zone(Some(name)).unwrap_or_else(|| panic!("{name} is unknown"));
            let ms = case["ms"].as_f64().unwrap();
            assert_eq!(zone_offset(&zone, ms), case["offset"].as_i64().unwrap() as i32, "{name} at {}", case["at"]);
        }
    }

    /// A system tz database without the legacy links (Ubuntu's split-off
    /// `tzdata-legacy`) still resolves every fixture zone: the bundled copy
    /// backs it.
    #[test]
    fn the_bundled_database_knows_every_fixture_zone() {
        let bundled = jiff::tz::TimeZoneDatabase::bundled();
        for case in fixture()["zoneCases"].as_array().unwrap() {
            let name = case["zone"].as_str().unwrap();
            assert!(bundled.get(name).is_ok(), "{name} is not bundled");
        }
    }

    #[test]
    fn resolve_cases_replay() {
        for case in fixture()["resolveCases"].as_array().unwrap() {
            let team_zone = case["teamZone"].as_str();
            assert_eq!(resolve_zone_name(team_zone).as_deref(), case["zone"].as_str(), "{}", case["name"]);
        }
    }
}
