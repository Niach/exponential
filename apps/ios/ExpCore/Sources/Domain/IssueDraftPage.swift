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
    /// The trailing primary button — alone: EXP-1247 dropped the close
    /// button and its confirm ×4 (discarding is the leave prompt's answer).
    public static let create = "Create"
    /// A draft with no title, in the Drafts list (and the collapsed header).
    public static let untitled = "Untitled draft"
    /// One coalesced `issueDrafts.upsert` this long after the last
    /// title/description edit.
    public static let autosaveDebounceMs: Double = 800
    /// EXP-1231: the toast when another client discarded this draft.
    public static let discardedElsewhere = "Draft discarded elsewhere"
    /// EXP-1231: a seen row gone this long with no issue made from it =
    /// discarded elsewhere (a row back within it = a resync; editing resumes).
    public static let discardedGraceMs: Double = 3000

    /// EXP-1212: leaving a draft WITH content is HELD and asks.
    public enum Leave {
        /// The prompt's ONE line: the question, no body.
        public static let title = "Save this issue as a draft?"
        /// Delete, then continue (a quiet destructive text button on the
        /// leading edge, never focused; no second confirmation).
        public static let discard = "Discard"
        /// Save, then continue (the plain button beside Create).
        public static let keep = "Save draft"
        /// The page's Create: the DEFAULT answer (primary, trailing, Return);
        /// the held navigation then continues.
        public static let create = "Create issue"
    }

    /// How the page is being left.
    public enum Exit: Equatable, Sendable {
        /// Any way out: Back, a tab, a pushed screen, a link.
        case leave
        /// The page's own exits: a successful Create, a leave-prompt Discard.
        case own
    }

    /// What the page asks before an exit goes through.
    public enum Prompt: Equatable, Sendable {
        case none
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

    /// The leave prompt's answers.
    public enum LeaveChoice: Equatable, Sendable {
        case discard
        case keep
        case create
    }

    /// The leave prompt's choices in reading order: Discard (leading) …
    /// Save draft · Create issue (trailing). `canKeep` = the page writes a
    /// draft row; a mode that never does (sub-issue compose) has nothing to
    /// keep, so it offers Discard and Create only.
    public static func leaveChoices(canKeep: Bool) -> [LeaveChoice] {
        canKeep ? [.discard, .keep, .create] : [.discard, .create]
    }

    /// The leave prompt's default answer (initial focus + Return): Create,
    /// unless it is disabled, then Save draft; nil when neither can take it
    /// (no Keep and Create disabled). Never Discard.
    public static func leaveDefault(canKeep: Bool, createEnabled: Bool) -> LeaveChoice? {
        if createEnabled { return .create }
        return canKeep ? .keep : nil
    }

    /// EXP-1212: a draft with no content never asks (it goes and is deleted
    /// as before); one with content asks on leaving; the page's own exits
    /// never ask.
    public static func prompt(for exit: Exit, hasContent: Bool) -> Prompt {
        guard hasContent else { return .none }
        switch exit {
        case .leave: return .leave
        case .own: return .none
        }
    }

    /// The leave prompt's Create runs the page's Create: disabled without a
    /// title (or while a create is already in flight).
    public static func leaveCreateEnabled(title: String, creating: Bool = false) -> Bool {
        !creating && !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// EXP-1231: what the synced store says about this page's draft.
    public enum Fate: Equatable, Sendable {
        /// Still a draft (or not synced yet): keep editing.
        case open
        /// An issue carries this draft's id: created elsewhere, open it.
        case created(issueId: String)
        /// The row this page saw is gone (discarded elsewhere once the grace
        /// passes with no issue made from it).
        case gone
    }

    /// EXP-1231: `createdIssueId` = an issue whose `draft_id` is this draft
    /// (proof it was created, wherever); `seen` = the row was observed in the
    /// local store during this page's life; `present` = it is there now.
    /// Created wins over everything; a row never seen is never gone.
    public static func fate(seen: Bool, present: Bool, createdIssueId: String?) -> Fate {
        if let createdIssueId { return .created(issueId: createdIssueId) }
        return seen && !present ? .gone : .open
    }
}
