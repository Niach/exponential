import Foundation

// MARK: - Input/Output types

public struct CreateIssueInput: Encodable, Sendable {
    public let projectId: String
    public let title: String
    public var status: String?
    public var priority: String?
    public var assigneeId: String?
    public var description: IssueDescription?
    public var dueDate: String?
    public var dueTime: String?
    public var endTime: String?
    public var labelIds: [String]?
    public var recurrenceInterval: Int?
    public var recurrenceUnit: String?

    public init(
        projectId: String,
        title: String,
        status: String? = nil,
        priority: String? = nil,
        assigneeId: String? = nil,
        description: IssueDescription? = nil,
        dueDate: String? = nil,
        dueTime: String? = nil,
        endTime: String? = nil,
        labelIds: [String]? = nil,
        recurrenceInterval: Int? = nil,
        recurrenceUnit: String? = nil
    ) {
        self.projectId = projectId
        self.title = title
        self.status = status
        self.priority = priority
        self.assigneeId = assigneeId
        self.description = description
        self.dueDate = dueDate
        self.dueTime = dueTime
        self.endTime = endTime
        self.labelIds = labelIds
        self.recurrenceInterval = recurrenceInterval
        self.recurrenceUnit = recurrenceUnit
    }
}

public struct UpdateIssueInput: Encodable, Sendable {
    public let id: String
    public var title: String?
    public var status: String?
    public var priority: String?
    public var assigneeId: String?
    public var description: IssueDescription?
    public var dueDate: String?
    public var dueTime: String?
    public var endTime: String?
    public var recurrenceInterval: Int?
    public var recurrenceUnit: String?
    public var archivedAt: String?

    // Fields listed here are encoded as JSON null (not omitted).
    // Use this when the server must distinguish "clear this field" from "don't touch it".
    public var explicitNulls: Set<String> = []

    public init(
        id: String,
        title: String? = nil,
        status: String? = nil,
        priority: String? = nil,
        assigneeId: String? = nil,
        description: IssueDescription? = nil,
        dueDate: String? = nil,
        dueTime: String? = nil,
        endTime: String? = nil,
        recurrenceInterval: Int? = nil,
        recurrenceUnit: String? = nil,
        archivedAt: String? = nil,
        explicitNulls: Set<String> = []
    ) {
        self.id = id
        self.title = title
        self.status = status
        self.priority = priority
        self.assigneeId = assigneeId
        self.description = description
        self.dueDate = dueDate
        self.dueTime = dueTime
        self.endTime = endTime
        self.recurrenceInterval = recurrenceInterval
        self.recurrenceUnit = recurrenceUnit
        self.archivedAt = archivedAt
        self.explicitNulls = explicitNulls
    }

    enum CodingKeys: String, CodingKey {
        case id, title, status, priority, assigneeId, description
        case dueDate, dueTime, endTime, recurrenceInterval, recurrenceUnit, archivedAt
    }

    public func encode(to encoder: Encoder) throws {
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

public struct DeleteIssueInput: Encodable, Sendable {
    public let id: String

    public init(id: String) {
        self.id = id
    }
}

public struct IssueDescription: Encodable, Sendable {
    public let text: String

    public init(text: String) {
        self.text = text
    }
}

public struct IssueResult: Decodable, Sendable {
    public let issue: IssueResultData

    public init(issue: IssueResultData) {
        self.issue = issue
    }
}

public struct IssueResultData: Decodable, Sendable {
    public let id: String

    public init(id: String) {
        self.id = id
    }
}

// MARK: - API

public final class IssuesApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func create(accountId: String, _ input: CreateIssueInput) async throws -> String {
        let result: IssueResult = try await trpc.mutation(accountId: accountId, path: "issues.create", input: input)
        return result.issue.id
    }

    public func update(accountId: String, _ input: UpdateIssueInput) async throws {
        let _: IssueResult = try await trpc.mutation(accountId: accountId, path: "issues.update", input: input)
    }

    public func delete(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "issues.delete", input: DeleteIssueInput(id: id))
    }
}
