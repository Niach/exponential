//! #EXP-1645 (fixtures/team-image.json): THE TEAM MARK + TEAM IMAGE. Rust twin
//! of `packages/domain-contract/src/team-image.ts`; iOS ExpCore and Android
//! replay the same `markCases`.

use std::sync::LazyLock;

use serde::Deserialize;

const TEAM_IMAGE_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/team-image.json");

#[derive(Debug, Clone, Deserialize)]
pub struct TeamImageCopy {
    /// The Settings › General row's label.
    pub row: String,
    /// The action pill.
    pub upload: String,
    /// The icon-only ghost's tooltip.
    pub remove: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TeamImageTestIds {
    pub row: String,
    pub upload: String,
    pub remove: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamImageMarkSpec {
    /// The image's corner radius, as a fraction of its side.
    pub radius_fraction: f32,
    /// The concept of the no-image mark.
    pub glyph: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamImageRowSpec {
    pub mark_size: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamImageUpload {
    /// `/api/teams/{teamId}/icon`.
    pub path: String,
    pub max_bytes: usize,
    /// The side of the PNG the server stores.
    pub side: u32,
    pub accept: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamImageSpec {
    pub copy: TeamImageCopy,
    pub test_ids: TeamImageTestIds,
    pub mark: TeamImageMarkSpec,
    pub row: TeamImageRowSpec,
    pub upload: TeamImageUpload,
}

pub static TEAM_IMAGE: LazyLock<TeamImageSpec> =
    LazyLock::new(|| serde_json::from_str(TEAM_IMAGE_JSON).expect("team-image.json parses"));

/// What a team wears.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeamMark {
    Image { src: String },
    Glyph,
}

/// The team's image when `icon_url` holds one, else the group glyph.
pub fn team_mark(icon_url: Option<&str>) -> TeamMark {
    match icon_url.map(str::trim).filter(|src| !src.is_empty()) {
        Some(src) => TeamMark::Image { src: src.to_string() },
        None => TeamMark::Glyph,
    }
}

/// The upload route of a team.
pub fn upload_path(team_id: &str) -> String {
    TEAM_IMAGE.upload.path.replace("{teamId}", team_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture() -> Value {
        serde_json::from_str(TEAM_IMAGE_JSON).unwrap()
    }

    #[test]
    fn the_fixture_parses() {
        assert_eq!(TEAM_IMAGE.copy.row, "Team image");
        assert_eq!(TEAM_IMAGE.copy.upload, "Upload");
        assert_eq!(TEAM_IMAGE.copy.remove, "Remove image");
        assert_eq!(TEAM_IMAGE.test_ids.row, "team-image");
        assert_eq!(TEAM_IMAGE.test_ids.upload, "team-image-upload");
        assert_eq!(TEAM_IMAGE.test_ids.remove, "team-image-remove");
        assert_eq!(TEAM_IMAGE.mark.radius_fraction, 0.25);
        assert_eq!(TEAM_IMAGE.mark.glyph, "ui-team");
        assert_eq!(TEAM_IMAGE.row.mark_size, 28.);
        assert_eq!(TEAM_IMAGE.upload.max_bytes, 10 * 1024 * 1024);
        assert_eq!(TEAM_IMAGE.upload.side, 512);
        assert_eq!(TEAM_IMAGE.upload.accept.len(), 5);
    }

    #[test]
    fn mark_cases_replay() {
        let fixture = fixture();
        let cases = fixture["markCases"].as_array().unwrap();
        assert_eq!(cases.len(), 5);
        for case in cases {
            let want = match case["mark"]["kind"].as_str().unwrap() {
                "image" => TeamMark::Image { src: case["mark"]["src"].as_str().unwrap().to_string() },
                _ => TeamMark::Glyph,
            };
            assert_eq!(team_mark(case["iconUrl"].as_str()), want, "{}", case["iconUrl"]);
        }
    }

    #[test]
    fn the_upload_path_names_the_team() {
        assert_eq!(upload_path("t-1"), "/api/teams/t-1/icon");
    }
}
