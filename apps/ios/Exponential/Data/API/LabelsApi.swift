import Foundation

struct CreateLabelInput: Encodable {
    let name: String
    let color: String
    let workspaceId: String
}

struct UpdateLabelInput: Encodable {
    let id: String
    var name: String?
    var color: String?
}

struct DeleteLabelInput: Encodable {
    let id: String
}

struct AddIssueLabelInput: Encodable {
    let issueId: String
    let labelId: String
}

struct RemoveIssueLabelInput: Encodable {
    let issueId: String
    let labelId: String
}

struct LabelResult: Decodable {
    let label: LabelResultData
}

struct LabelResultData: Decodable {
    let id: String
}

final class LabelsApi: Sendable {
    private let trpc: TrpcClient

    init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    func create(accountId: String, _ input: CreateLabelInput) async throws -> String {
        let result: LabelResult = try await trpc.mutation(accountId: accountId, path: "labels.create", input: input)
        return result.label.id
    }

    func update(accountId: String, _ input: UpdateLabelInput) async throws {
        let _: LabelResult = try await trpc.mutation(accountId: accountId, path: "labels.update", input: input)
    }

    func delete(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "labels.delete", input: DeleteLabelInput(id: id))
    }

    func addToIssue(accountId: String, issueId: String, labelId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "issueLabels.add", input: AddIssueLabelInput(issueId: issueId, labelId: labelId))
    }

    func removeFromIssue(accountId: String, issueId: String, labelId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "issueLabels.remove", input: RemoveIssueLabelInput(issueId: issueId, labelId: labelId))
    }
}
