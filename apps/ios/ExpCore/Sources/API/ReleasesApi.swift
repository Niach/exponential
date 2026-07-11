import Foundation

// tRPC surface for the releases router (EXP-56). Mobile is view/manage only:
// create, ship/unship, delete, and issue membership — reads come entirely from
// the synced `releases` shape.

/// Name is server-generated (sequential `Release N`) — creation is one tap.
public struct CreateReleaseInput: Encodable, Sendable {
    public let workspaceId: String

    public init(workspaceId: String) {
        self.workspaceId = workspaceId
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
        // Keyed `encode` of an Optional emits explicit JSON null for nil —
        // only `encodeIfPresent`/synthesis drop the key.
        try container.encode(releaseId, forKey: .releaseId)
    }
}

public struct AddReleaseIssuesInput: Encodable, Sendable {
    public let releaseId: String
    public let issueIds: [String]

    public init(releaseId: String, issueIds: [String]) {
        self.releaseId = releaseId
        self.issueIds = issueIds
    }
}

private struct CreateReleaseOutput: Decodable {
    struct R: Decodable { let id: String }
    let release: R
}

public final class ReleasesApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Create an auto-named release and return its id so the caller can wait
    /// for the synced row and navigate straight to the detail.
    public func create(accountId: String, workspaceId: String) async throws -> String {
        let out: CreateReleaseOutput = try await trpc.mutation(
            accountId: accountId,
            path: "releases.create",
            input: CreateReleaseInput(workspaceId: workspaceId)
        )
        return out.release.id
    }

    /// Bundle issues into a release (the add-issues sheet's confirm). The
    /// server caps issueIds at 200 per call — chunk sequentially so any
    /// selection size lands (wire contract: clients chunk >200 ids). The
    /// issues shape echoes the release_id writes back into GRDB.
    public func addIssues(accountId: String, releaseId: String, issueIds: [String]) async throws {
        for start in stride(from: 0, to: issueIds.count, by: 200) {
            let chunk = Array(issueIds[start..<min(start + 200, issueIds.count)])
            try await trpc.mutationVoid(
                accountId: accountId,
                path: "releases.addIssues",
                input: AddReleaseIssuesInput(releaseId: releaseId, issueIds: chunk)
            )
        }
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
