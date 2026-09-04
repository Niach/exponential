import Foundation

/// Input for `codingSessions.mergePr` (EXP-734).
public struct MergeSessionPrInput: Encodable, Sendable {
    public let sessionId: String

    public init(sessionId: String) {
        self.sessionId = sessionId
    }
}

/// EXP-734: the run's own pull request — the one an action or chat run opened
/// through MCP `exponential_pr_open({repositoryId, head})`, which links no
/// issue at all. Issue and batch runs merge through `IssuesApi.mergePr`.
public final class CodingSessionsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Squash-merge the session's own PR via the GitHub App. Nothing is
    /// written locally: the server flips the row's `pr_state` to `merged` (and
    /// ends the run unless the team keeps sessions on merge), and both land
    /// here through Electric sync.
    public func mergePr(accountId: String, sessionId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "codingSessions.mergePr",
            input: MergeSessionPrInput(sessionId: sessionId)
        )
    }
}
