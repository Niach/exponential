//! EXP-1154/EXP-1251: `results-guide` (Special components): the GUIDE face
//! (Changes + Results merged), the IDE half of the styleguide entry the web
//! page draws.
//!
//! The REAL page (`crate::session_results::render`) over static groups, the
//! shared parser's own output (`domain::session_results::
//! parse_session_result_groups`): the Stack card on top
//! (`crate::session_results::stack_card` over `domain::pr_stack::
//! stack_view`), the `Summary` topic leading as a plain paragraph; every
//! other topic wears the group band with its muted `01 / 02` caption, its
//! text, ONE Changes row (`<>` Changes · N files · +A −D ›, the shared
//! coverage rule) and its tiles; then the automatic `Other changes` section
//! and the `Show complete diff` row. Below it the section diff page's back
//! row (`crate::session_results::guide_page_header`) and the PR-body lead an
//! open pull request with no report shows (`pr_body_block`).
//!
//! No `sync::Store` here: no team, so no `#IDENT` pills, and the image
//! cache has no transport (a tile paints its placeholder box).

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};

use domain::diff::DiffStatus;
use domain::rows::Issue;
use domain::session_results::{parse_session_result_groups, SessionResultGroup};

use crate::diff_pane::PaneFile;
use crate::markdown::ImageCache;
use crate::session_results::{
    guide_diff_files, guide_page_view, pr_body_block, GuidePage, GuideSpec, PrBodyContent,
};

pub(crate) const ID: &str = "results-guide";
pub(crate) const OWNER: &str = "EXP-1154";

/// A report as an agent files it: a Summary, two numbered topics with their
/// files, one picture.
pub(crate) fn groups() -> Vec<SessionResultGroup> {
    parse_session_result_groups(Some(&serde_json::json!([
        {"topic": "Merge the review page", "text": "The Reviews rows open the issue on its Guide; the standalone review screen is gone.",
         "files": ["apps/desktop/crates/ui/src/reviews_view.rs", "apps/desktop/crates/ui/src/screens.rs"]},
        {"topic": "Summary", "text": "The review of a pull request is the issue's Work screen now, and its Guide reads the report over the diff."},
        {"topic": "Results as the Guide", "text": "Each section's Changes row opens the diff of just its files.",
         "files": ["apps/desktop/crates/ui/src/session_results.rs"]},
        {"topic": "Results as the Guide", "label": "desktop", "attachmentId": "sg-guide-shot", "width": 1440, "height": 900},
    ])))
}

/// The diff the Changes rows count against: two files a section names, one
/// nobody does (it lands in Other changes).
fn loaded() -> Vec<PaneFile> {
    vec![
        PaneFile::from_parts("apps/desktop/crates/ui/src/reviews_view.rs", DiffStatus::Modified, 12, 9),
        PaneFile::from_parts("apps/desktop/crates/ui/src/session_results.rs", DiffStatus::Modified, 140, 22),
        PaneFile::from_parts("apps/desktop/crates/ui/src/lib.rs", DiffStatus::Modified, 1, 0),
    ]
}

/// A synced-shape issue row with an open pull request on `exp/<identifier>`.
fn issue(identifier: &str, title: &str, base: &str) -> Issue {
    serde_json::from_value(serde_json::json!({
        "id": format!("sg-{identifier}"),
        "board_id": "sg-board",
        "number": 1,
        "identifier": identifier,
        "title": title,
        "status": "in_review",
        "branch": format!("exp/{identifier}"),
        "pr_base_branch": base,
        "pr_url": format!("https://github.com/o/r/pull/{identifier}"),
        "pr_state": "open",
    }))
    .expect("a styleguide issue row deserializes")
}

/// A stack of three, the guide's issue in the middle.
fn stack() -> Vec<Issue> {
    vec![
        issue("VAPP-98", "Exponential UI renderer hardening round 1", "exp/VAPP-93"),
        issue("VAPP-93", "ui.exponential.at: the Exponential UI site", "master"),
        issue("VAPP-100", "SwiftUI parity with the round-1/2 contract", "exp/VAPP-98"),
    ]
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let images = window
        .use_keyed_state("styleguide-results-guide-images", cx, |_, _| ImageCache::new(None))
        .clone();
    let issues = stack();
    let stack = issues
        .iter()
        .find(|issue| issue.identifier == "VAPP-98")
        .and_then(|subject| domain::pr_stack::stack_view(subject, &issues))
        .map(|view| {
            // The ghost shows on the hovered row: the demo pins the top one.
            let hovered = view.rows.first().map(|row| row.issue_id.clone());
            crate::session_results::stack_card(
                &view,
                Some("main".to_string()),
                None,
                Some(std::rc::Rc::new(|_, _, _| {})),
                hovered,
                None,
                cx,
            )
        });
    let page = crate::session_results::render(
        &groups(),
        560.,
        &images,
        None,
        GuideSpec {
            diff: Some(loaded()),
            on_open: None,
            stack,
            fallback_lead: None,
            unnumbered: false,
        },
        cx,
    );
    let view = guide_page_view(&groups(), &guide_diff_files(&loaded()), GuidePage::Section(1), false)
        .expect("section 1 names loaded files");
    let back = crate::session_results::guide_page_header(&view, std::rc::Rc::new(|_, _| {}), cx);
    let fallback = pr_body_block(
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
        .child(div().w(px(640.)).h(px(720.)).child(page))
        .child(div().w(px(640.)).child(back))
        .child(div().w(px(640.)).child(fallback))
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::session_results::guide_coverage;

    /// The demo is the shared rules' own output: Summary leads (filed
    /// second), two numbered sections with their Changes counts, the
    /// unnamed file in Other changes, and a section page of its own files.
    #[test]
    fn the_demo_reads_as_a_guide() {
        let groups = groups();
        let diff = guide_diff_files(&loaded());
        let coverage = guide_coverage(
            &groups,
            |group| group.topic.as_str(),
            |group| group.files.as_slice(),
            Some(&diff),
        );
        assert_eq!(coverage.lead.as_ref().map(|lead| lead.group.topic.as_str()), Some("Summary"));
        assert_eq!(
            coverage
                .sections
                .iter()
                .map(|section| (section.group.topic.as_str(), section.index, section.total))
                .collect::<Vec<_>>(),
            vec![("Merge the review page", 1, 2), ("Results as the Guide", 2, 2)]
        );
        let first = coverage.sections[0].changes.as_ref().unwrap();
        assert_eq!((first.file_count(), first.additions, first.deletions), (1, 12, 9));
        assert_eq!(coverage.sections[0].missing, vec!["apps/desktop/crates/ui/src/screens.rs"]);
        let (title, other) = coverage.other.as_ref().unwrap();
        assert_eq!((*title, other.file_count()), ("Other changes", 1));
        assert_eq!(coverage.complete.as_ref().unwrap().file_count(), 3);
        let page = guide_page_view(&groups, &diff, GuidePage::Section(2), false).unwrap();
        assert_eq!(page.caption.as_deref(), Some("02 / 02"));
        assert_eq!(page.paths, vec!["apps/desktop/crates/ui/src/session_results.rs"]);
        let issues = stack();
        let subject = issues.iter().find(|issue| issue.identifier == "VAPP-98").unwrap();
        let view = domain::pr_stack::stack_view(subject, &issues).unwrap();
        assert_eq!(
            view.rows.iter().map(|row| (row.identifier.as_str(), row.is_current)).collect::<Vec<_>>(),
            vec![("VAPP-100", false), ("VAPP-98", true), ("VAPP-93", false)]
        );
        assert_eq!(view.base_branch.as_deref(), Some("master"));
    }
}
