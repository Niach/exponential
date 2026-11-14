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
    /// `http` / `stdio` (Settings › MCP's `toolsState`).
    public let transport: String
    public let enabledByDefault: Bool
    public let connection: Connection
    /// `oauth` / `secret` / `none`.
    public let auth: String
    /// #VAPP-185: the apps whose head package declares this server in
    /// `upstreams` (Settings › MCP's "Used by …").
    public let usedBy: [String]
    /// #VAPP-239: the label of the device the server runs on for app calls
    /// (nil = the server broker calls it): the row's "Runs on …".
    public let deviceLabel: String?

    public init(
        id: String,
        name: String,
        url: String? = nil,
        command: String? = nil,
        transport: String? = nil,
        enabledByDefault: Bool = false,
        connection: Connection,
        auth: String = "oauth",
        usedBy: [String] = [],
        deviceLabel: String? = nil
    ) {
        self.id = id
        self.name = name
        self.url = url
        self.command = command
        self.transport = transport ?? (url != nil ? "http" : "stdio")
        self.enabledByDefault = enabledByDefault
        self.connection = connection
        self.auth = auth
        self.usedBy = usedBy
        self.deviceLabel = deviceLabel
    }

    enum CodingKeys: String, CodingKey {
        case id, name, url, command, transport, enabledByDefault, connection, auth, usedBy, device
    }

    private struct Device: Decodable {
        let label: String?
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        name = (try? c.decode(String.self, forKey: .name)) ?? ""
        url = try? c.decodeIfPresent(String.self, forKey: .url)
        command = try? c.decodeIfPresent(String.self, forKey: .command)
        transport = (try? c.decodeIfPresent(String.self, forKey: .transport)) ?? (url != nil ? "http" : "stdio")
        enabledByDefault = (try? c.decodeIfPresent(Bool.self, forKey: .enabledByDefault)) ?? false
        connection = (try? c.decode(Connection.self, forKey: .connection))
            ?? Connection(status: "not_connected")
        auth = (try? c.decodeIfPresent(String.self, forKey: .auth)) ?? "oauth"
        usedBy = (try? c.decodeIfPresent([String].self, forKey: .usedBy)) ?? []
        deviceLabel = (try? c.decodeIfPresent(Device.self, forKey: .device))?.label
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
        if let host = hostOf(server.url) { return host }
        let command = server.command?.trimmingCharacters(in: .whitespacesAndNewlines)
        return (command?.isEmpty ?? true) ? nil : command
    }

    // #EXP-1795: the picker row's glyph, web `getMcpServerIcon`, Android
    // `mcpServerGlyph` (McpServerPick.kt).

    /// A URL's host the way web's `new URL(url).host` reads it: lowercased,
    /// the port kept unless it is the scheme's default; nil when unparseable.
    public static func hostOf(_ url: String?) -> String? {
        guard let url, let parts = URLComponents(string: url.trimmingCharacters(in: .whitespacesAndNewlines)),
              let host = parts.host?.lowercased(), !host.isEmpty else { return nil }
        let defaultPort: Int? = switch parts.scheme?.lowercased() {
        case "http", "ws": 80
        case "https", "wss": 443
        default: nil
        }
        guard let port = parts.port, port != defaultPort else { return host }
        return "\(host):\(port)"
    }

    private static func pathOf(_ url: String) -> String? {
        guard let parts = URLComponents(string: url.trimmingCharacters(in: .whitespacesAndNewlines)) else { return nil }
        return parts.path.isEmpty ? "/" : parts.path
    }

    /// The contract `mcpCatalog` mark a server's URL belongs to: a hosted
    /// entry by exact host, else a `{host}` template whose path the URL carries.
    public static func catalogMark(for url: String?) -> String? {
        guard let url, let host = hostOf(url) else { return nil }
        let urls = DomainContract.mcpCatalogUrls
        let marks = DomainContract.mcpCatalogMarks
        let isTemplate = { (i: Int) in urls[i].contains("{host}") }
        if let i = urls.indices.first(where: { !isTemplate($0) && hostOf(urls[$0]) == host }) { return marks[i] }
        guard let path = pathOf(url) else { return nil }
        return urls.indices.first(where: {
            isTemplate($0) && pathOf(urls[$0].replacingOccurrences(of: "{host}", with: "template.invalid")) == path
        }).map { marks[$0] }
    }

    /// What a server row leads with.
    public enum Glyph: Equatable, Sendable {
        /// A catalog server's real brand mark (contract `mcpCatalog` slug).
        case brand(String)
        /// A stdio command: `session-shell`.
        case shell
        /// Anything else: `ui-mcp`.
        case plug
    }

    public static func glyph(_ server: McpServerRow) -> Glyph {
        if let mark = catalogMark(for: server.url) { return .brand(mark) }
        let hasCommand = !(server.command ?? "").isEmpty
        let hasUrl = !(server.url ?? "").isEmpty
        return hasCommand && !hasUrl ? .shell : .plug
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

/// One tool an MCP server lists (Settings › MCP's open rows): a team
/// server's (`mcpServers.tools`) or an app's (`appAccess.list` apps' `tools`).
public struct McpToolRow: Decodable, Identifiable, Equatable, Sendable {
    public let name: String
    public let title: String
    public let blurb: String
    public var id: String { name }

    public init(name: String, title: String, blurb: String = "") {
        self.name = name
        self.title = title
        self.blurb = blurb
    }

    enum CodingKeys: String, CodingKey { case name, title, blurb }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        let title = (try? c.decodeIfPresent(String.self, forKey: .title)) ?? ""
        self.title = title.isEmpty ? name : title
        blurb = (try? c.decodeIfPresent(String.self, forKey: .blurb)) ?? ""
    }
}

/// `mcpServers.tools {serverId}`: a team server's tools listed with the
/// caller's own credential (`tools` nil + `error` on a failure).
public struct McpServerTools: Decodable, Equatable, Sendable {
    public let ok: Bool
    public let tools: [McpToolRow]?
    public let error: String?

    public init(ok: Bool, tools: [McpToolRow]?, error: String? = nil) {
        self.ok = ok
        self.tools = tools
        self.error = error
    }
}

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
        /// Its exposed tools in package order (the open Team apps row).
        public let tools: [McpToolRow]
        /// #FEED-242: the level of members without their own row
        /// (`none | read | write`; missing = write).
        public var defaultAccess: String

        public init(
            id: String, slug: String, name: String, icon: String? = nil, color: String? = nil, toolCount: Int,
            tools: [McpToolRow] = [], defaultAccess: String = "write"
        ) {
            self.id = id
            self.slug = slug
            self.name = name
            self.icon = icon
            self.color = color
            self.toolCount = toolCount
            self.tools = tools
            self.defaultAccess = defaultAccess
        }

        /// #FEED-242: `defaultAccess` as a level (unknown = Full).
        public var defaultLevel: McpSettings.Level { McpSettings.Level(rawValue: defaultAccess) ?? .write }

        enum CodingKeys: String, CodingKey { case id, slug, name, icon, color, toolCount, tools, defaultAccess }

        public init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            id = try c.decode(String.self, forKey: .id)
            slug = try c.decode(String.self, forKey: .slug)
            name = try c.decode(String.self, forKey: .name)
            icon = try c.decodeIfPresent(String.self, forKey: .icon)
            color = try c.decodeIfPresent(String.self, forKey: .color)
            toolCount = try c.decode(Int.self, forKey: .toolCount)
            tools = (try? c.decodeIfPresent([McpToolRow].self, forKey: .tools)) ?? []
            defaultAccess = (try? c.decodeIfPresent(String.self, forKey: .defaultAccess)) ?? "write"
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
    public var apps: [App]
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

private struct AppAccessMineInput: Encodable {
    let appId: String
}

/// eApps R3: the caller's own level for an app (`appAccess.mine`).
public struct AppAccessMine: Decodable, Sendable {
    public let level: String
}

private struct AppAccessSetInput: Encodable {
    let appId: String
    let userId: String
    let level: String
}

/// #FEED-242 `appAccess.setDefault {appId, level}` (owners only).
private struct AppAccessSetDefaultInput: Encodable {
    let appId: String
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

    /// Settings › MCP: an open team server row's tools (http only; the
    /// server caches a member's answer 5 minutes).
    public func tools(accountId: String, serverId: String) async throws -> McpServerTools {
        try await trpc.query(
            accountId: accountId,
            path: "mcpServers.tools",
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

    /// eApps R3: the caller's own level for one app (`none | read | write`).
    public func appAccessMine(accountId: String, appId: String) async throws -> AppAccessMine {
        try await trpc.query(accountId: accountId, path: "appAccess.mine", input: AppAccessMineInput(appId: appId))
    }

    public func setAppAccess(accountId: String, appId: String, userId: String, level: McpSettings.Level) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "appAccess.set",
            input: AppAccessSetInput(appId: appId, userId: userId, level: level.rawValue)
        )
    }

    /// #FEED-242: an app's default level for members without their own row.
    public func setAppDefaultAccess(accountId: String, appId: String, level: McpSettings.Level) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "appAccess.setDefault",
            input: AppAccessSetDefaultInput(appId: appId, level: level.rawValue)
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
