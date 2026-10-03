//! EXP-1170 — THE New issue page: its copy and its autosave cadence.
//!
//! There is no create dialog any more. Every "New issue" opener mints a
//! draft id and navigates to the issue detail IN DRAFT MODE (desktop
//! `ui::issue_draft_screen`), and these strings are the only words that page
//! adds. Mirrored ×4 (web `lib/issue-draft-page.ts`, iOS ExpCore
//! `IssueDraftPage`, Android `domain/IssueDraftPage`) and locked by the
//! contract fixture `packages/domain-contract/fixtures/issue-draft.json`.

/// The identifier slot of the collapsed title.
pub const HEADER: &str = "New issue";
/// The large title input's placeholder.
pub const TITLE_PLACEHOLDER: &str = "Issue title";
/// The description editor's placeholder.
pub const DESCRIPTION_PLACEHOLDER: &str = "Add description...";
/// The primary button that files the draft.
pub const CREATE: &str = "Create";
/// The overflow menu's only item.
pub const DISCARD: &str = "Discard draft";
/// A draft with no title (the collapsed title, the Drafts list).
pub const UNTITLED: &str = "Untitled draft";
/// One coalesced `issueDrafts.upsert` this long after the last
/// title/description edit.
pub const AUTOSAVE_DEBOUNCE_MS: u64 = 800;

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/issue-draft.json");

    fn fixture() -> serde_json::Value {
        serde_json::from_str(FIXTURE).unwrap()
    }

    #[test]
    fn issue_draft_copy_matches_the_fixture() {
        let fixture = fixture();
        let copy = &fixture["copy"];
        assert_eq!(copy["header"].as_str(), Some(HEADER));
        assert_eq!(copy["titlePlaceholder"].as_str(), Some(TITLE_PLACEHOLDER));
        assert_eq!(
            copy["descriptionPlaceholder"].as_str(),
            Some(DESCRIPTION_PLACEHOLDER)
        );
        assert_eq!(copy["create"].as_str(), Some(CREATE));
        assert_eq!(copy["discard"].as_str(), Some(DISCARD));
        assert_eq!(copy["untitled"].as_str(), Some(UNTITLED));
        assert_eq!(copy.as_object().unwrap().len(), 6, "a new string needs a test");
    }

    #[test]
    fn issue_draft_autosave_matches_the_fixture() {
        let fixture = fixture();
        let autosave = &fixture["autosave"];
        assert_eq!(autosave["debounceMs"].as_u64(), Some(AUTOSAVE_DEBOUNCE_MS));
        assert_eq!(autosave.as_object().unwrap().len(), 1, "a new constant needs a test");
    }
}
