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
//! FACE DOTS ([`face_dots`]): state lives on the face tabs, never a title;
//! the Run tab draws its tone as the agent's brand mark ([`FACE_MARK`]).

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

/// FACE DOTS: a face tab's state dot is this wide.
pub const FACE_DOT: f32 = 6.;
/// …and trails the tab's label by this much.
pub const FACE_DOT_GAP: f32 = 6.;

/// FACE MARKS: a live run's Run tab wears its agent's brand mark (never a
/// dot), this big…
pub const FACE_MARK: f32 = 14.;
/// …LEADING the tab's label by this much…
pub const FACE_MARK_GAP: f32 = 6.;
/// …with the Running row's amber badge this wide at its top trailing corner
/// while the run needs input.
pub const FACE_MARK_BADGE: f32 = 6.;

/// One face of a detail (the face toggle's segments), contract spelling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DetailFace {
    Issue,
    Run,
    Changes,
    Results,
}

impl DetailFace {
    /// The contract key (`issue` / `run` / `changes` / `results`).
    pub fn key(self) -> &'static str {
        match self {
            Self::Issue => "issue",
            Self::Run => "run",
            Self::Changes => "changes",
            Self::Results => "results",
        }
    }
}

/// A face dot's tone — a row of the session-dot table (×4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaceDotTone {
    Running,
    NeedsInput,
    Review,
}

impl FaceDotTone {
    /// The contract key (`running` / `needs_input` / `review`).
    pub fn key(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::NeedsInput => "needs_input",
            Self::Review => "review",
        }
    }
}

/// EXP-1162 FACE DOTS: state lives on the face tabs, never on a title. The
/// Run tab wears a dot while its run is LIVE (`needs_input` while it waits
/// on a person, else `running`); an OPEN pull request dots Results, or
/// Changes when there is no Results face. A face not on show carries
/// nothing; an ended run carries nothing. At most one dot per face.
pub fn face_dots(
    faces: &[DetailFace],
    run_live: bool,
    needs_input: bool,
    pr_open: bool,
) -> Vec<(DetailFace, FaceDotTone)> {
    let mut dots = Vec::with_capacity(2);
    if run_live && faces.contains(&DetailFace::Run) {
        let tone = if needs_input {
            FaceDotTone::NeedsInput
        } else {
            FaceDotTone::Running
        };
        dots.push((DetailFace::Run, tone));
    }
    if pr_open {
        let target = [DetailFace::Results, DetailFace::Changes]
            .into_iter()
            .find(|face| faces.contains(face));
        if let Some(face) = target {
            dots.push((face, FaceDotTone::Review));
        }
    }
    dots
}

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
        assert_eq!(constants["faceDot"].as_f64(), Some(FACE_DOT as f64));
        assert_eq!(constants["faceDotGap"].as_f64(), Some(FACE_DOT_GAP as f64));
        assert_eq!(constants["faceMark"].as_f64(), Some(FACE_MARK as f64));
        assert_eq!(constants["faceMarkGap"].as_f64(), Some(FACE_MARK_GAP as f64));
        assert_eq!(constants["faceMarkBadge"].as_f64(), Some(FACE_MARK_BADGE as f64));
        assert_eq!(constants.as_object().unwrap().len(), 11, "a new constant needs a test");
    }

    fn face(key: &str) -> DetailFace {
        [DetailFace::Issue, DetailFace::Run, DetailFace::Changes, DetailFace::Results]
            .into_iter()
            .find(|face| face.key() == key)
            .unwrap_or_else(|| panic!("unknown face: {key}"))
    }

    /// Runs the named `faceDots` case and compares the dots as a key map.
    fn dots_case(name: &str) {
        let fixture = fixture();
        let case = fixture["faceDots"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == name)
            .unwrap_or_else(|| panic!("fixture case missing: {name}"));
        let faces: Vec<DetailFace> = case["faces"]
            .as_array()
            .unwrap()
            .iter()
            .map(|key| face(key.as_str().unwrap()))
            .collect();
        let got: std::collections::BTreeMap<String, String> = face_dots(
            &faces,
            case["runLive"].as_bool().unwrap(),
            case["needsInput"].as_bool().unwrap(),
            case["prOpen"].as_bool().unwrap(),
        )
        .into_iter()
        .map(|(face, tone)| (face.key().to_string(), tone.key().to_string()))
        .collect();
        let want: std::collections::BTreeMap<String, String> = case["dots"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(face, tone)| (face.clone(), tone.as_str().unwrap().to_string()))
            .collect();
        assert_eq!(got, want, "case: {name}");
    }

    #[test]
    fn detail_chrome_face_dots_no_run_and_no_pull_request() {
        dots_case("no run and no pull request: no dots");
    }

    #[test]
    fn detail_chrome_face_dots_a_live_run_dots_the_run_tab() {
        dots_case("a live run dots the Run tab");
    }

    #[test]
    fn detail_chrome_face_dots_a_run_waiting_on_a_person_is_amber() {
        dots_case("a run waiting on a person dots the Run tab amber");
    }

    #[test]
    fn detail_chrome_face_dots_an_ended_run_carries_no_dot() {
        dots_case("an ended run carries no dot, even if it still says needs input");
    }

    #[test]
    fn detail_chrome_face_dots_an_open_pull_request_dots_results() {
        dots_case("an open pull request dots Results");
    }

    #[test]
    fn detail_chrome_face_dots_an_open_pull_request_without_results_dots_changes() {
        dots_case("an open pull request with no Results face dots Changes");
    }

    #[test]
    fn detail_chrome_face_dots_a_live_run_with_an_open_pull_request_dots_both() {
        dots_case("a live run with an open pull request dots both");
    }

    #[test]
    fn detail_chrome_face_dots_an_open_pull_request_with_neither_face() {
        dots_case("an open pull request with neither face shows no dot");
    }

    #[test]
    fn detail_chrome_face_dots_an_issue_less_run_dots_its_run_tab() {
        dots_case("an issue-less run dots its Run tab too");
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
        assert_eq!(fixture()["faceDots"].as_array().unwrap().len(), 9);
    }
}
