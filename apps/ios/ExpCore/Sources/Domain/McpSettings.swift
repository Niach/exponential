import Foundation

/// #VAPP-185: Settings › MCP, ONE page ×4 (`fixtures/settings-mcp.json`, its
/// `$comment` is the spec): Overview · Servers (the built-ins first, every
/// row expandable to its tools) · Access (+ Activity) · Clients (Mine |
/// Everyone). A 1:1 port of the TS reference `settings-mcp.ts` (web
/// `mcp-settings-page.tsx`, desktop `settings/mcp.rs`, Android
/// `McpSettings.kt`), locked by `McpSettingsTests` replaying the fixture's
/// cases.
public enum McpSettings {
    public static let title = "MCP"

    public enum Section: String, CaseIterable, Sendable {
        case overview, servers, access, clients

        /// The label over the section's group; the Overview line has none.
        public var label: String? {
            switch self {
            case .overview: nil
            case .servers: "Servers"
            case .access: "Access"
            case .clients: "Clients"
            }
        }

        public var testId: String { "settings-mcp-\(rawValue)" }
    }

    /// The muted line at the top of the page (`overview`).
    public static let overview =
        "MCP servers your team's sessions, agents and apps can call. Built-ins are always on; connect your own account to the others."

    public static let copy = "Copy"
    public static let copied = "Copied"
    public static let revoke = "Revoke"
    public static let activity = "Activity"

    public enum Scope: String, CaseIterable, Sendable {
        case mine, everyone

        public var label: String {
            switch self {
            case .mine: "Mine"
            case .everyone: "Everyone"
            }
        }
    }

    public enum Level: String, CaseIterable, Sendable, Codable {
        case none, read, write

        public var label: String {
            switch self {
            case .none: "None"
            case .read: "Read only"
            case .write: "Full"
            }
        }
    }

    public static let kindLabels: [String: String] = ["builtin": "Exponential tools", "apps": "Apps"]
    public static let accessLabels: [String: String] = ["read": "Read only", "full": "Full"]
    public static let statusLabels: [String: String] = [
        "ok": "OK", "denied": "Denied", "invalid": "Invalid", "failed": "Failed", "upstream": "Upstream",
    ]
    public static let never = "never used"
    public static let builtinApp = "Exponential"
    public static let firstParty = "API key"

    public enum TestId {
        public static let page = "settings-mcp-page"
        public static let builtin = "settings-mcp-builtin"
        public static let appsServer = "settings-mcp-apps-server"
        public static let serverTools = "settings-mcp-server-tools"
        public static let activity = "settings-mcp-activity"
    }

    // MARK: - Servers (owner 2026-11-12: ONE group, every row expandable)

    /// The built-in servers (`builtins`): Exponential always, Team apps when
    /// an app exposes tools.
    public enum Builtin: String, CaseIterable, Sendable {
        case exponential, apps

        public var name: String {
            switch self {
            case .exponential: "Exponential"
            case .apps: "Team apps"
            }
        }

        public var path: String {
            switch self {
            case .exponential: "/api/mcp"
            case .apps: "/api/mcp/apps"
            }
        }

        /// Its key in `serverOrder` (`builtin:<id>`).
        public var key: String { "builtin:\(rawValue)" }
    }

    public static let builtinPill = "Built-in"
    public static let toolsConnect = "Connect to see its tools."
    public static let toolsStdio = "A local command: its tools list where a session runs it."
    public static let toolsNone = "No tools."
    public static let retry = "Retry"

    /// "Could not list its tools: <reason>" (TS `toolsFailedLine`).
    public static func toolsFailedLine(_ reason: String) -> String {
        "Could not list its tools: \(reason)"
    }

    /// A built-in's URL: the instance origin + its path (TS `builtinEndpoint`).
    public static func builtinEndpoint(instanceUrl: String, _ builtin: Builtin) -> String {
        var base = Substring(instanceUrl)
        while base.hasSuffix("/") { base = base.dropLast() }
        return String(base) + builtin.path
    }

    /// One row of the Servers group: a built-in or a team server.
    public enum ServerEntry<Server> {
        case builtin(Builtin)
        case server(Server)
    }

    /// The Servers group's order (TS `mcpServerOrder`, `serverOrder.cases`):
    /// built-ins first (Exponential, then Team apps when an app exposes
    /// tools), then the team's servers in list order.
    public static func serverOrder<Server>(appTools: Bool, servers: [Server]) -> [ServerEntry<Server>] {
        [.builtin(.exponential)] + (appTools ? [.builtin(.apps)] : []) + servers.map { .server($0) }
    }

    /// What an open team server row shows under it (TS `mcpToolsState`,
    /// `toolsStateCases`).
    public enum ToolsState: String, Sendable {
        /// tRPC `mcpServers.tools` on first open.
        case list
        /// The muted `toolsConnect` line (sign in first).
        case connect
        /// The muted `toolsStdio` line (a local command).
        case stdio
    }

    public static func toolsState(transport: String, status: String) -> ToolsState {
        guard transport == "http" else { return .stdio }
        return status == "connected" || status == "not_needed" ? .list : .connect
    }

    /// A server row's "Used by …" caption (nil = no app uses it).
    public static func usedByCaption(_ apps: [String]) -> String? {
        guard !apps.isEmpty else { return nil }
        let sorted = apps.sorted { $0.compare($1, options: .caseInsensitive, locale: Locale(identifier: "en")) == .orderedAscending }
        return "Used by " + sorted.joined(separator: ", ")
    }

    /// #VAPP-239: a device-routed server row's "Runs on …" part
    /// (`runsOnCases`; nil = the server broker calls it).
    public static func runsOnCaption(_ device: String?) -> String? {
        guard let device, !device.isEmpty else { return nil }
        return "Runs on " + device
    }

    /// A client row's caption: [member ·] kinds (apps access) · last used.
    /// `lastUsed` = the compact relative time, nil = never.
    public static func clientCaption(
        scope: Scope, userName: String, kinds: [String], appsAccess: String?, lastUsed: String?
    ) -> String {
        let kindText = ["builtin", "apps"]
            .filter { kinds.contains($0) }
            .map { kind -> String in
                let label = kindLabels[kind] ?? kind
                if kind == "apps", let appsAccess, !appsAccess.isEmpty {
                    return "\(label) (\(accessLabels[appsAccess] ?? appsAccess))"
                }
                return label
            }
            .joined(separator: ", ")
        return [scope == .everyone ? userName : nil, kindText, lastUsed ?? never]
            .compactMap { $0 }
            .filter { !$0.isEmpty }
            .joined(separator: " · ")
    }

    /// An activity row's caption: app · [member ·] client · when.
    public static func activityCaption(
        owner: Bool, appName: String?, userName: String, clientName: String?, when: String
    ) -> String {
        [appName ?? builtinApp, owner ? userName : nil, clientName ?? firstParty, when]
            .compactMap { $0 }
            .filter { !$0.isEmpty }
            .joined(separator: " · ")
    }

    /// "1 tool" | "3 tools".
    public static func toolCountLabel(_ n: Int) -> String {
        n == 1 ? "\(n) tool" : "\(n) tools"
    }

    /// The status Pill's word (an unknown status reads as itself).
    public static func statusLabel(_ status: String) -> String {
        statusLabels[status] ?? status
    }

    /// The built-in Exponential tools group's rows (web `builtinExpTools`):
    /// the contract's `expToolDisplay` rows, in contract order.
    public struct BuiltinTool: Equatable, Sendable, Identifiable {
        public let name: String
        public let title: String
        public let blurb: String
        public var id: String { name }
    }

    /// #EXP-1657: one band of the BUILT-IN section = one contract tool group.
    public struct BuiltinToolBand: Equatable, Sendable, Identifiable {
        public let id: String
        public let label: String
        public let tools: [BuiltinTool]
    }

    private static func builtinTool(_ i: Int) -> BuiltinTool {
        BuiltinTool(
            name: DomainContract.expToolPrefix + DomainContract.expToolNames[i],
            title: DomainContract.expToolTitles[i],
            blurb: DomainContract.expToolBlurbs[i]
        )
    }

    /// Every contract row but the retired `aliasOf` ones (their
    /// `expToolRowGroups` entry is ""), in contract order.
    public static let builtinTools: [BuiltinTool] = DomainContract.expToolNames.indices
        .filter { !DomainContract.expToolRowGroups[$0].isEmpty }
        .map(builtinTool)

    /// The bands the BUILT-IN section shows for a query (TS
    /// `builtinToolBands`): contract group order and labels, rows filtered by
    /// `filterBuiltinTools`, a band with no match left out (all gone = the
    /// empty state).
    public static func builtinToolBands(query: String) -> [BuiltinToolBand] {
        DomainContract.expToolGroupIds.indices.compactMap { g in
            let id = DomainContract.expToolGroupIds[g]
            let rows = DomainContract.expToolNames.indices
                .filter { DomainContract.expToolRowGroups[$0] == id }
                .map(builtinTool)
            let shown = filterBuiltinTools(rows, query: query)
            guard !shown.isEmpty else { return nil }
            return BuiltinToolBand(id: id, label: DomainContract.expToolGroupLabels[g], tools: shown)
        }
    }

    public static let builtinFilterPlaceholder = "Filter tools"

    /// The group's filter (web `filterBuiltinTools`): a case-insensitive
    /// substring over the title, the blurb and the wire name.
    public static func filterBuiltinTools(_ tools: [BuiltinTool], query: String) -> [BuiltinTool] {
        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !needle.isEmpty else { return tools }
        return tools.filter {
            $0.title.lowercased().contains(needle)
                || $0.blurb.lowercased().contains(needle)
                || $0.name.lowercased().contains(needle)
        }
    }
}

extension McpSettings.ServerEntry: Identifiable where Server: Identifiable, Server.ID == String {
    /// `builtin:<id>` or the team server's own id.
    public var id: String {
        switch self {
        case .builtin(let builtin): builtin.key
        case .server(let server): server.id
        }
    }
}
