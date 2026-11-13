import Foundation

// #EXP-1815 (notification-delivery.json, its `$comment` is the spec): where a
// notification shows, when it clears, who mutes it ×4. A 1:1 port of the TS
// reference `packages/domain-contract/src/notification-delivery.ts`, replayed
// by `NotificationDeliveryTests` (every `*Cases` array). In ExpPush so the
// Notification Service Extension links it.
public enum NotificationDelivery {
    public enum DataKeys {
        public static let thread = "thread"
        public static let source = "source"
        public static let sourceName = "sourceName"
        public static let dismissType = "dismiss"
        public static let ids = "ids"
        public static let threads = "threads"
        public static let all = "all"
        public static let teamId = "teamId"
    }

    public enum Limits {
        public static let maxIds = 60
        public static let maxThreads = 30
    }

    public enum Copy {
        public static let toastAction = "Open"
        public static let sessionsBand = "Sessions"
        public static let assistant = "Personal Assistant"
        public static let temporary = "Temporary sessions"
        public static let agentsBand = "Team agents"
    }

    public enum Channels {
        public static let assistant = AndroidChannel(id: "source-assistant", name: "Personal Assistant", group: nil)
        public static let temporary = AndroidChannel(id: "source-temporary", name: "Temporary sessions", group: nil)
        public static let agentPrefix = "source-agent-"
        public static let agentsGroupId = "team-agents"
        public static let agentsGroupName = "Team agents"
        public static let fallback = "issues_default"
    }

    // MARK: - Thread

    public struct ThreadRow: Sendable {
        public let type: String
        public let issueId: String?
        /// The root of the row's session (a client without it synced: the session id).
        public let sessionRootId: String?
        public let deviceId: String?

        public init(type: String, issueId: String?, sessionRootId: String?, deviceId: String?) {
            self.type = type
            self.issueId = issueId
            self.sessionRootId = sessionRootId
            self.deviceId = deviceId
        }
    }

    /// The thread every screen of the Inbox covers.
    public static let inboxThread = "inbox"

    /// The key a notification stacks and clears under.
    public static func notificationThread(_ row: ThreadRow) -> String {
        if let issueId = row.issueId, !issueId.isEmpty { return "issue:\(issueId)" }
        if let rootId = row.sessionRootId, !rootId.isEmpty { return "session:\(rootId)" }
        if row.type == "agent_signed_out", let deviceId = row.deviceId, !deviceId.isEmpty { return "device:\(deviceId)" }
        return inboxThread
    }

    // MARK: - Presentation

    public enum Platform: String, Sendable {
        case web, desktop, ios, android
    }

    public enum Presentation: String, Sendable {
        case none, toast, system
    }

    public static func presentation(
        platform: Platform, active: Bool, viewing: String?, thread: String, muted: Bool
    ) -> Presentation {
        if muted { return .none }
        if !active { return platform == .web ? .toast : .system }
        if viewing == inboxThread || viewing == thread { return .none }
        return .toast
    }

    // MARK: - Clearing

    public struct ReadRow: Sendable {
        public let id: String
        public let thread: String

        public init(id: String, thread: String) {
            self.id = id
            self.thread = thread
        }
    }

    private static func distinct(_ values: [String]) -> [String] {
        var seen = Set<String>()
        return values.filter { seen.insert($0).inserted }
    }

    /// The silent push for the rows one write turned read; nil = no push.
    public static func dismissPayload(rows: [ReadRow], all: Bool, teamId: String?) -> [String: String]? {
        if rows.isEmpty { return nil }
        let base = ["type": DataKeys.dismissType]
        func everything() -> [String: String] {
            var payload = base
            payload[DataKeys.all] = "1"
            if let teamId, !teamId.isEmpty { payload[DataKeys.teamId] = teamId }
            return payload
        }
        if all { return everything() }
        let ids = distinct(rows.map(\.id))
        if ids.count <= Limits.maxIds { return base.merging([DataKeys.ids: ids.joined(separator: ",")]) { $1 } }
        let threads = distinct(rows.map(\.thread))
        if threads.count <= Limits.maxThreads {
            return base.merging([DataKeys.threads: threads.joined(separator: ",")]) { $1 }
        }
        return everything()
    }

    public struct Delivered: Sendable {
        public let notificationId: String?
        public let thread: String?
        public let teamId: String?

        public init(notificationId: String?, thread: String?, teamId: String?) {
            self.notificationId = notificationId
            self.thread = thread
            self.teamId = teamId
        }

        /// A delivered notification's keys, read off its `userInfo` (the
        /// extension keeps `notificationId`, `thread` and `teamId` there).
        public init(userInfo: [AnyHashable: Any]) {
            func string(_ key: String) -> String? {
                (userInfo[key] as? String).flatMap { $0.isEmpty ? nil : $0 }
            }
            self.init(notificationId: string("notificationId"), thread: string(DataKeys.thread), teamId: string(DataKeys.teamId))
        }
    }

    private static func splitList(_ value: String?) -> [String] {
        (value ?? "").split(separator: ",").map(String.init).filter { !$0.isEmpty }
    }

    /// Does a dismiss push remove this delivered notification?
    public static func dismissMatches(_ payload: [String: String], _ delivered: Delivered) -> Bool {
        guard payload["type"] == DataKeys.dismissType else { return false }
        if payload[DataKeys.all] == "1" {
            guard let teamId = payload[DataKeys.teamId], !teamId.isEmpty else { return true }
            return delivered.teamId == teamId
        }
        if let id = delivered.notificationId, splitList(payload[DataKeys.ids]).contains(id) { return true }
        guard let thread = delivered.thread else { return false }
        return splitList(payload[DataKeys.threads]).contains(thread)
    }

    /// A push's string data (an opened push's `userInfo`).
    public static func data(_ userInfo: [AnyHashable: Any]) -> [String: String] {
        var data: [String: String] = [:]
        for (key, value) in userInfo {
            if let key = key as? String, let value = value as? String { data[key] = value }
        }
        return data
    }

    // MARK: - Source switches

    public struct SourceSession: Sendable {
        public let agentId: String?
        public let agentRole: String?
        public let startedByAgentId: String?
        public let startedByAgentRole: String?

        public init(agentId: String?, agentRole: String?, startedByAgentId: String?, startedByAgentRole: String?) {
            self.agentId = agentId
            self.agentRole = agentRole
            self.startedByAgentId = startedByAgentId
            self.startedByAgentRole = startedByAgentRole
        }
    }

    /// `assistant` | `temporary` | `agent:<agentId>`, read off the session's ROOT.
    public static func notificationSource(_ root: SourceSession) -> String {
        let agentId = root.agentId ?? root.startedByAgentId
        let role = root.agentId != nil ? root.agentRole : root.startedByAgentRole
        guard let agentId else { return "temporary" }
        return role == "assistant" ? "assistant" : "agent:\(agentId)"
    }

    /// `{assistant?, temporary?, agents?: {<agentId>: bool}}`, absent = on.
    public struct SourcePrefs: Codable, Equatable, Sendable {
        public var assistant: Bool?
        public var temporary: Bool?
        public var agents: [String: Bool]?

        public init(assistant: Bool? = nil, temporary: Bool? = nil, agents: [String: Bool]? = nil) {
            self.assistant = assistant
            self.temporary = temporary
            self.agents = agents
        }
    }

    private static let agentPrefix = "agent:"

    public static func sourceAllowed(_ prefs: SourcePrefs?, _ source: String?) -> Bool {
        guard let source, let prefs else { return true }
        if source == "assistant" { return prefs.assistant != false }
        if source == "temporary" { return prefs.temporary != false }
        if source.hasPrefix(agentPrefix) { return prefs.agents?[String(source.dropFirst(agentPrefix.count))] != false }
        return true
    }

    /// The prefs after one switch flips (`key` = a settings row's key).
    public static func toggleSourcePref(_ prefs: SourcePrefs, key: String, on: Bool) -> SourcePrefs {
        var next = prefs
        if key == "assistant" {
            next.assistant = on
        } else if key == "temporary" {
            next.temporary = on
        } else if key.hasPrefix(agentPrefix) {
            var agents = prefs.agents ?? [:]
            agents[String(key.dropFirst(agentPrefix.count))] = on
            next.agents = agents
        }
        return next
    }

    // MARK: - Settings

    public struct SettingsTeam: Sendable {
        public let id: String
        public let name: String

        public init(id: String, name: String) {
            self.id = id
            self.name = name
        }
    }

    public struct SettingsAgent: Sendable {
        public let id: String
        public let teamId: String
        public let name: String
        public let role: String

        public init(id: String, teamId: String, name: String, role: String) {
            self.id = id
            self.teamId = teamId
            self.name = name
            self.role = role
        }
    }

    public struct SettingsRow: Equatable, Sendable, Identifiable {
        public let key: String
        public let label: String
        public let detail: String?
        public var id: String { key }

        public init(key: String, label: String, detail: String?) {
            self.key = key
            self.label = label
            self.detail = detail
        }
    }

    public struct SettingsGroup: Equatable, Sendable, Identifiable {
        public let band: String
        public let rows: [SettingsRow]
        public var id: String { band }

        public init(band: String, rows: [SettingsRow]) {
            self.band = band
            self.rows = rows
        }
    }

    public static func settingsRows(teams: [SettingsTeam], agents: [SettingsAgent]) -> [SettingsGroup] {
        var groups = [
            SettingsGroup(band: Copy.sessionsBand, rows: [
                SettingsRow(key: "assistant", label: Copy.assistant, detail: nil),
                SettingsRow(key: "temporary", label: Copy.temporary, detail: nil),
            ]),
        ]
        let teamNames = Dictionary(teams.map { ($0.id, $0.name) }, uniquingKeysWith: { first, _ in first })
        // Code-unit order, like the reference's `<` (never locale-aware).
        let listed = agents
            .filter { $0.role != "assistant" && teamNames[$0.teamId] != nil }
            .map { (agent: $0, team: teamNames[$0.teamId] ?? "") }
            .sorted { a, b in
                if a.team != b.team { return Array(a.team.utf16).lexicographicallyPrecedes(b.team.utf16) }
                if a.agent.name != b.agent.name {
                    return Array(a.agent.name.utf16).lexicographicallyPrecedes(b.agent.name.utf16)
                }
                return Array(a.agent.id.utf16).lexicographicallyPrecedes(b.agent.id.utf16)
            }
        if !listed.isEmpty {
            let many = teams.count > 1
            groups.append(SettingsGroup(band: Copy.agentsBand, rows: listed.map {
                SettingsRow(key: "agent:\($0.agent.id)", label: $0.agent.name, detail: many ? $0.team : nil)
            }))
        }
        return groups
    }

    // MARK: - Android channels

    public struct AndroidChannel: Equatable, Sendable {
        public let id: String
        public let name: String?
        public let group: String?

        public init(id: String, name: String?, group: String?) {
            self.id = id
            self.name = name
            self.group = group
        }
    }

    public static func androidChannel(source: String?, sourceName: String?) -> AndroidChannel {
        if source == "assistant" { return Channels.assistant }
        if source == "temporary" { return Channels.temporary }
        if let source, source.hasPrefix(agentPrefix) {
            let name = (sourceName?.isEmpty == false) ? sourceName : Channels.agentsGroupName
            return AndroidChannel(
                id: "\(Channels.agentPrefix)\(source.dropFirst(agentPrefix.count))",
                name: name,
                group: Channels.agentsGroupId
            )
        }
        return AndroidChannel(id: Channels.fallback, name: nil, group: nil)
    }
}
