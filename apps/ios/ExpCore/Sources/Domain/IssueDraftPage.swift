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
    /// The close button's accessibility label (EXP-1191).
    public static let discard = "Discard draft"
    /// A draft with no title, in the Drafts list (and the collapsed header).
    public static let untitled = "Untitled draft"
    /// One coalesced `issueDrafts.upsert` this long after the last
    /// title/description edit.
    public static let autosaveDebounceMs: Double = 800

    /// EXP-1212: the close button on a draft WITH content asks first.
    public enum DiscardConfirm {
        public static let title = "Discard draft?"
        public static let body = "This draft and its files will be deleted."
        /// The destructive answer; the other is the platform's Cancel.
        public static let confirm = "Discard"
    }

    /// EXP-1212: leaving a draft WITH content any other way is HELD and asks.
    public enum Leave {
        public static let title = "This issue is still a draft"
        public static let body = "Create it now, keep it as a draft or discard it."
        /// The page's Create; the held navigation then continues.
        public static let create = "Create"
        /// Save, then continue.
        public static let keep = "Keep as draft"
        /// Delete, then continue (destructive, no second confirmation).
        public static let discard = "Discard"
    }

    /// How the page is being left.
    public enum Exit: Equatable, Sendable {
        /// The close (`×`) button.
        case discard
        /// Any other way out: Back, a tab, a pushed screen, a link.
        case leave
        /// The page's own exits: a successful Create, a confirmed Discard.
        case own
    }

    /// What the page asks before an exit goes through.
    public enum Prompt: Equatable, Sendable {
        case none
        case discardConfirm
        case leave
    }

    /// A title, a description or an attachment is content; chips never are.
    /// A reopened draft whose file list is not known yet (`attachmentsKnown`
    /// false) counts as content: it may be a file-only draft. A brand-new
    /// draft is known to have none.
    public static func hasContent(
        title: String,
        description: String,
        attachmentCount: Int,
        attachmentsKnown: Bool = true
    ) -> Bool {
        !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            || !description.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            || attachmentCount > 0
            || !attachmentsKnown
    }

    /// The leave prompt's answers, in reading order.
    public enum LeaveChoice: Equatable, Sendable {
        case create
        case keep
        case discard
    }

    /// The leave prompt's choices. `canKeep` = the page writes a draft row; a
    /// mode that never does (sub-issue compose) has nothing to keep, so it
    /// offers Create and Discard only (plus the platform's Cancel).
    public static func leaveChoices(canKeep: Bool) -> [LeaveChoice] {
        canKeep ? [.create, .keep, .discard] : [.create, .discard]
    }

    /// EXP-1212: a draft with no content never asks (it goes and is deleted
    /// as before); one with content asks on Discard and on leaving; the
    /// page's own exits never ask.
    public static func prompt(for exit: Exit, hasContent: Bool) -> Prompt {
        guard hasContent else { return .none }
        switch exit {
        case .discard: return .discardConfirm
        case .leave: return .leave
        case .own: return .none
        }
    }

    /// The leave prompt's Create runs the page's Create: disabled without a
    /// title (or while a create is already in flight).
    public static func leaveCreateEnabled(title: String, creating: Bool = false) -> Bool {
        !creating && !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }
}
