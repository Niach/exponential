import fixture from "@exp/domain-contract/fixtures/issue-draft.json"

// EXP-1170: the New issue PAGE, locked ×4 against the ONE contract fixture
// (desktop domain::issue_draft, iOS ExpCore IssueDraftPage, Android
// domain/IssueDraftPage). There is no create dialog any more: every "New
// issue" opener mints a draft id and navigates to the issue detail in DRAFT
// mode, which autosaves to `issue_drafts` until Create files it.

/** The page's copy: header slot, placeholders, the two actions, the list label. */
export const ISSUE_DRAFT_COPY = fixture.copy

/** Quiet time after the last title/description edit before the draft is written. */
export const ISSUE_DRAFT_AUTOSAVE_MS = fixture.autosave.debounceMs
