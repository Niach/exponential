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
    var archivedAt: String?

    // Fields listed here are encoded as JSON null (not omitted).
    // Use this when the server must distinguish "clear this field" from "don't touch it".
    var explicitNulls: Set<String> = []

    enum CodingKeys: String, CodingKey {
        case id, title, status, priority, assigneeId, description
        case dueDate, dueTime, endTime, recurrenceInterval, recurrenceUnit, archivedAt
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encodeIfPresent(title, forKey: .title)
        try c.encodeIfPresent(status, forKey: .status)
        try c.encodeIfPresent(priority, forKey: .priority)
        try encodeNullable(assigneeId, forKey: .assigneeId, in: &c)
        try encodeNullable(description, forKey: .description, in: &c)
        try encodeNullable(dueDate, forKey: .dueDate, in: &c)
        try encodeNullable(dueTime, forKey: .dueTime, in: &c)
        try encodeNullable(endTime, forKey: .endTime, in: &c)
        try encodeNullable(recurrenceInterval, forKey: .recurrenceInterval, in: &c)
        try encodeNullable(recurrenceUnit, forKey: .recurrenceUnit, in: &c)
        try encodeNullable(archivedAt, forKey: .archivedAt, in: &c)
    }

    private func encodeNullable<T: Encodable>(_ value: T?, forKey key: CodingKeys, in container: inout KeyedEncodingContainer<CodingKeys>) throws {
        if value != nil {
            try container.encode(value, forKey: key)
        } else if explicitNulls.contains(key.rawValue) {
            try container.encodeNil(forKey: key)
        }
    }
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

    func create(accountId: String, _ input: CreateIssueInput) async throws -> String {
        let result: IssueResult = try await trpc.mutation(accountId: accountId, path: "issues.create", input: input)
        return result.issue.id
    }

    func update(accountId: String, _ input: UpdateIssueInput) async throws {
        let _: IssueResult = try await trpc.mutation(accountId: accountId, path: "issues.update", input: input)
    }

    func delete(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "issues.delete", input: DeleteIssueInput(id: id))
    }
}
