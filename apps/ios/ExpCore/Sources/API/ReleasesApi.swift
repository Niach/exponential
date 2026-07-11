import Foundation

// tRPC surface for the releases router (EXP-56). Mobile is view/manage only:
// create, ship/unship, delete, and issue membership — reads come entirely from
// the synced `releases` shape.

public struct CreateReleaseInput: Encodable, Sendable {
    public let workspaceId: String
    public let name: String
    public var description: String?
    /// Plain `YYYY-MM-DD` date string (dateOnlySchema server-side).
    public var targetDate: String?

    public init(workspaceId: String, name: String, description: String? = nil, targetDate: String? = nil) {
        self.workspaceId = workspaceId
        self.name = name
        self.description = description
        self.targetDate = targetDate
    }
}

public struct MarkReleaseShippedInput: Encodable, Sendable {
    public let id: String
    public let shipped: Bool

    public init(id: String, shipped: Bool) {
        self.id = id
        self.shipped = shipped
    }
}

public struct DeleteReleaseInput: Encodable, Sendable {
    public let id: String

    public init(id: String) {
        self.id = id
    }
}

/// `releaseId` is a REQUIRED nullable key server-side
/// (`z.string().uuid().nullable()`), so a nil must encode as an explicit JSON
/// null — the synthesized Encodable would drop the key entirely.
public struct SetIssueReleaseInput: Encodable, Sendable {
    public let issueId: String
    public let releaseId: String?

    public init(issueId: String, releaseId: String?) {
        self.issueId = issueId
        self.releaseId = releaseId
    }

    enum CodingKeys: String, CodingKey {
        case issueId, releaseId
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(issueId, forKey: .issueId)
        if let releaseId {
            try container.encode(releaseId, forKey: .releaseId)
        } else {
            try container.encodeNil(forKey: .releaseId)
        }
    }
}

public final class ReleasesApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func create(accountId: String, _ input: CreateReleaseInput) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "releases.create", input: input)
    }

    public func markShipped(accountId: String, id: String, shipped: Bool) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "releases.markShipped",
            input: MarkReleaseShippedInput(id: id, shipped: shipped)
        )
    }

    public func delete(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "releases.delete",
            input: DeleteReleaseInput(id: id)
        )
    }

    /// Move an issue into a release (or out, with releaseId: nil). Writes the
    /// issues table — the Electric issues shape echoes the change into GRDB
    /// and the live observations refresh the UI (the standard iOS pattern).
    public func setIssueRelease(accountId: String, issueId: String, releaseId: String?) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "releases.setIssueRelease",
            input: SetIssueReleaseInput(issueId: issueId, releaseId: releaseId)
        )
    }
}
