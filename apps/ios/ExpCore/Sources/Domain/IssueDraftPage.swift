import Foundation

/// EXP-1170 — the NEW ISSUE page: a draft opened in the issue face's layout.
/// Its copy and the autosave debounce, mirrored ×4 (web
/// `lib/issue-draft-page.ts`, desktop `domain::issue_draft`, Android
/// `domain/IssueDraftPage.kt`) and locked by the contract fixture
/// `domain-contract/fixtures/issue-draft.json`.
public enum IssueDraftPage {
    /// The header's identifier slot.
    public static let header = "New issue"
    public static let titlePlaceholder = "Issue title"
    public static let descriptionPlaceholder = "Add description..."
    /// The trailing primary button.
    public static let create = "Create"
    /// The overflow menu's ONLY item.
    public static let discard = "Discard draft"
    /// A draft with no title, in the Drafts list (and the collapsed header).
    public static let untitled = "Untitled draft"
    /// One coalesced `issueDrafts.upsert` this long after the last
    /// title/description edit.
    public static let autosaveDebounceMs: Double = 800
}
