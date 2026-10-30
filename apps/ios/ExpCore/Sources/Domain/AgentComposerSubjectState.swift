import Foundation

/// EXP-1450: the Agent composer's SUBJECT (the issue chips or one prompt)
/// and the agent row's recipient, kept APART from the launch options
/// (`LaunchOptionsState`): switching the device, runner agent, account,
/// model, effort or repository never reaches it (composer-home.json
/// `agentRow.controlCases`). A tap on the agent row keeps the issue chips
/// and drops only a prompt (`agentRow.pickCases`,
/// `ComposerAgentRow.subjectAfterPick`); a picked agent gets the chips
/// forwarded as the send's `issueIds`. Picking an issue or a prompt is a
/// launch: it REPLACES the other subject and the recipient (exclusivity by
/// swap; the row follows to Temporary session, `subjectCases`).
public struct AgentComposerSubjectState: Equatable, Sendable {
    /// Checked issue ids in pick order (the send's `issueIds` order).
    public private(set) var checked: [String] = []
    /// The picked prompt (NEW-2); nil = issues or a free request.
    public private(set) var promptId: String?
    /// The agent row's picked agent (S-3); nil = Temporary session.
    public private(set) var agentId: String?

    public init() {}

    /// The subject as the pick rules read it.
    public var value: ComposerAgentRow.SubjectValue? {
        if let promptId { return .prompt(promptId) }
        return checked.isEmpty ? nil : .issues(checked)
    }

    /// A play button's preselection: a prompt wins over issues; neither
    /// keeps the current subject.
    public mutating func seed(issueIds: [String], promptId: String?) {
        if let promptId {
            self.promptId = promptId
            agentId = nil
            checked = []
        } else if !issueIds.isEmpty {
            self.promptId = nil
            agentId = nil
            checked = issueIds
        }
    }

    /// Check or uncheck an issue: it REPLACES a prompt and the recipient.
    public mutating func toggleIssue(_ id: String) {
        promptId = nil
        agentId = nil
        if let index = checked.firstIndex(of: id) {
            checked.remove(at: index)
        } else {
            checked.append(id)
        }
    }

    /// The `/` menu's pick: the prompt REPLACES the issue chips and the
    /// recipient.
    public mutating func pickPrompt(_ id: String) {
        promptId = id
        agentId = nil
        checked = []
    }

    public mutating func clearPrompt() {
        promptId = nil
    }

    /// The agent row's tap on an agent: the issue chips STAY, a prompt goes.
    public mutating func pickAgent(_ id: String) {
        apply(ComposerAgentRow.subjectAfterPick(value, pick: id))
        agentId = id
    }

    /// The agent row's Temporary session: the subject stays.
    public mutating func clearAgent() {
        apply(ComposerAgentRow.subjectAfterPick(value, pick: ComposerAgentRow.temporaryPick))
        agentId = nil
    }

    /// After a send: the subject clears, the row's pick stays.
    public mutating func clearSubject() {
        checked = []
        promptId = nil
    }

    private mutating func apply(_ next: ComposerAgentRow.SubjectValue?) {
        switch next {
        case let .issues(ids)?:
            checked = ids
            promptId = nil
        case let .prompt(id)?:
            promptId = id
            checked = []
        case nil:
            checked = []
            promptId = nil
        }
    }
}
