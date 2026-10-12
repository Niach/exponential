import Foundation
import GRDB

/// EXP-1284: reads over the two thread shapes (`threads` + `thread_items`).
/// Both are team-scoped server-side, so every row here is one the user may
/// see; an issue item whose board is trashed or archived never syncs (the
/// shape's static predicates), while run items always do. A thread's `title`
/// may be nil — deriving the display name is the screen's job, not this one.
public enum ThreadQueries {
    /// A team's threads, most recent activity first (`updated_at` bumps on
    /// every link/unlink server-side), ties broken by id for a stable order.
    public static func threads(db: Database, teamId: String) throws -> [ThreadEntity] {
        try ThreadEntity
            .filter(Column("team_id") == teamId)
            .order(Column("updated_at").desc, Column("id").asc)
            .fetchAll(db)
    }

    /// One thread by id.
    public static func thread(db: Database, id: String) throws -> ThreadEntity? {
        try ThreadEntity.fetchOne(db, key: id)
    }

    /// A thread's issues and runs in display order (`sort_order` ascending,
    /// then creation, then id).
    public static func items(db: Database, threadId: String) throws -> [ThreadItemEntity] {
        try ThreadItemEntity
            .filter(Column("thread_id") == threadId)
            .order(Column("sort_order").asc, Column("created_at").asc, Column("id").asc)
            .fetchAll(db)
    }

    /// Every thread holding the issue (an issue may sit in several), most
    /// recent activity first — the first one is the thread an issue start
    /// joins (`joinThreadForStart`: the issue's newest thread).
    public static func threadsHoldingIssue(db: Database, issueId: String) throws -> [ThreadEntity] {
        let threadIds = try ThreadItemEntity
            .filter(Column("kind") == DomainContract.threadItemKindIssue)
            .filter(Column("issue_id") == issueId)
            .select(Column("thread_id"), as: String.self)
            .fetchAll(db)
        guard !threadIds.isEmpty else { return [] }
        return try ThreadEntity
            .filter(threadIds.contains(Column("id")))
            .order(Column("updated_at").desc, Column("id").asc)
            .fetchAll(db)
    }

    /// The ONE thread a run sits in (a run is in exactly one; nil = its item
    /// has not synced yet, or a legacy run predating threads).
    public static func threadOfSession(db: Database, sessionId: String) throws -> ThreadEntity? {
        guard let threadId = try ThreadItemEntity
            .filter(Column("kind") == DomainContract.threadItemKindRun)
            .filter(Column("session_id") == sessionId)
            .select(Column("thread_id"), as: String.self)
            .fetchOne(db)
        else { return nil }
        return try ThreadEntity.fetchOne(db, key: threadId)
    }
}
