import Foundation

// Mirrors apps/web/src/lib/trpc/coding-sessions.ts. The desktop launcher opens a
// coding_sessions row when it spawns `claude` and ends it when the run closes,
// so coordination clients show a live "coding now" badge (the coding_sessions
// Electric shape). Only `id` is needed back — the full row also syncs over
// Electric.

/// Minimal decode of the returned `coding_sessions` row (extra fields ignored).
public struct CodingSessionRef: Decodable, Sendable {
    public let id: String

    public init(id: String) {
        self.id = id
    }
}

/// Server envelope: `codingSessions.start` returns `{ session: <row> }`.
public struct CodingSessionResult: Decodable, Sendable {
    public let session: CodingSessionRef

    public init(session: CodingSessionRef) {
        self.session = session
    }
}

private struct StartInput: Encodable {
    let issueId: String
    let deviceLabel: String?
}

private struct EndInput: Encodable {
    let id: String
}

public final class CodingSessionsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Insert a `running` coding_sessions row for the issue. Returns the new row's
    /// id (used to key the terminal + end the session).
    public func start(accountId: String, issueId: String, deviceLabel: String?) async throws -> CodingSessionRef {
        let result: CodingSessionResult = try await trpc.mutation(
            accountId: accountId,
            path: "codingSessions.start",
            input: StartInput(issueId: issueId, deviceLabel: deviceLabel)
        )
        return result.session
    }

    /// Flip a coding_sessions row to `ended` (stamps `endedAt`).
    public func end(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "codingSessions.end",
            input: EndInput(id: id)
        )
    }
}
