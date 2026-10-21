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
        /// `copy.schedule.cron` / `.cronTz`.
        public static let scheduleCron = "Cron {cron}"
        public static let scheduleCronTz = "Cron {cron} ({tz})"
        /// Contract `eventBuiltinNames` → the Triggers row's sentence.
        public static let events: [String: String] = [
            "issue.created": "When an issue is created",
            "issue.status_changed": "When an issue's status changes",
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

    /// One Triggers row's text; a disabled trigger appends ` · Paused`.
    public static func triggerSummary(_ when: TriggerWhen, enabled: Bool) -> String {
        let text: String
        switch when {
        case let .schedule(schedule):
            if let tz = schedule.tz, !tz.isEmpty {
                text = Copy.scheduleCronTz
                    .replacingOccurrences(of: "{cron}", with: schedule.cron)
                    .replacingOccurrences(of: "{tz}", with: tz)
            } else {
                text = Copy.scheduleCron.replacingOccurrences(of: "{cron}", with: schedule.cron)
            }
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
}
