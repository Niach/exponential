import Foundation
import GRDB

/// #EDE-114: the synced `messages` of every chat, OPENED with the tree key
/// (sealed-content.json); a row whose key is not here yet is stored with
/// `body: null` and opened again when a key arrives (and every
/// `lockedRetry`). Taken in a few seconds after the table moved, from a mark
/// on `createdAt` (inclusive: a row landing in the same instant is not lost).
extension ChatStoreHub {
    /// A synced message as the store keeps it (nil = it belongs to no chat).
    public static func storedMessage(_ row: MessageEntity, opener: SealedOpener) -> [String: Any]? {
        guard let sessionId = row.sessionId, !sessionId.isEmpty, let createdAt = ChatStoreWire.isoMs(row.createdAt) else { return nil }
        let rootId = row.rootId.isEmpty ? sessionId : row.rootId
        let body: Any
        if SealedEnvelope.isSealedBody(row.body) {
            let opened = opener.crypto.openBody(row.body, keys: opener.ring.keys(rootId: rootId))
            body = opened.status.isReadable ? (opened.text ?? "") as Any : NSNull()
        } else {
            body = row.body
        }
        return [
            "id": row.id, "rootId": rootId, "sessionId": sessionId,
            "authorKind": row.authorKind, "body": body, "createdAt": createdAt,
        ]
    }

    static func messagesJson(_ rows: [[String: Any]]) -> String? {
        guard !rows.isEmpty, let data = try? JSONSerialization.data(withJSONObject: rows) else { return nil }
        return String(data: data, encoding: .utf8)
    }

    /// Watches the account's `messages` table (writes only, never reads).
    func startMessages(accountId: String) {
        guard let pool = try? lock.withLock({ db })?.pool(forAccountId: accountId) else { return }
        let observer = DatabaseRegionObservation(tracking: Table("messages"))
            .start(in: pool, onError: { _ in }, onChange: { [weak self] _ in
                self?.scheduleMessages(accountId: accountId, after: ChatStoreHub.messagesDebounce)
            })
        lock.withLock { messageObservers[accountId] = observer }
    }

    func scheduleMessages(accountId: String, after delay: DispatchTimeInterval) {
        let first = lock.withLock { messagesScheduled.insert(accountId).inserted }
        guard first else { return }
        let generation = generation(accountId: accountId)
        queue.asyncAfter(deadline: .now() + delay) { [self] in
            lock.withLock { _ = messagesScheduled.remove(accountId) }
            guard self.generation(accountId: accountId) == generation else { return }
            catchUpMessages(accountId: accountId)
        }
    }

    /// A key arrived: the still-sealed messages are opened again.
    func keysChanged() {
        let open = lock.withLock { () -> [String] in
            lockedTriedAt = [:]
            return Array(stores.keys)
        }
        for accountId in open { scheduleMessages(accountId: accountId, after: .seconds(1)) }
    }

    /// The synced messages since the mark (on the writer queue), then (when
    /// due) the ones stored sealed, opened again.
    func catchUpMessages(accountId: String) {
        let (db, opener) = lock.withLock { (self.db, self.opener) }
        guard let store = store(accountId: accountId), let pool = try? db?.pool(forAccountId: accountId) else { return }
        var mark = (try? store.meta(key: Self.messagesMark)) ?? nil ?? ""
        while true {
            let since = mark
            let rows = (try? pool.read { db in
                try MessageEntity.fetchAll(
                    db,
                    sql: "SELECT * FROM messages WHERE session_id IS NOT NULL AND session_id != '' AND created_at >= ? ORDER BY created_at LIMIT ?",
                    arguments: [since, Self.messagesChunk]
                )
            }) ?? []
            guard let json = Self.messagesJson(rows.compactMap { Self.storedMessage($0, opener: opener) }) else { break }
            guard (try? store.putMessages(messagesJson: json)) != nil else { return }
            let newest = rows.map(\.createdAt).max() ?? since
            guard newest > mark else { break }
            mark = newest
            try? store.setMeta(key: Self.messagesMark, value: mark)
            if rows.count < Self.messagesChunk { break }
        }
        let due = lock.withLock { () -> Bool in
            guard lockedTriedAt[accountId].map({ Date().timeIntervalSince($0) >= Self.lockedRetry }) ?? true else { return false }
            lockedTriedAt[accountId] = Date()
            return true
        }
        guard due, let locked = try? store.lockedMessageIds(), !locked.isEmpty else { return }
        for start in stride(from: 0, to: locked.count, by: Self.lockedChunk) {
            let ids = Array(locked[start..<min(start + Self.lockedChunk, locked.count)])
            let rows = (try? pool.read { db in try MessageEntity.fetchAll(db, keys: ids) }) ?? []
            let opened = rows.compactMap { Self.storedMessage($0, opener: opener) }.filter { !($0["body"] is NSNull) }
            if let json = Self.messagesJson(opened) { _ = try? store.putMessages(messagesJson: json) }
        }
    }
}
