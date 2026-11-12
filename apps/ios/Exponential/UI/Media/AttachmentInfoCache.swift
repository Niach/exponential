import ExpCore
import ExpUI
import Foundation
import GRDB

/// EXP-824 — attachment id → `AttachmentMediaInfo` off the local store, for
/// renderers that have no live attachments observation of their own: comment
/// cards and the agent transcript. Same shape as `IssueRefChipCache`: a
/// short-TTL memo over a synchronous GRDB read, so a media block resolves on
/// first render and a row landing later is picked up within seconds (the
/// issue detail feeds its editor from its live observation instead).
/// #PERF-5: a layout pass `peek`s (never a blocking read).
@MainActor
enum AttachmentInfoCache {
    private struct Key: Hashable, Sendable {
        let accountId: String
        let attachmentId: String
    }

    private static let memo = StoreMemo<Key, AttachmentMediaInfo>()

    static func info(_ attachmentId: String, db: DatabaseManager, accountId: String) -> AttachmentMediaInfo? {
        memo.value(Key(accountId: accountId, attachmentId: attachmentId.lowercased())) {
            guard let pool = try? db.pool(forAccountId: accountId) else { return nil }
            return ((try? pool.read { try fetch(attachmentId, $0) }) ?? nil)
        }
    }

    /// #PERF-5: `info` for a layout pass (the read-only Markdown): memory
    /// only, an unknown or stale row loads off main and renders when it lands.
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
        { id in info(id, db: db, accountId: accountId) }
    }
}
