import Foundation

/// EXP-792/1249: the team's MCP servers as a launch picker reads them —
/// `mcpServers.list({teamId})`, each row with the CALLER's own connection
/// (the server holds every member's credential, so readiness is the same on
/// every machine). The phone only PICKS servers for a run; adding and
/// connecting them stays in Settings on web / the IDE.
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

    public init(
        id: String,
        name: String,
        url: String? = nil,
        command: String? = nil,
        enabledByDefault: Bool = false,
        connection: Connection
    ) {
        self.id = id
        self.name = name
        self.url = url
        self.command = command
        self.enabledByDefault = enabledByDefault
        self.connection = connection
    }

    enum CodingKeys: String, CodingKey {
        case id, name, url, command, enabledByDefault, connection
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
}
