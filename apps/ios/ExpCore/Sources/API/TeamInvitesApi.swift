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
    /// The recipient (Members' invite form): the server mails the link and
    /// seats the person as a placeholder member at once (EXP-630).
    public var email: String?
    /// The placeholder's display name; nil = the mailbox local part.
    public var name: String?

    public init(teamId: String, role: String = "member", email: String? = nil, name: String? = nil) {
        self.teamId = teamId
        self.role = role
        self.email = email
        self.name = name
    }
}

public struct RevokeInviteInput: Encodable, Sendable {
    public let id: String

    public init(id: String) {
        self.id = id
    }
}

/// Only the token is decoded — the invite row itself arrives over the
/// `team_invites` shape, and the token is a bearer secret the shape's column
/// allowlist deliberately drops.
public struct CreateInviteResult: Decodable, Sendable {
    public let token: String
    /// An emailed invite: whether the mail went out (nil = a link invite).
    public let emailDelivered: Bool?

    public init(token: String, emailDelivered: Bool? = nil) {
        self.token = token
        self.emailDelivered = emailDelivered
    }
}

public struct GetInviteByTokenInput: Encodable, Sendable {
    public let token: String

    public init(token: String) {
        self.token = token
    }
}

/// #EXP-1632: what the invite landing card shows before the caller accepts
/// (`teamInvites.getByToken`, web `/invite/$token`, Android `InvitePreview`).
public struct InvitePreview: Decodable, Sendable, Equatable {
    public let id: String
    public let teamId: String
    public let role: String
    public let acceptedAt: String?
    public let expiresAt: String?
    public let teamName: String

    public init(
        id: String, teamId: String, role: String,
        acceptedAt: String? = nil, expiresAt: String? = nil, teamName: String
    ) {
        self.id = id
        self.teamId = teamId
        self.role = role
        self.acceptedAt = acceptedAt
        self.expiresAt = expiresAt
        self.teamName = teamName
    }
}

private struct GetInviteByTokenResult: Decodable {
    let invite: InvitePreview
}

public final class TeamInvitesApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func accept(accountId: String, token: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "teamInvites.accept", input: AcceptInviteInput(token: token))
    }

    /// #EXP-1632: the invite's landing card (team, role, used/expired),
    /// read before the caller chooses to accept.
    public func getByToken(accountId: String, token: String) async throws -> InvitePreview {
        let result: GetInviteByTokenResult = try await trpc.query(
            accountId: accountId,
            path: "teamInvites.getByToken",
            input: GetInviteByTokenInput(token: token)
        )
        return result.invite
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

    /// Members' invite form (web `InviteMemberForm`): mail the link to
    /// `email` and seat the placeholder member (owner-only server-side).
    public func create(accountId: String, teamId: String, email: String, name: String?) async throws -> CreateInviteResult {
        try await trpc.mutation(
            accountId: accountId,
            path: "teamInvites.create",
            input: CreateInviteInput(teamId: teamId, email: email, name: name)
        )
    }

    /// Revoke a pending invite (the Pending invites row's trash).
    public func revoke(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "teamInvites.revoke", input: RevokeInviteInput(id: id))
    }
}
