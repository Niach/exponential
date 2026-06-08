import Foundation

// Human-session companion procedures used by the desktop apps to surface and
// clean up agents the signed-in owner registered. Mirrors the relevant parts of
// apps/web/src/lib/trpc/companion/setup.ts. (Registration itself stays in the
// platform agent service because it needs the minted agent credential, but
// listing/revoking are plain owner-authorized calls.)

public struct CompanionAgentSummary: Decodable, Sendable, Identifiable {
    public let id: String
    public let name: String
    public let deviceId: String
    public let lastSeenAt: String?
}

private struct ListMineResult: Decodable { let agents: [CompanionAgentSummary] }
private struct AgentIdInput: Encodable { let agentId: String }

public final class CompanionApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Every agent the signed-in account owns, across ALL workspaces — lets the
    /// UI surface an agent registered against the wrong (e.g. public/shared)
    /// workspace so the owner can revoke it.
    public func listMine(accountId: String) async throws -> [CompanionAgentSummary] {
        let result: ListMineResult = try await trpc.query(
            accountId: accountId,
            path: "agent.listMine"
        )
        return result.agents
    }

    /// Revoke an owned agent (its credential is revoked server-side and it stops
    /// working until re-registered).
    public func revoke(accountId: String, agentId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "agent.revoke",
            input: AgentIdInput(agentId: agentId)
        )
    }
}
