import Foundation

// Invite LINKS, both directions (EXP-725): the app mints one (`create` →
// the raw token, wrapped into `<instance>/invite/<token>` by
// `WebLinks.invite`) and accepts one. Link-only on purpose — emailed invites
// stay a web surface, and the mint is shown ONLY while the team has free
// seats: at the cap the control is REMOVED rather than explained, so no
// seat/billing copy can reach an App Store build (3.1.1). The server is still
// the authority: a mint over the cap throws PRECONDITION_FAILED
// (`Error.isPlanLimitError`).

public struct AcceptInviteInput: Encodable, Sendable {
    public let token: String

    public init(token: String) {
        self.token = token
    }
}

public struct CreateInviteInput: Encodable, Sendable {
    public let teamId: String
    /// The server defaults to `member`; sent explicitly because the app never
    /// offers the owner role here.
    public let role: String

    public init(teamId: String, role: String = "member") {
        self.teamId = teamId
        self.role = role
    }
}

/// Only the token is decoded — the invite row itself arrives over the
/// `team_invites` shape, and the token is a bearer secret the shape's column
/// allowlist deliberately drops.
public struct CreateInviteResult: Decodable, Sendable {
    public let token: String

    public init(token: String) {
        self.token = token
    }
}

public final class TeamInvitesApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func accept(accountId: String, token: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "teamInvites.accept", input: AcceptInviteInput(token: token))
    }

    /// Mint a member invite link (owner-only server-side). Returns the raw
    /// token; `WebLinks.invite` turns it into the shareable URL.
    public func create(accountId: String, teamId: String) async throws -> String {
        let result: CreateInviteResult = try await trpc.mutation(
            accountId: accountId,
            path: "teamInvites.create",
            input: CreateInviteInput(teamId: teamId)
        )
        return result.token
    }
}
