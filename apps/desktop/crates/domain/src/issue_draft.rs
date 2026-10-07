//! EXP-1170 — THE New issue page: its copy and its autosave cadence.
//!
//! There is no create dialog any more. Every "New issue" opener mints a
//! draft id and navigates to the issue detail IN DRAFT MODE (desktop
//! `ui::issue_draft_screen`), and these strings are the only words that page
//! adds. Mirrored ×4 (web `lib/issue-draft-page.ts`, iOS ExpCore
//! `IssueDraftPage`, Android `domain/IssueDraftPage`) and locked by the
//! contract fixture `packages/domain-contract/fixtures/issue-draft.json`.
//!
//! EXP-1212: a draft WITH content never goes silently. The close button asks
//! [`exit_prompt`]'s `DiscardConfirm` first; any other way off the page is
//! HELD behind the `Leave` question (Discard · Save draft · Create issue,
//! Create issue the default). Each prompt is ONE question: there is no body.

/// The identifier slot of the collapsed title.
pub const HEADER: &str = "New issue";
/// The large title input's placeholder.
pub const TITLE_PLACEHOLDER: &str = "Issue title";
/// The description editor's placeholder.
pub const DESCRIPTION_PLACEHOLDER: &str = "Add description...";
/// The primary button that files the draft.
pub const CREATE: &str = "Create";
/// The close (`ui-close`) button's tooltip (EXP-1191: no overflow menu).
pub const DISCARD: &str = "Discard draft";
/// A draft with no title (the collapsed title, the Drafts list).
pub const UNTITLED: &str = "Untitled draft";
/// EXP-1212: the close button's confirmation on a draft with content —
/// the one-line question and the destructive button (beside the platform's
/// Cancel).
pub const DISCARD_CONFIRM_TITLE: &str = "Discard this draft and its files?";
pub const DISCARD_CONFIRM: &str = "Discard";
/// EXP-1212: the question a HELD navigation off a draft with content asks.
pub const LEAVE_TITLE: &str = "Save this issue as a draft?";
/// The page's Create, then the held navigation continues: the DEFAULT
/// answer (primary, trailing, Enter).
pub const LEAVE_CREATE: &str = "Create issue";
/// Save, then continue (the plain button beside Create issue).
pub const LEAVE_KEEP: &str = "Save draft";
/// Delete (no second confirmation), then continue.
pub const LEAVE_DISCARD: &str = "Discard";
/// One coalesced `issueDrafts.upsert` this long after the last
/// title/description edit.
pub const AUTOSAVE_DEBOUNCE_MS: u64 = 800;
/// EXP-1231: the info toast of a page whose draft another client discarded.
pub const DISCARDED_ELSEWHERE: &str = "Draft discarded elsewhere";
/// EXP-1231: how long a synced row may be gone (a resync, this client's own
/// racing write) before the page concludes it was discarded elsewhere.
pub const DISCARDED_GRACE_MS: u64 = 3000;

/// EXP-1231: what the synced store says about this page's draft. The same
/// draft may be open on several clients; whichever one creates or discards
/// it consumes the row for all of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DraftFate {
    /// Still a draft: keep editing.
    Open,
    /// An issue carries this draft's id (`issues.draft_id`): the page becomes
    /// that issue's detail at once, no prompt, no toast.
    Created { issue_id: String },
    /// The synced row went with no such issue: after
    /// [`DISCARDED_GRACE_MS`] the page leaves with [`DISCARDED_ELSEWHERE`].
    Gone,
}

/// EXP-1231: the ONE rule ×4. `created_issue_id` = an issue whose `draft_id`
/// is this draft (proof it was created, wherever); `seen` = the row has been
/// observed in the synced collection during this page's life (a row never
/// seen is never "gone": a brand-new page's first write may not have synced);
/// `present` = it is there now. Created wins; Gone = seen and not present.
pub fn draft_fate(seen: bool, present: bool, created_issue_id: Option<&str>) -> DraftFate {
    match created_issue_id {
        Some(issue_id) => DraftFate::Created {
            issue_id: issue_id.to_string(),
        },
        None if seen && !present => DraftFate::Gone,
        None => DraftFate::Open,
    }
}

/// EXP-1212: how the page is being left. The page's OWN exits (a successful
/// Create, a confirmed Discard) are neither: they never ask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DraftExit {
    /// The close button ("Discard draft").
    Discard,
    /// Any other way off the page: Back/Forward, a rail entry, a tab, opening
    /// another screen, a team switch.
    Leave,
}

/// EXP-1212: what an exit asks before it happens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DraftPrompt {
    /// Nothing: the exit goes at once (an empty draft is deleted as before).
    None,
    /// "Discard this draft and its files?" with a destructive Discard and
    /// Cancel.
    DiscardConfirm,
    /// The navigation is held: Discard · Save draft · Create issue.
    Leave,
}

/// EXP-1212: the ONE rule ×4. `has_content` = a non-blank title, a non-blank
/// description or an attachment (exactly what makes the autosave write a row).
pub fn exit_prompt(has_content: bool, exit: DraftExit) -> DraftPrompt {
    match (has_content, exit) {
        (false, _) => DraftPrompt::None,
        (true, DraftExit::Discard) => DraftPrompt::DiscardConfirm,
        (true, DraftExit::Leave) => DraftPrompt::Leave,
    }
}

/// EXP-1212: the leave dialog's Create is the page's Create — disabled
/// without a title.
pub fn leave_create_enabled(title: &str) -> bool {
    !title.trim().is_empty()
}

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
        let confirm = &copy["discardConfirm"];
        assert_eq!(confirm["title"].as_str(), Some(DISCARD_CONFIRM_TITLE));
        assert_eq!(confirm["confirm"].as_str(), Some(DISCARD_CONFIRM));
        assert_eq!(confirm.as_object().unwrap().len(), 2, "a new string needs a test");
        let leave = &copy["leave"];
        assert_eq!(leave["title"].as_str(), Some(LEAVE_TITLE));
        assert_eq!(leave["create"].as_str(), Some(LEAVE_CREATE));
        assert_eq!(leave["keep"].as_str(), Some(LEAVE_KEEP));
        assert_eq!(leave["discard"].as_str(), Some(LEAVE_DISCARD));
        assert_eq!(leave.as_object().unwrap().len(), 4, "a new string needs a test");
        assert_eq!(copy["discardedElsewhere"].as_str(), Some(DISCARDED_ELSEWHERE));
        assert_eq!(copy.as_object().unwrap().len(), 9, "a new string needs a test");
    }

    #[test]
    fn exit_prompt_table() {
        use DraftExit::*;
        assert_eq!(exit_prompt(false, Discard), DraftPrompt::None);
        assert_eq!(exit_prompt(false, Leave), DraftPrompt::None);
        assert_eq!(exit_prompt(true, Discard), DraftPrompt::DiscardConfirm);
        assert_eq!(exit_prompt(true, Leave), DraftPrompt::Leave);
    }

    #[test]
    fn leave_create_needs_a_title() {
        assert!(!leave_create_enabled(""));
        assert!(!leave_create_enabled("   "));
        assert!(leave_create_enabled(" Fix login "));
    }

    #[test]
    fn issue_draft_autosave_matches_the_fixture() {
        let fixture = fixture();
        let autosave = &fixture["autosave"];
        assert_eq!(autosave["debounceMs"].as_u64(), Some(AUTOSAVE_DEBOUNCE_MS));
        assert_eq!(autosave.as_object().unwrap().len(), 1, "a new constant needs a test");
    }

    #[test]
    fn issue_draft_concurrency_matches_the_fixture() {
        let fixture = fixture();
        let concurrency = &fixture["concurrency"];
        assert_eq!(
            concurrency["discardedGraceMs"].as_u64(),
            Some(DISCARDED_GRACE_MS)
        );
        assert_eq!(concurrency.as_object().unwrap().len(), 1, "a new constant needs a test");
    }

    #[test]
    fn draft_fate_table() {
        let created = || DraftFate::Created {
            issue_id: "i1".into(),
        };
        // (seen, present, created) → fate
        assert_eq!(draft_fate(false, false, None), DraftFate::Open, "never seen is never gone");
        assert_eq!(draft_fate(false, true, None), DraftFate::Open);
        assert_eq!(draft_fate(true, true, None), DraftFate::Open);
        assert_eq!(draft_fate(true, false, None), DraftFate::Gone);
        // Created wins over everything.
        assert_eq!(draft_fate(false, false, Some("i1")), created());
        assert_eq!(draft_fate(true, false, Some("i1")), created());
        assert_eq!(draft_fate(true, true, Some("i1")), created());
        assert_eq!(draft_fate(false, true, Some("i1")), created());
    }
}
