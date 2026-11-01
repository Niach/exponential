import Foundation

/// #VAPP-185: Settings › MCP, ONE page ×4 (`fixtures/settings-mcp.json`, its
/// `$comment` is the spec): Endpoint · Servers · Clients (Mine | Everyone) ·
/// Access (+ Activity) · Built-in Exponential tools. A 1:1 port of the TS
/// reference `settings-mcp.ts` (web `mcp-settings-page.tsx`, desktop
/// `settings/mcp.rs`, Android `McpSettings.kt`), locked by
/// `McpSettingsTests` replaying the fixture's cases.
public enum McpSettings {
    public static let title = "MCP"

    public enum Section: String, CaseIterable, Sendable {
        case endpoint, servers, clients, access, builtin

        public var label: String {
            switch self {
            case .endpoint: "Endpoint"
            case .servers: "Servers"
            case .clients: "Clients"
            case .access: "Access"
            case .builtin: "Built-in Exponential tools"
            }
        }

        public var testId: String { "settings-mcp-\(rawValue)" }
    }

    public static let copy = "Copy"
    public static let copied = "Copied"
    public static let endpointPath = "/api/mcp/apps"
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
        public static let activity = "settings-mcp-activity"
    }

    /// The global apps endpoint on an instance (`<instance>/api/mcp/apps`).
    public static func endpoint(instanceUrl: String) -> String {
        let base = instanceUrl.hasSuffix("/") ? String(instanceUrl.dropLast()) : instanceUrl
        return base + endpointPath
    }

    /// A server row's "Used by …" caption (nil = no app uses it).
    public static func usedByCaption(_ apps: [String]) -> String? {
        guard !apps.isEmpty else { return nil }
        let sorted = apps.sorted { $0.compare($1, options: .caseInsensitive, locale: Locale(identifier: "en")) == .orderedAscending }
        return "Used by " + sorted.joined(separator: ", ")
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

    public static let builtinTools: [BuiltinTool] = DomainContract.expToolNames.indices.map { i in
        BuiltinTool(
            name: DomainContract.expToolPrefix + DomainContract.expToolNames[i],
            title: DomainContract.expToolTitles[i],
            blurb: DomainContract.expToolBlurbs[i]
        )
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
