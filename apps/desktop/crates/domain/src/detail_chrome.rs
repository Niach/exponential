//! EXP-1162 — THE detail chrome: when the header's collapsed title shows,
//! and the edge strips' sizes.
//!
//! ONE pure rule, mirrored ×4 (web `lib/detail-chrome.ts`, iOS ExpCore
//! `DetailChrome`, Android `domain/DetailChrome`) and locked by the contract
//! fixture `packages/domain-contract/fixtures/detail-chrome.json`. Points are
//! desktop px (= web px = iOS pt = Android dp).
//!
//! The issue face keeps its large title as a ROW of the scrolling content;
//! the header breaks into the collapsed title (a threshold, never a
//! progressive morph) once that row's bottom edge meets the header's. A face
//! with no title row of its own (Run, Changes, Results) is always collapsed.
//! gpui cannot blur what is behind a view, so the desktop draws the scrim
//! fade alone ([`EDGE_BLUR`] and [`SCRIM`] are carried for parity).

/// How long the collapsed title fades in, in milliseconds.
pub const COLLAPSE_MS: u64 = 160;
/// How far the collapsed title rises while it fades in.
pub const COLLAPSE_RISE: f32 = 4.;
/// The strip under the header band that fades the scrim to nothing.
pub const EDGE_TOP: f32 = 24.;
/// The strip above the bottom bar that fades the scrim to nothing, upwards.
pub const EDGE_BOTTOM: f32 = 32.;
/// The backdrop blur under the header band (clients that can blur).
pub const EDGE_BLUR: f32 = 8.;
/// The header band's page-background alpha.
pub const SCRIM: f32 = 0.72;

/// `collapsed = !has_title_row || title_bottom <= header_bottom`, both in the
/// same coordinate space (the header band's bottom edge). A title row not
/// measured yet (`None`) stays expanded.
pub fn is_title_collapsed(has_title_row: bool, title_bottom: Option<f64>, header_bottom: f64) -> bool {
    if !has_title_row {
        return true;
    }
    title_bottom.is_some_and(|bottom| bottom <= header_bottom)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/detail-chrome.json");

    fn fixture() -> serde_json::Value {
        serde_json::from_str(FIXTURE).unwrap()
    }

    fn case(name: &str) -> bool {
        let fixture = fixture();
        let case = fixture["collapse"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == name)
            .unwrap_or_else(|| panic!("fixture case missing: {name}"));
        let got = is_title_collapsed(
            case["hasTitleRow"].as_bool().unwrap(),
            case["titleBottom"].as_f64(),
            case["headerBottom"].as_f64().unwrap(),
        );
        assert_eq!(got, case["collapsed"].as_bool().unwrap(), "case: {name}");
        got
    }

    #[test]
    fn detail_chrome_constants_match_the_fixture() {
        let fixture = fixture();
        let constants = &fixture["constants"];
        assert_eq!(constants["collapseMs"].as_u64(), Some(COLLAPSE_MS));
        assert_eq!(constants["collapseRise"].as_f64(), Some(COLLAPSE_RISE as f64));
        assert_eq!(constants["edgeTop"].as_f64(), Some(EDGE_TOP as f64));
        assert_eq!(constants["edgeBottom"].as_f64(), Some(EDGE_BOTTOM as f64));
        assert_eq!(constants["edgeBlur"].as_f64(), Some(EDGE_BLUR as f64));
        assert!((constants["scrim"].as_f64().unwrap() - SCRIM as f64).abs() < 1e-6);
    }

    #[test]
    fn detail_chrome_the_title_row_sits_below_the_header() {
        assert!(!case("the title row sits below the header"));
    }

    #[test]
    fn detail_chrome_one_point_of_the_title_still_shows() {
        assert!(!case("one point of the title still shows"));
    }

    #[test]
    fn detail_chrome_the_titles_bottom_edge_meets_the_headers() {
        assert!(case("the title's bottom edge meets the header's: it breaks"));
    }

    #[test]
    fn detail_chrome_the_title_scrolled_far_away() {
        assert!(case("the title scrolled far away"));
    }

    #[test]
    fn detail_chrome_a_face_with_no_title_row_is_always_collapsed() {
        assert!(case("a face with no title row is always collapsed"));
    }

    #[test]
    fn detail_chrome_a_title_row_not_measured_yet_stays_expanded() {
        assert!(!case("a title row not measured yet stays expanded"));
    }

    /// Every fixture case is covered by a named test above.
    #[test]
    fn detail_chrome_every_fixture_case_is_named() {
        assert_eq!(fixture()["collapse"].as_array().unwrap().len(), 6);
    }
}
