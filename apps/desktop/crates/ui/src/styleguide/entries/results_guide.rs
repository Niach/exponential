//! EXP-1154: `results-guide` (Special components): the Results face as the
//! GUIDE, the IDE half of the styleguide entry the web page draws.
//!
//! The REAL page (`crate::session_results::render`) over static groups, the
//! shared parser's own output (`domain::session_results::
//! parse_session_result_groups`): the `Summary` topic leads as a plain
//! paragraph (no band, no number); every other topic wears the group band
//! with its muted `01 / 02` caption, its text, the files it touched (flat
//! hairline rows: the dimmed-directory path, `+N −M` only when the loaded
//! diff has the path) and its tiles. Then the PR-body fallback
//! (`crate::session_results::render_pr_body`): an open pull request with no
//! run report shows its GitHub body as one unnumbered group.
//!
//! No `sync::Store` here: no team, so no `#IDENT` pills, and the image
//! cache has no transport (a tile paints its placeholder box).

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};

use domain::diff::DiffStatus;
use domain::session_results::{parse_session_result_groups, SessionResultGroup};

use crate::diff_pane::PaneFile;
use crate::markdown::ImageCache;
use crate::session_results::{render_pr_body, GuideFiles, PrBodyContent};

pub(crate) const ID: &str = "results-guide";
pub(crate) const OWNER: &str = "EXP-1154";

/// A report as an agent files it: a Summary, two numbered topics with their
/// files, one picture.
pub(crate) fn groups() -> Vec<SessionResultGroup> {
    parse_session_result_groups(Some(&serde_json::json!([
        {"topic": "Merge the review page", "text": "The Reviews rows open the issue on its Changes face; the standalone review screen is gone.",
         "files": ["apps/desktop/crates/ui/src/reviews_view.rs", "apps/desktop/crates/ui/src/screens.rs"]},
        {"topic": "Summary", "text": "The review of a pull request is the issue's Work screen now, and Results reads as a guide."},
        {"topic": "Results as the Guide", "text": "Numbered sections list the files each change touched, with counts from the loaded diff.",
         "files": ["apps/desktop/crates/ui/src/session_results.rs"]},
        {"topic": "Results as the Guide", "label": "desktop", "attachmentId": "sg-guide-shot", "width": 1440, "height": 900},
    ])))
}

/// The diff the file rows count against: one path of the report is loaded,
/// the others are not (they render without counts).
fn loaded() -> Vec<PaneFile> {
    vec![
        PaneFile::from_parts("apps/desktop/crates/ui/src/reviews_view.rs", DiffStatus::Modified, 12, 9),
        PaneFile::from_parts("apps/desktop/crates/ui/src/session_results.rs", DiffStatus::Modified, 140, 22),
    ]
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let images = window
        .use_keyed_state("styleguide-results-guide-images", cx, |_, _| ImageCache::new(None))
        .clone();
    let guide = GuideFiles {
        loaded: Some(loaded()),
        on_open: None,
    };
    let page = crate::session_results::render(&groups(), 560., &images, None, Some(guide), cx);
    let fallback = render_pr_body(
        Some("EXP-1154: merge the review page into the issue"),
        PrBodyContent::Ready("Opened before the run filed its report: the GitHub body, read as is."),
        None,
        cx,
    );
    div()
        .flex()
        .flex_col()
        .gap_4()
        .pt_2()
        .child(div().w(px(640.)).h(px(560.)).child(page))
        .child(div().w(px(640.)).h(px(160.)).child(fallback))
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::session_results::{guide_file_rows, session_results_guide};

    /// The demo is the shared rules' own output: Summary leads (filed
    /// second), two numbered sections, and only loaded paths carry counts.
    #[test]
    fn the_demo_reads_as_a_guide() {
        let groups = groups();
        let (lead, sections) = session_results_guide(&groups, |group| group.topic.as_str());
        assert_eq!(lead.map(|group| group.topic.as_str()), Some("Summary"));
        assert_eq!(
            sections
                .iter()
                .map(|section| (section.group.topic.as_str(), section.index, section.total))
                .collect::<Vec<_>>(),
            vec![("Merge the review page", 1, 2), ("Results as the Guide", 2, 2)]
        );
        let diff: Vec<(String, u32, u32)> = loaded()
            .iter()
            .map(|file| (file.path.to_string(), file.additions, file.deletions))
            .collect();
        let rows = guide_file_rows(&sections[0].group.files, Some(&diff));
        assert_eq!(rows[0].counts, Some((12, 9)));
        assert_eq!(rows[1].counts, None);
    }
}
