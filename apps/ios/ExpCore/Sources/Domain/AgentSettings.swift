import Foundation

// EXP-1291 Threads v2: Team settings → Agents, managed exactly like boards (a
// Settings row per agent, the board page layout per agent). A 1:1 port of the
// TS reference `packages/domain-contract/src/agent-settings.ts` (desktop
// `domain::agent_settings`, Android `domain/AgentSettings.kt`), byte-locked by
// `fixtures/agent-settings.json` (`summaryCases`, `validationCases`, replayed
// by `AgentSettingsTests`; EXP-1306 `accountValueCases`). The fixture's `$comment` holds the rules.

public enum AgentSettings {
    /// One row of the agent page, top to bottom (fixture `fields`).
    public struct Field: Equatable, Sendable, Identifiable {
        /// identity | device | account | triggers | instructions | delete.
        public let key: String
        public let label: String
        /// icon-color-name | device-picker | account-picker | trigger-list |
        /// markdown | danger.
        public let control: String
        public let required: Bool
        /// Hidden from members (they read).
        public let ownerOnly: Bool

        public var id: String { key }

        public init(key: String, label: String, control: String, required: Bool, ownerOnly: Bool = false) {
            self.key = key
            self.label = label
            self.control = control
            self.required = required
            self.ownerOnly = ownerOnly
        }
    }

    public static let fields: [Field] = [
        Field(key: "identity", label: "Name", control: "icon-color-name", required: true),
        Field(key: "device", label: "Device", control: "device-picker", required: true),
        Field(key: "account", label: "Account", control: "account-picker", required: false),
        Field(key: "triggers", label: "Triggers", control: "trigger-list", required: false),
        Field(key: "instructions", label: "Instructions", control: "markdown", required: false),
        Field(key: "delete", label: "Delete agent", control: "danger", required: false, ownerOnly: true),
    ]

    /// Fixture `copy`.
    public enum Copy {
        public static let title = "Agents"
        public static let newAgent = "New agent"
        public static let namePlaceholder = "Agent name"
        public static let instructionsPlaceholder = "What this agent takes care of…"
        public static let addTrigger = "Add"
        public static let noSchedule = "Waits for messages"
        public static let paused = "Paused"
        public static let deleteConfirm = "Delete this agent? Its conversation is deleted with it."
        /// Contract `eventBuiltinNames` → the Triggers row's sentence.
        public static let events: [String: String] = [
            "issue.created": "When an issue is created",
            "issue.status_changed": "When an issue's status changes",
            "issue.assigned": "When an issue is assigned",
            "pr.opened": "When a pull request opens",
            "pr.merged": "When a pull request merges",
            "session.ended": "When a session ends",
        ]
        /// A custom event name's sentence.
        public static let event = "On {name}"
        public static let nameRequired = "Name the agent"
        public static let nameTooLong = "Use at most 64 characters"
        public static let deviceRequired = "Pick a device"
        public static let instructionsTooLong = "Instructions are too long"
        /// EXP-1306: the Account row with no login reported (muted).
        public static let accountLastUsed = "Last used"
    }

    /// Fixture `testIds`.
    public enum TestIds {
        public static let list = "agents-settings-list"
        public static let page = "agent-settings-page"
    }

    /// One Triggers row's text (a schedule = `SchedulePicker.scheduleSummary`);
    /// a disabled trigger appends ` · Paused`.
    public static func triggerSummary(_ when: TriggerWhen, enabled: Bool) -> String {
        let text: String
        switch when {
        case let .schedule(schedule):
            // NEW-2: THE schedule picker's sentence (`SchedulePicker`).
            text = SchedulePicker.scheduleSummary(cron: schedule.cron, tz: schedule.tz)
        case let .event(event):
            text = Copy.events[event.name]
                ?? Copy.event.replacingOccurrences(of: "{name}", with: event.name)
        }
        return enabled ? text : "\(text) · \(Copy.paused)"
    }

    public static func triggerSummary(_ trigger: AgentTrigger) -> String {
        triggerSummary(trigger.when, enabled: trigger.enabled)
    }

    /// The agent page's validated fields.
    public struct Form: Equatable, Sendable {
        public var name: String
        public var deviceId: String?
        public var instructions: String

        public init(name: String, deviceId: String?, instructions: String) {
            self.name = name
            self.deviceId = deviceId
            self.instructions = instructions
        }
    }

    public struct FormError: Equatable, Sendable {
        /// A `Field.key`.
        public let field: String
        public let message: String

        public init(field: String, message: String) {
            self.field = field
            self.message = message
        }
    }

    /// Field errors in field order; [] = valid.
    public static func validate(_ form: Form) -> [FormError] {
        var errors: [FormError] = []
        let name = form.name.trimmingCharacters(in: .whitespacesAndNewlines)
        if name.isEmpty {
            errors.append(FormError(field: "identity", message: Copy.nameRequired))
        } else if name.unicodeScalars.count > DomainContract.sessionsSendAgentNameMax {
            errors.append(FormError(field: "identity", message: Copy.nameTooLong))
        }
        if (form.deviceId ?? "").isEmpty {
            errors.append(FormError(field: "device", message: Copy.deviceRequired))
        }
        if form.instructions.utf8.count > DomainContract.sessionsSendAgentInstructionsMaxBytes {
            errors.append(FormError(field: "instructions", message: Copy.instructionsTooLong))
        }
        return errors
    }

    /// EXP-1306: what the Account row reads (the agent's brand mark + this
    /// login's email), the composer chip's value: the pinned `account` (under
    /// `cli` when set), else `cli`'s first login, else the last used one,
    /// else the first; nil = no login reported (`Copy.accountLastUsed`).
    public static func agentAccountValue(
        options: [AccountOption], account: String?, cli: String?
    ) -> AccountOption? {
        if let account, let pinned = options.first(where: { $0.id == account && (cli == nil || $0.agent == cli) }) {
            return pinned
        }
        if let cli, let first = options.first(where: { $0.agent == cli }) { return first }
        return options.first(where: \.isLastUsed) ?? options.first
    }

    // MARK: - #EXP-1383 agents act as themselves, take assignments

    /// Fixture `assignee.copy`.
    public enum AssigneeCopy {
        public static let unassigned = "Unassigned"
        public static let nudge = "{identifier} was assigned to you: {title}"
        /// An agent assignee / actor whose row is gone (deleted, or not
        /// synced) — web `AGENT_FALLBACK_NAME`.
        public static let agentFallback = "an agent"
    }

    /// The agent an actor / assignee resolves to (an `agents` row's mark).
    public struct ActorAgent: Equatable, Sendable {
        public let id: String
        public let name: String
        public let icon: String?
        public let color: String?

        public init(id: String, name: String, icon: String?, color: String?) {
            self.id = id
            self.name = name
            self.icon = icon
            self.color = color
        }

        public init(_ agent: AgentDefinitionEntity) {
            self.init(id: agent.id, name: agent.name, icon: agent.icon, color: agent.color)
        }
    }

    /// Who an issue event / comment / issue creation reads as.
    public enum IssueActor: Equatable, Sendable {
        case agent(ActorAgent)
        case user(id: String)

        public var agent: ActorAgent? {
            if case let .agent(agent) = self { return agent }
            return nil
        }
    }

    /// `actor.actorCases`: a synced agent first, else the member, else nil
    /// (the server / a widget reporter).
    public static func issueActor(
        actorAgentId: String?, actorUserId: String?, agents: [ActorAgent]
    ) -> IssueActor? {
        if let actorAgentId, !actorAgentId.isEmpty, let agent = agents.first(where: { $0.id == actorAgentId }) {
            return .agent(agent)
        }
        if let actorUserId, !actorUserId.isEmpty { return .user(id: actorUserId) }
        return nil
    }

    public static func issueActor(
        actorAgentId: String?, actorUserId: String?, agents: [AgentDefinitionEntity]
    ) -> IssueActor? {
        issueActor(actorAgentId: actorAgentId, actorUserId: actorUserId, agents: agents.map(ActorAgent.init))
    }

    /// An agent row in the assignee picker (`assigneeOptions` input).
    public struct AssigneeAgentOption: Equatable, Sendable {
        public let id: String
        public let name: String
        public let sortOrder: Double

        public init(id: String, name: String, sortOrder: Double) {
            self.id = id
            self.name = name
            self.sortOrder = sortOrder
        }
    }

    /// The `agent:` prefix of an agent option id.
    public static let assigneeAgentPrefix = "agent:"

    /// `assignee.optionCases`: the picker's option ids ×4 — `unassigned`,
    /// `user:<id>` per member in the given order, then `agent:<id>` per team
    /// agent by sortOrder, then name (then id).
    public static func assigneeOptions(memberIds: [String], agents: [AssigneeAgentOption]) -> [String] {
        let sorted = agents.sorted { lhs, rhs in
            if lhs.sortOrder != rhs.sortOrder { return lhs.sortOrder < rhs.sortOrder }
            let byName = lhs.name.localizedCompare(rhs.name)
            if byName != .orderedSame { return byName == .orderedAscending }
            return lhs.id < rhs.id
        }
        return ["unassigned"]
            + memberIds.map { "user:\($0)" }
            + sorted.map { "\(assigneeAgentPrefix)\($0.id)" }
    }

    /// The team's agents in the assignee picker's order.
    public static func assigneeAgents(_ agents: [AgentDefinitionEntity]) -> [AgentDefinitionEntity] {
        let byId = Dictionary(agents.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        return assigneeOptions(
            memberIds: [],
            agents: agents.map { AssigneeAgentOption(id: $0.id, name: $0.name, sortOrder: $0.sortOrder) }
        )
        .compactMap { id in
            guard id.hasPrefix(assigneeAgentPrefix) else { return nil }
            return byId[String(id.dropFirst(assigneeAgentPrefix.count))]
        }
    }

    /// The solo-team rule: a team of one member hides its assignee control
    /// only while it has no agents (an agent is someone to assign to).
    public static func showsAssignee(memberCount: Int, agentCount: Int) -> Bool {
        memberCount != 1 || agentCount > 0
    }
}

/// #EXP-1383 (agent-settings.json `assignee`): an issue's assignee is ONE of
/// two mutually exclusive columns, a member (`assigneeId`) or a team agent
/// (`assigneeAgentId`). Every client write sends BOTH, the cleared one as an
/// explicit null, so picking an agent unassigns the person and vice versa.
public struct IssueAssignee: Equatable, Hashable, Sendable {
    public let assigneeId: String?
    public let assigneeAgentId: String?

    public static let unassigned = IssueAssignee(assigneeId: nil, assigneeAgentId: nil)

    /// The two-column write: an agent wins and clears the person.
    public init(assigneeId: String?, assigneeAgentId: String?) {
        if let assigneeAgentId, !assigneeAgentId.isEmpty {
            self.assigneeId = nil
            self.assigneeAgentId = assigneeAgentId
        } else {
            self.assigneeId = (assigneeId?.isEmpty ?? true) ? nil : assigneeId
            self.assigneeAgentId = nil
        }
    }

    public static func user(_ id: String?) -> IssueAssignee { IssueAssignee(assigneeId: id, assigneeAgentId: nil) }
    public static func agent(_ id: String) -> IssueAssignee { IssueAssignee(assigneeId: nil, assigneeAgentId: id) }

    public var isUnassigned: Bool { assigneeId == nil && assigneeAgentId == nil }

    /// The ONE picker value: `agent:<id>` for an agent, the bare user id for
    /// a member, "" for nobody (`AssigneePicker.unassignedValue`).
    public var pickerValue: String {
        if let assigneeAgentId { return AgentSettings.assigneeAgentPrefix + assigneeAgentId }
        return assigneeId ?? ""
    }

    /// A pick back into the columns.
    public init(pickerValue value: String?) {
        guard let value, !value.isEmpty else {
            self.init(assigneeId: nil, assigneeAgentId: nil)
            return
        }
        if value.hasPrefix(AgentSettings.assigneeAgentPrefix) {
            self.init(assigneeId: nil, assigneeAgentId: String(value.dropFirst(AgentSettings.assigneeAgentPrefix.count)))
        } else {
            self.init(assigneeId: value, assigneeAgentId: nil)
        }
    }
}
