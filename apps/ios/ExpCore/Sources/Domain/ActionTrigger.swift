import Foundation

// SLOP-2: an action carries its triggers. `actions.triggers` is a jsonb array
// on the synced actions shape; each element is a RUNNER (a stable id, the
// enabled flag, the bound device and the optional agent/account/model/effort
// pins) flattened beside its WHEN-part (`AutomationTrigger`: schedule or
// event). The bound device fires it locally — nothing on a phone ever does.
//
// Reads are TOLERANT (web `parseActionTrigger(s)` parity): an element without
// an id or a device, or with an unreadable when-part, is skipped, never a
// throw. Writes are a WHOLE-ARRAY replace through `actions.update({ id,
// triggers })` — `ActionTriggerInput` is one element of that array.

/// One stored trigger of an action.
public struct ActionTrigger: Identifiable, Sendable, Equatable {
    /// Stable; also what a triggered start carries as `automationId`
    /// (`coding_sessions.automation_id`).
    public let id: String
    /// A paused trigger keeps its config.
    public let enabled: Bool
    /// The steer device id (`devices.device_id`) that fires it locally.
    public let deviceId: String
    /// nil = the device's launch defaults.
    public let agent: String?
    /// The agent profile id on the bound device (belongs to `agent`); nil =
    /// unpinned, that machine's last used login for it.
    public let account: String?
    public let model: String?
    public let effort: String?
    public let when: AutomationTrigger

    public init(
        id: String,
        enabled: Bool = true,
        deviceId: String,
        agent: String? = nil,
        account: String? = nil,
        model: String? = nil,
        effort: String? = nil,
        when: AutomationTrigger
    ) {
        self.id = id
        self.enabled = enabled
        self.deviceId = deviceId
        self.agent = agent
        self.account = account
        self.model = model
        self.effort = effort
        self.when = when
    }

    // MARK: - Tolerant parse

    /// The readable triggers of the stored/synced JSON array string, in
    /// order. Anything else — nil, malformed JSON, a non-array — is empty.
    public static func parseList(_ raw: String?) -> [ActionTrigger] {
        guard let raw, !raw.isEmpty,
              let data = raw.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data)
        else { return [] }
        return parseList(json: json)
    }

    static func parseList(json: Any) -> [ActionTrigger] {
        guard let entries = json as? [Any] else { return [] }
        return entries.compactMap { entry in
            (entry as? [String: Any]).flatMap(parse(object:))
        }
    }

    /// ONE stored trigger: its when-part plus the runner. nil without a
    /// string id or device, or with an unreadable when-part.
    static func parse(object: [String: Any]) -> ActionTrigger? {
        guard let when = AutomationTrigger.parse(object: object),
              let id = optionalString(object["id"]),
              let deviceId = optionalString(object["deviceId"])
        else { return nil }
        return ActionTrigger(
            id: id,
            // Only an explicit `false` pauses: a missing flag is an enabled
            // trigger.
            enabled: !isExplicitFalse(object["enabled"]),
            deviceId: deviceId,
            agent: optionalString(object["agent"]),
            account: optionalString(object["account"]),
            model: optionalString(object["model"]),
            effort: optionalString(object["effort"]),
            when: when
        )
    }

    private static func optionalString(_ value: Any?) -> String? {
        guard let string = value as? String, !string.isEmpty else { return nil }
        return string
    }

    /// A JSON `false` and nothing else — JSONSerialization hands numbers and
    /// booleans back as NSNumber alike, and a `0` is not a pause.
    private static func isExplicitFalse(_ value: Any?) -> Bool {
        guard let number = value as? NSNumber,
              CFGetTypeID(number) == CFBooleanGetTypeID()
        else { return false }
        return !number.boolValue
    }
}

// MARK: - List glyphs

/// One trigger glyph on an action row: drawn at all when the action has a
/// trigger of that kind, `active` while at least one of them is enabled (a
/// paused kind is muted).
public struct TriggerBadge: Sendable, Equatable {
    public let active: Bool

    public init(active: Bool) {
        self.active = active
    }
}

/// What an action row's trigger glyphs draw (web `triggerBadges`):
/// `schedule` (clock) and `event` (bolt), each nil when the action has no
/// trigger of that kind.
public struct TriggerBadges: Sendable, Equatable {
    public let schedule: TriggerBadge?
    public let event: TriggerBadge?

    public init(schedule: TriggerBadge? = nil, event: TriggerBadge? = nil) {
        self.schedule = schedule
        self.event = event
    }

    public var isEmpty: Bool { schedule == nil && event == nil }

    public static func of(_ triggers: [ActionTrigger]) -> TriggerBadges {
        func badge(_ isKind: (AutomationTrigger) -> Bool) -> TriggerBadge? {
            let ofKind = triggers.filter { isKind($0.when) }
            guard !ofKind.isEmpty else { return nil }
            return TriggerBadge(active: ofKind.contains(where: \.enabled))
        }
        return TriggerBadges(
            schedule: badge { $0.isSchedule },
            event: badge { !$0.isSchedule }
        )
    }

    /// A suggestion seed's glyph: its one (not yet created) trigger, drawn
    /// active.
    public static func of(suggested trigger: AutomationTrigger?) -> TriggerBadges {
        guard let trigger else { return TriggerBadges() }
        return trigger.isSchedule
            ? TriggerBadges(schedule: TriggerBadge(active: true))
            : TriggerBadges(event: TriggerBadge(active: true))
    }
}

public extension AutomationTrigger {
    /// `kind: "schedule"` (else an event trigger).
    var isSchedule: Bool {
        if case .schedule = self { return true }
        return false
    }
}

// MARK: - Runs

public enum ActionRunTitle {
    /// A run's title in its OWN action's Runs list (web `actionRunTitle`,
    /// ×4): every row there ran the same action, so the row says what started
    /// it instead of repeating the action's name.
    public static func of(startedReason: String?) -> String {
        switch startedReason {
        case "schedule": return "Scheduled run"
        case "event": return "Event run"
        // A person started it.
        case nil, "": return "Manual run"
        // Another run started it (`agent`, `workflow`, or a reason added later).
        default: return "Agent run"
        }
    }
}

// MARK: - Wire encoding

/// One element of the `actions.update({ triggers })` array. The server
/// replaces the WHOLE array: to add, send the existing triggers plus one
/// WITHOUT `id` (the server mints it); to edit or toggle, send the array with
/// that element changed (its `id` kept); to delete, send the array without
/// it. Unset pins are omitted, an event trigger names its `source`.
public struct ActionTriggerInput: Sendable, Equatable {
    /// nil = a new trigger.
    public var id: String?
    public var enabled: Bool
    public var deviceId: String
    public var agent: String?
    public var account: String?
    public var model: String?
    public var effort: String?
    public var when: AutomationTrigger

    public init(
        id: String? = nil,
        enabled: Bool = true,
        deviceId: String,
        agent: String? = nil,
        account: String? = nil,
        model: String? = nil,
        effort: String? = nil,
        when: AutomationTrigger
    ) {
        self.id = id
        self.enabled = enabled
        self.deviceId = deviceId
        self.agent = agent
        self.account = account
        self.model = model
        self.effort = effort
        self.when = when
    }

    /// A stored trigger as it is sent back unchanged.
    public init(_ trigger: ActionTrigger) {
        self.init(
            id: trigger.id,
            enabled: trigger.enabled,
            deviceId: trigger.deviceId,
            agent: trigger.agent,
            account: trigger.account,
            model: trigger.model,
            effort: trigger.effort,
            when: trigger.when
        )
    }
}

extension ActionTriggerInput: Encodable {
    private enum RunnerKeys: String, CodingKey {
        case id, enabled, deviceId, agent, account, model, effort, source
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: RunnerKeys.self)
        try c.encodeIfPresent(id, forKey: .id)
        try c.encode(enabled, forKey: .enabled)
        try c.encode(deviceId, forKey: .deviceId)
        try c.encodeIfPresent(agent, forKey: .agent)
        try c.encodeIfPresent(account, forKey: .account)
        try c.encodeIfPresent(model, forKey: .model)
        try c.encodeIfPresent(effort, forKey: .effort)
        if !when.isSchedule {
            try c.encode("exponential", forKey: .source)
        }
        // The when-part's keys land in the SAME object, beside the runner.
        try when.encode(to: encoder)
    }
}

public extension Array where Element == ActionTrigger {
    /// The whole-array write that ADDS `input` (sent without an id).
    func adding(_ input: ActionTriggerInput) -> [ActionTriggerInput] {
        var added = input
        added.id = nil
        return map(ActionTriggerInput.init) + [added]
    }

    /// The whole-array write that swaps the element `id` for `input` (its id
    /// kept), every other trigger untouched and in place.
    func replacing(id: String, with input: ActionTriggerInput) -> [ActionTriggerInput] {
        map { trigger in
            guard trigger.id == id else { return ActionTriggerInput(trigger) }
            var replaced = input
            replaced.id = id
            return replaced
        }
    }

    /// The whole-array write that flips ONE trigger's enabled flag.
    func settingEnabled(id: String, _ enabled: Bool) -> [ActionTriggerInput] {
        map { trigger in
            var input = ActionTriggerInput(trigger)
            if trigger.id == id { input.enabled = enabled }
            return input
        }
    }

    /// The whole-array write that DELETES the element `id`.
    func removing(id: String) -> [ActionTriggerInput] {
        filter { $0.id != id }.map(ActionTriggerInput.init)
    }
}

// MARK: - The creator-run trigger block

/// What a suggestion with a trigger asks the creator agent to set up on the
/// action it creates — the iOS twin of the web's `TriggerSpec`.
public struct TriggerSpec: Sendable, Equatable {
    public let trigger: AutomationTrigger
    public let deviceId: String
    public let agent: String?
    public let model: String?
    public let effort: String?

    public init(
        trigger: AutomationTrigger,
        deviceId: String,
        agent: String? = nil,
        model: String? = nil,
        effort: String? = nil
    ) {
        self.trigger = trigger
        self.deviceId = deviceId
        self.agent = agent
        self.model = model
        self.effort = effort
    }
}

public enum TriggerNote {
    /// The machine-readable block a suggestion appends to the builtin
    /// "Create action" request: the creator agent creates the action, then
    /// passes this JSON verbatim as `triggers` to
    /// `exponential_actions_update`. BYTE-IDENTICAL to the web's
    /// `formatTriggerBlock` — the when-part's keys first, in their stored
    /// order, then `deviceId`, then agent, model, effort only when set;
    /// compact JSON, no spaces.
    public static func format(_ spec: TriggerSpec) -> String {
        var parts = spec.trigger.wireJSONParts
        parts.append("\"deviceId\":\(AutomationTrigger.jsonQuoted(spec.deviceId))")
        if let agent = spec.agent, !agent.isEmpty {
            parts.append("\"agent\":\(AutomationTrigger.jsonQuoted(agent))")
        }
        if let model = spec.model, !model.isEmpty {
            parts.append("\"model\":\(AutomationTrigger.jsonQuoted(model))")
        }
        if let effort = spec.effort, !effort.isEmpty {
            parts.append("\"effort\":\(AutomationTrigger.jsonQuoted(effort))")
        }
        let payload = "{" + parts.joined(separator: ",") + "}"
        return "\n\nTrigger — after creating the action, call exponential_actions_update with its id and `triggers` set to exactly this array: `[\(payload)]`. A triggered run fills no inputs, so declare none as required."
    }
}
