import Foundation

public struct GoogleStatusResult: Decodable, Sendable {
    public let connected: Bool
    public let connectedAt: String?

    public init(connected: Bool, connectedAt: String?) {
        self.connected = connected
        self.connectedAt = connectedAt
    }
}

public struct GithubTokenResult: Decodable, Sendable {
    public let token: String?

    public init(token: String?) {
        self.token = token
    }
}

private struct EmptyIntegrationInput: Encodable {}

public final class IntegrationsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func googleStatus(accountId: String) async throws -> GoogleStatusResult {
        // Server defines integrations.google.status as a `.query` (GET).
        try await trpc.query(accountId: accountId, path: "integrations.google.status")
    }

    public func googleDisconnect(accountId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "integrations.google.disconnect", input: EmptyIntegrationInput())
    }

    public func googleBackfill(accountId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "integrations.google.backfill", input: EmptyIntegrationInput())
    }

    /// The owner's connected GitHub token (`integrations.github.token`, a
    /// `.query`), for handing to agent-core for clone/push. Resolves server-side
    /// via the calling account's session, so call it with the OWNER account's id
    /// (its human session bearer) — not an agent credential. Nil when not
    /// connected.
    public func githubToken(accountId: String) async throws -> String? {
        let result: GithubTokenResult = try await trpc.query(accountId: accountId, path: "integrations.github.token")
        return result.token
    }
}
