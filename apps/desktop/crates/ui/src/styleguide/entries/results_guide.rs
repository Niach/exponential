//! EXP-1154/EXP-1251/EXP-1294: `results-guide` (Special components): the
//! GUIDE tab of a conversation (Changes + Results merged), the IDE half of the
//! styleguide entry the web page draws.
//!
//! The REAL page (`crate::session_results::render`) over static groups, the
//! shared parser's own output (`domain::session_results::
//! parse_session_result_groups`): the Stack card on top
//! (`crate::session_results::stack_card` over `domain::pr_stack::
//! stack_view`, keyed on synced `pull_requests` rows), the `Summary` topic leading as a plain paragraph; every
//! other topic wears the group band with its muted `01 / 02` caption, its
//! text, ONE Changes row (`<>` Changes · N files · +A −D ›, the shared
//! coverage rule) and its tiles; then the automatic `Other changes` section
//! and the `Show complete diff` row. Below it the section diff page's back
//! row (`crate::session_results::guide_page_header`).
//!
//! No `sync::Store` here: no team, so no `#IDENT` pills, and the image
//! cache has no transport (a tile paints its placeholder box).

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};

use domain::diff::DiffStatus;
use domain::session_results::{parse_session_result_groups, SessionResultGroup};

use crate::diff_pane::PaneFile;
use crate::markdown::ImageCache;
use crate::session_results::{guide_diff_files, guide_page_view, GuidePage, GuideSpec};

use super::fixture_rows::PrDemo;

pub(crate) const ID: &str = "results-guide";
pub(crate) const OWNER: &str = "EXP-1154";

/// A report as an agent files it: a Summary, two numbered topics with their
/// files, one picture.
pub(crate) fn groups() -> Vec<SessionResultGroup> {
    parse_session_result_groups(Some(&serde_json::json!([
        {"topic": "Sessions as one tree", "text": "Every session hangs off one root; its conversation lists a card per session.",
         "files": ["apps/desktop/crates/ui/src/thread_screen.rs", "apps/desktop/crates/ui/src/screens.rs"]},
        {"topic": "Summary", "text": "The Guide is a tab of the conversation now, and it reads the report over the diff."},
        {"topic": "Results as the Guide", "text": "Each section's Changes row opens the diff of just its files.",
         "files": ["apps/desktop/crates/ui/src/session_results.rs"]},
        {"topic": "Results as the Guide", "label": "desktop", "attachmentId": "sg-guide-shot", "width": 1440, "height": 900},
    ])))
}

/// The diff the Changes rows count against: two files a section names, one
/// nobody does (it lands in Other changes).
fn loaded() -> Vec<PaneFile> {
    vec![
        PaneFile::from_parts("apps/desktop/crates/ui/src/thread_screen.rs", DiffStatus::Modified, 12, 9),
        PaneFile::from_parts("apps/desktop/crates/ui/src/session_results.rs", DiffStatus::Modified, 140, 22),
        PaneFile::from_parts("apps/desktop/crates/ui/src/lib.rs", DiffStatus::Modified, 1, 0),
    ]
}

/// A stack of three pull requests; the guide's (VAPP-98) sits in the middle.
fn stack() -> PrDemo {
    PrDemo::open(&[
        ("VAPP-98", "Exponential UI renderer hardening round 1", 98, "exp/VAPP-93"),
        ("VAPP-93", "ui.exponential.at: the Exponential UI site", 93, "master"),
        ("VAPP-100", "SwiftUI parity with the round-1/2 contract", 100, "exp/VAPP-98"),
    ])
}

/// The Stack card's model for the guide's pull request.
fn stack_view() -> Option<domain::pr_stack::StackView> {
    let demo = stack();
    let pull = demo.pull("VAPP-98")?;
    demo.with_set(|set| domain::pr_stack::stack_view(pull, set))
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let images = window
        .use_keyed_state("styleguide-results-guide-images", cx, |_, _| ImageCache::new(None))
        .clone();
    let stack = stack_view()
        .map(|view| {
            // The ghost shows on the hovered row: the demo pins the top one.
            let hovered = view.rows.first().map(|row| row.pull_request_id.clone());
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
            scroll: None,
        },
        cx,
    );
    let view = guide_page_view(&groups(), &guide_diff_files(&loaded()), GuidePage::Section(1), false)
        .expect("section 1 names loaded files");
    let back = crate::session_results::guide_page_header(&view, std::rc::Rc::new(|_, _| {}), cx);
    div()
        .flex()
        .flex_col()
        .gap_4()
        .pt_2()
        .child(div().w(px(640.)).h(px(720.)).child(page))
        .child(div().w(px(640.)).child(back))
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
            vec![("Sessions as one tree", 1, 2), ("Results as the Guide", 2, 2)]
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
        let view = stack_view().unwrap();
        assert_eq!(
            view.rows.iter().map(|row| (row.identifier.as_str(), row.is_current)).collect::<Vec<_>>(),
            vec![("VAPP-100", false), ("VAPP-98", true), ("VAPP-93", false)]
        );
        assert_eq!(view.base_branch.as_deref(), Some("master"));
    }
}
