import Foundation

// Mirrors apps/web/src/lib/trpc/threads.ts (EXP-1284) — the unit of work: a
// thread holds n issues and m runs. Rows arrive over the `threads` +
// `thread-items` shapes (22nd/23rd); the reads that need no sync and the four
// writes live here. Every write answers with a `txid` the client does not
// need (the rows land over the shapes), so it is never decoded.

/// The camelCase thread row tRPC answers with. Everything but the identity
/// columns is tolerated absent so a leaner server response never fails the
/// call — the write already happened.
public struct ThreadDto: Decodable, Sendable {
    public let id: String
    public let teamId: String
    public let title: String?
    public let createdBy: String?
    /// The thread's Guide as stored TEXT (the jsonb array re-encoded), the
    /// same form the shape's `results` column lands in.
    public let results: String?
    public let createdAt: String?
    public let updatedAt: String?

    public init(
        id: String,
        teamId: String,
        title: String? = nil,
        createdBy: String? = nil,
        results: String? = nil,
        createdAt: String? = nil,
        updatedAt: String? = nil
    ) {
        self.id = id
        self.teamId = teamId
        self.title = title
        self.createdBy = createdBy
        self.results = results
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, teamId, title, createdBy, results, createdAt, updatedAt
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        teamId = try c.decode(String.self, forKey: .teamId)
        title = try c.decodeIfPresent(String.self, forKey: .title)
        createdBy = try c.decodeIfPresent(String.self, forKey: .createdBy)
        // jsonb: a native array over tRPC — re-encoded type-faithfully.
        results = c.decodeWireJsonString(forKey: .results)
        createdAt = try c.decodeIfPresent(String.self, forKey: .createdAt)
        updatedAt = try c.decodeIfPresent(String.self, forKey: .updatedAt)
    }

    /// Mirror the server's row into the local store so a screen renders
    /// before the Electric long-poll delivers it (the `IssueDraftDto.entity`
    /// pattern). Sync re-delivers the same row and overwrites this one.
    public func entity() -> ThreadEntity {
        ThreadEntity(
            id: id,
            teamId: teamId,
            title: title,
            createdBy: createdBy,
            results: results,
            createdAt: createdAt ?? "",
            updatedAt: updatedAt ?? ""
        )
    }
}

/// One camelCase `thread_items` row (`threads.get`). The server-only board
/// trash/archive mirrors may ride along and are simply not decoded.
public struct ThreadItemDto: Decodable, Sendable {
    public let id: String
    public let threadId: String
    /// `issue` | `run` (`DomainContract.threadItemKindValues`).
    public let kind: String
    public let issueId: String?
    public let sessionId: String?
    public let teamId: String
    public let boardId: String?
    public let addedBy: String?
    public let addedBySessionId: String?
    public let source: String
    public let sortOrder: Double
    public let createdAt: String?
    public let updatedAt: String?

    public init(
        id: String,
        threadId: String,
        kind: String,
        issueId: String? = nil,
        sessionId: String? = nil,
        teamId: String,
        boardId: String? = nil,
        addedBy: String? = nil,
        addedBySessionId: String? = nil,
        source: String = DomainContract.threadItemSourceUser,
        sortOrder: Double = 0,
        createdAt: String? = nil,
        updatedAt: String? = nil
    ) {
        self.id = id
        self.threadId = threadId
        self.kind = kind
        self.issueId = issueId
        self.sessionId = sessionId
        self.teamId = teamId
        self.boardId = boardId
        self.addedBy = addedBy
        self.addedBySessionId = addedBySessionId
        self.source = source
        self.sortOrder = sortOrder
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, threadId, kind, issueId, sessionId, teamId, boardId
        case addedBy, addedBySessionId, source, sortOrder, createdAt, updatedAt
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        threadId = try c.decode(String.self, forKey: .threadId)
        kind = try c.decode(String.self, forKey: .kind)
        issueId = try c.decodeIfPresent(String.self, forKey: .issueId)
        sessionId = try c.decodeIfPresent(String.self, forKey: .sessionId)
        teamId = try c.decode(String.self, forKey: .teamId)
        boardId = try c.decodeIfPresent(String.self, forKey: .boardId)
        addedBy = try c.decodeIfPresent(String.self, forKey: .addedBy)
        addedBySessionId = try c.decodeIfPresent(String.self, forKey: .addedBySessionId)
        source = (try? c.decodeIfPresent(String.self, forKey: .source))
            ?? DomainContract.threadItemSourceUser
        sortOrder = (try? c.decodeWireDouble(forKey: .sortOrder)) ?? 0
        createdAt = try c.decodeIfPresent(String.self, forKey: .createdAt)
        updatedAt = try c.decodeIfPresent(String.self, forKey: .updatedAt)
    }

    /// Mirror into the local store, like `ThreadDto.entity()`.
    public func entity() -> ThreadItemEntity {
        ThreadItemEntity(
            id: id,
            threadId: threadId,
            kind: kind,
            issueId: issueId,
            sessionId: sessionId,
            teamId: teamId,
            boardId: boardId,
            addedBy: addedBy,
            addedBySessionId: addedBySessionId,
            source: source,
            sortOrder: sortOrder,
            createdAt: createdAt ?? "",
            updatedAt: updatedAt ?? ""
        )
    }
}

/// `threads.get` answers `{ thread, items }`.
public struct ThreadDetail: Decodable, Sendable {
    public let thread: ThreadDto
    public let items: [ThreadItemDto]

    public init(thread: ThreadDto, items: [ThreadItemDto]) {
        self.thread = thread
        self.items = items
    }
}

/// `threads.create` / `threads.update` answer `{ thread, txid }`.
public struct ThreadResult: Decodable, Sendable {
    public let thread: ThreadDto

    public init(thread: ThreadDto) {
        self.thread = thread
    }
}

/// `threads.link` answers `{ added, txid }` — `added` false = it was already
/// there (an issue already in the thread, a run already in it).
public struct ThreadLinkResult: Decodable, Sendable {
    public let added: Bool

    public init(added: Bool) {
        self.added = added
    }
}

/// `threads.unlink` answers `{ removed, txid, newThreadId? }`. Unlinking a RUN
/// moves it into a fresh thread of its own; `newThreadId` names that one.
public struct ThreadUnlinkResult: Decodable, Sendable {
    public let removed: Bool
    public let newThreadId: String?

    public init(removed: Bool, newThreadId: String? = nil) {
        self.removed = removed
        self.newThreadId = newThreadId
    }
}

/// `threads.create`: a new thread in a team, optionally titled and seeded
/// with issues. The nil fields are omitted from the wire.
public struct CreateThreadInput: Encodable, Sendable {
    public let teamId: String
    public let title: String?
    public let issueIds: [String]?

    public init(teamId: String, title: String? = nil, issueIds: [String]? = nil) {
        self.teamId = teamId
        self.title = title
        self.issueIds = issueIds
    }
}

/// `threads.update`: retitle. A nil (or empty) title CLEARS it back to the
/// derived display name, so nil goes out as an explicit JSON `null` rather
/// than being omitted.
public struct UpdateThreadInput: Encodable, Sendable {
    public let id: String
    public let title: String?

    public init(id: String, title: String?) {
        self.id = id
        self.title = title
    }

    enum CodingKeys: String, CodingKey {
        case id, title
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        if let title {
            try c.encode(title, forKey: .title)
        } else {
            try c.encodeNil(forKey: .title)
        }
    }
}

/// `threads.link` / `threads.unlink`: exactly ONE of `issueId` / `sessionId`.
/// Build it through `issue(…)` / `run(…)` so the pair can never be both.
public struct ThreadLinkInput: Encodable, Sendable {
    /// The thread id.
    public let id: String
    public let issueId: String?
    public let sessionId: String?

    private init(id: String, issueId: String?, sessionId: String?) {
        self.id = id
        self.issueId = issueId
        self.sessionId = sessionId
    }

    public static func issue(threadId: String, issueId: String) -> ThreadLinkInput {
        ThreadLinkInput(id: threadId, issueId: issueId, sessionId: nil)
    }

    public static func run(threadId: String, sessionId: String) -> ThreadLinkInput {
        ThreadLinkInput(id: threadId, issueId: nil, sessionId: sessionId)
    }
}

private struct ThreadIdInput: Encodable {
    let id: String
}

public final class ThreadsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// One thread with its items — a direct read for a thread the shapes have
    /// not delivered yet (a deep link, a just-started run).
    public func get(accountId: String, id: String) async throws -> ThreadDetail {
        try await trpc.query(accountId: accountId, path: "threads.get", input: ThreadIdInput(id: id))
    }

    /// Open a new thread. Returns the server row so the caller can mirror it.
    public func create(accountId: String, _ input: CreateThreadInput) async throws -> ThreadDto {
        let result: ThreadResult = try await trpc.mutation(
            accountId: accountId, path: "threads.create", input: input
        )
        return result.thread
    }

    /// Retitle a thread; nil / "" clears the title.
    public func update(accountId: String, id: String, title: String?) async throws -> ThreadDto {
        let result: ThreadResult = try await trpc.mutation(
            accountId: accountId, path: "threads.update", input: UpdateThreadInput(id: id, title: title)
        )
        return result.thread
    }

    /// Link an issue into a thread, or MOVE a run into it (a run sits in
    /// exactly one thread).
    @discardableResult
    public func link(accountId: String, _ input: ThreadLinkInput) async throws -> ThreadLinkResult {
        try await trpc.mutation(accountId: accountId, path: "threads.link", input: input)
    }

    /// Take an issue out of a thread, or move a run into a fresh thread of
    /// its own (`newThreadId`).
    @discardableResult
    public func unlink(accountId: String, _ input: ThreadLinkInput) async throws -> ThreadUnlinkResult {
        try await trpc.mutation(accountId: accountId, path: "threads.unlink", input: input)
    }
}
