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
@MainActor
enum AttachmentInfoCache {
    private struct Key: Hashable {
        let accountId: String
        let attachmentId: String
    }

    private static var entries: [Key: (value: AttachmentMediaInfo?, at: Date)] = [:]
    private static let ttl: TimeInterval = 5
    private static let capacity = 512

    static func info(_ attachmentId: String, db: DatabaseManager, accountId: String) -> AttachmentMediaInfo? {
        let key = Key(accountId: accountId, attachmentId: attachmentId.lowercased())
        let now = Date()
        if let hit = entries[key], now.timeIntervalSince(hit.at) < ttl { return hit.value }
        let value = fetch(attachmentId, db: db, accountId: accountId)
        if entries.count >= capacity { entries.removeAll(keepingCapacity: true) }
        entries[key] = (value, now)
        return value
    }

    private static func fetch(_ attachmentId: String, db: DatabaseManager, accountId: String) -> AttachmentMediaInfo? {
        guard let pool = try? db.pool(forAccountId: accountId) else { return nil }
        let row = (try? pool.read { db in
            try AttachmentEntity.filter(Column("id") == attachmentId).fetchOne(db)
        }) ?? nil
        return row.map(AttachmentMediaInfo.init)
    }

    /// A resolver closure for an `IssueEditorModel` bound to one account.
    static func resolver(db: DatabaseManager, accountId: String) -> (String) -> AttachmentMediaInfo? {
        { id in info(id, db: db, accountId: accountId) }
    }
}
