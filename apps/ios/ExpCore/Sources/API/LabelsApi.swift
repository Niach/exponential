import Foundation

public struct CreateLabelInput: Encodable, Sendable {
    public let name: String
    public let color: String
    public let teamId: String

    public init(name: String, color: String, teamId: String) {
        self.name = name
        self.color = color
        self.teamId = teamId
    }
}

public struct UpdateLabelInput: Encodable, Sendable {
    public let id: String
    public var name: String?
    public var color: String?

    public init(id: String, name: String? = nil, color: String? = nil) {
        self.id = id
        self.name = name
        self.color = color
    }
}

public struct DeleteLabelInput: Encodable, Sendable {
    public let id: String

    public init(id: String) {
        self.id = id
    }
}

public struct AddIssueLabelInput: Encodable, Sendable {
    public let issueId: String
    public let labelId: String

    public init(issueId: String, labelId: String) {
        self.issueId = issueId
        self.labelId = labelId
    }
}

public struct RemoveIssueLabelInput: Encodable, Sendable {
    public let issueId: String
    public let labelId: String

    public init(issueId: String, labelId: String) {
        self.issueId = issueId
        self.labelId = labelId
    }
}

/// Input for `issueLabels.bulkAdd` / `issueLabels.bulkRemove` — one label
/// across many issues in ONE transaction (`issueIds` capped at 200).
public struct BulkIssueLabelInput: Encodable, Sendable {
    public let labelId: String
    public let issueIds: [String]

    public init(labelId: String, issueIds: [String]) {
        self.labelId = labelId
        self.issueIds = issueIds
    }
}

public struct LabelResult: Decodable, Sendable {
    public let label: LabelResultData

    public init(label: LabelResultData) {
        self.label = label
    }
}

public struct LabelResultData: Decodable, Sendable {
    public let id: String

    public init(id: String) {
        self.id = id
    }
}

public final class LabelsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func create(accountId: String, _ input: CreateLabelInput) async throws -> String {
        let result: LabelResult = try await trpc.mutation(accountId: accountId, path: "labels.create", input: input)
        return result.label.id
    }

    public func update(accountId: String, _ input: UpdateLabelInput) async throws {
        let _: LabelResult = try await trpc.mutation(accountId: accountId, path: "labels.update", input: input)
    }

    public func delete(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "labels.delete", input: DeleteLabelInput(id: id))
    }

    public func addToIssue(accountId: String, issueId: String, labelId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "issueLabels.add", input: AddIssueLabelInput(issueId: issueId, labelId: labelId))
    }

    public func removeFromIssue(accountId: String, issueId: String, labelId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "issueLabels.remove", input: RemoveIssueLabelInput(issueId: issueId, labelId: labelId))
    }

    /// Add one label to many issues in a single transaction (selection bar).
    /// Unlike the per-issue `addToIssue`, the server records a `label_added`
    /// timeline event only for rows it actually inserted, so passing issues
    /// that already carry the label writes no spurious duplicate events.
    public func bulkAddToIssues(accountId: String, issueIds: [String], labelId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "issueLabels.bulkAdd", input: BulkIssueLabelInput(labelId: labelId, issueIds: issueIds))
    }

    /// Remove one label from many issues in a single transaction; events are
    /// likewise recorded only for rows that really went away.
    public func bulkRemoveFromIssues(accountId: String, issueIds: [String], labelId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "issueLabels.bulkRemove", input: BulkIssueLabelInput(labelId: labelId, issueIds: issueIds))
    }
}
