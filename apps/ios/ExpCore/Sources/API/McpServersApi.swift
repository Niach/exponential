import Foundation

/// EXP-792/1249: the team's MCP servers as a launch picker reads them —
/// `mcpServers.list({teamId})`, each row with the CALLER's own connection
/// (the server holds every member's credential, so readiness is the same on
/// every machine). #VAPP-185: the phone's Settings › MCP lists them with the
/// member's own Connect (on the web, in the browser) / Disconnect; adding and
/// editing stay on web / the IDE.
public struct McpServerRow: Decodable, Identifiable, Equatable, Sendable {
    public struct Connection: Decodable, Equatable, Sendable {
        /// `connected` / `not_needed` / `expired` / `error` / `not_connected`…
        public let status: String

        public init(status: String) {
            self.status = status
        }
    }

    public let id: String
    public let name: String
    public let url: String?
    public let command: String?
    public let enabledByDefault: Bool
    public let connection: Connection
    /// `oauth` / `secret` / `none`.
    public let auth: String
    /// #VAPP-185: the apps whose head package declares this server in
    /// `upstreams` (Settings › MCP's "Used by …").
    public let usedBy: [String]

    public init(
        id: String,
        name: String,
        url: String? = nil,
        command: String? = nil,
        enabledByDefault: Bool = false,
        connection: Connection,
        auth: String = "oauth",
        usedBy: [String] = []
    ) {
        self.id = id
        self.name = name
        self.url = url
        self.command = command
        self.enabledByDefault = enabledByDefault
        self.connection = connection
        self.auth = auth
        self.usedBy = usedBy
    }

    enum CodingKeys: String, CodingKey {
        case id, name, url, command, enabledByDefault, connection, auth, usedBy
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        name = (try? c.decode(String.self, forKey: .name)) ?? ""
        url = try? c.decodeIfPresent(String.self, forKey: .url)
        command = try? c.decodeIfPresent(String.self, forKey: .command)
        enabledByDefault = (try? c.decodeIfPresent(Bool.self, forKey: .enabledByDefault)) ?? false
        connection = (try? c.decode(Connection.self, forKey: .connection))
            ?? Connection(status: "not_connected")
        auth = (try? c.decodeIfPresent(String.self, forKey: .auth)) ?? "oauth"
        usedBy = (try? c.decodeIfPresent([String].self, forKey: .usedBy)) ?? []
    }
}

public enum McpServers {
    /// What a row the caller cannot use yet says under its name (web
    /// `mcpNotReadyLabel`); nil = ready.
    public static func notReadyLabel(_ server: McpServerRow) -> String? {
        switch server.connection.status {
        case "connected", "not_needed": nil
        case "expired", "error": "Reconnect first"
        default: "Connect first"
        }
    }

    public static func isReady(_ server: McpServerRow) -> Bool { notReadyLabel(server) == nil }

    /// The pick a composer opens with (web `preselectMcpServerIds`): the
    /// caller's saved pick narrowed to ready servers, else every ready server
    /// enabled by default.
    public static func preselect(_ servers: [McpServerRow], saved: [String]?) -> [String] {
        let ready = servers.filter(isReady)
        if let saved {
            let known = Set(ready.map(\.id))
            return saved.filter { known.contains($0) }
        }
        return ready.filter(\.enabledByDefault).map(\.id)
    }

    /// The row's second line: the URL's host, else the command.
    public static func location(_ server: McpServerRow) -> String? {
        if let url = server.url, let host = URL(string: url)?.host, !host.isEmpty { return host }
        let command = server.command?.trimmingCharacters(in: .whitespacesAndNewlines)
        return (command?.isEmpty ?? true) ? nil : command
    }

    /// The submenu row's value: the number picked, none at zero.
    public static func pickedValue(_ picked: [String]) -> String? {
        picked.isEmpty ? nil : "\(picked.count)"
    }
}

private struct McpServersListInput: Encodable {
    let teamId: String
}

private struct McpServerIdInput: Encodable {
    let serverId: String
}

// MARK: - #VAPP-185: Settings › MCP (clients, access, activity)

/// One OAuth client a member authorized (`mcpClients.list`).
public struct McpClientRow: Decodable, Identifiable, Equatable, Sendable {
    public let clientId: String
    public let name: String
    public let icon: String?
    public let userId: String
    public let userName: String
    public let userEmail: String?
    /// `builtin` / `apps`, sorted.
    public let kinds: [String]
    /// The apps grant's access (`read` / `full`), nil without one.
    public let appsAccess: String?
    public let connectedAt: String?
    public let lastUsedAt: String?

    public var id: String { "\(clientId):\(userId)" }

    public init(
        clientId: String, name: String, icon: String? = nil, userId: String, userName: String,
        userEmail: String? = nil, kinds: [String], appsAccess: String? = nil,
        connectedAt: String? = nil, lastUsedAt: String? = nil
    ) {
        self.clientId = clientId
        self.name = name
        self.icon = icon
        self.userId = userId
        self.userName = userName
        self.userEmail = userEmail
        self.kinds = kinds
        self.appsAccess = appsAccess
        self.connectedAt = connectedAt
        self.lastUsedAt = lastUsedAt
    }
}

public struct McpClientsList: Decodable, Equatable, Sendable {
    public let canSeeEveryone: Bool
    public let clients: [McpClientRow]

    public init(canSeeEveryone: Bool, clients: [McpClientRow]) {
        self.canSeeEveryone = canSeeEveryone
        self.clients = clients
    }
}

/// `appAccess.list`: the apps exposing tools + one row per member (owners:
/// every member; a member: their own) with a level per app.
public struct McpAppAccess: Decodable, Equatable, Sendable {
    public struct App: Decodable, Identifiable, Equatable, Sendable {
        public let id: String
        public let slug: String
        public let name: String
        public let icon: String?
        public let color: String?
        public let toolCount: Int

        public init(id: String, slug: String, name: String, icon: String? = nil, color: String? = nil, toolCount: Int) {
            self.id = id
            self.slug = slug
            self.name = name
            self.icon = icon
            self.color = color
            self.toolCount = toolCount
        }
    }

    public struct Row: Decodable, Identifiable, Equatable, Sendable {
        public let userId: String
        public let name: String
        public let email: String?
        public let image: String?
        public let role: String?
        public var levels: [String: McpSettings.Level]
        public var id: String { userId }

        public init(
            userId: String, name: String, email: String? = nil, image: String? = nil, role: String? = nil,
            levels: [String: McpSettings.Level]
        ) {
            self.userId = userId
            self.name = name
            self.email = email
            self.image = image
            self.role = role
            self.levels = levels
        }

        /// The member's level for an app (no row = Full, the default).
        public func level(_ appId: String) -> McpSettings.Level { levels[appId] ?? .write }
    }

    public let canEdit: Bool
    public let apps: [App]
    public var rows: [Row]

    public init(canEdit: Bool, apps: [App], rows: [Row]) {
        self.canEdit = canEdit
        self.apps = apps
        self.rows = rows
    }
}

/// One `mcp_calls` row (`mcpActivity.list`).
public struct McpActivityCall: Decodable, Identifiable, Equatable, Sendable {
    public let id: String
    public let tool: String
    public let status: String
    public let error: String?
    public let appName: String?
    public let userName: String
    public let clientName: String?
    public let createdAt: String

    public init(
        id: String, tool: String, status: String, error: String? = nil, appName: String? = nil,
        userName: String, clientName: String? = nil, createdAt: String
    ) {
        self.id = id
        self.tool = tool
        self.status = status
        self.error = error
        self.appName = appName
        self.userName = userName
        self.clientName = clientName
        self.createdAt = createdAt
    }
}

public struct McpActivityList: Decodable, Equatable, Sendable {
    public let owner: Bool
    public let calls: [McpActivityCall]

    public init(owner: Bool, calls: [McpActivityCall]) {
        self.owner = owner
        self.calls = calls
    }
}

private struct McpClientsListInput: Encodable {
    let teamId: String
    let scope: String
}

struct McpClientRevokeInput: Encodable {
    let clientId: String
    let teamId: String
    /// Only in Everyone (an owner revoking a member's client).
    let userId: String?

    enum CodingKeys: String, CodingKey { case clientId, teamId, userId }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(clientId, forKey: .clientId)
        try c.encode(teamId, forKey: .teamId)
        try c.encodeIfPresent(userId, forKey: .userId)
    }
}

private struct AppAccessSetInput: Encodable {
    let appId: String
    let userId: String
    let level: String
}

public final class McpServersApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func list(accountId: String, teamId: String) async throws -> [McpServerRow] {
        try await trpc.query(
            accountId: accountId,
            path: "mcpServers.list",
            input: McpServersListInput(teamId: teamId)
        )
    }

    /// Drops the caller's OWN connection to a server.
    public func disconnect(accountId: String, serverId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "mcpServers.disconnect",
            input: McpServerIdInput(serverId: serverId)
        )
    }

    /// #VAPP-185: the MCP clients (`everyone` = owners only).
    public func clients(accountId: String, teamId: String, scope: McpSettings.Scope) async throws -> McpClientsList {
        try await trpc.query(
            accountId: accountId,
            path: "mcpClients.list",
            input: McpClientsListInput(teamId: teamId, scope: scope.rawValue)
        )
    }

    /// Revokes a client: the caller's own (`userId` nil) or, an owner in
    /// Everyone, a member's.
    public func revokeClient(accountId: String, teamId: String, clientId: String, userId: String?) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "mcpClients.revoke",
            input: McpClientRevokeInput(clientId: clientId, teamId: teamId, userId: userId)
        )
    }

    public func appAccess(accountId: String, teamId: String) async throws -> McpAppAccess {
        try await trpc.query(
            accountId: accountId,
            path: "appAccess.list",
            input: McpServersListInput(teamId: teamId)
        )
    }

    public func setAppAccess(accountId: String, appId: String, userId: String, level: McpSettings.Level) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "appAccess.set",
            input: AppAccessSetInput(appId: appId, userId: userId, level: level.rawValue)
        )
    }

    public func activity(accountId: String, teamId: String) async throws -> McpActivityList {
        try await trpc.query(
            accountId: accountId,
            path: "mcpActivity.list",
            input: McpServersListInput(teamId: teamId)
        )
    }
}
