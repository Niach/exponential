import Foundation

// MARK: - Input/Output types

struct CreateIssueInput: Encodable {
    let projectId: String
    let title: String
    var status: String?
    var priority: String?
    var assigneeId: String?
    var description: IssueDescription?
    var dueDate: String?
    var dueTime: String?
    var endTime: String?
    var labelIds: [String]?
    var recurrenceInterval: Int?
    var recurrenceUnit: String?
}

struct UpdateIssueInput: Encodable {
    let id: String
    var title: String?
    var status: String?
    var priority: String?
    var assigneeId: String?
    var description: IssueDescription?
    var dueDate: String?
    var dueTime: String?
    var endTime: String?
    var recurrenceInterval: Int?
    var recurrenceUnit: String?
}

struct DeleteIssueInput: Encodable {
    let id: String
}

struct IssueDescription: Encodable {
    let text: String
}

struct IssueResult: Decodable {
    let issue: IssueResultData
}

struct IssueResultData: Decodable {
    let id: String
}

// MARK: - API

final class IssuesApi: Sendable {
    private let trpc: TrpcClient

    init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    func create(_ input: CreateIssueInput) async throws -> String {
        let result: IssueResult = try await trpc.mutation(path: "issues.create", input: input)
        return result.issue.id
    }

    func update(_ input: UpdateIssueInput) async throws {
        let _: IssueResult = try await trpc.mutation(path: "issues.update", input: input)
    }

    func delete(id: String) async throws {
        try await trpc.mutationVoid(path: "issues.delete", input: DeleteIssueInput(id: id))
    }
}
