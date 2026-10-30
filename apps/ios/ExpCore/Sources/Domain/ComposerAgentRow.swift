import Foundation

/// S-3 (owner 2026-10-28): the New-chat composer's AGENT ROW, a 1:1 port of
/// the TS reference `packages/domain-contract/src/composer-agent-row.ts`
/// (desktop `domain::composer_agent_row`, Android `ComposerAgentRow.kt`),
/// locked by `fixtures/composer-home.json` `agentRow.cases`
/// (`ComposerAgentRowTests`). The fixture's `$comment` holds the rules: the
/// circles = the chats agent bubble at `size`, the viewer's Assistant first
/// (brand mark) and preselected, then the team agents they see, then ONE
/// dotted circle (`temporaryIcon`) = Temporary session.
public enum ComposerAgentRow {
    /// The dotted circle's option id (Temporary session = a normal session).
    public static let temporaryPick = "temporary"
    /// The dotted circle's concept glyph (`packages/icons/icons.json`).
    public static let temporaryIcon = "ui-icon-placeholder"

    /// Circle diameter (pt), the chats agent bubble at a smaller size.
    public static let size: Double = 40
    /// The selected circle's foreground ring width.
    public static let ringWidth: Double = 2
    /// The background gap between the disc and its ring.
    public static let ringGap: Double = 3
    /// Between circles.
    public static let gap: Double = 10
    /// Label line → circles.
    public static let labelToRow: Double = 8
    /// Circles → the composer card.
    public static let rowToCard: Double = 12

    /// Fixture `agentRow.copy`.
    public enum Copy {
        /// `{name}` = the picked agent's name.
        public static let to = "To {name}"
        public static let temporary = "Temporary session"
    }

    /// Fixture `agentRow.testIds`.
    public enum TestIds {
        public static let row = "composer-agent-row"
        public static let label = "composer-agent-label"
        public static func option(_ id: String) -> String { "composer-agent-\(id)" }
    }

    public struct State<Agent: AgentRoleOrdered> {
        /// The circles in order (`AgentSettings.orderedAgents`); the dotted
        /// circle follows them.
        public let agents: [Agent]
        /// Option ids in order, `temporaryPick` last.
        public let options: [String]
        /// The selected option id (never empty).
        public let selected: String
        /// The picked agent; nil = Temporary session (the + menu's
        /// `temporary` condition).
        public let agent: Agent?
        /// The label line over the circles.
        public let label: String

        public var isTemporary: Bool { agent == nil }
    }

    /// The row for a viewer: `picked` = the last tap (nil / an id that left
    /// the list = the default: the Assistant, else Temporary session).
    public static func composerAgentRow<Agent: AgentRoleOrdered>(
        agents: [Agent], me: String?, picked: String?
    ) -> State<Agent> {
        let ordered = AgentSettings.orderedAgents(agents, me: me)
        let options = ordered.map(\.agentId) + [temporaryPick]
        let fallback = ordered.first { AgentSettings.isAssistant(role: $0.agentRole) }?.agentId ?? temporaryPick
        let selected: String
        if let picked, !picked.isEmpty, options.contains(picked) {
            selected = picked
        } else {
            selected = fallback
        }
        let agent = ordered.first { $0.agentId == selected }
        let label = agent.map { Copy.to.replacingOccurrences(of: "{name}", with: $0.agentName) } ?? Copy.temporary
        return State(agents: ordered, options: options, selected: selected, agent: agent, label: label)
    }

    /// The pick a page opened WITH a subject starts on (`agentRow.seedCases`):
    /// issues, a prompt run or a device seed = a launch, so Temporary
    /// session; nil = the default.
    public static func seedPick(issueIds: [String], promptId: String?, deviceId: String?) -> String? {
        !issueIds.isEmpty || promptId != nil || deviceId != nil ? temporaryPick : nil
    }

    /// The pick after the composer's subject changed (`agentRow.subjectCases`):
    /// an issue or prompt subject is a launch, so the row follows to
    /// Temporary session; any other subject keeps `current`.
    public static func pickForSubject(_ subject: AgentComposerPrompt.Subject?, current: String?) -> String? {
        switch subject {
        case .issues?, .prompt?: temporaryPick
        default: current
        }
    }
    /// The composer's subject as the pick rules read it (TS
    /// `ComposerSubjectValue`): issue chips, or one prompt; nil = none.
    public enum SubjectValue: Equatable, Sendable {
        case issues([String])
        case prompt(String)
    }

    /// The subject after a tap on the row (`agentRow.pickCases`, EXP-1450):
    /// a pick NEVER drops issue chips (a picked agent gets them forwarded as
    /// the send's `issueIds`); only a prompt goes when an AGENT is picked (a
    /// prompt starts a new root).
    public static func subjectAfterPick(_ subject: SubjectValue?, pick: String) -> SubjectValue? {
        if pick != temporaryPick, case .prompt? = subject { return nil }
        return subject
    }
}
