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

    // MARK: - S-3 THE ASSISTANT (fixture `assistant`, contract `agentRole`)

    /// Fixture `assistant.copy`.
    public enum AssistantCopy {
        public static let title = "Assistant"
        /// The default name (the member may rename it).
        public static let name = "Assistant"
        public static let runsOn = "Runs on"
        public static let checkIn = "Check in"
        public static let voice = "Voice"
        public static let noDevice = "Connect a desktop to set up your Assistant"
        public static let deleteRefused = "The Assistant can't be deleted"
        public static let scheduleRefused = "The Assistant checks in on a schedule only"
        /// #EXP-1439: `agents.update` refuses an Assistant's instructions.
        public static let promptFixed = "The Assistant's prompt is built in"
        public static let legacyInstructions = "Your Assistant now runs on Exponential's built-in prompt, so its instructions field is gone. Your old instructions, to move into a team agent if you still need them:"
    }

    /// The Assistant page's sections top to bottom (fixture
    /// `assistant.sections`): identity | runs-on | check-in | voice. No
    /// Instructions (#EXP-1439: the prompt is built in), no Delete.
    public static let assistantSections = ["identity", "runs-on", "check-in", "voice"]

    /// One check-in preset (fixture `assistant.schedules`).
    public enum AssistantScheduleId: String, Equatable, Sendable, CaseIterable {
        case off
        case every15m = "15m"
        case every30m = "30m"
        case hourly = "1h"
        case daily
    }

    public struct AssistantSchedule: Equatable, Sendable, Identifiable {
        public let id: AssistantScheduleId
        public let label: String
        /// nil = `off` (no trigger).
        public let cron: String?
    }

    /// THE Picker rows of the `Check in` section, in order.
    public static let assistantSchedules: [AssistantSchedule] = [
        AssistantSchedule(id: .off, label: "Off", cron: nil),
        AssistantSchedule(id: .every15m, label: "Every 15 minutes", cron: "*/15 * * * *"),
        AssistantSchedule(id: .every30m, label: "Every 30 minutes", cron: "*/30 * * * *"),
        AssistantSchedule(id: .hourly, label: "Every hour", cron: "0 * * * *"),
        AssistantSchedule(id: .daily, label: "Every day", cron: "0 9 * * *"),
    ]

    public static let assistantDefaultSchedule: AssistantScheduleId = .hourly

    /// #EXP-1439: the built-in, versioned prompt every Assistant runs on
    /// (fixture `assistant.prompt`; server-side, read-only here).
    public static let assistantPromptVersion = 2
    public static let assistantPrompt = "You are this member's Assistant in this team: the main agent that manages every other chat, agent and pull request they have here. They talk to you; you run the rest.\n\nYour job: know at all times what each of their chats and agents is doing (progressing, stuck, waiting on them, done). Start, steer, nudge, move and end their chats to get the work done. Answer a chat's question yourself when what they told you settles it; escalate only real decisions.\n\nYour status card: keep ONE live status card (an Exponential UI surface via exponential_show): in progress (chat, PR, state), shipped since they last wrote, stuck, decisions needed. Every surface you show updates that card in place. Keep it short: one row per chat, newest first; drop finished items once they have seen them.\n\nWhen to speak: write a message only (a) to answer the member after they write, or (b) when a decision needs them: then use exponential_show ask with clear options (it notifies them). Scheduled check-ins, a PR opened, still running, a child's routine reply: update the card and end your turn silently. Never repeat what the card already says.\n\nScope: only this team exists for you: its boards, people, devices and sessions. Never use a device or account of another team.\n\nStyle: short, concrete, no filler. Name issues as #IDENT."

    /// The Assistant's Model row value (`modelValueCases`): the pinned
    /// model, else the dispatcher root's default for the login's CLI (never
    /// `Last used`).
    public static func assistantModelValue(model: String?, cli: String?) -> String {
        if let model, !model.trimmingCharacters(in: .whitespaces).isEmpty { return model }
        return cli == "codex"
            ? DomainContract.deviceAgentDefaultsCodexThreadModel
            : DomainContract.deviceAgentDefaultsThreadModel
    }

    /// NEW-99 (`runsOnCases`): the Runs on device picker's devices. An
    /// Assistant's device is the member's voice device, so it lists desktops
    /// only (a bound non-desktop stays, naming itself); a team agent keeps all.
    public static func runsOnDeviceIds(
        _ devices: [(deviceId: String, kind: String?)],
        boundId: String?,
        assistant: Bool
    ) -> [String] {
        devices
            .filter { !assistant || $0.kind == "desktop" || $0.deviceId == boundId }
            .map(\.deviceId)
    }

    /// `assistant.scheduleCases`: the preset stored triggers stand for; nil
    /// = not an Assistant's (an event, a custom cron, more than one) and
    /// `agents.update` refuses it. A paused preset reads `off`.
    public static func assistantScheduleOf(_ triggers: [(when: TriggerWhen, enabled: Bool)]) -> AssistantScheduleId? {
        if triggers.isEmpty { return .off }
        guard triggers.count == 1, let trigger = triggers.first,
              case let .schedule(schedule) = trigger.when,
              let preset = assistantSchedules.first(where: { $0.cron != nil && $0.cron == schedule.cron })
        else { return nil }
        return trigger.enabled ? preset.id : .off
    }

    public static func assistantScheduleOf(_ triggers: [AgentTrigger]) -> AssistantScheduleId? {
        assistantScheduleOf(triggers.map { (when: $0.when, enabled: $0.enabled) })
    }

    /// The whole-array `triggers` write a preset saves as (`off` = none).
    public static func assistantScheduleTriggers(_ id: AssistantScheduleId, tz: String? = nil) -> [AgentTriggerInput] {
        guard let cron = assistantSchedules.first(where: { $0.id == id })?.cron else { return [] }
        let zone = (tz?.isEmpty ?? true) ? nil : tz
        return [AgentTriggerInput(enabled: true, when: .schedule(ScheduleTrigger(cron: cron, tz: zone)))]
    }

    public static func isAssistant(role: String?) -> Bool {
        role == DomainContract.agentRoleAssistant
    }

    /// `assistant.visibleCases`: another member's Assistant is never listed
    /// (settings, composer, chats).
    public static func visibleAgent(role: String?, ownerId: String?, me: String?) -> Bool {
        !isAssistant(role: role) || ownerId == me
    }

    public static func visibleAgent(_ agent: some AgentRoleOrdered, me: String?) -> Bool {
        visibleAgent(role: agent.agentRole, ownerId: agent.agentOwnerId, me: me)
    }

    /// `assistant.orderCases`: the agents a member sees, in order: their
    /// Assistant first, then the team agents by sortOrder, then name.
    public static func orderedAgents<T: AgentRoleOrdered>(_ agents: [T], me: String?) -> [T] {
        agents
            .filter { visibleAgent($0, me: me) }
            .enumerated()
            .sorted { lhs, rhs in
                let a = lhs.element, b = rhs.element
                let aAssistant = isAssistant(role: a.agentRole), bAssistant = isAssistant(role: b.agentRole)
                if aAssistant != bAssistant { return aAssistant }
                if a.agentSortOrder != b.agentSortOrder { return a.agentSortOrder < b.agentSortOrder }
                let byName = a.agentName.localizedCompare(b.agentName)
                if byName != .orderedSame { return byName == .orderedAscending }
                return lhs.offset < rhs.offset
            }
            .map(\.element)
    }
}

/// S-3: what `AgentSettings.orderedAgents` / `visibleAgent` /
/// `ComposerAgentRow` read off an agent row.
public protocol AgentRoleOrdered {
    var agentId: String { get }
    var agentName: String { get }
    var agentRole: String? { get }
    var agentOwnerId: String? { get }
    var agentSortOrder: Double { get }
}

extension AgentDefinitionEntity: AgentRoleOrdered {
    public var agentId: String { id }
    public var agentName: String { name }
    public var agentRole: String? { role }
    public var agentOwnerId: String? { ownerId }
    public var agentSortOrder: Double { sortOrder }
}

extension AgentLike: AgentRoleOrdered {
    public var agentId: String { id }
    public var agentName: String { name }
    public var agentRole: String? { role }
    public var agentOwnerId: String? { ownerId }
    public var agentSortOrder: Double { sortOrder }
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
