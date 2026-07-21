import Foundation

// Accept-only surface: inviting members is a web-only flow (EXP-216 — the
// App Store build must never reach the seat-cap billing copy), but invite
// LINKS still open in the app, so accepting stays.

public struct AcceptInviteInput: Encodable, Sendable {
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
}
