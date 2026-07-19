import Foundation

// MARK: - Wire types (tRPC helpdesk router — apps/web/src/lib/trpc/helpdesk.ts)
//
// Standalone support tickets are NOT Electric-synced: natives poll these
// member-only tRPC endpoints. tRPC runs without a transformer, so timestamps
// arrive as ISO-8601 strings and booleans as native JSON scalars.

public enum SupportThreadFilter: String, Codable, Sendable, CaseIterable {
    case open
    case resolved
}

/// One inbox row from `helpdesk.listThreads`. `unread` = the reporter spoke
/// last (there is no per-member read state).
public struct SupportThreadRow: Decodable, Sendable, Identifiable {
    public let id: String
    public let teamId: String
    public let title: String
    public let status: String
    public let linkedIssueId: String?
    public let reporterEmail: String
    public let reporterName: String?
    public let lastReporterSeenAt: String?
    public let createdAt: String
    public let updatedAt: String
    public let lastMessage: SupportLastMessage?
    public let unread: Bool

    public init(
        id: String,
        teamId: String,
        title: String,
        status: String,
        linkedIssueId: String?,
        reporterEmail: String,
        reporterName: String?,
        lastReporterSeenAt: String?,
        createdAt: String,
        updatedAt: String,
        lastMessage: SupportLastMessage?,
        unread: Bool
    ) {
        self.id = id
        self.teamId = teamId
        self.title = title
        self.status = status
        self.linkedIssueId = linkedIssueId
        self.reporterEmail = reporterEmail
        self.reporterName = reporterName
        self.lastReporterSeenAt = lastReporterSeenAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
        self.lastMessage = lastMessage
        self.unread = unread
    }
}

public struct SupportLastMessage: Decodable, Sendable {
    public let body: String
    public let direction: String
    public let createdAt: String

    public init(body: String, direction: String, createdAt: String) {
        self.body = body
        self.direction = direction
        self.createdAt = createdAt
    }
}

/// The thread header inside `helpdesk.getThread` (a full `support_threads`
/// row server-side; unknown keys — token bookkeeping etc. — are dropped).
public struct SupportThreadInfo: Decodable, Sendable, Identifiable {
    public let id: String
    public let teamId: String
    public let title: String
    public let status: String
    public let linkedIssueId: String?
    public let reporterEmail: String
    public let reporterName: String?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        teamId: String,
        title: String,
        status: String,
        linkedIssueId: String?,
        reporterEmail: String,
        reporterName: String?,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.teamId = teamId
        self.title = title
        self.status = status
        self.linkedIssueId = linkedIssueId
        self.reporterEmail = reporterEmail
        self.reporterName = reporterName
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

/// One conversation entry. `authorUserId` is nil for the external reporter;
/// `visibility == "internal"` marks member-only notes (never emailed).
public struct SupportMessage: Decodable, Sendable, Identifiable {
    public let id: String
    public let threadId: String
    public let authorUserId: String?
    public let direction: String
    public let visibility: String
    public let body: String
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        threadId: String,
        authorUserId: String?,
        direction: String,
        visibility: String,
        body: String,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.threadId = threadId
        self.authorUserId = authorUserId
        self.direction = direction
        self.visibility = visibility
        self.body = body
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    public var isInbound: Bool { direction == "inbound" }
    public var isInternal: Bool { visibility == "internal" }
}

/// Minimal projection of the escalated issue for the linked-issue chip.
public struct SupportLinkedIssue: Decodable, Sendable, Identifiable {
    public let id: String
    public let identifier: String?
    public let title: String
    public let status: String
    public let boardId: String

    public init(id: String, identifier: String?, title: String, status: String, boardId: String) {
        self.id = id
        self.identifier = identifier
        self.title = title
        self.status = status
        self.boardId = boardId
    }
}

public struct SupportThreadDetail: Decodable, Sendable {
    public let thread: SupportThreadInfo
    public let messages: [SupportMessage]
    public let linkedIssue: SupportLinkedIssue?

    public init(thread: SupportThreadInfo, messages: [SupportMessage], linkedIssue: SupportLinkedIssue?) {
        self.thread = thread
        self.messages = messages
        self.linkedIssue = linkedIssue
    }
}

/// `helpdesk.escalate` result: the ordinary issue filed from the ticket.
public struct SupportEscalatedIssue: Decodable, Sendable {
    public let id: String
    public let identifier: String?
    public let title: String

    public init(id: String, identifier: String?, title: String) {
        self.id = id
        self.identifier = identifier
        self.title = title
    }
}

// MARK: - Inputs

private struct ListThreadsInput: Encodable {
    let teamId: String
    let filter: String
}

private struct ThreadIdInput: Encodable {
    let threadId: String
}

private struct MessageInput: Encodable {
    let threadId: String
    let body: String
}

private struct EscalateInput: Encodable {
    let threadId: String
    let boardId: String
    // Optional — JSONEncoder omits the key when nil, matching the zod schema.
    let title: String?
}

private struct EscalateResult: Decodable {
    let issue: SupportEscalatedIssue
}

// MARK: - Api

public final class HelpdeskApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func listThreads(
        accountId: String, teamId: String, filter: SupportThreadFilter
    ) async throws -> [SupportThreadRow] {
        try await trpc.query(
            accountId: accountId,
            path: "helpdesk.listThreads",
            input: ListThreadsInput(teamId: teamId, filter: filter.rawValue)
        )
    }

    public func getThread(accountId: String, threadId: String) async throws -> SupportThreadDetail {
        try await trpc.query(
            accountId: accountId,
            path: "helpdesk.getThread",
            input: ThreadIdInput(threadId: threadId)
        )
    }

    /// Public reply — emailed to the reporter (body max 10 000 chars).
    public func reply(accountId: String, threadId: String, body: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "helpdesk.reply",
            input: MessageInput(threadId: threadId, body: body)
        )
    }

    /// Internal note — member-only, never emailed.
    public func note(accountId: String, threadId: String, body: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "helpdesk.note",
            input: MessageInput(threadId: threadId, body: body)
        )
    }

    public func close(accountId: String, threadId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "helpdesk.close",
            input: ThreadIdInput(threadId: threadId)
        )
    }

    public func reopen(accountId: String, threadId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "helpdesk.reopen",
            input: ThreadIdInput(threadId: threadId)
        )
    }

    /// File an ordinary issue on a board of the ticket's team and link it.
    /// Rejects when the ticket already has a linked issue.
    public func escalate(
        accountId: String, threadId: String, boardId: String, title: String? = nil
    ) async throws -> SupportEscalatedIssue {
        let result: EscalateResult = try await trpc.mutation(
            accountId: accountId,
            path: "helpdesk.escalate",
            input: EscalateInput(threadId: threadId, boardId: boardId, title: title)
        )
        return result.issue
    }
}
