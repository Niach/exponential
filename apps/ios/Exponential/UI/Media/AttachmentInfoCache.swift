import ExpCore
import ExpUI
import Foundation
import GRDB

/// EXP-824 — attachment id → `AttachmentMediaInfo` off the local store, for
/// renderers that have no live attachments observation of their own: comment
/// cards and the agent transcript. Same shape as `IssueRefChipCache`: a
/// short-TTL memo over the local store, so a row landing later is picked up
/// within seconds (the issue detail feeds its editor from its live
/// observation instead).
/// #PERF-5: never a blocking read: every caller `peek`s.
@MainActor
enum AttachmentInfoCache {
    private struct Key: Hashable, Sendable {
        let accountId: String
        let attachmentId: String
    }

    private static let memo = StoreMemo<Key, AttachmentMediaInfo>()

    /// #PERF-5: memory only: an unknown or stale row loads off main and
    /// renders when it lands (`StoreMemoLoads`).
    static func peek(_ attachmentId: String, db: DatabaseManager, accountId: String) -> AttachmentMediaInfo? {
        memo.peek(Key(accountId: accountId, attachmentId: attachmentId.lowercased())) {
            guard let pool = try? db.pool(forAccountId: accountId) else { return nil }
            return ((try? await pool.read { try fetch(attachmentId, $0) }) ?? nil)
        }
    }

    private nonisolated static func fetch(_ attachmentId: String, _ db: Database) throws -> AttachmentMediaInfo? {
        try AttachmentEntity.filter(Column("id") == attachmentId).fetchOne(db).map(AttachmentMediaInfo.init)
    }

    /// A resolver closure for an `IssueEditorModel` bound to one account.
    static func resolver(db: DatabaseManager, accountId: String) -> (String) -> AttachmentMediaInfo? {
        { id in peek(id, db: db, accountId: accountId) }
    }
}
